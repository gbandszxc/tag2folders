//! 预览：不触碰文件系统地生成 源→目标 映射。
//! 移植自 backend/api/routes/preview.py。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::metadata::{FALLBACK_ALBUM, FALLBACK_ARTIST, FALLBACK_GENRE, FALLBACK_YEAR};
use crate::core::organizer::{check_write_access, claim_key, plan_targets};
use crate::core::path_security::validate_target_dir;
use crate::core::path_util::{
    dir_writable_executable, is_within, normpath_str, paths_equal_ci, resolve_lenient,
};
use crate::core::template::{render_path, validate_template};
use crate::core::{AudioMetadata, FileMappingItem, MappingStatus, OrganizeMode};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PreviewRequest {
    pub files: Vec<AudioMetadata>,
    pub template: String,
    pub target_dir: String,
    pub mode: OrganizeMode,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PreviewResponse {
    pub template: String,
    pub target_dir: String,
    pub total: usize,
    pub mappings: Vec<FileMappingItem>,
    #[serde(default)]
    pub template_errors: Vec<String>,
    /// 嵌套目标目录树：目录名 → 子树；特殊键 `__files__` → 文件名数组
    pub directory_tree: serde_json::Value,
}

/// 预览错误。Template 对应 FastAPI 422 `{"template_errors": [...]}`；
/// Validation 对应 400 字符串 detail。
#[derive(Debug)]
pub enum PreviewError {
    Template(Vec<String>),
    Validation(String),
}

impl std::fmt::Display for PreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreviewError::Template(errs) => {
                write!(f, "{{\"template_errors\":{:}}}", serde_json::to_string(errs).unwrap_or_default())
            }
            PreviewError::Validation(m) => write!(f, "{m}"),
        }
    }
}

/// Python `Path(p).parent` 的父目录语义：单组件路径 Python 返回 "."，
/// Rust `parent()` 返回 ""，这里统一为 "."。
fn src_parent(p: &str) -> &Path {
    match Path::new(p).parent() {
        Some(par) if !par.as_os_str().is_empty() => par,
        _ => Path::new("."),
    }
}

/// 组件级求 child 相对 root 的剩余路径段（等价 Python `relative_to` 成功时
/// 返回的相对路径的 parts；Windows 下按 PureWindowsPath 语义大小写不敏感比较，
/// 但保留 child 原始大小写）。root 不是 child 的前缀时返回 None；
/// 两路径相等时返回空列表（Python 返回 "."，其 parts 为空元组）。
fn relative_parts(child: &Path, root: &Path) -> Option<Vec<String>> {
    let cc: Vec<String> = child
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let rc: Vec<String> = root
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if rc.len() > cc.len() {
        return None;
    }
    for (a, b) in rc.iter().zip(cc.iter()) {
        if !component_eq(a, b) {
            return None;
        }
    }
    Some(cc[rc.len()..].to_vec())
}

#[cfg(windows)]
fn component_eq(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

#[cfg(not(windows))]
fn component_eq(a: &str, b: &str) -> bool {
    a == b
}

/// 生成预览映射。纯只读，不创建任何文件/目录。
/// 三遍算法 + 文件-目录冲突检测，逐行移植 Python `generate_preview`。
pub fn generate_preview(req: &PreviewRequest) -> Result<PreviewResponse, PreviewError> {
    // 模板校验失败 → FastAPI 422 {"template_errors": [...]}
    let template_errors = validate_template(&req.template);
    if !template_errors.is_empty() {
        return Err(PreviewError::Template(template_errors));
    }

    // target_dir 校验失败 → FastAPI 400 字符串 detail
    let target_root = validate_target_dir(&req.target_dir).map_err(PreviewError::Validation)?;

    let n = req.files.len();

    // ── Pass 1：渲染相对目标并计算原始绝对目标 ──
    // (relative_target, normpath 规范化后的绝对目标, boundary_ok)
    let mut render_results: Vec<(String, String, bool)> = Vec::with_capacity(n);
    for file_info in &req.files {
        let relative_target = render_path(&req.template, file_info);
        let raw_abs = target_root.join(&relative_target);

        // 仅用 resolve 验证边界约束。resolve 结果不作为 final_target 存储：
        // 大小写不敏感文件系统上 resolve() 会把用户指定的大小写重命名
        // （song.mp3 → Song.mp3）改写回磁盘上的既有拼写，使其悄悄变成原地空操作。
        // raw_abs 保留用户意图的大小写。
        let resolved = resolve_lenient(&raw_abs);
        // 边界检查：resolve 后的路径必须位于 target_root 之内且不等于 target_root 自身。
        // 与 target_root 相等的路径能通过 relative_to()（返回 "."），但 organize_file()
        // 总把 final_target 当文件路径处理——会把音频字节写进目录本身。
        let boundary_ok =
            is_within(&resolved, &target_root) && !paths_equal_ci(&resolved, &target_root);

        // 存储前先在字符串层面规范化 '..'/'.' 段，使 plan_targets() 看到的
        // 键与 _run_organize() 将使用的字符串一致。等价模板如 'A/../song.mp3'
        // 与 'B/../song.mp3' 规范化后都是 'song.mp3'；不规范化就会表现为不同键，
        // plan_targets() 检测不到重复，导致预检以 422 拒绝整个批次。
        // os.path.normpath 纯字符串去 '..'、不碰文件系统，因此保留大小写重命名意图。
        render_results.push((
            relative_target,
            normpath_str(&raw_abs.to_string_lossy()),
            boundary_ok,
        ));
    }

    // ── 预 Pass 2：MOVE 模式的重复源路径检测 ──
    // 镜像 organize.py 的预检规则：一个源文件只能被移动一次。为每个条目算一个
    // 规范键（文件存在则 resolve 解析符号链接；不存在则退回 normpath）。
    // 键出现多于一次的源在 MOVE 模式下不可执行。
    let mut src_keys: Vec<String> = Vec::with_capacity(n);
    for fi in &req.files {
        let p = Path::new(&fi.path);
        let key = if p.exists() || p.is_symlink() {
            resolve_lenient(p).to_string_lossy().into_owned()
        } else {
            normpath_str(&fi.path)
        };
        src_keys.push(key);
    }
    let mut dup_move_srcs: HashSet<String> = HashSet::new();
    if req.mode == OrganizeMode::Move {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for k in &src_keys {
            *counts.entry(k.as_str()).or_insert(0) += 1;
        }
        dup_move_srcs = counts
            .into_iter()
            .filter(|&(_, v)| v > 1)
            .map(|(k, _)| k.to_string())
            .collect();
    }

    // ── Pass 2：规划最终目标（消解磁盘冲突 + 批内碰撞）──
    // 只把可整理条目交给 plan_targets()：本阶段已确定会被拒绝的条目必须排除，
    // 否则它们会抢占 _1/_2 重命名后缀槽位，或让混合批次中的合法文件
    // 莫名收到 batch_conflict 状态。排除条件：
    //   • boundary_ok=False → boundary_error
    //   • readable=False → unreadable
    //   • move 模式、非原地、源父目录不可写 → write_error
    //   • move 模式、源路径重复 → write_error
    //   • copy 模式、大小写不敏感盘上的纯大小写重命名 → write_error
    let organizable_idx: Vec<usize> = (0..n)
        .filter(|&i| {
            let fi = &req.files[i];
            let raw = &render_results[i].1;
            render_results[i].2
                && fi.readable
                && !(req.mode == OrganizeMode::Move
                    && normpath_str(&fi.path) != normpath_str(raw)
                    && !dir_writable_executable(src_parent(&fi.path)))
                && !(req.mode == OrganizeMode::Move && dup_move_srcs.contains(&src_keys[i]))
                && !(req.mode == OrganizeMode::Copy
                    && claim_key(Path::new(raw)) == claim_key(Path::new(&fi.path))
                    && normpath_str(raw) != normpath_str(&fi.path))
        })
        .collect();
    let mut final_targets: Vec<String> = render_results
        .iter()
        .map(|(_, raw, _)| raw.clone())
        .collect();
    {
        let org_raws: Vec<String> = organizable_idx
            .iter()
            .map(|&i| render_results[i].1.clone())
            .collect();
        let org_sources: Vec<String> = organizable_idx
            .iter()
            .map(|&i| req.files[i].path.clone())
            .collect();
        let planned = run_plan_targets(&org_raws, &org_sources);
        for (k, &i) in organizable_idx.iter().enumerate() {
            final_targets[i] = planned[k].clone();
        }
    }

    // ── Pass 2 与 Pass 3 之间：文件-目录祖先冲突检测 ──
    // 两个最终目标互为祖先/后代（如 foo.mp3 与 foo.mp3/bar.mp3）时冲突：
    // 批次一旦执行，前一个文件会挡住后一个的 mkdir()。organize 的
    // _preflight_check() 拒绝的正是这种形状；这里同样检测，保证预览与整理一致。
    // 只有可整理条目参与——boundary_error/unreadable/write_error 条目不会进入
    // 整理调用，不会产生真实冲突。
    let organizable_set: HashSet<usize> = organizable_idx.iter().copied().collect();
    let ft_norms: Vec<Option<PathBuf>> = final_targets
        .iter()
        .enumerate()
        .map(|(i, ft)| {
            organizable_set
                .contains(&i)
                .then(|| PathBuf::from(claim_key(Path::new(ft))))
        })
        .collect();
    let mut file_vs_dir_conflict_set: HashSet<usize> = HashSet::new();
    for i in 0..ft_norms.len() {
        let Some(norm_i) = &ft_norms[i] else { continue };
        for j in 0..i {
            let Some(norm_j) = &ft_norms[j] else { continue };
            if norm_i.starts_with(norm_j) || norm_j.starts_with(norm_i) {
                file_vs_dir_conflict_set.insert(i);
                file_vs_dir_conflict_set.insert(j);
            }
        }
    }
    // ── Pass 2b：排除文件-目录冲突条目后重跑 plan_targets ──
    // 冲突条目在 Pass 3 会被标 write_error 并被整理调用排除。若它们留在
    // organizable_idx 里，其永远无法执行的 final_target 仍会占用 plan_targets()
    // 的冲突消解后缀槽（_1、_2…），迫使合法条目落入不必要的改名路径。
    // 剔除后重跑，让后缀槽只分配给真正会执行的文件。
    let mut organizable_idx = organizable_idx;
    if !file_vs_dir_conflict_set.is_empty() {
        organizable_idx.retain(|&i| !file_vs_dir_conflict_set.contains(&i));
        let org_raws: Vec<String> = organizable_idx
            .iter()
            .map(|&i| render_results[i].1.clone())
            .collect();
        let org_sources: Vec<String> = organizable_idx
            .iter()
            .map(|&i| req.files[i].path.clone())
            .collect();
        let planned = run_plan_targets(&org_raws, &org_sources);
        // 重置 final_targets 为原始路径，再回填收窄后集合的规划结果；
        // 冲突条目保留原始路径作占位（整理执行前会被排除）。
        for (i, ft) in final_targets.iter_mut().enumerate() {
            *ft = render_results[i].1.clone();
        }
        for (k, &i) in organizable_idx.iter().enumerate() {
            final_targets[i] = planned[k].clone();
        }
    }
    // 注：Python 在此处还会刷新 _organizable_set，但其后并无任何读取（下方
    // 碰撞检测直接遍历 organizable_idx），属死代码，不移植。

    // ── Pass 3：构建带状态的 FileMappingItem ──
    // 检测哪些原始目标在规划前已有磁盘冲突。排除自冲突：源 == 渲染目标
    // （原地整理）时文件已在正确位置——不是真冲突。
    let mut on_disk_set: HashSet<String> = HashSet::new();
    for i in 0..n {
        let raw = &render_results[i].1;
        let raw_path = Path::new(raw.as_str());
        // 悬空符号链接视作已占用：exists() 对其返回 False，但 shutil 会跟随
        // 链接写到别处——用户看到的文件位置将与预览不同。
        if (raw_path.exists() || raw_path.is_symlink())
            && claim_key(raw_path) != claim_key(Path::new(&req.files[i].path))
        {
            on_disk_set.insert(raw.clone());
        }
    }
    // 只在可整理条目之间检测批内碰撞。不可整理条目（boundary_error、
    // unreadable 等）不会执行，不得为合法文件抢占碰撞槽。
    let mut org_raw_counts: HashMap<String, usize> = HashMap::new();
    for &i in &organizable_idx {
        *org_raw_counts
            .entry(claim_key(Path::new(&render_results[i].1)))
            .or_insert(0) += 1;
    }
    let batch_collision_set: HashSet<String> = organizable_idx
        .iter()
        .map(|&i| render_results[i].1.clone())
        .filter(|raw| {
            org_raw_counts
                .get(&claim_key(Path::new(raw)))
                .is_some_and(|&c| c > 1)
        })
        .collect();

    let mut mappings: Vec<FileMappingItem> = Vec::with_capacity(n);
    for (i, file_info) in req.files.iter().enumerate() {
        let (relative_target, raw_abs, mut boundary_ok) = (
            render_results[i].0.as_str(),
            render_results[i].1.as_str(),
            render_results[i].2,
        );
        let final_target = final_targets[i].as_str();

        // 冲突改名后再次校验：原始渲染路径 resolve 到 target_root 自身时
        // （如模板 "{artist}/.." 且 artist 非空），磁盘冲突消解器可能生成
        // /dest_1 这类逃逸目标目录的同级路径。
        if boundary_ok && !is_within(Path::new(final_target), &target_root) {
            boundary_ok = false;
        }

        let ft_path = Path::new(final_target);
        let src_path = Path::new(file_info.path.as_str());

        let mut status = if !boundary_ok {
            MappingStatus::BoundaryError
        } else if !file_info.readable {
            MappingStatus::Unreadable
        } else if file_vs_dir_conflict_set.contains(&i) {
            // 一个 final_target 是另一个的祖先/后代：整理预检将这种文件-目录
            // 碰撞判为拒绝。用 write_error 让前端在调用整理前排除这些映射。
            // 先于 conflict/batch_conflict 判定：既不可执行又有冲突的映射要以
            // write_error 面世——handleStartOrganize 会过滤 write_error，但会把
            // conflict 标签的映射提交给整理，导致整个批次被 422 拒绝。
            MappingStatus::WriteError
        } else if normpath_str(&file_info.path) != normpath_str(final_target)
            && check_write_access(ft_path).is_some()
        {
            // 目标路径无法创建：祖先是文件或不可写。先于 conflict/batch_conflict
            // 判定，理由同上。final_target == source 时跳过：organize_file()
            // 是空操作、不碰文件系统，无需写权限。
            MappingStatus::WriteError
        } else if req.mode == OrganizeMode::Move
            && normpath_str(&file_info.path) != normpath_str(final_target)
            && !dir_writable_executable(src_parent(&file_info.path))
        {
            // MOVE 模式要求源文件父目录可写+可进入（用于 rename/unlink）。
            // 先于 conflict/batch_conflict 判定，避免不可移动源的映射被提交。
            // 例外：final_target == source 是精确的原地空操作——organize_file()
            // 直接返回、不碰文件系统，父目录无需可写。
            MappingStatus::WriteError
        } else if req.mode == OrganizeMode::Move && dup_move_srcs.contains(&src_keys[i]) {
            // 该源路径在 MOVE 批次中出现多于一次。organize_file() 预检以
            // "Duplicate source in move batch" 拒绝重复移动源。标 write_error
            // 让前端在调用整理前过滤掉所有重复出现。
            MappingStatus::WriteError
        } else if req.mode == OrganizeMode::Copy
            && claim_key(ft_path) == claim_key(src_path)
            && normpath_str(final_target) != normpath_str(&file_info.path)
        {
            // COPY 模式在大小写不敏感文件系统上：目标与源仅大小写不同
            // （如 song.mp3 → Song.mp3）。二者是同一个底层文件；organize_file()
            // 显式拒绝。在此浮出，让 UI 过滤该映射。
            MappingStatus::WriteError
        } else if on_disk_set.contains(raw_abs) {
            MappingStatus::Conflict
        } else if batch_collision_set.contains(raw_abs) {
            MappingStatus::BatchConflict
        } else if file_info.artist == FALLBACK_ARTIST
            || file_info.album == FALLBACK_ALBUM
            || file_info.year == FALLBACK_YEAR
            || file_info.genre == FALLBACK_GENRE
        {
            MappingStatus::MissingMetadata
        } else {
            MappingStatus::Ok
        };

        // 检测链式重命名：plan_targets() 之所以重命名该映射，是因为它本应占用的
        // 冲突消解槽已被批次中另一条目抢占。上面的原始路径碰撞集合捕捉不到这种
        // 情况；比较 final_target 与原始渲染路径，把隐藏的重命名浮出来给 UI 展示。
        let chained_conflict =
            matches!(status, MappingStatus::Ok | MappingStatus::MissingMetadata)
                && final_target != raw_abs;
        if chained_conflict {
            status = MappingStatus::Conflict;
        }

        mappings.push(FileMappingItem {
            source: file_info.path.clone(),
            target: raw_abs.to_string(),
            final_target: final_target.to_string(),
            relative_target: relative_target.to_string(),
            status,
            conflict: on_disk_set.contains(raw_abs) || chained_conflict,
            batch_conflict: batch_collision_set.contains(raw_abs)
                || file_vs_dir_conflict_set.contains(&i),
        });
    }

    let directory_tree = build_directory_tree(&mappings, &target_root);

    Ok(PreviewResponse {
        template: req.template.clone(),
        target_dir: target_root.to_string_lossy().into_owned(),
        total: mappings.len(),
        mappings,
        template_errors: Vec::new(),
        directory_tree,
    })
}

/// plan_targets 的空调用守卫：空列表直接返回空（与 Python 端条件一致）。
fn run_plan_targets(raws: &[String], sources: &[String]) -> Vec<String> {
    if raws.is_empty() {
        Vec::new()
    } else {
        plan_targets(raws, Some(sources))
    }
}

/// 构建嵌套目录树（`__files__` 哨兵键），排除 boundary_error/unreadable/write_error。
pub fn build_directory_tree(mappings: &[FileMappingItem], target_root: &Path) -> serde_json::Value {
    // 前端在整理调用前会排除的状态——在此镜像，使目录预览只展示
    // 真正会被创建的文件。
    // 哨兵键用于在每个树节点内存储文件列表。目录组件恰好叫 '__files__' 时
    // 追加一个空字节转义：空字节在所有文件系统路径组件（POSIX、APFS、NTFS）
    // 中均非法，因此 '__files__\0' 永远不会与树里的真实目录条目冲突。
    const SENTINEL: &str = "__files__";
    const ESCAPED_SENTINEL: &str = "__files__\u{0}";
    let mut tree = serde_json::Map::new();
    for m in mappings {
        if matches!(
            m.status,
            MappingStatus::BoundaryError | MappingStatus::Unreadable | MappingStatus::WriteError
        ) {
            continue;
        }
        // 树基于各映射相对 target_root 的 *final_target* 构建，
        // 反映执行后用户在磁盘上实际看到的重命名规划路径。
        let Some(parts) = relative_parts(Path::new(&m.final_target), target_root) else {
            continue;
        };
        let mut node: &mut serde_json::Map<String, serde_json::Value> = &mut tree;
        for part in &parts[..parts.len().saturating_sub(1)] {
            // 转义哨兵名，避免名为 '__files__' 的目录与存储同级文件列表的键冲突
            let key = if part == SENTINEL {
                ESCAPED_SENTINEL
            } else {
                part.as_str()
            };
            if !node.contains_key(key) {
                node.insert(key.to_string(), serde_json::Value::Object(Default::default()));
            }
            node = node
                .get_mut(key)
                .and_then(serde_json::Value::as_object_mut)
                .expect("树节点必为对象");
        }
        let filename = parts.last().cloned().unwrap_or_else(|| m.final_target.clone());
        node.entry(SENTINEL.to_string())
            .or_insert_with(|| serde_json::Value::Array(Vec::new()))
            .as_array_mut()
            .expect("哨兵键必为数组")
            .push(serde_json::Value::String(filename));
    }
    serde_json::Value::Object(tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 测试辅助（移植自 tests/test_api.py 的 _make_file_info/_preview）──

    fn make_file_info(
        path: &str,
        artist: &str,
        album: &str,
        title: &str,
        readable: bool,
    ) -> AudioMetadata {
        AudioMetadata {
            path: path.to_string(),
            ext: "mp3".to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            title: title.to_string(),
            track: "1".to_string(),
            year: "2024".to_string(),
            genre: "Rock".to_string(),
            readable,
            error: String::new(),
        }
    }

    /// 缺省文件：与 Python `_make_file_info()` 默认值一致。
    fn default_file_info() -> AudioMetadata {
        make_file_info("/tmp/test.mp3", "Artist", "Album", "Title", true)
    }

    /// Python `_preview` 未传 mode → Pydantic 默认 "copy"。
    fn preview(
        files: Vec<AudioMetadata>,
        template: &str,
        target_dir: &Path,
    ) -> Result<PreviewResponse, PreviewError> {
        generate_preview(&PreviewRequest {
            files,
            template: template.to_string(),
            target_dir: target_dir.to_string_lossy().into_owned(),
            mode: OrganizeMode::Copy,
        })
    }

    /// 每个测试独立的临时目录（uuid 子目录，避免并行冲突）。
    fn temp_subdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("t2f_preview_{tag}_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 递归收集目录下所有条目路径（排序后比较，规避遍历顺序差异）。
    fn snapshot_entries(dir: &Path, out: &mut Vec<String>) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                out.push(p.to_string_lossy().into_owned());
                if p.is_dir() {
                    snapshot_entries(&p, out);
                }
            }
        }
        out.sort();
    }

    // ── Preview: 边界校验 ────────────────────────────────────────────────────

    #[test]
    fn test_preview_boundary_escape_flagged() {
        // 模板含字面 ../ 段——字段值（artist/title 等）渲染前已清洗，
        // 值里的斜杠和点无法越界；只有模板本身含 ../ 字面段才能穿越边界。
        let tmp = temp_subdir("boundary_escape");
        let file_info = make_file_info("/tmp/test.mp3", "Artist", "Album", "song", true);
        let resp = preview(vec![file_info], "../outside/{title}.{ext}", &tmp).unwrap();
        assert_eq!(resp.mappings.len(), 1);
        assert_eq!(resp.mappings[0].status, MappingStatus::BoundaryError);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_preview_normal_file_not_boundary_error() {
        // 正常模板产生 ok（或其他非 boundary）状态。
        let tmp = temp_subdir("normal_file");
        let resp = preview(
            vec![default_file_info()],
            "{artist}/{album}/{title}.{ext}",
            &tmp,
        )
        .unwrap();
        assert_ne!(resp.mappings[0].status, MappingStatus::BoundaryError);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── Preview: 批内碰撞 ────────────────────────────────────────────────────

    #[test]
    fn test_preview_intra_batch_collision_produces_unique_final_targets() {
        // 两个文件映射到同一路径时必须得到不同的 final_target。
        let tmp = temp_subdir("collision_unique");
        let f1 = make_file_info("/tmp/a.mp3", "Same", "Same", "Same", true);
        let f2 = make_file_info("/tmp/b.mp3", "Same", "Same", "Same", true);
        let resp = preview(vec![f1, f2], "{artist}/{album}/{title}.{ext}", &tmp).unwrap();
        let finals: Vec<&str> = resp.mappings.iter().map(|m| m.final_target.as_str()).collect();
        assert_ne!(finals[0], finals[1], "批内碰撞必须产生不同的 final_target");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_preview_intra_batch_collision_status() {
        // 第二个碰撞的文件获得 batch_conflict 状态。
        let tmp = temp_subdir("collision_status");
        let f1 = make_file_info("/tmp/a.mp3", "Same", "Same", "Same", true);
        let f2 = make_file_info("/tmp/b.mp3", "Same", "Same", "Same", true);
        let resp = preview(vec![f1, f2], "{artist}/{album}/{title}.{ext}", &tmp).unwrap();
        let statuses: Vec<MappingStatus> = resp.mappings.iter().map(|m| m.status).collect();
        // 第一个可为 ok 或 batch_conflict；第二个必须是 batch_conflict
        assert_eq!(statuses[1], MappingStatus::BatchConflict);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── Preview: 不改动文件系统 ──────────────────────────────────────────────

    #[test]
    fn test_preview_does_not_create_directories() {
        // 预览不得创建任何目录或文件。
        let tmp = temp_subdir("no_mutation");
        let mut before = Vec::new();
        snapshot_entries(&tmp, &mut before);
        let _ = preview(
            vec![default_file_info()],
            "{artist}/{album}/{title}.{ext}",
            &tmp,
        );
        let mut after = Vec::new();
        snapshot_entries(&tmp, &mut after);
        assert_eq!(before, after, "预览不得改动文件系统");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── Preview: directory_tree 反映重命名后的 final_target ─────────────────

    #[test]
    fn test_preview_directory_tree_reflects_renamed_final_target() {
        // 磁盘冲突导致 plan_targets 把 song.mp3 改名为 song_1.mp3 时，
        // directory_tree 必须展示 song_1.mp3 而非 song.mp3。
        let tmp = temp_subdir("tree_renamed");
        let target_dir = tmp.join("out");
        let artist_dir = target_dir.join("Artist").join("Album");
        std::fs::create_dir_all(&artist_dir).unwrap();
        std::fs::write(artist_dir.join("Title.mp3"), b"existing").unwrap();

        let file_info = make_file_info("/tmp/test.mp3", "Artist", "Album", "Title", true);
        let resp = preview(
            vec![file_info],
            "{artist}/{album}/{title}.{ext}",
            &target_dir,
        )
        .unwrap();

        // 映射表应展示带 _1 后缀的 final_target
        assert!(
            resp.mappings[0].final_target.ends_with("Title_1.mp3"),
            "final_target 应带 _1 后缀；got: {}",
            resp.mappings[0].final_target
        );
        // 目录树也应展示 _1，而非原名
        let files_in_album = &resp.directory_tree["Artist"]["Album"]["__files__"];
        let names: Vec<&str> = files_in_album
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(
            names.contains(&"Title_1.mp3"),
            "directory_tree 必须反映重命名后的路径；got: {names:?}"
        );
        assert!(
            !names.contains(&"Title.mp3"),
            "directory_tree 不得展示重命名前的名字"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_preview_directory_tree_excludes_boundary_error() {
        // boundary_error 状态的映射不得出现在 directory_tree。
        // 含 ../outside/ 的模板不应产生任何树条目。
        let tmp = temp_subdir("tree_boundary");
        let file_info = make_file_info("/tmp/test.mp3", "Artist", "Album", "Song", true);
        let resp = preview(vec![file_info], "../outside/{title}.{ext}", &tmp).unwrap();

        assert_eq!(resp.mappings[0].status, MappingStatus::BoundaryError);
        // 树不得包含 ".." 或任何来自越界路径的条目
        let tree = resp.directory_tree.as_object().expect("树必为对象");
        assert!(
            !tree.contains_key(".."),
            "directory_tree 不得包含 '..' 条目；got keys: {:?}",
            tree.keys().collect::<Vec<_>>()
        );
        assert!(tree.is_empty() || !tree.contains_key(".."));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_preview_directory_tree_batch_collision_shows_distinct_names() {
        // 批内两个文件碰撞时，directory_tree 必须展示两个不同的规划名
        // （_1 后缀），而非同名重复条目。
        let tmp = temp_subdir("tree_collision");
        let f1 = make_file_info("/tmp/a.mp3", "Artist", "Album", "Same", true);
        let f2 = make_file_info("/tmp/b.mp3", "Artist", "Album", "Same", true);
        let resp = preview(vec![f1, f2], "{artist}/{album}/{title}.{ext}", &tmp).unwrap();

        let finals: Vec<&str> = resp.mappings.iter().map(|m| m.final_target.as_str()).collect();
        assert_ne!(finals[0], finals[1]);

        let files_in_album = &resp.directory_tree["Artist"]["Album"]["__files__"];
        let names: Vec<&str> = files_in_album
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(names.len(), 2, "树中应有 2 个文件；got: {names:?}");
        let uniq: HashSet<&str> = names.iter().copied().collect();
        assert_eq!(uniq.len(), 2, "两个规划文件名必须互不相同；got: {names:?}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_preview_directory_tree_excludes_unreadable_and_write_error() {
        // directory_tree 只能包含真正会被创建的文件：unreadable / write_error
        // 状态的映射会被前端在调用整理前过滤，其路径不得出现在树中。
        let tmp = temp_subdir("tree_excluded");
        let target_dir = tmp.join("out");
        std::fs::create_dir_all(&target_dir).unwrap();

        // 一个 blocker 使某映射得到 write_error（镜像原测试的构造）
        std::fs::write(target_dir.join("blocked"), b"i am a file").unwrap();

        // 文件 1：正常可读——应出现在树的 Music/ 下
        let f_ok = make_file_info("/tmp/ok.mp3", "Artist", "Album", "OkSong", true);
        // 文件 2：不可读——不得出现在树中
        let f_unread = make_file_info("/tmp/bad.mp3", "Artist", "Album", "BadSong", false);

        let resp = preview(vec![f_ok, f_unread], "Music/{title}.{ext}", &target_dir).unwrap();
        let statuses: Vec<MappingStatus> = resp.mappings.iter().map(|m| m.status).collect();
        assert!(
            statuses.contains(&MappingStatus::Unreadable),
            "f_unread 必须是 unreadable"
        );

        // 展平树收集全部文件名
        fn collect_files(node: &serde_json::Value, files: &mut Vec<String>) {
            if let Some(arr) = node.get("__files__").and_then(|v| v.as_array()) {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        files.push(s.to_string());
                    }
                }
            }
            if let Some(obj) = node.as_object() {
                for (k, v) in obj {
                    if k != "__files__" && v.is_object() {
                        collect_files(v, files);
                    }
                }
            }
        }
        let mut all_files = Vec::new();
        collect_files(&resp.directory_tree, &mut all_files);
        assert!(
            !all_files.iter().any(|f| f == "BadSong.mp3"),
            "不可读文件不得出现在 directory_tree；found in: {all_files:?}"
        );
        assert!(
            all_files.iter().any(|f| f == "OkSong.mp3"),
            "可读文件必须出现在 directory_tree；tree files: {all_files:?}"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_preview_directory_tree_files_sentinel_not_corrupted_by_dirname() {
        // 模板渲染出名为 '__files__' 的目录组件时，哨兵键不得被子字典覆盖。
        // 修复后 '__files__' 目录组件存进一个不会与文件列表哨兵冲突的转义键。
        let tmp = temp_subdir("tree_sentinel");
        let target_dir = tmp.join("out");
        std::fs::create_dir_all(&target_dir).unwrap();

        // 渲染路径含 '__files__' 目录：'{artist}/{title}.{ext}'
        // 且 artist='__files__' → '__files__/song.mp3'
        let f_sentinel_dir =
            make_file_info("/tmp/sentinel.mp3", "__files__", "Album", "song", true);
        // 根层普通文件——修复前向该条目 append 会作用在 '__files__' 子目录的
        // 字典上（Python 端会 AttributeError → 500）。
        let f_normal = make_file_info("/tmp/normal.mp3", "Artist", "Album", "normal", true);

        let resp = preview(
            vec![f_sentinel_dir, f_normal],
            "{artist}/{title}.{ext}",
            &target_dir,
        )
        .unwrap();

        let tree = resp.directory_tree.as_object().expect("树必为对象");
        // 根层的 __files__ 条目（若存在）必须是文件名数组，而非遍历
        // '__files__' 目录组件后残留的子字典
        if let Some(v) = tree.get("__files__") {
            assert!(
                v.is_array(),
                "树根的 '__files__' 键必须是文件名数组；got: {v:?}"
            );
        }
        // 目录组件 '__files__' 走转义键 '__files__\0'，其内部哨兵列表存 song.mp3
        let escaped = tree
            .get("__files__\u{0}")
            .and_then(|v| v.get("__files__"))
            .and_then(|v| v.as_array())
            .expect("转义键 '__files__\\0' 下应存在文件列表");
        let names: Vec<&str> = escaped.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(names, vec!["song.mp3"]);
        // 普通文件不受影响
        let normal = tree
            .get("Artist")
            .and_then(|v| v.get("__files__"))
            .and_then(|v| v.as_array())
            .expect("Artist 下应存在文件列表");
        let normal_names: Vec<&str> = normal.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(normal_names, vec!["normal.mp3"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

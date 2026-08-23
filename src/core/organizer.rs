//! 文件整理：计划目标路径 + 执行移动/复制。
//! 路径工具见 `core::path_util`。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use crate::core::path_util;
use crate::core::{FileMappingItem, OrganizeMode};

/// 单文件整理结果，对应 Python `OrganizeResult`。
/// source/planned_target 与 Python 端一一对应（错误消息内已内联使用），
/// 保留完整契约以便调用方诊断。
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct OrganizeResult {
    pub source: String,
    pub planned_target: String,
    pub actual_target: String,
    pub success: bool,
    pub error_message: String,
}

/// OS 级默认：macOS 与 Windows 的文件系统通常大小写不敏感。
/// 等价 Python `_FOLD_PATHS = platform.system() in ("Darwin", "Windows")`，
/// 用于文件系统探测不可用时的回退。
#[cfg(any(windows, target_os = "macos"))]
const FOLD_PATHS: bool = true;
#[cfg(not(any(windows, target_os = "macos")))]
const FOLD_PATHS: bool = false;

/// 探测结果缓存（等价 Python `functools.lru_cache(maxsize=32)`）。
static PROBE_CACHE: LazyLock<Mutex<HashMap<PathBuf, bool>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 探测 *dir* 所在文件系统是否大小写不敏感（带缓存，等价 Python
/// `_probe_case_insensitive` 的 lru_cache）：对目录名做大小写变换后
/// `same_file` 判定；名字无字母时向父级上溯。
pub fn probe_case_insensitive(dir: &Path) -> bool {
    let cache = &PROBE_CACHE;
    if let Ok(guard) = cache.lock() {
        if let Some(hit) = guard.get(dir) {
            return *hit;
        }
    }
    let result = probe_uncached(dir);
    if let Ok(mut guard) = cache.lock() {
        guard.insert(dir.to_path_buf(), result);
    }
    result
}

/// 无缓存的探测实现（含向父级上溯的递归）。
///
/// 通过检查目录自身名字的大小写变体是否能解析到同一目录来判定，
/// 无需创建任何文件，预览/预检因此保持严格无副作用。
fn probe_uncached(dir: &Path) -> bool {
    // 根目录或盘根没有名字可测：直接用 OS 级默认
    let name = match dir.file_name().and_then(|n| n.to_str()) {
        Some(n) if !n.is_empty() => n,
        _ => return FOLD_PATHS,
    };
    // 优先大写形式；若名字本身就是全大写则改试小写
    let upper = name.to_uppercase();
    let alt = if name != upper { upper } else { name.to_lowercase() };
    if alt == name {
        // 名字在大小写变换下不变（如纯数字 '2024'）：
        // 上溯找带字母的祖先做可靠探测
        return match dir.parent() {
            Some(parent) => probe_uncached(parent),
            None => FOLD_PATHS,
        };
    }
    let alt_path = match dir.parent() {
        Some(parent) => parent.join(alt),
        None => return FOLD_PATHS,
    };
    if !alt_path.exists() {
        return false;
    }
    // 验证大小写变体路径与原路径是同一个目录（同 inode），
    // 而非大小写敏感文件系统上恰好存在的另一个兄弟目录
    path_util::same_file(dir, &alt_path)
}

/// 冲突键：大小写不敏感文件系统上返回小写字符串，否则原样。
/// 从 p 的最近存在祖先目录探测文件系统属性。
pub fn claim_key(p: &Path) -> String {
    let s = p.to_string_lossy().into_owned();
    // 从 p 的父级向上找第一个存在的目录用于探测，
    // 这样在大小写敏感的 macOS（APFS/HFS+）卷上也能得到正确结果
    let mut probe_dir = match p.parent() {
        Some(parent) => parent.to_path_buf(),
        None => return if FOLD_PATHS { s.to_lowercase() } else { s },
    };
    loop {
        if probe_dir.is_dir() {
            return if probe_case_insensitive(&probe_dir) {
                s.to_lowercase()
            } else {
                s
            };
        }
        let next = probe_dir.parent().map(Path::to_path_buf);
        match next {
            Some(parent) => probe_dir = parent,
            // 到达文件系统根仍未找到可探测目录：用 OS 默认
            None => return if FOLD_PATHS { s.to_lowercase() } else { s },
        }
    }
}

/// 按 Python pathlib 语义拆分文件名：返回 (stem, suffix)。
/// 规则：取 name 中最后一个 '.'（索引 i），仅当 0 < i < len(name)-1 时
/// 才视为扩展名分隔点；前导点（隐藏文件）与尾随点不算扩展名。
fn python_stem_suffix(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => name.split_at(i),
        _ => (name, ""),
    }
}

/// 给定原始目标路径列表，计算消解磁盘冲突与批内碰撞后的最终路径。
/// 附加 `sources` 时，目标等于自身源的条目（原地整理）不加后缀。
/// 返回与输入等长等序的列表。
pub fn plan_targets(raw_targets: &[String], sources: Option<&[String]>) -> Vec<String> {
    let mut claimed: HashSet<String> = HashSet::new();
    let mut result: Vec<String> = Vec::with_capacity(raw_targets.len());

    for (i, raw) in raw_targets.iter().enumerate() {
        let path = Path::new(raw);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (stem, suffix) = python_stem_suffix(&name);
        let parent = path.parent().unwrap_or_else(|| Path::new("")).to_path_buf();

        // 对应源文件的冲突键：目标等于自身源（原地整理）时
        // 不视为磁盘冲突，文件已在正确位置，不能追加 _1 后缀
        let source_key = sources.map(|s| claim_key(Path::new(&s[i])));

        let mut candidate = path.to_path_buf();
        let mut counter = 1usize;
        loop {
            let key = claim_key(&candidate);
            // 真实文件与悬空符号链接都视为已占用：
            // Path.exists() 跟随符号链接，对悬空链接返回 False，
            // 必须配合 is_symlink() 才能检出
            let occupied = candidate.exists() || candidate.is_symlink();
            let is_own_source = match &source_key {
                Some(sk) => key == *sk,
                None => false,
            };
            if !claimed.contains(&key) && !(occupied && !is_own_source) {
                claimed.insert(key);
                result.push(candidate.to_string_lossy().into_owned());
                break;
            }
            candidate = parent.join(format!("{stem}_{counter}{suffix}"));
            counter += 1;
        }
    }

    result
}

/// 只读探测：目标路径是否可写。返回 None 表示可写，Some(错误说明) 不可写。
/// 绝不创建目录；等价 Python `check_write_access`（含错误文案）。
pub fn check_write_access(target_path: &Path) -> Option<String> {
    let mut ancestor = match target_path.parent() {
        Some(parent) => parent.to_path_buf(),
        // 根路径自身作为祖先（对应 Python Path 根的 parent 是自身）
        None => target_path.to_path_buf(),
    };
    let mut visited: HashSet<PathBuf> = HashSet::new();
    // 在第一个"存在或为符号链接（含悬空）"的祖先处停下；
    // 不检查 is_symlink 的话悬空链接会被跳过，误判为可写
    while !ancestor.exists() && !ancestor.is_symlink() {
        if !visited.insert(ancestor.clone()) {
            return Some(format!(
                "Cannot determine write access for: {}",
                target_path.display()
            ));
        }
        let next = ancestor.parent().map(Path::to_path_buf);
        match next {
            Some(parent) => ancestor = parent,
            None => {
                return Some(format!(
                    "Cannot determine write access for: {}",
                    target_path.display()
                ))
            }
        }
    }

    // 祖先链中的悬空符号链接不是可用目录
    if ancestor.is_symlink() && !ancestor.exists() {
        return Some(format!(
            "Target ancestor is a broken symlink: {}. Cannot create path: {}",
            ancestor.display(),
            target_path.display()
        ));
    }

    // 第一个存在的祖先必须是目录；若是普通文件（或任何非目录），
    // mkdir(parents=True) 会抛 FileExistsError / NotADirectoryError
    if !ancestor.is_dir() {
        return Some(format!(
            "Target ancestor is not a directory (it is a file): {}. Cannot create path: {}",
            ancestor.display(),
            target_path.display()
        ));
    }

    // 在目录下创建文件/子目录需要写权限（添加目录项）+ 执行/搜索权限
    // （遍历目录），只查 W_OK 不够
    if !path_util::dir_writable_executable(&ancestor) {
        return Some(format!(
            "No write+execute permission for directory: {}",
            ancestor.display()
        ));
    }
    None
}

/// 移动/复制单个文件到预计划路径。按需创建父目录；执行期竞态冲突时
/// 回退到 `_1` 后缀策略；大小写重命名语义与 Python 端一致。
pub fn organize_file(source: &str, planned_target: &str, mode: OrganizeMode) -> OrganizeResult {
    let source_path = Path::new(source);
    let planned_target_path = Path::new(planned_target);

    // 精确原地：源与目标是同一路径，无论模式都无事可做。
    // 用 normpath 字符串比较而非 Path 相等：Windows 上 Path 相等是
    // 大小写不敏感的，会把仅大小写不同的重命名误判为原地跳过
    if path_util::normpath_str(source) == path_util::normpath_str(planned_target) {
        return OrganizeResult {
            source: source.to_string(),
            planned_target: planned_target.to_string(),
            actual_target: planned_target.to_string(),
            success: true,
            error_message: String::new(),
        };
    }

    // 大小写不敏感文件系统上，仅大小写不同的两条路径折叠为同一冲突键
    let is_case_only_rename = claim_key(planned_target_path) == claim_key(source_path);

    // COPY 模式无法完成仅大小写重命名：源与目标是同一文件，
    // copy 会触发 SameFileError。返回明确的失败结果
    if is_case_only_rename && mode == OrganizeMode::Copy {
        return OrganizeResult {
            source: source.to_string(),
            planned_target: planned_target.to_string(),
            actual_target: planned_target.to_string(),
            success: false,
            error_message: "Case-only rename is not supported in copy mode: source and \
destination are the same file on this filesystem."
                .to_string(),
        };
    }

    // MOVE 模式的仅大小写重命名绕过 safe_target：目标"存在"正是因为
    // 它就是源文件本身，追加 _1 反而会阻碍更新磁盘上的显示名
    let final_target = if is_case_only_rename {
        planned_target.to_string()
    } else {
        // 仅对计划后出现的竞态冲突重新追加后缀
        safe_target(planned_target)
    };
    let target_path = Path::new(&final_target);

    match execute(source_path, target_path, mode) {
        Ok(()) => OrganizeResult {
            source: source.to_string(),
            planned_target: planned_target.to_string(),
            actual_target: final_target,
            success: true,
            error_message: String::new(),
        },
        Err(err) => {
            let error_message = if err.kind() == io::ErrorKind::PermissionDenied {
                format!("Permission denied: {err}")
            } else {
                err.to_string()
            };
            OrganizeResult {
                source: source.to_string(),
                planned_target: planned_target.to_string(),
                actual_target: final_target,
                success: false,
                error_message,
            }
        }
    }
}

/// 执行 mkdir(parents=True, exist_ok=True) + move/copy。
fn execute(source: &Path, target: &Path, mode: OrganizeMode) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        // 空 parent（裸相对文件名）对应 Python 的 Path('.')，无需创建
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    match mode {
        OrganizeMode::Move => move_file(source, target),
        OrganizeMode::Copy => copy2(source, target),
    }
}

/// `shutil.move` 语义：先 rename（同卷零拷贝），失败（如跨卷）
/// 回退为 copy2 + remove。
fn move_file(source: &Path, target: &Path) -> io::Result<()> {
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy2(source, target)?;
            fs::remove_file(source)
        }
    }
}

/// `shutil.copy2` 语义：fs::copy 复制内容与权限位，
/// 再尽力复制 mtime/atime（filetime，失败忽略）。
fn copy2(source: &Path, target: &Path) -> io::Result<()> {
    let meta = fs::metadata(source)?;
    fs::copy(source, target)?;
    let atime = filetime::FileTime::from_last_access_time(&meta);
    let mtime = filetime::FileTime::from_last_modification_time(&meta);
    let _ = filetime::set_file_times(target, atime, mtime);
    Ok(())
}

/// 竞态兜底：目标被占时追加 `_1, _2, ...` 后缀。
fn safe_target(target: &str) -> String {
    let p = Path::new(target);
    // 真实文件与悬空符号链接都视为已占用
    if !p.exists() && !p.is_symlink() {
        return target.to_string();
    }
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, suffix) = python_stem_suffix(&name);
    let parent = p.parent().unwrap_or_else(|| Path::new("")).to_path_buf();
    let mut counter = 1usize;
    loop {
        let candidate = parent.join(format!("{stem}_{counter}{suffix}"));
        if !candidate.exists() && !candidate.is_symlink() {
            return candidate.to_string_lossy().into_owned();
        }
        counter += 1;
    }
}

/// Python `str(Path)` 风格显示：空路径显示为 "."（与 pathlib 一致）。
fn py_display(p: &Path) -> String {
    let s = p.to_string_lossy().into_owned();
    if s.is_empty() {
        ".".to_string()
    } else {
        s
    }
}

/// 等价 Python `os.access(p, os.R_OK)`。
fn is_readable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        match CString::new(p.as_os_str().to_string_lossy().as_bytes()) {
            Ok(c) => unsafe { libc::access(c.as_ptr(), libc::R_OK) == 0 },
            Err(_) => false,
        }
    }
    #[cfg(windows)]
    {
        // Python os.access(R_OK) 对常规文件近似为存在性 + 非拒绝访问；
        // Windows 无 POSIX access，用打开探测：能打开即认为可读。
        std::fs::File::open(p).is_ok()
    }
}

/// 整理执行前的只读预检。
/// 返回错误列表（空 = 全部通过）。检查项：
/// 源存在且为文件、移动模式源父目录可写、移动模式源不重复、
/// 复制模式大小写重命名拒绝、目标不越界且不等于目标根、
/// 批内最终目标不重复及文件-目录祖先冲突、目标可写。
/// 错误文案由测试锁定，改动需同步用例。
pub fn preflight_check(
    mappings: &[FileMappingItem],
    mode: OrganizeMode,
    target_root: &Path,
) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();
    // claim_key(tgt)：大小写不敏感文件系统上已折叠
    let mut seen_final_keys: HashSet<String> = HashSet::new();
    // 规范化路径，用于文件-目录碰撞检测
    let mut seen_final_paths: Vec<PathBuf> = Vec::new();
    // 规范化源路径，用于 MOVE 模式重复源检测
    let mut seen_sources: HashSet<String> = HashSet::new();

    for mapping in mappings {
        // 1. 源必须存在、是普通文件、且当前进程可读。
        //    三项都在任务创建前检查，任何一个不可读的源都会让整批被拒绝，
        //    避免部分文件系统变更
        let src = Path::new(&mapping.source);
        if !src.exists() {
            errors.push(format!("Source not found: {}", mapping.source));
            continue;
        }
        if !src.is_file() {
            errors.push(format!("Source is not a file: {}", mapping.source));
            continue;
        }
        if !is_readable(src) {
            errors.push(format!("Source is not readable: {}", mapping.source));
            continue;
        }

        // 1b. MOVE 模式下重命名/删除源需要其父目录的写+执行权限，
        //     否则任务中途抛 PermissionError，破坏预检的全有或全无保证。
        //     例外：final_target == source 是精确原地空操作，
        //     organize_file 直接返回成功、不碰文件系统，父目录无需可写；
        //     否则会错误拒绝在只读树上重跑整理。
        //     用 normpath 字符串比较而非 Path 相等：Windows 上 Path 相等
        //     大小写不敏感，会把仅大小写重命名误判为原地空操作
        let src_str = src.to_string_lossy().into_owned();
        let is_inplace = path_util::normpath_str(&src_str)
            == path_util::normpath_str(&mapping.final_target);
        if mode == OrganizeMode::Move && !is_inplace {
            // Python Path("song.mp3").parent == Path(".")；Rust 空父路径映射到 "."
            let parent = src
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            if !path_util::dir_writable_executable(parent) {
                errors.push(format!(
                    "Source parent directory is not writable (move requires write+execute on parent): {}",
                    py_display(parent)
                ));
                continue;
            }
        }

        // 1c. MOVE 模式下同一源不能出现两次：第一条映射会把文件移走，
        //     第二条会因 ENOENT 失败，造成预检本应阻止的部分执行
        let src_key = path_util::resolve_lenient(src).to_string_lossy().into_owned();
        if mode == OrganizeMode::Move && !seen_sources.insert(src_key) {
            errors.push(format!(
                "Duplicate source in move batch (file can only be moved once): {}",
                mapping.source
            ));
            continue;
        }

        // 2d. COPY 模式下的仅大小写重命名意味着源与目标是同一文件
        //     （大小写不敏感文件系统）。organize_file 会返回失败，
        //     在预检阶段检出可返回 422 而不是带错任务。
        //     同样用 normpath 字符串比较，避免 Path 相等的大小写不敏感误判
        if mode == OrganizeMode::Copy
            && claim_key(Path::new(&mapping.final_target)) == claim_key(src)
            && path_util::normpath_str(&mapping.final_target)
                != path_util::normpath_str(&src_str)
        {
            errors.push(format!(
                "Case-only copy is not supported: source and destination are the same file on this filesystem: {} -> {}",
                mapping.source, mapping.final_target
            ));
            continue;
        }

        // 3. final_target 必须在 target_root 之内（边界检查）且不能
        //     解析为 target_root 自身：organize_file 始终把 final_target
        //     当文件路径用，直接写目标根会破坏目标目录
        let tgt = path_util::resolve_lenient(Path::new(&mapping.final_target));
        if !path_util::is_within(&tgt, target_root) {
            errors.push(format!(
                "Target escapes the target directory: {}",
                mapping.final_target
            ));
            continue;
        }
        if path_util::paths_equal_ci(&tgt, target_root) {
            errors.push(format!(
                "Target resolves to the target directory itself (not a valid file path): {}",
                mapping.final_target
            ));
            continue;
        }

        // 3. 批内最终目标不得重复，且不得出现文件-目录路径碰撞：
        //    若一个计划目标是另一个的祖先（如 foo.mp3 与 foo.mp3/bar.mp3），
        //    先创建的文件会挡住后者的 mkdir。
        //    注意：本分支不 continue，仍会落到下面的写权限检查
        let tgt_key = claim_key(&tgt);
        if seen_final_keys.contains(&tgt_key) {
            errors.push(format!(
                "Duplicate final target in batch: {}",
                mapping.final_target
            ));
        } else {
            let mut conflict_found = false;
            // 用 claim_key 后的路径做祖先检查：大小写不敏感文件系统上
            // 'Foo.mp3' 与 'FOO.MP3/bar.mp3' 才能被正确检出为碰撞。
            // is_relative_to 对应组件级 starts_with
            let tgt_norm = PathBuf::from(&tgt_key);
            for ep in &seen_final_paths {
                let ep_norm = PathBuf::from(claim_key(ep));
                if tgt_norm.starts_with(&ep_norm) || ep_norm.starts_with(&tgt_norm) {
                    errors.push(format!(
                        "Target path conflicts with another target in batch (file-vs-directory collision): {}",
                        mapping.final_target
                    ));
                    conflict_found = true;
                    break;
                }
            }
            if !conflict_found {
                seen_final_keys.insert(tgt_key);
                seen_final_paths.push(tgt.clone());
            }
        }

        // 4. 写权限检查 —— 不创建目录（只读探测）。
        //    精确原地空操作跳过：organize_file 直接返回成功，
        //    不需要目标祖先可写（文件已在原地）
        if !is_inplace {
            if let Some(write_error) = check_write_access(&tgt) {
                errors.push(write_error);
            }
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用临时目录：创建唯一子目录，Drop 时清理。
    /// （对应 pytest 的 tmp_path fixture）
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let dir = std::env::temp_dir()
                .join(format!("t2f_org_{}", uuid::Uuid::new_v4().simple()));
            fs::create_dir_all(&dir).expect("创建临时目录失败");
            TempDir(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// 逐组件拼接路径并转为字符串（保证使用平台原生分隔符）。
    fn path_of(root: &Path, parts: &[&str]) -> String {
        let mut p = root.to_path_buf();
        for part in parts {
            p.push(part);
        }
        p.to_string_lossy().into_owned()
    }

    fn write_bytes(path: &Path, data: &[u8]) {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).unwrap();
            }
        }
        fs::write(path, data).unwrap();
    }

    /// 递归收集 root 下所有路径（对应 pytest tmp_path.rglob("*")）。
    fn collect_tree(root: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(rd) = fs::read_dir(root) {
            for entry in rd.flatten() {
                let p = entry.path();
                p.is_dir().then(|| collect_tree(&p, out));
                out.push(p);
            }
        }
        out.sort();
    }

    // ── plan_targets ────────────────────────────────────────────────────────

    #[test]
    fn plan_targets_no_conflict() {
        // 无冲突的文件保持原始目标路径不变
        let tmp = TempDir::new();
        let targets = vec![
            path_of(tmp.path(), &["a", "song1.mp3"]),
            path_of(tmp.path(), &["a", "song2.mp3"]),
        ];
        let result = plan_targets(&targets, None);
        assert_eq!(result, targets);
    }

    #[test]
    fn plan_targets_on_disk_conflict() {
        // 磁盘冲突触发 _1 后缀策略
        let tmp = TempDir::new();
        write_bytes(&tmp.path().join("song.mp3"), b"data");

        let result = plan_targets(&[path_of(tmp.path(), &["song.mp3"])], None);
        assert_eq!(result, vec![path_of(tmp.path(), &["song_1.mp3"])]);
    }

    #[test]
    fn plan_targets_on_disk_conflict_increments() {
        // 后缀计数器递增直到找到空位
        let tmp = TempDir::new();
        write_bytes(&tmp.path().join("song.mp3"), b"1");
        write_bytes(&tmp.path().join("song_1.mp3"), b"2");

        let result = plan_targets(&[path_of(tmp.path(), &["song.mp3"])], None);
        assert_eq!(result, vec![path_of(tmp.path(), &["song_2.mp3"])]);
    }

    #[test]
    fn plan_targets_intra_batch_collision() {
        // 两个源映射到同一目标时得到不同的计划路径
        let tmp = TempDir::new();
        let same = path_of(tmp.path(), &["out", "song.mp3"]);
        let result = plan_targets(&[same.clone(), same.clone()], None);
        assert_eq!(result[0], same);
        assert_eq!(result[1], path_of(tmp.path(), &["out", "song_1.mp3"]));
    }

    #[test]
    fn plan_targets_combined_on_disk_and_batch() {
        // 磁盘冲突 + 批内碰撞同时确定性地消解
        let tmp = TempDir::new();
        fs::create_dir(tmp.path().join("out")).unwrap();
        write_bytes(&tmp.path().join("out").join("song.mp3"), b"existing");

        let raw = path_of(tmp.path(), &["out", "song.mp3"]);
        let result = plan_targets(&[raw.clone(), raw], None);
        // 第一条：_1（raw 已在磁盘上）；第二条：_2（_1 已被第一条占用）
        assert_eq!(result[0], path_of(tmp.path(), &["out", "song_1.mp3"]));
        assert_eq!(result[1], path_of(tmp.path(), &["out", "song_2.mp3"]));
    }

    #[test]
    fn plan_targets_source_equals_target_no_rename() {
        // 渲染出的目标与源是同一文件（原地整理）时不加 _1 后缀：
        // 文件已在正确位置
        let tmp = TempDir::new();
        let src = tmp.path().join("song.mp3");
        write_bytes(&src, b"audio");

        let src_str = src.to_string_lossy().into_owned();
        let result = plan_targets(std::slice::from_ref(&src_str), Some(std::slice::from_ref(&src_str)));
        assert_eq!(
            result[0], src_str,
            "原地整理目标必须保持路径不变，得到 {result:?}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn plan_targets_case_insensitive_intra_batch_collision() {
        // 大小写不敏感文件系统上 'Song.mp3' 与 'song.mp3' 是同一文件，
        // 批内必须视为碰撞，保证 preview == organize
        let tmp = TempDir::new();
        let raw1 = path_of(tmp.path(), &["out", "Song.mp3"]);
        let raw2 = path_of(tmp.path(), &["out", "song.mp3"]);

        let result = plan_targets(&[raw1, raw2], None);
        assert_ne!(
            result[0], result[1],
            "大小写变体目标在大小写不敏感文件系统上必须得到不同路径"
        );
    }

    // ── check_write_access ──────────────────────────────────────────────────

    #[test]
    fn check_write_access_no_side_effects() {
        // check_write_access 不创建任何目录
        let tmp = TempDir::new();
        let deep = tmp.path().join("a").join("b").join("c").join("file.mp3");
        let mut before = Vec::new();
        collect_tree(tmp.path(), &mut before);

        let result = check_write_access(&deep);

        let mut after = Vec::new();
        collect_tree(tmp.path(), &mut after);
        assert_eq!(before, after, "check_write_access 不得创建目录");
        assert_eq!(result, None); // tmp 可写
    }

    #[test]
    fn check_write_access_preflight_failure_leaves_no_dirs() {
        // 预检失败后，有效（可写）路径的目标目录也不应被创建（AC-5 负向要求）
        let tmp = TempDir::new();
        let valid_target = tmp.path().join("new_dir").join("song.mp3");
        let nonexistent_source = "/tmp/does_not_exist_xyz_abc.mp3";

        // 1. 对 valid_target 的写权限探测
        let write_err = check_write_access(&valid_target);
        assert_eq!(write_err, None);

        // 2. 源检查失败（模拟预检拒绝）
        assert!(!Path::new(nonexistent_source).exists());

        // 3. 目标目录必须未被创建
        assert!(
            !valid_target.parent().unwrap().exists(),
            "失败的预检不得创建目标目录"
        );
    }

    #[test]
    fn check_write_access_ancestor_is_file_rejected() {
        // 第一个存在的祖先若是普通文件则返回错误
        let tmp = TempDir::new();
        let blocker = tmp.path().join("out");
        write_bytes(&blocker, b"i am a file, not a dir");

        let target = tmp.path().join("out").join("subdir").join("song.mp3");
        let error = check_write_access(&target);

        let error = error.expect("祖先为普通文件时应拒绝");
        let lower = error.to_lowercase();
        assert!(
            lower.contains("not a directory") || lower.contains("file"),
            "错误文案应说明祖先不是目录：{error}"
        );
    }

    // ── organize_file ───────────────────────────────────────────────────────

    #[test]
    fn organize_file_copy() {
        // 复制保留源文件并创建目标
        let tmp = TempDir::new();
        let src = tmp.path().join("src.mp3");
        write_bytes(&src, b"audio");
        let dst = path_of(tmp.path(), &["out", "dst.mp3"]);

        let result = organize_file(
            &src.to_string_lossy(),
            &dst,
            OrganizeMode::Copy,
        );

        assert!(result.success, "错误：{}", result.error_message);
        assert!(src.exists());
        assert!(Path::new(&result.actual_target).exists());
    }

    #[test]
    fn organize_file_move() {
        // 移动删除源文件并创建目标
        let tmp = TempDir::new();
        let src = tmp.path().join("src.mp3");
        write_bytes(&src, b"audio");
        let dst = path_of(tmp.path(), &["out", "dst.mp3"]);

        let result = organize_file(
            &src.to_string_lossy(),
            &dst,
            OrganizeMode::Move,
        );

        assert!(result.success, "错误：{}", result.error_message);
        assert!(!src.exists());
        assert!(Path::new(&result.actual_target).exists());
    }

    #[test]
    fn organize_file_in_place_skip_safe_target() {
        // 源 == 计划目标（原地整理）时不得重命名为 _1 后缀路径，
        // safe_target 必须被完全跳过
        let tmp = TempDir::new();
        let src = tmp.path().join("song.mp3");
        write_bytes(&src, b"audio");

        let src_str = src.to_string_lossy().into_owned();
        let result = organize_file(&src_str, &src_str, OrganizeMode::Move);

        assert!(result.success, "原地整理必须成功；错误：{}", result.error_message);
        assert_eq!(
            result.actual_target, src_str,
            "原地整理必须保持原路径，得到 {result:?}"
        );
        assert!(src.exists(), "原地 move 后源文件必须仍然存在");
    }

    #[test]
    fn organize_file_in_place_copy_is_noop() {
        // COPY 模式下源 == 计划目标应视为成功的空操作，
        // 而不是对 copy(source, source) 抛 SameFileError
        let tmp = TempDir::new();
        let src = tmp.path().join("song.mp3");
        write_bytes(&src, b"audio");

        let src_str = src.to_string_lossy().into_owned();
        let result = organize_file(&src_str, &src_str, OrganizeMode::Copy);

        assert!(result.success, "原地 copy 必须成功；错误：{}", result.error_message);
        assert_eq!(
            result.actual_target, src_str,
            "原地 copy 必须保持原路径，得到 {result:?}"
        );
        assert!(src.exists(), "原地 copy 空操作后文件必须仍然存在");
    }

    #[test]
    fn organize_file_race_condition_fallback() {
        // 计划目标在执行时被占用，safe_target 追加后缀
        let tmp = TempDir::new();
        let src = tmp.path().join("src.mp3");
        write_bytes(&src, b"audio");
        let planned = tmp.path().join("out").join("song.mp3");
        write_bytes(&planned, b"already here"); // 模拟竞态冲突

        let result = organize_file(
            &src.to_string_lossy(),
            &planned.to_string_lossy(),
            OrganizeMode::Copy,
        );

        assert!(result.success, "错误：{}", result.error_message);
        assert_eq!(result.actual_target, path_of(tmp.path(), &["out", "song_1.mp3"]));
    }

    #[cfg(windows)]
    #[test]
    fn organize_file_case_only_rename_move_executes() {
        // 大小写不敏感文件系统上 song.mp3 -> Song.mp3 是合法的仅大小写重命名，
        // 必须真正执行并更新磁盘显示名，而不是当作空操作
        let tmp = TempDir::new();
        let src = tmp.path().join("song.mp3");
        write_bytes(&src, b"audio");

        let target = tmp.path().join("Song.mp3");
        let result = organize_file(
            &src.to_string_lossy(),
            &target.to_string_lossy(),
            OrganizeMode::Move,
        );

        assert!(result.success, "仅大小写重命名必须成功；错误：{}", result.error_message);
        // 磁盘显示名必须是 Song.mp3（大写 S）
        let names: Vec<String> = fs::read_dir(tmp.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.iter().any(|n| n == "Song.mp3"),
            "大小写重命名后显示名必须是 'Song.mp3'，实际：{names:?}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn organize_file_case_only_copy_returns_failure() {
        // 大小写不敏感文件系统上 song.mp3 -> Song.mp3 的复制是逻辑不可能：
        // 源与目标共享同一 inode，必须返回 success=False 并附错误说明
        let tmp = TempDir::new();
        let src = tmp.path().join("song.mp3");
        write_bytes(&src, b"audio");

        let target = tmp.path().join("Song.mp3");
        let result = organize_file(
            &src.to_string_lossy(),
            &target.to_string_lossy(),
            OrganizeMode::Copy,
        );

        assert!(
            !result.success,
            "大小写不敏感文件系统上的仅大小写 copy 必须失败：源与目标共享同一 inode"
        );
        assert!(
            !result.error_message.is_empty(),
            "必须附带解释 copy 失败原因的错误信息"
        );
    }

    // ── probe_case_insensitive ──────────────────────────────────────────────

    #[test]
    fn probe_case_insensitive_agrees_with_os_default() {
        // probe 结果必须与 OS 级默认一致：macOS/Windows 返回 true，
        // Linux 返回 false；same_file 判定避免兄弟目录假阳性
        let tmp = TempDir::new();
        let result = probe_case_insensitive(tmp.path());
        assert_eq!(
            result, FOLD_PATHS,
            "probe_case_insensitive({:?}) 返回 {result}，期望 {FOLD_PATHS}（OS 文件系统默认）",
            tmp.path()
        );
    }
}

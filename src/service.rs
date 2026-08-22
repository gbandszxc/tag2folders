//! 服务层：对应源项目 src-tauri/src/commands.rs（原 FastAPI 的 api/routes）。
//!
//! 与源的差异（去 Tauri 化）：
//! - 去掉 `#[tauri::command]` / `tauri::AppHandle` / `tauri::Emitter` 包装，
//!   改为普通函数，由 GPUI UI 层直接调用；
//! - 错误类型从 `serde_json::Value`（字符串或对象）改为 `ServiceError` 枚举，
//!   与源端 JSON 形状一一对应（`json!("...")` → `Message`、
//!   `{"template_errors": [...]}` → `TemplateErrors`、
//!   `{"preflight_errors": [...]}` → `PreflightErrors`），
//!   错误文案逐字保留；
//! - `exit_app` 命令不移植：GPUI 下由 UI 层 `cx.quit()` 承担。
//!
//! 函数签名/入参/返回结构遵循 docs/SOURCE_SPEC.md 第 5 章的类型契约。

use serde::Serialize;

use crate::core::{self, metadata, organizer, path_security, preview, scanner};
use crate::task;

// ── 错误类型 ─────────────────────────────────────────────────────────────────

/// 服务层错误。变体与源项目 `serde_json::Value` 错误形状一一对应，
/// 前端可见的错误文案（`Display`）与源项目完全一致：
/// 数组类错误按前端 toError 规则以 `\n` 连接（见 SOURCE_SPEC 5.2）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceError {
    /// 源端 `Err(json!(String))`：纯字符串错误
    Message(String),
    /// 源端 `Err(json!({ "template_errors": [...] }))`：模板校验错误
    TemplateErrors(Vec<String>),
    /// 源端 `Err(json!({ "preflight_errors": [...] }))`：整理预检错误
    PreflightErrors(Vec<String>),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceError::Message(m) => write!(f, "{m}"),
            // 前端 toError：arr.join('\n')
            ServiceError::TemplateErrors(errs) => write!(f, "{}", errs.join("\n")),
            ServiceError::PreflightErrors(errs) => write!(f, "{}", errs.join("\n")),
        }
    }
}

impl std::error::Error for ServiceError {}

impl From<String> for ServiceError {
    fn from(msg: String) -> Self {
        ServiceError::Message(msg)
    }
}

// ── 扫描（原 POST /api/scan）────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ScanResponse {
    pub source_dir: String,
    pub total: usize,
    pub files: Vec<core::AudioMetadata>,
}

pub fn scan_directory(
    source_dir: String,
    recursive: Option<bool>,
) -> Result<ScanResponse, ServiceError> {
    let recursive = recursive.unwrap_or(true);
    let resolved = path_security::validate_source_dir(&source_dir).map_err(ServiceError::Message)?;
    let resolved_str = resolved.to_string_lossy().into_owned();

    let file_paths =
        scanner::scan_directory(&resolved_str, recursive).map_err(|e| ServiceError::Message(e.to_string()))?;

    let files = metadata::extract_metadata_batch(&file_paths);
    Ok(ScanResponse {
        source_dir: resolved_str,
        total: files.len(),
        files,
    })
}

// ── 预览（原 POST /api/preview）──────────────────────────────────────────────

pub fn generate_preview(
    req: preview::PreviewRequest,
) -> Result<preview::PreviewResponse, ServiceError> {
    preview::generate_preview(&req).map_err(|e| match e {
        preview::PreviewError::Template(errs) => ServiceError::TemplateErrors(errs),
        preview::PreviewError::Validation(m) => ServiceError::Message(m),
    })
}

// ── 整理（原 POST /api/organize + SSE）──────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct OrganizeStartResponse {
    pub task_id: String,
    pub total: usize,
}

pub fn start_organize(
    mappings: Vec<core::FileMappingItem>,
    mode: core::OrganizeMode,
    target_dir: String,
) -> Result<OrganizeStartResponse, ServiceError> {
    if mappings.is_empty() {
        return Err(ServiceError::Message("No file mappings provided.".to_string()));
    }
    let target_root =
        path_security::validate_target_dir(&target_dir).map_err(ServiceError::Message)?;

    // 只读预检：任一失败则整批拒绝，不产生任何文件系统变更
    let preflight_errors = organizer::preflight_check(&mappings, mode, &target_root);
    if !preflight_errors.is_empty() {
        return Err(ServiceError::PreflightErrors(preflight_errors));
    }

    let total = mappings.len();
    let task_id = task::create_task(total);
    let tid = task_id.clone();
    // 源项目即用 std::thread::spawn（非 tauri::async_runtime），GPUI 版沿用
    std::thread::spawn(move || {
        task::run_organize(tid, mappings, mode);
    });

    Ok(OrganizeStartResponse { task_id, total })
}

// ── 任务状态（原 GET /api/tasks/{id}/status）────────────────────────────────

pub fn get_task_status(task_id: String) -> Result<task::ProgressEvent, ServiceError> {
    task::get_snapshot(&task_id).ok_or_else(|| {
        ServiceError::Message(format!("Task not found: {task_id}"))
    })
}

// ── 目录浏览（原 GET /api/browse，DirPicker 使用）───────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BrowseResponse {
    pub base_dir: String,
    pub entries: Vec<DirEntry>,
}

pub fn browse_dirs(path: String) -> Result<BrowseResponse, ServiceError> {
    let base = if path.is_empty() {
        if cfg!(windows) {
            // Windows：返回可用盘符
            let mut drives = Vec::new();
            for letter in b'A'..=b'Z' {
                let drive = format!("{}:\\", letter as char);
                if std::path::Path::new(&drive).exists() {
                    drives.push(DirEntry {
                        name: drive.clone(),
                        path: drive,
                    });
                }
            }
            return Ok(BrowseResponse {
                base_dir: String::new(),
                entries: drives,
            });
        } else {
            std::env::var("HOME").map(std::path::PathBuf::from).unwrap_or_default()
        }
    } else {
        std::path::PathBuf::from(&path)
    };

    let resolved = core::path_util::resolve_lenient(&base);
    if !resolved.is_dir() {
        return Err(ServiceError::Message(format!("路径不存在：{path}")));
    }

    let mut entries = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&resolved) {
        let mut items: Vec<_> = rd.filter_map(|e| e.ok()).collect();
        items.sort_by_key(|e| e.file_name());
        for item in items {
            // 无权限项静默跳过（对齐 Python 端 PermissionError → pass）
            if let Ok(ft) = item.file_type() {
                if ft.is_dir() {
                    entries.push(DirEntry {
                        name: item.file_name().to_string_lossy().into_owned(),
                        path: item.path().to_string_lossy().into_owned(),
                    });
                }
            }
        }
    }

    Ok(BrowseResponse {
        base_dir: resolved.to_string_lossy().into_owned(),
        entries,
    })
}

//! 路径安全校验：规范化并验证路径，防目录穿越。
//! 错误为 `Err(String)`，格式 `"{context} ..."`。

use std::path::{Path, PathBuf};

use crate::core::path_util;

/// 等价 Python `safe_resolve`：strip → 拒绝 `..` 段 → `resolve_lenient`
/// 并要求结果为绝对路径。
pub fn safe_resolve(raw_path: &str, context: &str) -> Result<PathBuf, String> {
    let raw_path = raw_path.trim();
    if raw_path.is_empty() {
        return Err(format!("{context} must not be empty."));
    }

    // 在解析前拒绝显式的穿越段
    let normalized = raw_path.replace('\\', "/");
    if normalized.split('/').any(|p| p == "..") {
        return Err(format!(
            "{context} contains disallowed path traversal components."
        ));
    }

    let resolved = path_util::resolve_lenient(Path::new(raw_path));

    // 解析后确认路径未逃逸出文件系统根（如符号链接戏法）：
    // 解析结果必须是绝对路径
    if !resolved.is_absolute() {
        return Err(format!(
            "{context} could not be resolved to an absolute path."
        ));
    }

    Ok(resolved)
}

/// 校验并解析扫描源目录。
pub fn validate_source_dir(raw_path: &str) -> Result<PathBuf, String> {
    safe_resolve(raw_path, "Source directory")
}

/// 校验并解析整理目标目录（存在但非目录 → Err，
/// 文案 "Target directory path exists but is not a directory."）。
pub fn validate_target_dir(raw_path: &str) -> Result<PathBuf, String> {
    let resolved = safe_resolve(raw_path, "Target directory")?;
    if resolved.exists() && !resolved.is_dir() {
        return Err("Target directory path exists but is not a directory.".to_string());
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_resolve_empty() {
        let err = safe_resolve("   ", "Source directory").unwrap_err();
        assert_eq!(err, "Source directory must not be empty.");
    }

    #[test]
    fn test_safe_resolve_rejects_traversal() {
        // 正斜杠与反斜杠形式的 `..` 段都要拒绝
        let err = safe_resolve("C:/data/../secret", "Path").unwrap_err();
        assert_eq!(err, "Path contains disallowed path traversal components.");
        let err = safe_resolve("..\\..\\etc", "Path").unwrap_err();
        assert_eq!(err, "Path contains disallowed path traversal components.");
    }

    #[test]
    fn test_safe_resolve_normal_absolute_path() {
        let dir = std::env::temp_dir().join("t2f_safe_resolve_ok");
        std::fs::create_dir_all(&dir).expect("创建临时目录失败");
        let resolved =
            safe_resolve(dir.to_string_lossy().as_ref(), "Path").expect("合法路径应通过");
        assert!(resolved.is_absolute());
    }

    #[test]
    fn test_validate_target_dir_rejects_existing_file() {
        let file = std::env::temp_dir().join("t2f_target_not_dir.txt");
        std::fs::write(&file, b"x").expect("创建临时文件失败");
        let err = validate_target_dir(file.to_string_lossy().as_ref()).unwrap_err();
        assert_eq!(err, "Target directory path exists but is not a directory.");
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn test_validate_source_dir_empty_message() {
        // Python 端 context 固定为 "Source directory"
        let err = validate_source_dir("").unwrap_err();
        assert_eq!(err, "Source directory must not be empty.");
    }
}

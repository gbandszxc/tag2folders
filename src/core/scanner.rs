//! 音频文件扫描：发现目录下的受支持音频文件。
//! 移植自 backend/core/scanner.py。

use std::fs;
use std::path::Path;

use crate::core::path_util;

/// 受支持的音频扩展名（小写、含点），与 Python 端 SUPPORTED_EXTENSIONS 一致。
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    ".mp3", ".flac", ".ogg", ".m4a", ".wav", ".aac", ".wma", ".ape", ".opus",
];

/// 扫描错误，对应 Python 端抛出的异常（FastAPI 层转换为 404/400/403）。
#[derive(Debug)]
pub enum ScanError {
    NotFound(String),
    NotADirectory(String),
    PermissionDenied(String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::NotFound(m) => write!(f, "Directory not found: {m}"),
            ScanError::NotADirectory(m) => write!(f, "Not a directory: {m}"),
            ScanError::PermissionDenied(m) => write!(f, "Permission denied: {m}"),
        }
    }
}

/// 递归扫描 *path* 下的音频文件，返回排序后的绝对路径列表。
///
/// 行为对齐 Python `scan_directory`：
/// - 先解析为绝对路径（`resolve_lenient`，容忍不存在路径的词法解析）
/// - 不存在 → NotFound；非目录 → NotADirectory；不可读 → PermissionDenied
/// - 无权限的子目录静默跳过；结果排序返回
/// - 不跟随符号链接（is_file(follow_symlinks=False) 语义）
pub fn scan_directory(path: &str, recursive: bool) -> Result<Vec<String>, ScanError> {
    // Python: root = Path(path).resolve()
    let root = path_util::resolve_lenient(Path::new(path));

    // Python: root.exists() / root.is_dir()（均跟随符号链接）
    if !root.exists() {
        return Err(ScanError::NotFound(path.to_string()));
    }
    if !root.is_dir() {
        return Err(ScanError::NotADirectory(path.to_string()));
    }

    // Python 端用 os.scandir(root) 提前探测读权限。目录已确认存在，
    // read_dir 失败即视为访问被拒（含 Windows 共享冲突等访问性问题）。
    if fs::read_dir(&root).is_err() {
        return Err(ScanError::PermissionDenied(path.to_string()));
    }

    let mut results: Vec<String> = Vec::new();
    collect_audio_files(&root, recursive, &mut results);
    // Python: sorted(results) —— 按字符串逐字符排序（UTF-8 字节序与码点序一致）
    results.sort();
    Ok(results)
}

/// 对应 Python `_iter_audio_files`：遍历目录收集受支持的音频文件路径。
/// 无权限的子目录静默跳过。
fn collect_audio_files(root: &Path, recursive: bool, results: &mut Vec<String>) {
    // Python: except PermissionError: return
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return, // 无权限子目录静默跳过
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();
        // follow_symlinks=False：DirEntry::file_type 基于目录项自身的元数据，
        // 不跟随符号链接（符号链接本身既不算文件也不算目录）。
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() {
            if is_supported_ext(&entry_path) {
                // Python: str(Path(entry.path)) —— 目录项绝对路径的字符串形式
                results.push(entry_path.to_string_lossy().into_owned());
            }
        } else if file_type.is_dir() && recursive {
            collect_audio_files(&entry_path, recursive, results);
        }
    }
}

/// 对应 Python `entry_path.suffix.lower() in SUPPORTED_EXTENSIONS`。
fn is_supported_ext(path: &Path) -> bool {
    let Some(ext) = path.extension() else {
        return false;
    };
    let ext = format!(".{}", ext.to_string_lossy().to_lowercase());
    SUPPORTED_EXTENSIONS.contains(&ext.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// 创建唯一临时目录，返回其路径（调用方负责清理）。
    fn make_temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("t2f_scanner_{tag}_{}_{}", std::process::id(), uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path, name: &str) {
        fs::write(path.join(name), b"").unwrap();
    }

    /// 与 scan_directory 内部一致：把根目录解析为绝对路径后再拼接子路径。
    fn expected(root: &Path, rel: impl AsRef<Path>) -> String {
        path_util::resolve_lenient(root)
            .join(rel)
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn scan_recursive_finds_nested_audio_files() {
        let dir = make_temp_dir("recursive");
        touch(&dir, "a.mp3");
        touch(&dir, "b.txt");
        fs::create_dir_all(dir.join("sub")).unwrap();
        touch(&dir.join("sub"), "c.flac");
        touch(&dir.join("sub"), "d.MP3"); // 大写扩展名也应命中

        let result = scan_directory(dir.to_str().unwrap(), true).unwrap();
        let expected = vec![
            expected(&dir, "a.mp3"),
            expected(&dir, Path::new("sub").join("c.flac")),
            expected(&dir, Path::new("sub").join("d.MP3")),
        ];
        assert_eq!(result, expected);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn scan_non_recursive_only_includes_top_level() {
        let dir = make_temp_dir("non_recursive");
        touch(&dir, "a.mp3");
        fs::create_dir_all(dir.join("sub")).unwrap();
        touch(&dir.join("sub"), "c.flac");

        let result = scan_directory(dir.to_str().unwrap(), false).unwrap();
        assert_eq!(result, vec![expected(&dir, "a.mp3")]);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn scan_nonexistent_directory_returns_not_found() {
        let missing = std::env::temp_dir().join("t2f_scanner_does_not_exist_xyzabc");
        let err = scan_directory(missing.to_str().unwrap(), true).unwrap_err();
        assert_eq!(err.to_string(), format!("Directory not found: {}", missing.display()));
    }

    #[test]
    fn scan_file_path_returns_not_a_directory() {
        let dir = make_temp_dir("file_path");
        let file = dir.join("plain.mp3");
        fs::write(&file, b"").unwrap();

        let err = scan_directory(file.to_str().unwrap(), true).unwrap_err();
        assert_eq!(err.to_string(), format!("Not a directory: {}", file.display()));

        fs::remove_dir_all(&dir).unwrap();
    }
}

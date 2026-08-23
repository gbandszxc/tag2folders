//! 路径工具：规范化、父目录推导、边界检查与权限探测等跨模块共享的路径语义。
//!
//! 修改签名前必须核对 preview/organizer 的调用方。

use std::collections::VecDeque;
use std::path::{Component, Path, PathBuf};

/// 等价于 Python `os.path.normpath`：纯词法规范化。
/// 去除 `.` 段、消化 `..` 段（不访问文件系统），Windows 上保留盘符前缀。
pub fn normpath(p: &Path) -> PathBuf {
    let mut stack: Vec<Component> = Vec::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => match stack.last() {
                Some(Component::Normal(_)) => {
                    stack.pop();
                }
                // 根/盘符之上的 `..` 不再上溯（`/..` → `/`）
                Some(Component::Prefix(_)) | Some(Component::RootDir) => {}
                _ => stack.push(comp),
            },
            other => stack.push(other),
        }
    }
    let mut out = PathBuf::new();
    for c in stack {
        out.push(c.as_os_str());
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// `normpath` 的字符串便捷形式（比较语义与 Python `os.path.normpath` 一致：区分大小写）。
pub fn normpath_str(s: &str) -> String {
    normpath(Path::new(s)).to_string_lossy().into_owned()
}

/// 等价于 Python `Path.resolve(strict=False)`：
/// 1. 先词法规范化（消化不存在的 `..` 段）；
/// 2. 对存在的最长前缀做 `dunce::canonicalize`（解析符号链接，避免 `\\?\` 前缀）；
/// 3. 不存在的尾部原样接回（保留用户指定的大小写，用于大小写重命名场景）。
pub fn resolve_lenient(p: &Path) -> PathBuf {
    let norm = normpath(p);
    if let Ok(c) = dunce::canonicalize(&norm) {
        return c;
    }
    let mut tail: VecDeque<std::ffi::OsString> = VecDeque::new();
    let mut cur = norm.clone();
    loop {
        if let Ok(c) = dunce::canonicalize(&cur) {
            let mut out = c;
            for t in &tail {
                out.push(t);
            }
            return out;
        }
        match cur.file_name() {
            Some(n) => tail.push_front(n.to_os_string()),
            None => break,
        }
        match cur.parent() {
            Some(par) if par != cur => cur = par.to_path_buf(),
            _ => break,
        }
    }
    norm
}

/// 组件级"child 位于 root 之内"判断（等价 Python `relative_to` 成功条件）。
/// Windows 上按 Python PureWindowsPath 语义做大小写不敏感比较。
pub fn is_within(child: &Path, root: &Path) -> bool {
    if child.starts_with(root) {
        return true;
    }
    #[cfg(windows)]
    {
        return ci_starts_with(child, root);
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
fn ci_starts_with(child: &Path, root: &Path) -> bool {
    let cc: Vec<String> = child
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let rc: Vec<String> = root
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    rc.len() <= cc.len() && rc.iter().zip(cc.iter()).all(|(a, b)| a == b)
}

/// 路径相等判断：Windows 上大小写不敏感（对齐 Python `PureWindowsPath.__eq__`），
/// Unix 上精确比较。
pub fn paths_equal_ci(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        let sa = a.to_string_lossy().to_lowercase();
        let sb = b.to_string_lossy().to_lowercase();
        sa == sb
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

/// 等价 Python `os.path.samefile`：Unix 用 (dev, ino)；Windows 近似为
/// 大小写不敏感相等且双方均存在（Windows 卷默认大小写不敏感，足够精确）。
pub fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
            _ => false,
        }
    }
    #[cfg(windows)]
    {
        paths_equal_ci(a, b) && a.exists() && b.exists()
    }
}

/// 等价 Python `os.access(p, os.W_OK | os.X_OK)`（对目录探测可写+可进入）。
/// - Unix：libc::access(W_OK | X_OK)
/// - Windows：UCRT `_waccess` 语义（Python os.access 即调它）——
///   READONLY 属性仅对**文件**生效；目录上的 READONLY 是资源管理器
///   "自定义文件夹"标志（desktop.ini），不阻止在其中创建/删除条目。
///   实测：os.access 对带 ReadOnly 位的目录返回 True，实际写入也成功。
///   仅 INVALID_FILE_ATTRIBUTES（不存在/无法访问）视为不可写。
pub fn dir_writable_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        match CString::new(p.as_os_str().to_string_lossy().as_bytes()) {
            Ok(c) => unsafe { libc::access(c.as_ptr(), libc::W_OK | libc::X_OK) == 0 },
            Err(_) => false,
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileAttributesW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_READONLY,
            INVALID_FILE_ATTRIBUTES,
        };
        let wide: Vec<u16> = p
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
        if attrs == INVALID_FILE_ATTRIBUTES {
            return false;
        }
        (attrs & FILE_ATTRIBUTE_READONLY) == 0 || (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normpath_digests_dotdot() {
        assert_eq!(normpath_str("a/../b"), "b");
        assert_eq!(normpath_str("a/./b"), normpath_str("a/b"));
        assert_eq!(normpath_str(""), ".");
    }

    #[test]
    fn normpath_root_stays_root() {
        let p = normpath(Path::new("/.."));
        assert!(p.parent().is_none() || p == Path::new("/"));
    }

    #[cfg(windows)]
    #[test]
    fn normpath_keeps_drive() {
        assert_eq!(normpath_str(r"C:\a\..\b"), r"C:\b");
    }

    #[test]
    fn is_within_component_wise() {
        assert!(is_within(Path::new("/a/b/c"), Path::new("/a")));
        assert!(!is_within(Path::new("/ab/c"), Path::new("/a")));
        assert!(is_within(Path::new("/a"), Path::new("/a")));
    }

    #[test]
    fn resolve_lenient_nonexistent_keeps_tail() {
        let base = std::env::temp_dir();
        let p = base.join("t2f_no_such_dir_xyz/sub/file.mp3");
        let r = resolve_lenient(&p);
        assert!(r.ends_with("t2f_no_such_dir_xyz/sub/file.mp3"));
        assert!(is_within(&r, &dunce::canonicalize(&base).unwrap()));
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::dir_writable_executable;

    /// Windows 目录的 READONLY 位是 Explorer「自定义文件夹」标志，
    /// 不阻止写入（UCRT access 语义 / Python os.access 对其返回 True）。
    /// 回归：此前带 ReadOnly 位的目录被判不可写，导致预览全量 write_error。
    #[test]
    fn readonly_directory_still_writable() {
        use windows_sys::Win32::Storage::FileSystem::{
            SetFileAttributesW, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY,
        };
        let dir = std::env::temp_dir().join(format!("t2f_ro_dir_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let wide: Vec<u16> = dir
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe { SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_READONLY) };
        assert!(dir_writable_executable(&dir), "目录 READONLY 位不应判为不可写");

        // 只读**文件**仍应判不可写（UCRT 语义保留）
        let file = dir.join("ro.txt");
        std::fs::write(&file, b"x").unwrap();
        let wide: Vec<u16> = file
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe { SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_READONLY) };
        assert!(!dir_writable_executable(&file), "只读文件应判不可写");

        unsafe { SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_NORMAL) }; // 清除位
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! 模板渲染：`{album}/{track}. {title}.{ext}`（track 不足两位补零）。
//! 移植自 backend/core/template.py。

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

use crate::core::AudioMetadata;

/// 支持的占位符（小写，按字母排序，与 Python `sorted(SUPPORTED_PLACEHOLDERS)` 一致）。
pub const SUPPORTED_PLACEHOLDERS: &[&str] =
    &["album", "artist", "ext", "genre", "title", "track", "year"];

/// Windows 保留设备名：无论大小写、无论是否带扩展名都不能作为路径段
/// （CON、con、CON.mp3 在 Windows 上均为保留名）。
const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// 占位符正则 `\{(\w+)\}`（与 Python 端一致，`\w` 为 Unicode 字符类）。
fn placeholder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{(\w+)\}").expect("内置占位符正则必然合法"))
}

/// 非法字符正则 `[<>:"/\\|?*\x00-\x1f]`（多数 OS 下文件/目录名中的不安全字符）。
fn unsafe_chars_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"[<>:"/\\|?*\x00-\x1F]"#).expect("内置非法字符正则必然合法")
    })
}

/// 校验模板。返回错误信息列表（空 = 合法）。错误文案需与 Python 端一致：
/// `Unsupported placeholder(s): ['album', ...].`
pub fn validate_template(template: &str) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();

    if template.trim().is_empty() {
        errors.push("Template must not be empty.".to_string());
        return errors;
    }

    let supported: HashSet<&str> = SUPPORTED_PLACEHOLDERS.iter().copied().collect();
    let found: HashSet<String> = placeholder_re()
        .captures_iter(template)
        .map(|c| c[1].to_string())
        .collect();
    let mut unsupported: Vec<String> = found
        .into_iter()
        .filter(|k| !supported.contains(k.as_str()))
        .collect();
    if !unsupported.is_empty() {
        unsupported.sort();
        errors.push(format!(
            "Unsupported placeholder(s): {}. Supported: {}.",
            py_repr_list(unsupported.iter().map(|s| s.as_str())),
            py_repr_list(SUPPORTED_PLACEHOLDERS.iter().copied()),
        ));
    }
    errors
}

/// 渲染目标相对路径：正斜杠分隔、无前导斜杠；逐段清洗非法字符、
/// Windows 保留名、尾部点/空格（但保留 `.`/`..` 段供边界检测）。
pub fn render_path(template: &str, meta: &AudioMetadata) -> String {
    let padded_track = format_track(meta.track.as_str());
    let values: [(&str, &str); 7] = [
        ("artist", meta.artist.as_str()),
        ("album", meta.album.as_str()),
        ("title", meta.title.as_str()),
        ("track", padded_track.as_str()),
        ("year", meta.year.as_str()),
        ("genre", meta.genre.as_str()),
        ("ext", meta.ext.as_str()),
    ];

    // 未知占位符原样保留（校验阶段已报错）
    let rendered = placeholder_re()
        .replace_all(template, |caps: &regex::Captures| {
            let key = &caps[1];
            match values.iter().find(|(k, _)| *k == key) {
                Some((_, v)) => sanitize_path_component(v),
                None => caps[0].to_string(),
            }
        })
        .into_owned();

    // 归一为正斜杠并去前导斜杠
    let rendered = rendered.replace('\\', "/");
    let rendered = rendered.trim_start_matches('/');

    // 逐段清洗字面量模板文本中的非法字符与 Windows 保留名。
    // 与 sanitize_path_component 不同：不去除首尾点/空格以保留 `..` 段
    // 供预览阶段做边界检测（在触碰任何文件前拒绝穿越尝试）。
    rendered
        .split('/')
        .map(sanitize_literal_segment)
        .collect::<Vec<_>>()
        .join("/")
}
/// 替换字面量模板段中的非法字符并转义 Windows 保留名。
/// 除 `.` 与 `..` 外的所有段都去除尾部点/空格——Win32 会静默丢弃
/// 尾部点/空格（`Album.` 落盘变为 `Album`），保留它们会导致预览与
/// 整理阶段对实际磁盘路径产生分歧；`.`/`..` 保留供预览边界检测拒绝穿越。
fn sanitize_literal_segment(part: &str) -> String {
    let mut safe = unsafe_chars_re().replace_all(part, "_").into_owned();
    let stem = stem_upper(&safe);
    if !stem.is_empty() && is_windows_reserved(&stem) {
        safe.push('_');
    }
    // 保留 `.` / `..`（预览边界检测依赖）；其余段去除尾部点/空格
    if safe != "." && safe != ".." {
        let stripped = safe.trim_end_matches(['.', ' ']);
        safe = if stripped.is_empty() {
            "_".to_string()
        } else {
            stripped.to_string()
        };
    }
    safe
}

/// 替换文件/目录名中的不安全字符。
fn sanitize_path_component(value: &str) -> String {
    let safe = unsafe_chars_re().replace_all(value, "_");
    // 去首尾空格与点（Windows 兼容）
    let safe = safe.trim_matches(|c| c == '.' || c == ' ');
    if safe.is_empty() {
        return "_".to_string();
    }
    let mut safe = safe.to_string();
    // 拒绝 Windows 保留设备名：CON/PRN/AUX/NUL/COM1-9/LPT1-9 无论大小写、
    // 无论是否带扩展名都不能作为路径段，追加下划线使其成为合法文件名。
    let stem = stem_upper(&safe);
    if is_windows_reserved(&stem) {
        safe.push('_');
    }
    safe
}

/// 等价 Python `safe.upper().split(".", 1)[0]`：首个 `.` 之前的部分（大写）。
fn stem_upper(s: &str) -> String {
    s.to_uppercase()
        .split('.')
        .next()
        .unwrap_or_default()
        .to_string()
}

fn is_windows_reserved(stem: &str) -> bool {
    WINDOWS_RESERVED.contains(&stem)
}

/// 格式化音轨号：纯数字且不足两位时左补零（`1` → `01`，`0` → `00`）。
/// 非纯数字（如 `A`、`01`、`12`、`Unknown`）或已是两位以上保持原样。
fn format_track(track: &str) -> String {
    let t = track.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.chars().all(|c| c.is_ascii_digit()) && t.len() < 2 {
        format!("{t:0>2}")
    } else {
        t.to_string()
    }
}

/// 模拟 Python 列表 repr：`['a', 'b']`（占位符为 `\w+`，不会含引号）。
fn py_repr_list<'a>(items: impl IntoIterator<Item = &'a str>) -> String {
    let inner: Vec<String> = items.into_iter().map(|s| format!("'{s}'")).collect();
    format!("[{}]", inner.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 对应 Python 测试的 `_make_meta()` 默认值。
    fn make_meta() -> AudioMetadata {
        AudioMetadata {
            path: "/tmp/test.mp3".to_string(),
            ext: "mp3".to_string(),
            artist: "The Beatles".to_string(),
            album: "Abbey Road".to_string(),
            title: "Come Together".to_string(),
            track: "1".to_string(),
            year: "1969".to_string(),
            genre: "Rock".to_string(),
            readable: true,
            error: String::new(),
        }
    }

    // ── validate_template ───────────────────────────────────────────────────

    #[test]
    fn test_validate_template_ok() {
        assert!(validate_template("{artist}/{album}/{title}.{ext}").is_empty());
    }

    #[test]
    fn test_validate_template_unknown_placeholder() {
        let errs = validate_template("{composer}/{title}.{ext}");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("composer"));
    }

    #[test]
    fn test_validate_template_empty() {
        let errs = validate_template("");
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn test_validate_template_whitespace_only() {
        let errs = validate_template("   ");
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn test_validate_template_multiple_unknowns() {
        let errs = validate_template("{composer}/{conductor}.{ext}");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("composer") || errs[0].contains("conductor"));
    }

    #[test]
    fn test_validate_template_error_message_exact() {
        // 错误文案与 Python 端逐字一致（列表按字母排序）
        let errs = validate_template("{conductor}/{composer}.{ext}");
        assert_eq!(
            errs,
            vec![
                "Unsupported placeholder(s): ['composer', 'conductor']. Supported: \
                 ['album', 'artist', 'ext', 'genre', 'title', 'track', 'year']."
                    .to_string()
            ]
        );
    }

    // ── render_path ─────────────────────────────────────────────────────────

    #[test]
    fn test_render_path_basic() {
        let meta = make_meta();
        let result = render_path("{artist}/{album}/{title}.{ext}", &meta);
        assert_eq!(result, "The Beatles/Abbey Road/Come Together.mp3");
    }

    #[test]
    fn test_render_path_track_year() {
        let meta = make_meta();
        let result = render_path("{year}/{track} - {title}.{ext}", &meta);
        assert_eq!(result, "1969/01 - Come Together.mp3");
    }

    #[test]
    fn test_render_path_track_padding() {
        let mut meta = make_meta();
        // 单数字补零
        meta.track = "1".into();
        assert_eq!(render_path("{track}. {title}.{ext}", &meta), "01. Come Together.mp3");
        // 已两位不补
        meta.track = "12".into();
        assert_eq!(render_path("{track}. {title}.{ext}", &meta), "12. Come Together.mp3");
        // 三位保持
        meta.track = "123".into();
        assert_eq!(render_path("{track}. {title}.{ext}", &meta), "123. Come Together.mp3");
        // 兜底 0 补为 00
        meta.track = "0".into();
        assert_eq!(render_path("{track}. {title}.{ext}", &meta), "00. Come Together.mp3");
        // 已补零的 01 保持
        meta.track = "01".into();
        assert_eq!(render_path("{track}. {title}.{ext}", &meta), "01. Come Together.mp3");
    }

    #[test]
    fn test_render_path_sanitizes_slashes_in_values() {
        let mut meta = make_meta();
        meta.title = "AC/DC Song".to_string();
        let result = render_path("{title}.{ext}", &meta);
        assert!(!result.contains('/'));
        assert_eq!(result, "AC_DC Song.mp3");
    }

    #[test]
    fn test_render_path_sanitizes_colons() {
        let mut meta = make_meta();
        meta.title = "Track: One".to_string();
        let result = render_path("{title}.{ext}", &meta);
        assert!(!result.contains(':'));
    }

    #[test]
    fn test_render_path_sanitizes_literal_colon_in_template() {
        // 模板字面量（非占位符）部分的不安全字符也必须被清洗
        let mut meta = make_meta();
        meta.year = "1979".to_string();
        meta.album = "The Wall".to_string();
        let result = render_path("{year}: {album}/{title}.{ext}", &meta);
        assert!(
            !result.contains(':'),
            "Literal ':' in template must be sanitized; got {result:?}"
        );
    }

    #[test]
    fn test_render_path_sanitizes_literal_question_mark_in_template() {
        let meta = make_meta();
        let result = render_path("Disc? {track}/{title}.{ext}", &meta);
        assert!(
            !result.contains('?'),
            "Literal '?' in template must be sanitized; got {result:?}"
        );
    }

    #[test]
    fn test_render_path_strips_leading_slash_from_values() {
        let mut meta = make_meta();
        meta.artist = "/Leading".to_string();
        let result = render_path("{artist}/{title}.{ext}", &meta);
        assert!(!result.starts_with('/'));
    }

    #[test]
    fn test_render_path_sanitizes_literal_reserved_segment() {
        // 字面量模板段中的 Windows 保留名也必须转义
        let mut meta = make_meta();
        meta.title = "Song".to_string();
        for reserved in ["CON", "PRN", "AUX", "NUL", "COM1", "LPT1"] {
            let result = render_path(&format!("{reserved}/{{title}}.{{ext}}"), &meta);
            let first_segment = result.split('/').next().unwrap_or_default();
            assert_eq!(
                first_segment,
                format!("{reserved}_"),
                "Literal template segment {reserved:?} must be escaped"
            );
        }
        // 路径中段的字面量保留名
        let result = render_path("Music/AUX/{title}.{ext}", &meta);
        let mid_segment = result.split('/').nth(1).unwrap_or_default();
        assert_eq!(mid_segment, "AUX_");
    }

    #[test]
    fn test_render_path_strips_trailing_dots_in_literal_segment() {
        let mut meta = make_meta();
        meta.title = "song".to_string();
        let result = render_path("Album./{title}.{ext}", &meta);
        let first_segment = result.split('/').next().unwrap_or_default();
        assert_eq!(first_segment, "Album");
    }

    #[test]
    fn test_render_path_strips_trailing_spaces_in_literal_segment() {
        let mut meta = make_meta();
        meta.title = "song".to_string();
        let result = render_path("Album /{title}.{ext}", &meta);
        let first_segment = result.split('/').next().unwrap_or_default();
        assert_eq!(first_segment, "Album");
    }

    #[test]
    fn test_render_path_preserves_dotdot_for_boundary_detection() {
        // `..` 段必须保留，供预览阶段做边界检测
        let mut meta = make_meta();
        meta.artist = "Artist".to_string();
        meta.title = "song".to_string();
        let result = render_path("{artist}/../{title}.{ext}", &meta);
        assert!(
            result.contains(".."),
            "'..' segment must be preserved for boundary detection; got {result:?}"
        );
    }

    #[test]
    fn test_render_path_sanitizes_windows_reserved_names() {
        for reserved in ["CON", "PRN", "AUX", "NUL", "COM1", "COM9", "LPT1", "LPT9"] {
            let mut meta = make_meta();
            meta.artist = reserved.to_string();
            let result = render_path("{artist}/{title}.{ext}", &meta);
            let first_segment = result.split('/').next().unwrap_or_default();
            assert_eq!(
                first_segment,
                format!("{reserved}_"),
                "artist={reserved:?} must render as {reserved:?}_"
            );
        }
    }

    #[test]
    fn test_render_path_sanitizes_windows_reserved_names_case_insensitive() {
        for variant in ["con", "Con", "nul", "Nul", "com1", "Com1"] {
            let mut meta = make_meta();
            meta.artist = variant.to_string();
            let result = render_path("{artist}/{title}.{ext}", &meta);
            let first_segment = result.split('/').next().unwrap_or_default();
            assert_eq!(first_segment, format!("{variant}_"));
        }
    }

    #[test]
    fn test_render_path_does_not_sanitize_non_reserved_names() {
        // 仅以保留名开头前缀的名字不受影响
        for non_reserved in ["CONSOLE", "NULL", "AUX2", "COM10", "LPT10", "CONFORM"] {
            let mut meta = make_meta();
            meta.artist = non_reserved.to_string();
            let result = render_path("{artist}/{title}.{ext}", &meta);
            let first_segment = result.split('/').next().unwrap_or_default();
            assert_eq!(first_segment, non_reserved);
        }
    }

    #[test]
    fn test_render_path_empty_segment_becomes_underscore() {
        // 空段（连续斜杠）清洗为 "_"（对齐 Python `rstrip(". ") or "_"`）
        let meta = make_meta();
        let result = render_path("{artist}//{title}.{ext}", &meta);
        assert_eq!(result, "The Beatles/_/Come Together.mp3");
    }

    #[test]
    fn test_render_path_component_all_whitespace_value() {
        // 占位符值清洗后为空 → "_"
        let mut meta = make_meta();
        meta.title = ". . ".to_string();
        let result = render_path("{artist}/{title}.{ext}", &meta);
        assert_eq!(result, "The Beatles/_.mp3");
    }
}

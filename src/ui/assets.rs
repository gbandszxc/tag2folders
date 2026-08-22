//! 资源源:把 `assets/icons/*.svg` 注册进 gpui 的 AssetSource。
//!
//! 策略:**编译期内嵌**(`include_str!`)+ 运行期文件系统回退。
//! - 内嵌保证 `cargo run`、打包单文件二进制都能取到图标;
//! - 文件系统回退方便开发期直接改 SVG 调试(路径相对可执行文件的 CWD,
//!   `cargo run` 的 CWD 即包根,因此用 `./assets/...`)。
//!
//! 同时该源也服务于 gpui-component 内部图标请求(如 Checkbox 的
//! `icons/check.svg`、Input 清空按钮的 `icons/circle-x.svg`)——
//! 这两个文件在 assets/icons/ 下已提供。

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// 内嵌图标表:path → SVG 文本。
static EMBEDDED: &[(&str, &str)] = &[
    ("icons/alert-circle.svg", include_str!("../../assets/icons/alert-circle.svg")),
    ("icons/alert-triangle.svg", include_str!("../../assets/icons/alert-triangle.svg")),
    ("icons/arrow-left.svg", include_str!("../../assets/icons/arrow-left.svg")),
    ("icons/arrow-right.svg", include_str!("../../assets/icons/arrow-right.svg")),
    ("icons/arrow-up.svg", include_str!("../../assets/icons/arrow-up.svg")),
    ("icons/check-circle.svg", include_str!("../../assets/icons/check-circle.svg")),
    ("icons/check.svg", include_str!("../../assets/icons/check.svg")),
    ("icons/chevron-down.svg", include_str!("../../assets/icons/chevron-down.svg")),
    ("icons/chevron-right.svg", include_str!("../../assets/icons/chevron-right.svg")),
    ("icons/circle-x.svg", include_str!("../../assets/icons/circle-x.svg")),
    ("icons/copy.svg", include_str!("../../assets/icons/copy.svg")),
    ("icons/external-link.svg", include_str!("../../assets/icons/external-link.svg")),
    ("icons/eye.svg", include_str!("../../assets/icons/eye.svg")),
    ("icons/file-audio.svg", include_str!("../../assets/icons/file-audio.svg")),
    ("icons/file.svg", include_str!("../../assets/icons/file.svg")),
    ("icons/filter.svg", include_str!("../../assets/icons/filter.svg")),
    ("icons/folder-open.svg", include_str!("../../assets/icons/folder-open.svg")),
    ("icons/folder.svg", include_str!("../../assets/icons/folder.svg")),
    ("icons/home.svg", include_str!("../../assets/icons/home.svg")),
    ("icons/info.svg", include_str!("../../assets/icons/info.svg")),
    ("icons/layers.svg", include_str!("../../assets/icons/layers.svg")),
    ("icons/lock.svg", include_str!("../../assets/icons/lock.svg")),
    ("icons/music.svg", include_str!("../../assets/icons/music.svg")),
    ("icons/play.svg", include_str!("../../assets/icons/play.svg")),
    ("icons/refresh-cw.svg", include_str!("../../assets/icons/refresh-cw.svg")),
    ("icons/search.svg", include_str!("../../assets/icons/search.svg")),
    ("icons/settings.svg", include_str!("../../assets/icons/settings.svg")),
    ("icons/sparkles.svg", include_str!("../../assets/icons/sparkles.svg")),
    ("icons/tag.svg", include_str!("../../assets/icons/tag.svg")),
    ("icons/terminal.svg", include_str!("../../assets/icons/terminal.svg")),
    ("icons/trash.svg", include_str!("../../assets/icons/trash.svg")),
    ("icons/x-circle.svg", include_str!("../../assets/icons/x-circle.svg")),
    ("icons/x.svg", include_str!("../../assets/icons/x.svg")),
];

#[derive(Default)]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        // 1) 内嵌表
        if let Some((_, content)) = EMBEDDED.iter().find(|(p, _)| *p == path) {
            return Ok(Some(Cow::Borrowed(content.as_bytes())));
        }
        // 2) 文件系统回退(开发期热改 SVG 用)
        if let Ok(bytes) = std::fs::read(path) {
            return Ok(Some(Cow::Owned(bytes)));
        }
        Ok(None)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let names = EMBEDDED
            .iter()
            .filter(|(p, _)| p.starts_with(path))
            .map(|(p, _)| SharedString::from(p.to_string()))
            .collect();
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::icon::Icon;

    /// 每个 Icon 的路径都能从内嵌表加载到非空 SVG,且为 24×24 stroke 风格。
    #[test]
    fn all_icons_load_from_embedded_table() {
        let assets = Assets;
        for icon in Icon::all() {
            let bytes = assets
                .load(icon.path())
                .expect("load ok")
                .unwrap_or_else(|| panic!("missing embedded asset: {}", icon.path()));
            let text = std::str::from_utf8(&bytes).unwrap();
            assert!(text.contains(r#"viewBox="0 0 24 24""#), "{}", icon.path());
            assert!(text.contains(r#"stroke="currentColor""#), "{}", icon.path());
            assert!(text.contains(r#"fill="none""#), "{}", icon.path());
        }
    }

    /// gpui-component 依赖的两个图标同样可用(Checkbox 勾选 / Input 清空按钮)。
    #[test]
    fn gpui_component_icons_available() {
        let assets = Assets;
        assert!(assets.load("icons/check.svg").unwrap().is_some());
        assert!(assets.load("icons/circle-x.svg").unwrap().is_some());
    }
}

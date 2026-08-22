//! 图标系统:对应源项目 CommonUI.tsx 的内联 SVG 图标(SOURCE_SPEC 2.1)。
//!
//! ## 着色机制结论(已读 gpui 0.2.2 源码验证)
//!
//! gpui 的 `svg()` 元素渲染管线(`gpui/src/svg_renderer.rs` + `elements/svg.rs`):
//! 1. 经 `AssetSource` 按路径加载 SVG 字节;
//! 2. 用 usvg/resvg 光栅化为 pixmap;
//! 3. **把 pixmap 降为纯 alpha 遮罩**(只取每个像素的 alpha 通道,颜色信息全部丢弃);
//! 4. 绘制时用该元素 `style.text.color`(即 `.text_color()` 传入的颜色)给遮罩上色。
//!
//! 因此:
//! - SVG 内写 `stroke="currentColor"` 完全可行——usvg 将其解析为不透明黑,
//!   反正只有 alpha 进入遮罩;
//! - **图标颜色 = 所在元素链上的文本颜色**,`icon(Icon::Folder).text_color(theme::AMBER_600)`
//!   即完成"变体换色",无需按颜色生成多份 SVG;
//! - 图标尺寸用 `.size(px(n))` 指定(SVG viewBox 为 24×24,等比缩放)。

#![allow(dead_code)] // token 表/图标枚举/服务辅助为后续页面 agent 预留,当前未全部使用

use gpui::prelude::*;
use gpui::{Pixels, Svg, px, svg};


/// 图标枚举(32 个,与源 CommonUI.tsx 导出一一对应;命名沿用 Lucide)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Folder,
    FolderOpen,
    Music,
    Play,
    Refresh,
    ArrowRight,
    ArrowLeft,
    ArrowUp,
    Check,
    CheckCircle,
    AlertTriangle,
    AlertCircle,
    XCircle,
    X,
    Copy,
    Search,
    Tag,
    Eye,
    Layers,
    Settings,
    ChevronRight,
    ChevronDown,
    Sparkles,
    File,
    FileAudio,
    Home,
    Lock,
    Terminal,
    ExternalLink,
    Trash,
    Filter,
    Info,
}

impl Icon {
    /// 全部图标(测试与遍历用)。
    pub fn all() -> &'static [Icon] {
        &[
            Icon::Folder,
            Icon::FolderOpen,
            Icon::Music,
            Icon::Play,
            Icon::Refresh,
            Icon::ArrowRight,
            Icon::ArrowLeft,
            Icon::ArrowUp,
            Icon::Check,
            Icon::CheckCircle,
            Icon::AlertTriangle,
            Icon::AlertCircle,
            Icon::XCircle,
            Icon::X,
            Icon::Copy,
            Icon::Search,
            Icon::Tag,
            Icon::Eye,
            Icon::Layers,
            Icon::Settings,
            Icon::ChevronRight,
            Icon::ChevronDown,
            Icon::Sparkles,
            Icon::File,
            Icon::FileAudio,
            Icon::Home,
            Icon::Lock,
            Icon::Terminal,
            Icon::ExternalLink,
            Icon::Trash,
            Icon::Filter,
            Icon::Info,
        ]
    }

    /// AssetSource 内的路径(见 [`crate::ui::assets`],同时兼容 gpui-component
    /// 的图标命名,如 `icons/check.svg`)。
    pub fn path(self) -> &'static str {
        match self {
            Icon::Folder => "icons/folder.svg",
            Icon::FolderOpen => "icons/folder-open.svg",
            Icon::Music => "icons/music.svg",
            Icon::Play => "icons/play.svg",
            Icon::Refresh => "icons/refresh-cw.svg",
            Icon::ArrowRight => "icons/arrow-right.svg",
            Icon::ArrowLeft => "icons/arrow-left.svg",
            Icon::ArrowUp => "icons/arrow-up.svg",
            Icon::Check => "icons/check.svg",
            Icon::CheckCircle => "icons/check-circle.svg",
            Icon::AlertTriangle => "icons/alert-triangle.svg",
            Icon::AlertCircle => "icons/alert-circle.svg",
            Icon::XCircle => "icons/x-circle.svg",
            Icon::X => "icons/x.svg",
            Icon::Copy => "icons/copy.svg",
            Icon::Search => "icons/search.svg",
            Icon::Tag => "icons/tag.svg",
            Icon::Eye => "icons/eye.svg",
            Icon::Layers => "icons/layers.svg",
            Icon::Settings => "icons/settings.svg",
            Icon::ChevronRight => "icons/chevron-right.svg",
            Icon::ChevronDown => "icons/chevron-down.svg",
            Icon::Sparkles => "icons/sparkles.svg",
            Icon::File => "icons/file.svg",
            Icon::FileAudio => "icons/file-audio.svg",
            Icon::Home => "icons/home.svg",
            Icon::Lock => "icons/lock.svg",
            Icon::Terminal => "icons/terminal.svg",
            Icon::ExternalLink => "icons/external-link.svg",
            Icon::Trash => "icons/trash.svg",
            Icon::Filter => "icons/filter.svg",
            Icon::Info => "icons/info.svg",
        }
    }
}

/// 构建一个图标元素(默认继承文本色,调用方链式 `.size()` / `.text_color()`)。
///
/// ```
/// icon(Icon::Folder).size(px(16.)).text_color(theme::AMBER_600)
/// ```
pub fn icon(i: Icon) -> Svg {
    svg().path(i.path())
}

/// 构建一个指定尺寸的图标(颜色仍继承文本色,必要时再链 `.text_color()`)。
pub fn icon_sized(i: Icon, size: Pixels) -> Svg {
    icon(i).size(size)
}

/// 16px 图标(源组件默认尺寸,SPEC 2.1"默认尺寸 16px")。
pub fn icon_16(i: Icon) -> Svg {
    icon(i).size(px(16.0))
}

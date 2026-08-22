//! 徽章(SOURCE_SPEC 2.3 .badge CSS 类)与状态徽章(SPEC 2.4 StatusBadge)。

#![allow(dead_code)]

use gpui::prelude::*;
use gpui::{App, RenderOnce, SharedString, Window, div, px};

use crate::ui::theme;
use crate::ui::{Icon, icon_sized};

/// `.badge` 语义色变体(SPEC 2.3 表)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeVariant {
    Emerald,
    Amber,
    Rose,
    Sky,
    Slate,
}

impl BadgeVariant {
    /// (背景, 文字, 边框)
    fn colors(self) -> (gpui::Rgba, gpui::Rgba, gpui::Rgba) {
        match self {
            BadgeVariant::Emerald => (theme::EMERALD_50, theme::EMERALD_700, theme::EMERALD_200),
            BadgeVariant::Amber => (theme::AMBER_100, theme::AMBER_900, theme::AMBER_300),
            BadgeVariant::Rose => (theme::ROSE_50, theme::ROSE_700, theme::ROSE_200),
            BadgeVariant::Sky => (theme::SKY_50, theme::SKY_700, theme::SKY_200),
            BadgeVariant::Slate => (theme::SLATE_100, theme::SLATE_700, theme::SLATE_200),
        }
    }
}

/// 徽章基座:`.badge` = inline-flex 居中、gap 4、padding 2px 8px、fontSize 11、
/// weight 600、圆角 full、line-height 1.4、nowrap。调用方继续链样式覆盖
/// (如版本徽章 padding 4px 10px / fontSize 11.5)。
pub fn badge(variant: BadgeVariant) -> gpui::Div {
    let (bg, fg, border) = variant.colors();
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .px(px(8.0))
        .py(px(2.0))
        .text_size(px(11.0))
        .font_weight(gpui::FontWeight(600.0))
        .rounded(theme::RADIUS_FULL)
        .line_height(gpui::relative(1.4))
        .whitespace_nowrap()
        .bg(bg)
        .text_color(fg)
        .border_1()
        .border_color(border)
}

/// 带文字的徽章(常见用法一步到位)。
pub fn badge_text(variant: BadgeVariant, text: impl Into<SharedString>) -> gpui::Div {
    badge(variant).child(text.into())
}

/// StatusBadge 的状态映射(SPEC 2.4 STATUS_CONFIG,label 原文)。
pub fn status_config(status: &str) -> (SharedString, BadgeVariant, Icon) {
    match status {
        "ok" => ("正常".into(), BadgeVariant::Emerald, Icon::CheckCircle),
        "conflict" => ("磁盘冲突".into(), BadgeVariant::Amber, Icon::AlertTriangle),
        "batch_conflict" => ("批内冲突".into(), BadgeVariant::Amber, Icon::AlertTriangle),
        "missing_metadata" => ("缺失信息".into(), BadgeVariant::Sky, Icon::Info),
        "unreadable" => ("不可读".into(), BadgeVariant::Slate, Icon::XCircle),
        "boundary_error" => ("路径越界".into(), BadgeVariant::Rose, Icon::AlertCircle),
        "write_error" => ("写入受阻".into(), BadgeVariant::Rose, Icon::AlertCircle),
        // 未知值:原样显示,slate + InfoIcon
        other => (other.to_string().into(), BadgeVariant::Slate, Icon::Info),
    }
}

/// 后端 `MappingStatus` 的字符串形式(序列化值,与源前端 status 字段一致)。
pub fn mapping_status_str(status: tag2folders_lib::core::MappingStatus) -> &'static str {
    use tag2folders_lib::core::MappingStatus::*;
    match status {
        Ok => "ok",
        Conflict => "conflict",
        BatchConflict => "batch_conflict",
        MissingMetadata => "missing_metadata",
        Unreadable => "unreadable",
        BoundaryError => "boundary_error",
        WriteError => "write_error",
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StatusBadgeSize {
    #[default]
    Md,
    Sm,
}

/// 状态徽章组件(SPEC 2.4):md = padding 2px 8px、fontSize 12、图标 13;
/// sm = padding 1px 6px、fontSize 11、图标 12。
#[derive(gpui::IntoElement)]
pub struct StatusBadge {
    status: SharedString,
    size: StatusBadgeSize,
    show_icon: bool,
}

impl StatusBadge {
    pub fn new(status: impl Into<SharedString>) -> Self {
        Self {
            status: status.into(),
            size: StatusBadgeSize::default(),
            show_icon: true,
        }
    }

    pub fn from_mapping_status(status: tag2folders_lib::core::MappingStatus) -> Self {
        Self::new(mapping_status_str(status))
    }

    pub fn size(mut self, size: StatusBadgeSize) -> Self {
        self.size = size;
        self
    }

    pub fn show_icon(mut self, show: bool) -> Self {
        self.show_icon = show;
        self
    }
}

impl RenderOnce for StatusBadge {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (label, variant, icon) = status_config(&self.status);
        let (pad_y, pad_x, font_size, icon_size) = match self.size {
            StatusBadgeSize::Md => (px(2.0), px(8.0), px(12.0), px(13.0)),
            StatusBadgeSize::Sm => (px(1.0), px(6.0), px(11.0), px(12.0)),
        };
        let mut el = badge(variant)
            .px(pad_x)
            .py(pad_y)
            .text_size(font_size)
            // 源组件 title 属性 = label(gpui 无原生 tooltip,悬浮提示为已知差异)
            .when(self.show_icon, |el| el.child(icon_sized(icon, icon_size)));
        el = el.child(label);
        el
    }
}

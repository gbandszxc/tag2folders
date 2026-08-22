//! 内联提示条(SOURCE_SPEC 7.6:无 toast,全部内联;rose=错误 / amber=警告 / sky=信息)。
//!
//! 样式基准(4.1.2 / 4.1.3 等):flex、gap 10、padding 10px 14px、圆角 8px、
//! 图标 15 + 文本 fontSize 12.5。文字色按 SPEC 7.9 陷阱:rose-800 / sky-800
//! 未定义,等效 #0f172a;amber 系文字色为 amber-800。
//!
//! 支持多行文本(PreviewPage 错误条 whiteSpace: pre-wrap)。

#![allow(dead_code)]

use gpui::prelude::*;
use gpui::{App, Pixels, RenderOnce, SharedString, Window, div, px};

use crate::ui::theme;
use crate::ui::{Icon, icon_sized};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertVariant {
    /// 错误(rose):bg rose-50 / border rose-200 / icon rose-600 / 文字 #0f172a
    Rose,
    /// 警告(amber):bg amber-50 / border amber-200 / icon amber-600 / 文字 amber-800
    Amber,
    /// 信息(sky):bg sky-50 / border sky-200 / icon sky-600 / 文字 #0f172a
    Sky,
}

impl AlertVariant {
    fn colors(self) -> (gpui::Rgba, gpui::Rgba, gpui::Rgba, gpui::Rgba) {
        // (背景, 边框, 图标色, 文字色)
        match self {
            AlertVariant::Rose => (
                theme::ROSE_50,
                theme::ROSE_200,
                theme::ROSE_600,
                theme::INHERITED_TEXT, // var(--rose-800) 未定义 → #0f172a(SPEC 7.9)
            ),
            AlertVariant::Amber => (
                theme::AMBER_50,
                theme::AMBER_200,
                theme::AMBER_600,
                theme::AMBER_800,
            ),
            AlertVariant::Sky => (
                theme::SKY_50,
                theme::SKY_200,
                theme::SKY_600,
                theme::INHERITED_TEXT, // var(--sky-800) 未定义 → #0f172a(SPEC 7.9)
            ),
        }
    }
}

/// 提示条组件。默认图标随变体(rose=AlertTriangle、amber=AlertTriangle、sky=Info),
/// 可用 [`AlertBar::icon`] 覆盖(如 ProgressPage 失败横幅用 AlertCircle)。
#[derive(gpui::IntoElement)]
pub struct AlertBar {
    variant: AlertVariant,
    text: SharedString,
    icon: Option<Icon>,
    icon_size: Pixels,
    font_size: Pixels,
    /// 是否保留换行(pre-wrap,PreviewPage 错误条 true)
    pre_wrap: bool,
    mt: Option<Pixels>,
    mb: Option<Pixels>,
    /// 内边距覆盖(如 ScanPage 空结果条为 12px 16px,非默认 10px 14px)
    pad_x: Option<Pixels>,
    pad_y: Option<Pixels>,
}

impl AlertBar {
    pub fn new(variant: AlertVariant, text: impl Into<SharedString>) -> Self {
        Self {
            variant,
            text: text.into(),
            icon: None,
            icon_size: px(15.0),
            font_size: px(12.5),
            pre_wrap: false,
            mt: None,
            mb: None,
            pad_x: None,
            pad_y: None,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn icon_size(mut self, size: Pixels) -> Self {
        self.icon_size = size;
        self
    }

    pub fn font_size(mut self, size: Pixels) -> Self {
        self.font_size = size;
        self
    }

    pub fn pre_wrap(mut self, yes: bool) -> Self {
        self.pre_wrap = yes;
        self
    }

    pub fn mt(mut self, v: Pixels) -> Self {
        self.mt = Some(v);
        self
    }

    pub fn mb(mut self, v: Pixels) -> Self {
        self.mb = Some(v);
        self
    }

    /// 覆盖水平内边距(默认 14px)。
    pub fn pad_x(mut self, v: Pixels) -> Self {
        self.pad_x = Some(v);
        self
    }

    /// 覆盖垂直内边距(默认 10px)。
    pub fn pad_y(mut self, v: Pixels) -> Self {
        self.pad_y = Some(v);
        self
    }
}

impl RenderOnce for AlertBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (bg, border, icon_color, text_color) = self.variant.colors();
        let default_icon = match self.variant {
            AlertVariant::Rose => Icon::AlertTriangle,
            AlertVariant::Amber => Icon::AlertTriangle,
            AlertVariant::Sky => Icon::Info,
        };
        let icon = self.icon.unwrap_or(default_icon);

        div()
            .flex()
            .items_start()
            .gap(px(10.0))
            .px(px(14.0))
            .py(px(10.0))
            .bg(bg)
            .border_1()
            .border_color(border)
            .rounded(theme::RADIUS_MD)
            .when_some(self.mt, |el, v| el.mt(v))
            .when_some(self.mb, |el, v| el.mb(v))
            .when_some(self.pad_x, |el, v| el.px(v))
            .when_some(self.pad_y, |el, v| el.py(v))
            .child(icon_sized(icon, self.icon_size).text_color(icon_color))
            .child(
                div()
                    .text_size(self.font_size)
                    .text_color(text_color)
                    .line_height(gpui::relative(1.5))
                    // 多行:gpui 文本默认按 \n 换行(WhiteSpace::Normal),pre_wrap 无需额外设置
                    .child(self.text),
            )
    }
}

//! 自绘按钮。
//!
//! 变体:primary / secondary / outline / ghost / danger。尺寸:sm / md / lg。

#![allow(dead_code)]

use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt, App, ClickEvent, ElementId, Pixels, RenderOnce, SharedString,
    Transformation, Window, div, percentage, px, svg,
};

use crate::ui::theme;
use crate::ui::{Icon, icon_sized};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    #[default]
    Secondary,
    Primary,
    Outline,
    Ghost,
    Danger,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonSize {
    #[default]
    Md,
    Sm,
    Lg,
}

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(gpui::IntoElement)]
/// 按钮组件(每次 render 重建,不持有状态)。
///
/// ```ignore
/// Button::new("scan")
///     .label("开始扫描")
///     .variant(ButtonVariant::Primary)
///     .size(ButtonSize::Lg)
///     .icon(Icon::Music, px(15.))
///     .loading(self.loading)
///     .disabled(self.source_dir.trim().is_empty())
///     .on_click(cx.listener(|this, _, _, cx| { /* ... */ }))
/// ```
pub struct Button {
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<(Icon, Pixels)>,
    icon_right: bool,
    loading: bool,
    disabled: bool,
    variant: ButtonVariant,
    size: ButtonSize,
    min_width: Option<Pixels>,
    height: Option<Pixels>,
    h_full: bool,
    pad_x: Option<Pixels>,
    pad_y: Option<Pixels>,
    text_size: Option<Pixels>,
    on_click: Option<ClickHandler>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            label: None,
            icon: None,
            icon_right: false,
            loading: false,
            disabled: false,
            variant: ButtonVariant::default(),
            size: ButtonSize::default(),
            min_width: None,
            height: None,
            h_full: false,
            pad_x: None,
            pad_y: None,
            text_size: None,
            on_click: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// 左侧图标(尺寸由调用方指定)。
    pub fn icon(mut self, icon: Icon, size: Pixels) -> Self {
        self.icon = Some((icon, size));
        self
    }

    /// 图标放右侧。
    pub fn icon_right(mut self) -> Self {
        self.icon_right = true;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// 最小宽度(源 ConfirmModal 底部按钮 minWidth 76/88)。
    pub fn min_w(mut self, w: Pixels) -> Self {
        self.min_width = Some(w);
        self
    }

    /// 覆盖高度(DirPicker 浏览按钮 height:38)。
    pub fn h(mut self, h: Pixels) -> Self {
        self.height = Some(h);
        self.h_full = false;
        self
    }

    /// 填满父容器垂直高度(与同行的Input完美拉伸对齐)。
    pub fn h_full(mut self) -> Self {
        self.h_full = true;
        self.height = None;
        self
    }
    /// 覆盖水平内边距(DirPicker 浏览按钮 padding 0 16px)。
    pub fn pad_x(mut self, x: Pixels) -> Self {
        self.pad_x = Some(x);
        self
    }

    /// 覆盖垂直内边距(目录树工具栏"全部折叠"按钮 padding 4px 8px)。
    pub fn pad_y(mut self, y: Pixels) -> Self {
        self.pad_y = Some(y);
        self
    }

    /// 覆盖字号(目录树工具栏按钮 fontSize 11)。
    pub fn text_size(mut self, size: Pixels) -> Self {
        self.text_size = Some(size);
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    /// 按钮内图标尺寸(loading 旋转图标:sm 12 / 其他 14)。
    fn spinner_size(&self) -> Pixels {
        match self.size {
            ButtonSize::Sm => px(12.0),
            _ => px(14.0),
        }
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // 键盘焦点:按元素 id 持久化的 FocusHandle(window 级 keyed state,
        // RenderOnce 每帧重建组件的标准焦点方案,同 gpui-component Button)
        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();

        let (pad_y, pad_x, font_size, radius, font_weight) = match self.size {
            ButtonSize::Sm => (px(5.0), px(10.0), px(12.0), theme::RADIUS_SM, 500),
            ButtonSize::Lg => (px(10.0), px(20.0), px(14.0), theme::RADIUS_LG, 600),
            ButtonSize::Md => (px(8.0), px(14.0), px(13.0), theme::RADIUS_MD, 500),
        };

        let effective_disabled = self.disabled || self.loading;
        let has_text = self.label.is_some();
        // 聚焦可见态仅对可交互按钮生效(禁用态不参与聚焦)
        let focused = !effective_disabled && focus_handle.is_focused(window);

        // 变体三态(常态/悬浮/按下)
        let (bg, fg, border, weight_override) = match self.variant {
            ButtonVariant::Primary => (theme::AMBER_500, theme::SLATE_800, theme::AMBER_600, Some(600)),
            ButtonVariant::Secondary => (theme::SLATE_100, theme::SLATE_700, theme::SLATE_200, None),
            ButtonVariant::Outline => (gpui::transparent_black().into(), theme::SLATE_700, theme::BORDER_DEFAULT, None),
            ButtonVariant::Ghost => (gpui::transparent_black().into(), theme::SLATE_600, gpui::transparent_black().into(), None),
            // Danger 常态文字用 rose-700(5.72:1 达 AA;rose-600 on rose-50 仅 4.28:1 不达标)
            ButtonVariant::Danger => (theme::ROSE_50, theme::ROSE_700, theme::ROSE_200, None),
        };
        let (h_bg, h_fg, h_border) = match self.variant {
            ButtonVariant::Primary => (theme::AMBER_600, theme::SLATE_900, theme::AMBER_700),
            ButtonVariant::Secondary => (theme::SLATE_200, theme::SLATE_900, theme::SLATE_300),
            ButtonVariant::Outline => (theme::SLATE_50, theme::SLATE_900, theme::SLATE_400),
            ButtonVariant::Ghost => (theme::SLATE_100, theme::SLATE_900, gpui::transparent_black().into()),
            ButtonVariant::Danger => (theme::ROSE_600, theme::TEXT_ON_PRIMARY, theme::ROSE_600),
        };
        let (a_bg, a_fg, a_border) = match self.variant {
            ButtonVariant::Primary => (theme::AMBER_700, theme::TEXT_ON_PRIMARY, theme::AMBER_800),
            ButtonVariant::Secondary => (theme::SLATE_200, theme::SLATE_900, theme::SLATE_300),
            ButtonVariant::Outline => (theme::SLATE_50, theme::SLATE_900, theme::SLATE_400),
            ButtonVariant::Ghost => (theme::SLATE_100, theme::SLATE_900, gpui::transparent_black().into()),
            ButtonVariant::Danger => (theme::ROSE_600, theme::TEXT_ON_PRIMARY, theme::ROSE_600),
        };
        let font_weight = weight_override.unwrap_or(font_weight);

        // 聚焦可见:复用既有 1px 边框改色,不引起布局位移
        let border = if focused { theme::BORDER_FOCUS } else { border };

        // 主按钮阴影:常态 0 1px 2px rgba(0,0,0,0.05),悬浮 0 2px 4px rgba(0,0,0,0.08)
        let (shadow_normal, shadow_hover) = if self.variant == ButtonVariant::Primary {
            (Some(theme::shadow_primary_btn()), Some(theme::shadow_primary_btn_hover()))
        } else {
            (None, None)
        };

        let spinner = if self.loading {
            Some(
                svg()
                    .path(Icon::Refresh.path())
                    .size(self.spinner_size())
                    .text_color(fg)
                    .with_animation(
                        SharedString::from(format!("btn-spin-{:?}", self.id)),
                        Animation::new(Duration::from_millis(theme::DURATION_SPIN_MS))
                            .repeat()
                            .with_easing(|t| t), // 线性
                        |el, delta| el.with_transformation(Transformation::rotate(percentage(delta))),
                    ),
            )
        } else {
            None
        };
        let left_icon = if !self.loading && !self.icon_right {
            self.icon.map(|(i, s)| icon_sized(i, s).text_color(fg))
        } else {
            None
        };
        let right_icon = if !self.loading && self.icon_right {
            self.icon.map(|(i, s)| icon_sized(i, s).text_color(fg))
        } else {
            None
        };

        let pad_y_to_apply = match (self.pad_y, self.height, self.h_full) {
            (Some(y), _, _) => Some(y),
            (None, Some(_), _) => None, // 已指定固定高度，不额外加垂直padding
            (None, None, true) => None, // 占满父容器高度，由垂直居中处理
            (None, None, false) => Some(pad_y),
        };

        let mut btn = div()
            .id(self.id)
            .flex()
            .items_center()
            .justify_center()
            .gap(if self.loading && has_text { px(4.0) } else { px(6.0) })
            .when_some(self.pad_x, |el, x| el.px(x))
            .when(!self.pad_x.is_some(), |el| el.px(pad_x))
            .when_some(pad_y_to_apply, |el, y| el.py(y))
            .when_some(self.height, |el, h| el.h(h))
            .when(self.h_full, |el| el.h_full())
            .when_some(self.min_width, |el, w| el.min_w(w))
            .when_some(self.text_size, |el, s| el.text_size(s))
            .when(self.text_size.is_none(), |el| el.text_size(font_size))
            .font_weight(gpui::FontWeight(font_weight as f32))
            .line_height(gpui::relative(1.25))
            .whitespace_nowrap()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_color(fg)
            .when_some(shadow_normal, |el, s| el.shadow(s))
            .when(!effective_disabled, |el| {
                // track_focus 使按钮可 Tab 聚焦;框架对聚焦元素自动把 Enter/Space 转发为 click
                el.track_focus(&focus_handle)
                    .cursor_pointer()
                    .hover(move |st| {
                        let st = st.bg(h_bg).text_color(h_fg).border_color(h_border);
                        if let Some(sh) = shadow_hover {
                            st.shadow(sh)
                        } else {
                            st
                        }
                    })
                    .active(|st| st.bg(a_bg).text_color(a_fg).border_color(a_border))
            })
            .when(effective_disabled, |el| {
                // 源禁用态:opacity 0.55 + not-allowed + 无悬浮
                el.opacity(0.55)
            });

        if let Some(handler) = self.on_click {
            if !effective_disabled {
                btn = btn.on_click(move |e, window, cx| handler(e, window, cx));
            }
        }

        if spinner.is_some() || left_icon.is_some() {
            btn = btn.children(spinner).children(left_icon);
        }
        if let Some(label) = self.label {
            btn = btn.child(label);
        }
        if right_icon.is_some() {
            btn = btn.children(right_icon);
        }
        btn
    }
}

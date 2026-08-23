//! 卡片:白底、圆角 12、边框 subtle、阴影 xs、overflow hidden;
//! 可选头部(title/subtitle/actions)与主体 padding 档位。

#![allow(dead_code)]

use gpui::prelude::*;
use gpui::{AnyElement, App, RenderOnce, SharedString, Window, div, px};

use crate::ui::theme;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CardPadding {
    None,
    Sm,
    #[default]
    Md,
    Lg,
}

#[derive(gpui::IntoElement)]
pub struct Card {
    title: Option<SharedString>,
    subtitle: Option<SharedString>,
    actions: Option<AnyElement>,
    children: Vec<AnyElement>,
    padding: CardPadding,
    /// 额外附加到卡片根上的样式钩子(如 marginTop);调用方以闭包给出。
    extra_style: Option<Box<dyn FnOnce(gpui::Div) -> gpui::Div + 'static>>,
}

impl Card {
    pub fn new() -> Self {
        Self {
            title: None,
            subtitle: None,
            actions: None,
            children: Vec::new(),
            padding: CardPadding::default(),
            extra_style: None,
        }
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// 头部右侧动作区(任意元素,如按钮组/徽章)。
    pub fn actions(mut self, actions: impl IntoElement) -> Self {
        self.actions = Some(actions.into_any_element());
        self
    }

    pub fn padding(mut self, padding: CardPadding) -> Self {
        self.padding = padding;
        self
    }

    /// 给卡片根追加样式(保留链式),供页面复用微调外边距等。
    pub fn map(mut self, f: impl FnOnce(gpui::Div) -> gpui::Div + 'static) -> Self {
        self.extra_style = Some(Box::new(f));
        self
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for Card {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut card = div()
            .flex()
            .flex_col()
            .bg(theme::BG_SURFACE)
            .rounded(theme::RADIUS_LG)
            .border_1()
            .border_color(theme::BORDER_SUBTLE)
            .shadow(theme::shadow_xs())
            .overflow_hidden();

        if let Some(f) = self.extra_style {
            card = f(card);
        }

        let has_header = self.title.is_some() || self.subtitle.is_some() || self.actions.is_some();
        let card = if has_header {
            let mut header = div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .px(px(20.0))
                .py(px(14.0))
                .border_b_1()
                .border_color(theme::BORDER_SUBTLE);
            if self.title.is_some() || self.subtitle.is_some() {
                let mut text_col = div().flex().flex_col();
                if let Some(title) = self.title {
                    text_col = text_col.child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(gpui::FontWeight(600.0))
                            .text_color(theme::SLATE_800)
                            .child(title),
                    );
                }
                if let Some(subtitle) = self.subtitle {
                    text_col = text_col.child(
                        div()
                            .mt(px(2.0))
                            .text_size(px(12.0))
                            .text_color(theme::SLATE_500)
                            .child(subtitle),
                    );
                }
                header = header.child(text_col);
            }
            if let Some(actions) = self.actions {
                header = header.child(actions);
            }
            card.child(header)
        } else {
            card
        };

        // 主体 padding:none 0 / sm 12x16 / md 18x22 / lg 24x28
        let (pad_y, pad_x) = match self.padding {
            CardPadding::None => (px(0.0), px(0.0)),
            CardPadding::Sm => (px(12.0), px(16.0)),
            CardPadding::Md => (px(18.0), px(22.0)),
            CardPadding::Lg => (px(24.0), px(28.0)),
        };

        let body = div().py(pad_y).px(pad_x).children(self.children);
        card.child(body)
    }
}

/// 卡片包装的便利函数(等价 `Card::new().child(...)` 链)。
pub fn card() -> Card {
    Card::new()
}

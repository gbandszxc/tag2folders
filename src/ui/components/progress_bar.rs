//! 进度条:轨道 height 12、圆角 full、背景 slate-100;
//! 填充 amber-500、圆角 full。源实现用 `transform: scaleX(pct/100)` + 250ms 过渡,
//! gpui 无 transform 过渡,直接按百分比宽度呈现。

#![allow(dead_code)]

use gpui::prelude::*;
use gpui::{App, RenderOnce, Window, div, px};

use crate::ui::theme;

/// `pct` 取值 0.0 ~ 1.0(内部 clamp;源逻辑 round(current/total*100) 由调用方完成)。
#[derive(gpui::IntoElement)]
pub struct ProgressBar {
    fraction: f32,
}

impl ProgressBar {
    pub fn new(current: usize, total: usize) -> Self {
        let fraction = if total > 0 {
            (current as f32 / total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self { fraction }
    }

    pub fn from_fraction(fraction: f32) -> Self {
        Self {
            fraction: fraction.clamp(0.0, 1.0),
        }
    }
}

impl RenderOnce for ProgressBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .w_full()
            .h(px(12.0))
            .rounded(theme::RADIUS_FULL)
            .bg(theme::SLATE_100)
            .overflow_hidden()
            .child(
                div()
                    .h_full()
                    .w(gpui::relative(self.fraction))
                    .bg(theme::AMBER_500)
                    .rounded(theme::RADIUS_FULL),
            )
    }
}

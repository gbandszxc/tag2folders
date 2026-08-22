//! 左侧步骤向导栏(SOURCE_SPEC 1.6)。
//!
//! 三步骤(文案/副标题/图标照抄 STEPS 常量);StepItem 状态机:
//! done(已完成)/ active(进行中)/ dimmed(未解锁,opacity 0.5)/ 默认(已解锁未激活)。
//! 点击/键盘规则:仅 `num <= max_unlocked_step` 可点,点击切换 current_step。
#![allow(dead_code)]

use std::time::Duration;


use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt, App, ClickEvent, RenderOnce, SharedString, Window, div, px,
};

use crate::ui::theme;
use crate::ui::{Icon, icon_sized};

/// 步骤定义(SOURCE_SPEC 1.6 STEPS 常量,顺序固定)。
pub struct StepDef {
    pub num: usize,
    pub label: &'static str,
    pub desc: &'static str,
    pub icon: Icon,
}

pub const STEPS: [StepDef; 3] = [
    StepDef { num: 1, label: "扫描文件", desc: "选择源目录与提取标签", icon: Icon::Music },
    StepDef { num: 2, label: "模板预览", desc: "规划命名与结构方案", icon: Icon::Eye },
    StepDef { num: 3, label: "执行整理", desc: "批量安全归档与监控", icon: Icon::Play },
];

type StepClickHandler = std::rc::Rc<dyn Fn(usize, &gpui::ClickEvent, &mut Window, &mut App)>;

#[derive(gpui::IntoElement)]
pub struct StepNav {
    current: usize,
    max_unlocked: usize,
    on_click: StepClickHandler,
}

impl StepNav {
    /// `current` / `max_unlocked` 均为 1|2|3。
    pub fn new(
        current: usize,
        max_unlocked: usize,
        on_click: impl Fn(usize, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            current,
            max_unlocked,
            on_click: std::rc::Rc::new(on_click),
        }
    }
}

impl RenderOnce for StepNav {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        // aside:宽 clamp(210,22vw,250) → 用弹性近似 22vw:固定 230px 中值 + min/max
        let mut col = div().flex().flex_col().gap(px(2.0));
        let steps = STEPS.len();
        for (ix, step) in STEPS.iter().enumerate() {
            let done = step.num < self.current;
            let active = step.num == self.current;
            let unlocked = step.num <= self.max_unlocked;
            let _ = unlocked;
            col = col.child(self.render_step_item(step, done, active));
            if ix + 1 < steps {
                // 步骤间连接线:marginLeft 30、h 24、w 2、色随解锁进度
                let connector_color = if step.num < self.max_unlocked {
                    theme::AMBER_400
                } else {
                    theme::SLATE_100
                };
                col = col.child(
                    div()
                        .ml(px(30.0))
                        .mt(px(2.0))
                        .mb(px(2.0))
                        .w(px(2.0))
                        .h(px(24.0))
                        .bg(connector_color),
                );
            }
        }
        col
    }
}

impl StepNav {
    fn render_step_item(&self, step: &StepDef, done: bool, active: bool) -> impl IntoElement {
        let unlocked = step.num <= self.max_unlocked;
        let dimmed = !unlocked;

        // 38×38 图标瓦片分态(SPEC 1.6 表)
        let (tile_bg, tile_fg, show_check) = if done {
            (theme::EMERALD_50, theme::EMERALD_600, true)
        } else if active {
            (theme::AMBER_500, theme::SLATE_800, false)
        } else if dimmed {
            (theme::SLATE_50, theme::SLATE_300, false)
        } else {
            (theme::SLATE_100, theme::SLATE_500, false)
        };

        let tile_icon = if show_check {
            icon_sized(Icon::Check, px(18.0))
        } else {
            icon_sized(step.icon, px(18.0))
        };

        // 标题/副标题颜色
        let title_color = if dimmed {
            theme::SLATE_400
        } else if active {
            theme::AMBER_900
        } else {
            theme::SLATE_800
        };
        let desc_color = if dimmed { theme::SLATE_400 } else { theme::SLATE_500 };

        // 右侧状态徽标四选一
        let status: gpui::AnyElement = if active {
            // 8×8 脉冲圆点,animate-pulse(透明度 1↔0.6,2s)
            div()
                .size(px(8.0))
                .rounded(theme::RADIUS_FULL)
                .bg(theme::AMBER_500)
                .with_animation(
                    SharedString::from(format!("step-pulse-{}", step.num)),
                    Animation::new(Duration::from_millis(theme::DURATION_PULSE_MS))
                        .repeat()
                        .with_easing(|t| 1.0 - (2.0 * t - 1.0).abs()), // 三角波 1→0→1
                    |el, eased| el.opacity(0.6 + 0.4 * eased),
                )
                .into_any_element()
        } else if done {
            div()
                .text_size(px(11.0))
                .font_weight(gpui::FontWeight(600.0))
                .text_color(theme::EMERALD_600)
                .child("已完成")
                .into_any_element()
        } else if unlocked {
            icon_sized(Icon::ChevronRight, px(15.0))
                .text_color(theme::SLATE_400)
                .into_any_element()
        } else {
            div()
                .text_size(px(11.0))
                .text_color(theme::SLATE_400)
                .child("未解锁")
                .into_any_element()
        };

        let num = step.num;
        let on_click = &self.on_click;

        div()
            .id(SharedString::from(format!("step-{num}")))
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(12.0))
            .py(px(9.0))
            .rounded(theme::RADIUS_LG)
            .when(dimmed, |el| el.opacity(0.5))
            .when(!dimmed && !active, |el| {
                el.cursor_pointer().hover(|st| st.bg(theme::SLATE_50))
            })
            .when(!unlocked, |el| el.cursor_default())
            .child(
                div()
                    .size(px(38.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.0))
                    .bg(tile_bg)
                    .text_color(tile_fg)
                    .when(done, |el| el.border_1().border_color(theme::EMERALD_200))
                    .when(active, |el| el.shadow(theme::shadow_step_active()))
                    .child(tile_icon),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight(if active { 700.0 } else { 600.0 }))
                            .text_color(title_color)
                            .child(step.label),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .text_size(px(11.0))
                            .text_color(desc_color)
                            .truncate()
                            .child(step.desc),
                    ),
            )
            .child(status)
            .when(unlocked, |el| {
                let on_click = on_click.clone();
                el.on_click(move |e, window, cx| on_click(num, e, window, cx))
            })
    }
}

/// 侧栏容器(SPEC 1.6 aside):白底、右边框、padding 20px 14px、纵向滚动。
///
/// 宽度源值为 `clamp(210px, 22vw, 250px)`。gpui 无 vw/clamp 长度,取区间中值
/// 230px 近似(1100px 设计窗口下 22vw=242,最小 900px 窗口下被夹为 210;
/// 固定 230 的最大偏差 ±12px,已记录 KNOWN_DIFFERENCES)。
pub fn step_nav_aside() -> gpui::Stateful<gpui::Div> {
    div()
        .id("step-nav-scroll")
        .flex()
        .flex_col()
        .flex_none()
        .w(px(230.0))
        .bg(theme::BG_SURFACE)
        .border_r_1()
        .border_color(theme::BORDER_SUBTLE)
        .px(px(14.0))
        .py(px(20.0))
        .overflow_y_scroll()
}

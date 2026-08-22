//! 模态框(SOURCE_SPEC 2.8 Modal)与确认弹窗(SPEC 2.9 ConfirmModal)。
//!
//! 实现为 `deferred(...)` 全屏遮罩 + 居中卡片(gpui 官方 popover 模式的模态同构)。
//! - 遮罩:`--bg-overlay` rgba(15,23,42,0.55),点击空白处触发关闭
//!   (源规则:mousedown 且 target === 遮罩;gpui 近似:遮罩层注册 on_mouse_down,
//!   卡片容器再用一个空 on_mouse_down 吞掉事件,效果等价)
//! - 内容:白底、圆角 16、边框 subtle、阴影 xl、scaleUp 入场动画
//! - ConfirmModal:**Escape → 取消、Enter → 确认为自绘实现**(gpui-component 的
//!   Modal 无此默认键绑定):卡片 `track_focus` 一个由调用方持有并聚焦的
//!   FocusHandle,on_key_down 在冒泡阶段捕获 escape/enter。
#![allow(dead_code)]

use std::time::Duration;


use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt, AnyElement, App, FocusHandle, KeyDownEvent, MouseDownEvent,
    MouseButton, Pixels, RenderOnce, SharedString, Window, deferred, div, px,
};

use crate::ui::theme;
use crate::ui::components::{Button, ButtonVariant};
use crate::ui::{Icon, icon_sized};

// ── Modal(通用,SPEC 2.8)────────────────────────────────────────────────────

type CloseHandler = Box<dyn Fn(&mut Window, &mut App) + 'static>;
type KeyHandler = Box<dyn Fn(&KeyDownEvent, &mut Window, &mut App) + 'static>;

#[derive(gpui::IntoElement)]
pub struct Modal {
    title: SharedString,
    /// 标题左侧图标(如 DirPicker 弹窗的 FolderOpenIcon 18 色 amber-700,可选)
    title_icon: Option<(Icon, Pixels)>,
    width: Pixels,
    footer: Option<AnyElement>,
    children: Vec<AnyElement>,
    on_close: CloseHandler,
    /// 点击遮罩是否可关闭(默认 true;loading 场景应设 false)
    close_on_overlay: bool,
    /// 键盘监听(Escape/Enter 等,DirPicker 弹窗用);调用方负责在打开时聚焦该句柄
    focus_handle: Option<FocusHandle>,
    on_key: Option<KeyHandler>,
}

impl Modal {
    pub fn new(
        title: impl Into<SharedString>,
        on_close: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            title_icon: None,
            width: px(520.0),
            footer: None,
            children: Vec::new(),
            on_close: Box::new(on_close),
            close_on_overlay: true,
            focus_handle: None,
            on_key: None,
        }
    }

    pub fn title_icon(mut self, icon: Icon, size: Pixels) -> Self {
        self.title_icon = Some((icon, size));
        self
    }

    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    pub fn close_on_overlay(mut self, yes: bool) -> Self {
        self.close_on_overlay = yes;
        self
    }

    /// 传入聚焦句柄 + 键盘回调(打开弹窗时调用方应 `handle.focus(window)`)。
    pub fn key_handler(
        mut self,
        focus: FocusHandle,
        f: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.focus_handle = Some(focus);
        self.on_key = Some(Box::new(f));
        self
    }
}

impl ParentElement for Modal {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl RenderOnce for Modal {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let close_on_overlay = self.close_on_overlay;

        // 头部:padding 16px 20px、下边框 subtle、标题 16/600/slate-900、
        // 右侧关闭按钮(ghost、padding 6、圆角 6、slate-400、XIcon 18)
        let mut header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .px(px(20.0))
            .py(px(16.0))
            .border_b_1()
            .border_color(theme::BORDER_SUBTLE);
        let mut title_row = div().flex().items_center().gap(px(8.0));
        if let Some((icon, size)) = self.title_icon {
            title_row = title_row.child(icon_sized(icon, size).text_color(theme::AMBER_700));
        }
        title_row = title_row.child(
            div()
                .text_size(px(16.0))
                .font_weight(gpui::FontWeight(600.0))
                .text_color(theme::SLATE_900)
                .child(self.title.clone()),
        );
        // on_close 分发进两处闭包(头部关闭按钮 + 遮罩点击),Rc 共享
        let on_close_shared = std::rc::Rc::new(self.on_close);
        let on_close_btn = {
            let on_close = on_close_shared.clone();
            div()
                .id("modal-close")
                .flex()
                .items_center()
                .justify_center()
                .p(px(6.0))
                .rounded(theme::RADIUS_SM)
                .text_color(theme::SLATE_400)
                .cursor_pointer()
                .hover(|st| st.bg(theme::SLATE_100))
                .child(icon_sized(Icon::X, px(18.0)))
                .on_click(move |_, window, cx| on_close(window, cx))
        };
        header = header.child(title_row).child(on_close_btn);

        // 卡片:width prop、maxHeight 86vh 近似(取 620px)、圆角 16、
        // 边框 subtle、阴影 xl、scaleUp 200ms
        let mut card = div()
            .flex()
            .flex_col()
            .w(self.width)
            .max_h(px(620.0))
            .bg(theme::BG_SURFACE)
            .rounded(theme::RADIUS_XL)
            .border_1()
            .border_color(theme::BORDER_SUBTLE)
            .shadow(theme::shadow_xl())
            .overflow_hidden()
            .child(header)
            .child(
                div()
                    .id("modal-body")
                    .flex_1()
                    .overflow_y_scroll()
                    .px(px(20.0))
                    .py(px(18.0))
                    .children(self.children),
            );
        if let Some(footer) = self.footer {
            // 底部:padding 14px 20px、上边框 subtle、背景 slate-50、右对齐
            card = card.child(
                div()
                    .flex()
                    .justify_end()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(20.0))
                    .py(px(14.0))
                    .border_t_1()
                    .border_color(theme::BORDER_SUBTLE)
                    .bg(theme::SLATE_50)
                    .child(footer),
            );
        }
        let card: AnyElement =
            if let (Some(focus), Some(on_key)) = (self.focus_handle.clone(), self.on_key) {
                div()
                    .id("modal-key")
                    .track_focus(&focus)
                    .on_key_down(move |e, window, cx| on_key(e, window, cx))
                    .child(card)
                    .into_any_element()
            } else {
                card.into_any_element()
            };
        // 入场动画(源 scaleUp;gpui div 无 transform,退化为 opacity fadeIn,已知差异)
        let card = div()
            .id("modal-anim")
            .child(card)
            .with_animation(
                "modal-scale-up",
                Animation::new(Duration::from_millis(theme::DURATION_MODAL_SCALE_MS))
                    .with_easing(theme::ease_scale_up),
                |el, delta| el.opacity(delta),
            );

        // 遮罩:全屏、bg-overlay、居中、padding 20
        let overlay = div()
            .id("modal-overlay")
            .absolute()
            .size_full()
            .top_0()
            .left_0()
            .flex()
            .items_center()
            .justify_center()
            .p(px(20.0))
            .bg(theme::BG_OVERLAY)
            .on_mouse_down(MouseButton::Left, {
                let on_close = on_close_shared;
                move |_: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                    if close_on_overlay {
                        on_close(window, cx);
                    }
                }
            })
            .child(
                // 卡片捕获自身 mousedown,阻止“点内容 = 点遮罩”误关
                div()
                    .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _: &mut Window, _: &mut App| {})
                    .child(card),
            );

        deferred(overlay)
    }
}

// ── ConfirmModal(SPEC 2.9)──────────────────────────────────────────────────

/// 语气配置(SPEC 2.9 表):(图标, 图标色, 徽章底, 徽章边框, 确认按钮变体)
fn tone_config(tone: ConfirmTone) -> (Icon, gpui::Rgba, gpui::Rgba, gpui::Rgba, ButtonVariant) {
    match tone {
        ConfirmTone::Warning => (
            Icon::AlertTriangle,
            theme::AMBER_800,
            theme::AMBER_100,
            theme::AMBER_300,
            ButtonVariant::Primary,
        ),
        ConfirmTone::Danger => (
            Icon::AlertCircle,
            theme::ROSE_600,
            theme::ROSE_100,
            theme::ROSE_200,
            ButtonVariant::Danger,
        ),
        ConfirmTone::Info => (
            Icon::Info,
            theme::SKY_600,
            theme::SKY_100,
            theme::SKY_200,
            ButtonVariant::Primary,
        ),
        ConfirmTone::Primary => (
            Icon::CheckCircle,
            theme::AMBER_800,
            theme::AMBER_100,
            theme::AMBER_300,
            ButtonVariant::Primary,
        ),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConfirmTone {
    #[default]
    Warning,
    Danger,
    Info,
    Primary,
}

/// ConfirmOptions(对应源 useConfirm 的 options;字段与默认值照抄 SPEC 2.9)。
#[derive(Clone)]
pub struct ConfirmOptions {
    pub title: Option<SharedString>,
    pub message: SharedString,
    pub description: Option<SharedString>,
    pub tip: Option<SharedString>,
    pub confirm_text: SharedString,
    pub cancel_text: SharedString,
    pub tone: ConfirmTone,
    pub width: Pixels,
}

impl ConfirmOptions {
    pub fn new(message: impl Into<SharedString>) -> Self {
        Self {
            title: None,
            message: message.into(),
            description: None,
            tip: None,
            confirm_text: "确定".into(),
            cancel_text: "取消".into(),
            tone: ConfirmTone::Warning,
            width: px(460.0),
        }
    }

    pub fn title(mut self, v: impl Into<SharedString>) -> Self {
        self.title = Some(v.into());
        self
    }
    pub fn description(mut self, v: impl Into<SharedString>) -> Self {
        self.description = Some(v.into());
        self
    }
    pub fn tip(mut self, v: impl Into<SharedString>) -> Self {
        self.tip = Some(v.into());
        self
    }
    pub fn confirm_text(mut self, v: impl Into<SharedString>) -> Self {
        self.confirm_text = v.into();
        self
    }
    pub fn cancel_text(mut self, v: impl Into<SharedString>) -> Self {
        self.cancel_text = v.into();
        self
    }
    pub fn tone(mut self, v: ConfirmTone) -> Self {
        self.tone = v;
        self
    }
    pub fn width(mut self, v: Pixels) -> Self {
        self.width = v;
        self
    }
}

type ConfirmHandler = Box<dyn Fn(bool, &mut Window, &mut App) + 'static>;

/// 确认弹窗:遮罩(点击/Escape → 取消)+ 卡片 + 图标药丸 + tip 横幅 + 底部按钮。
/// 确认按钮 autoFocus(= 打开时聚焦 `focus_handle`)→ Enter 触发确认。
///
/// `on_result(ok)`:true = 确认、false = 取消(含 Escape/遮罩点击)。
#[derive(gpui::IntoElement)]
pub struct ConfirmModal {
    options: ConfirmOptions,
    focus_handle: FocusHandle,
    on_result: ConfirmHandler,
    loading: bool,
}

impl ConfirmModal {
    pub fn new(
        options: ConfirmOptions,
        focus_handle: FocusHandle,
        on_result: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            options,
            focus_handle,
            on_result: Box::new(on_result),
            loading: false,
        }
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }
}

impl RenderOnce for ConfirmModal {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (tone_icon, tone_icon_color, pill_bg, pill_border, confirm_variant) =
            tone_config(self.options.tone);
        let loading = self.loading;
        // on_result 需要分发进 4 处闭包(取消按钮/确认按钮/键盘/遮罩),包 Rc 共享
        let on_result = std::rc::Rc::new(self.on_result);
        let on_cancel = {
            let r = on_result.clone();
            move |w: &mut Window, cx: &mut App| r(false, w, cx)
        };
        let on_confirm = {
            let r = on_result.clone();
            move |w: &mut Window, cx: &mut App| r(true, w, cx)
        };
        let on_key = {
            let r = on_result.clone();
            move |event: &KeyDownEvent, w: &mut Window, cx: &mut App| match event.keystroke.key.as_str() {
                "escape" => r(false, w, cx),
                "enter" if !loading => r(true, w, cx),
                _ => {}
            }
        };
        let on_overlay = {
            let r = on_result.clone();
            move |_: &MouseDownEvent, w: &mut Window, cx: &mut App| {
                if !loading {
                    r(false, w, cx);
                }
            }
        };

        // 正文区:padding 22px 24px 18px;左 40×40 语气图标徽章(圆角 12)+ 右内容
        let body = div()
            .flex()
            .items_start()
            .gap(px(14.0))
            .pl(px(24.0))
            .pr(px(24.0))
            .pt(px(22.0))
            .pb(px(18.0))
            .child(
                div()
                    .size(px(40.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.0))
                    .bg(pill_bg)
                    .border_1()
                    .border_color(pill_border)
                    .child(icon_sized(tone_icon, px(20.0)).text_color(tone_icon_color)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w(px(0.0))
                    .flex_1()
                    .when_some(self.options.title, |el, title| {
                        // 标题:15.5 / 700 / slate-900 / letterSpacing -0.01em / marginBottom 6
                        el.child(
                            div()
                                .mb(px(6.0))
                                .text_size(px(15.5))
                                .font_weight(gpui::FontWeight(700.0))
                                .text_color(theme::SLATE_900)
                                .child(title),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(13.5))
                            .line_height(gpui::relative(1.55))
                            .font_weight(gpui::FontWeight(500.0))
                            .text_color(theme::SLATE_700)
                            .child(self.options.message.clone()),
                    )
                    .when_some(self.options.description, |el, desc| {
                        el.child(
                            div()
                                .mt(px(6.0))
                                .text_size(px(12.5))
                                .line_height(gpui::relative(1.5))
                                .text_color(theme::SLATE_500)
                                .child(desc),
                        )
                    })
                    .when_some(self.options.tip, |el, tip| {
                        // tip 横幅:marginTop 14、padding 10px 14px、amber-50 底、
                        // amber-200 边框、圆角 8、字号 12、amber-900 文字、
                        // 前置 InfoIcon 15 色 amber-700
                        el.child(
                            div()
                                .mt(px(14.0))
                                .flex()
                                .items_start()
                                .gap(px(8.0))
                                .px(px(14.0))
                                .py(px(10.0))
                                .bg(theme::AMBER_50)
                                .border_1()
                                .border_color(theme::AMBER_200)
                                .rounded(theme::RADIUS_MD)
                                .child(icon_sized(Icon::Info, px(15.0)).text_color(theme::AMBER_700))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .line_height(gpui::relative(1.5))
                                        .text_color(theme::AMBER_900)
                                        .child(tip),
                                ),
                        )
                    }),
            );

        // 底部:padding 12px 20px、slate-50 底、上边框、右对齐;
        // 取消 secondary minWidth 76、确认(变体随 tone)minWidth 88
        let footer = div()
            .flex()
            .justify_end()
            .items_center()
            .gap(px(10.0))
            .px(px(20.0))
            .py(px(12.0))
            .bg(theme::SLATE_50)
            .border_t_1()
            .border_color(theme::BORDER_SUBTLE)
            .child(
                Button::new("confirm-cancel")
                    .label(self.options.cancel_text.clone())
                    .min_w(px(76.0))
                    .on_click(move |_, w, cx| on_cancel(w, cx)),
            )
            .child(
                Button::new("confirm-ok")
                    .label(self.options.confirm_text.clone())
                    .variant(confirm_variant)
                    .min_w(px(88.0))
                    .loading(loading)
                    .on_click(move |_, w, cx| on_confirm(w, cx)),
            );

        // 卡片:focus 句柄接住键盘(escape/enter),入场动画(源 scaleUp → 退化 opacity)
        let card = div()
            .id("confirm-card")
            .track_focus(&self.focus_handle)
            .on_key_down(on_key)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(self.options.width)
                    .bg(theme::BG_SURFACE)
                    .rounded(theme::RADIUS_XL)
                    .border_1()
                    .border_color(theme::BORDER_SUBTLE)
                    .shadow(theme::shadow_confirm_modal())
                    .overflow_hidden()
                    .child(body)
                    .child(footer),
            )
            .with_animation(
                "confirm-scale-up",
                Animation::new(Duration::from_millis(theme::DURATION_CONFIRM_SCALE_MS))
                    .with_easing(theme::ease_scale_up),
                |el, delta| el.opacity(delta),
            );

        // 遮罩:loading 时点击不关闭
        let overlay = div()
            .id("confirm-overlay")
            .absolute()
            .size_full()
            .top_0()
            .left_0()
            .flex()
            .items_center()
            .justify_center()
            .p(px(20.0))
            .bg(theme::BG_OVERLAY)
            .on_mouse_down(MouseButton::Left, on_overlay)
            .child(
                div()
                    .on_mouse_down(
                        MouseButton::Left,
                        |_: &MouseDownEvent, _: &mut Window, _: &mut App| {},
                    )
                    .child(card),
            );

        deferred(overlay)
    }
}

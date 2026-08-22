//! 应用外壳(SOURCE_SPEC 第 1 章):顶栏 + 左步骤栏 + 右工作区 + 状态机 + 重置确认。
//!
//! ## 状态架构(给后续页面 agent 的约定)
//!
//! - 根实体为 [`AppShell`](实现 Render),持有全部向导状态:
//!   `current_step` / `max_unlocked_step` / 三个页面结构体 / 待挂载的 Modal 状态;
//! - **页面是普通 struct(非独立 Entity)**,字段直接挂在 [`AppShell`] 上;
//!   页面内部需要高交互控件时,持有对应 `Entity`(如 `Entity<DirPickerState>`、
//!   `Entity<InputState>`),在页面构造函数里 `cx.new` + `cx.subscribe` 建立
//!   事件回路(订阅句柄 `Subscription` 存进 AppShell,随 reset 一起丢弃重建);
//! - `reset_key` 语义:源项目通过 React `key` 强制重挂载三页清空内部状态;
//!   GPUI 版 [`AppShell::reset`] 直接**重建页面结构体**(等价重挂载),并归位
//!   step/max_unlocked/清理 taskId;
//! - "三个页面始终挂载"的源语义(仅 display 切换、保留状态)由"struct 字段
//!   常驻 + render 按 current_step 切换"天然满足。

use gpui::prelude::*;
use gpui::{Context, Entity, FocusHandle, Subscription, Window, div, px};

use crate::ui::components::{
    BadgeVariant, Button, ButtonSize, ButtonVariant, ConfirmModal, ConfirmOptions, ConfirmTone,
    StepNav, badge, card, step_nav_aside,
};
use crate::ui::dir_picker::{DirPickerEvent, DirPickerState, render_dir_picker};
use crate::ui::theme;
use crate::ui::{Icon, icon_16};

// ── 页面占位(后续页面 agent 在此扩展)──────────────────────────────────────

/// 步骤 1:扫描文件(数据结构占位)。页面 agent 参考 SPEC 4.1。
pub struct ScanPage {
    /// 源目录选择(值 = `dir.read(cx).value()`)
    pub dir: Entity<DirPickerState>,
    // TODO(页面 agent):recursive/loading/error/files/hasScanned/filter* 等
}

impl ScanPage {
    fn new(window: &mut Window, cx: &mut Context<AppShell>) -> Self {
        let dir = cx.new(|cx| {
            DirPickerState::new("例如 D:\\Music 或 /Users/me/Music", window, cx)
        });
        Self { dir }
    }
}

/// 步骤 2:模板预览(数据结构占位)。页面 agent 参考 SPEC 4.2。
pub struct PreviewPage {
    /// 目标目录选择
    pub dir: Entity<DirPickerState>,
    // TODO(页面 agent):template/targetDir/mode/mappings/directoryTree 等
}

impl PreviewPage {
    fn new(window: &mut Window, cx: &mut Context<AppShell>) -> Self {
        let dir = cx.new(|cx| {
            let mut s = DirPickerState::new("留空则整理到源目录", window, cx);
            s.label = Some("目标目录".into());
            s
        });
        Self { dir }
    }
}

/// 步骤 3:执行整理(占位)。页面 agent 参考 SPEC 4.3(轮询/日志/进度)。
pub struct ProgressPage {
    // TODO(页面 agent):progress/started/log/done/errMsg/taskId(持久化)
}

impl ProgressPage {
    fn new() -> Self {
        Self {}
    }

    /// 是否存在进行中/未完成的整理任务(退出确认用,SPEC 1.5)。
    pub fn has_running_task(&self) -> bool {
        false // TODO(页面 agent):taskId 非空时 true
    }
}

// ── 确认弹窗状态 ─────────────────────────────────────────────────────────────

enum ConfirmAction {
    /// 顶栏"重置"(SPEC 1.4)
    Reset,
    /// 窗口关闭(SPEC 1.5;description/tip 视任务状态两变体)
    Exit,
}

struct PendingConfirm {
    options: ConfirmOptions,
    action: ConfirmAction,
}

// ── 根实体 ───────────────────────────────────────────────────────────────────

pub struct AppShell {
    /// 当前步骤 1|2|3
    current_step: usize,
    /// 已解锁的最大步骤 1|2|3(点击规则:只能访问 ≤ 此值,SPEC 1.6/1.7)
    max_unlocked_step: usize,
    /// 重置计数(源 resetKey;重建页面即等价重挂载,计数仅供诊断/动画 key)
    reset_key: u32,

    pub scan: ScanPage,
    pub preview: PreviewPage,
    pub progress: ProgressPage,

    /// 待挂载的确认弹窗(单例:重置/退出)
    confirm: Option<PendingConfirm>,
    confirm_focus: FocusHandle,
    /// 退出已确认(允许本次关窗)
    exit_confirmed: bool,

    _subs: Vec<Subscription>,
}

impl AppShell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let confirm_focus = cx.focus_handle();

        let mut shell = Self {
            current_step: 1,
            max_unlocked_step: 1,
            reset_key: 0,
            scan: ScanPage::new(window, cx),
            preview: PreviewPage::new(window, cx),
            progress: ProgressPage::new(),
            confirm: None,
            confirm_focus,
            exit_confirmed: false,
            _subs: Vec::new(),
        };

        // DirPicker 事件回路(占位;页面 agent 接手后补充 SPEC 4.1.8 的
        // "输入变更效应"与 Enter 快捷扫描、SPEC 4.2.3 的表单变更作废预览)
        let scan_dir = shell.scan.dir.clone();
        shell._subs.push(cx.subscribe(
            &scan_dir,
            |_this, _entity, ev: &DirPickerEvent, cx| match ev {
                DirPickerEvent::Changed(_) => cx.notify(),
                DirPickerEvent::Enter => {}
            },
        ));
        let preview_dir = shell.preview.dir.clone();
        shell._subs.push(cx.subscribe(
            &preview_dir,
            |_this, _entity, ev: &DirPickerEvent, cx| {
                if let DirPickerEvent::Changed(_) = ev {
                    cx.notify();
                }
            },
        ));

        shell
    }

    // ── 步骤状态机(SPEC 1.7)───────────────────────────────────────────────

    fn go_to_step(&mut self, step: usize, cx: &mut Context<Self>) {
        if step <= self.max_unlocked_step {
            self.current_step = step;
            cx.notify();
        }
    }

    /// 全量重置(SPEC 1.4 确认后 / SPEC 4.3.5 "完成并开启新任务"直接调用)。
    /// 语义:回步骤 1、max_unlocked=1、清空页面数据(重建页面结构体)、清 taskId。
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.current_step = 1;
        self.max_unlocked_step = 1;
        self.reset_key += 1;
        // 重建页面 = 源 resetKey 强制重挂载(内部状态与订阅全部丢弃重建)
        self.scan = ScanPage::new(window, cx);
        self.preview = PreviewPage::new(window, cx);
        self.progress = ProgressPage::new();
        // TODO(页面 agent):taskId 清除(含持久化)
        cx.notify();
    }

    // ── 确认弹窗 ─────────────────────────────────────────────────────────────

    fn open_reset_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let options = ConfirmOptions::new("确定要清空当前的扫描结果、整理模板配置并重新开始吗?")
            .title("确认重置全部数据?")
            .tip("若当前有正在后台执行的文件整理任务,重置将断开界面追踪。")
            .confirm_text("确认重置")
            .cancel_text("取消")
            .tone(ConfirmTone::Warning);
        self.confirm = Some(PendingConfirm {
            options,
            action: ConfirmAction::Reset,
        });
        self.confirm_focus.focus(window);
        cx.notify();
    }

    fn open_exit_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (description, tip) = if self.progress.has_running_task() {
            (
                Some("当前有正在进行或未完成的文件整理任务,退出将中断处理。"),
                Some("建议等待任务整理完成后再退出应用。"),
            )
        } else {
            (Some("退出后当前未保存的配置与扫描缓存将被清除。"), None)
        };
        let mut options = ConfirmOptions::new("确定要退出 Tag2Folders 吗?")
            .title("确认退出应用?")
            .confirm_text("确认退出")
            .cancel_text("取消")
            .tone(ConfirmTone::Warning);
        if let Some(d) = description {
            options = options.description(d);
        }
        if let Some(t) = tip {
            options = options.tip(t);
        }
        self.confirm = Some(PendingConfirm {
            options,
            action: ConfirmAction::Exit,
        });
        self.confirm_focus.focus(window);
        cx.notify();
    }

    fn handle_confirm(&mut self, ok: bool, window: &mut Window, cx: &mut Context<Self>) {
        let pending = match self.confirm.take() {
            Some(p) => p,
            None => return,
        };
        if ok {
            match pending.action {
                ConfirmAction::Reset => self.reset(window, cx),
                ConfirmAction::Exit => {
                    self.exit_confirmed = true;
                    cx.quit();
                }
            }
        }
        // 取消:不做任何事
        cx.notify();
    }

    // ── 渲染 ─────────────────────────────────────────────────────────────────

    fn render_header(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(58.0))
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            // 源 padding 0 clamp(16px, 3vw, 32px) → 取中值 24(已知差异)
            .px(px(24.0))
            .bg(theme::BG_SURFACE)
            .border_b_1()
            .border_color(theme::BORDER_SUBTLE)
            // 品牌区:gap 12
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .min_w(px(0.0))
                    .child(
                        div()
                            .size(px(34.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(10.0))
                            .bg(theme::AMBER_500)
                            .text_color(theme::SLATE_800)
                            .shadow(theme::shadow_brand_tile())
                            .child(icon_16(Icon::Tag).size(px(18.0))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_size(px(15.5))
                                    .font_weight(gpui::FontWeight(700.0))
                                    .text_color(theme::SLATE_900)
                                    .child("Tag2Folders"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(theme::SLATE_500)
                                    .truncate()
                                    .child("音频文件智能整理 · 扫描 → 预览 → 执行"),
                            ),
                    ),
            )
            // 右侧操作区:gap 10
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(10.0))
                    // 版本徽章:badge-amber + padding 4px 10px + fontSize 11.5
                    .child(
                        badge(BadgeVariant::Amber)
                            .px(px(10.0))
                            .py(px(4.0))
                            .text_size(px(11.5))
                            .child("v2.0.1"),
                    )
                    // 重置按钮:ghost sm + RefreshIcon 14(title 提示见已知差异)
                    .child(
                        Button::new("header-reset")
                            .label("重置")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .icon(Icon::Refresh, px(14.0))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_reset_confirm(window, cx);
                            })),
                    ),
            )
    }

    /// 当前页渲染。占位卡片 + 已接通的 DirPicker(页面 agent 替换为真实页面)。
    fn render_page(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let app: &mut gpui::App = cx;
        match self.current_step {
            1 => card()
                .title("扫描文件")
                .subtitle("选择包含音频文件的文件夹,扫描并读取标签信息")
                .child(render_dir_picker(&self.scan.dir, window, app))
                .child(
                    div()
                        .mt(px(14.0))
                        .text_size(px(13.0))
                        .text_color(theme::SLATE_500)
                        .child("扫描页占位 —— 由页面 agent 实现(SPEC 4.1)"),
                )
                .into_any_element(),
            2 => card()
                .title("整理配置")
                .subtitle("设置目标目录与命名模板,点击占位符即可插入")
                .child(render_dir_picker(&self.preview.dir, window, app))
                .child(
                    div()
                        .mt(px(14.0))
                        .text_size(px(13.0))
                        .text_color(theme::SLATE_500)
                        .child("预览页占位 —— 由页面 agent 实现(SPEC 4.2)"),
                )
                .into_any_element(),
            _ => card()
                .title("任务概览")
                .subtitle("整理任务的模式、目标与待处理数量")
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(theme::SLATE_500)
                        .child("进度页占位 —— 由页面 agent 实现(SPEC 4.3)"),
                )
                .into_any_element(),
        }
    }
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 根容器:100vh、纵向 flex、bg-app(SPEC 1.2)
        let shell = div()
            .id("app-shell")
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::BG_APP)
            .text_color(theme::TEXT_PRIMARY)
            .font_family(theme::FONT_SANS)
            .line_height(gpui::relative(theme::LINE_HEIGHT_BASE))
            .text_size(theme::FONT_SIZE_BASE)
            // 顶栏
            .child(self.render_header(window, cx))
            // 中段:aside + main(SPEC 1.2)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(step_nav_aside().child({
                        // 步骤点击:仅 ≤ max_unlocked_step 可达(SPEC 1.6)
                        let on_step = cx.listener(|this, step: &usize, _window, cx| {
                            this.go_to_step(*step, cx);
                        });
                        StepNav::new(
                            self.current_step,
                            self.max_unlocked_step,
                            move |step, _e, window, cx| on_step(&step, window, cx),
                        )
                    }))
                    .child(
                        // 右工作区:flex 1、内滚、padding clamp(16,2.5vw,32) → 24
                        div()
                            .id("workspace-scroll")
                            .flex()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_y_scroll()
                            .p(px(24.0))
                            .child(
                                div()
                                    .max_w(px(1080.0))
                                    .w_full()
                                    .mx_auto()
                                    .child(self.render_page(window, cx)),
                            ),
                    ),
            );

        // 确认弹窗(单例,deferred 遮罩盖在最上层)
        let confirm_el: gpui::AnyElement = match &self.confirm {
            Some(pending) => {
                let on_result = cx.listener(|this, ok: &bool, window, cx| {
                    this.handle_confirm(*ok, window, cx);
                });
                ConfirmModal::new(
                    pending.options.clone(),
                    self.confirm_focus.clone(),
                    move |ok, window, cx| on_result(&ok, window, cx),
                )
                .into_any_element()
            }
            None => div().into_any_element(),
        };

        div().child(shell).child(confirm_el)
    }
}

// ── 窗口关闭确认挂接 ─────────────────────────────────────────────────────────
//
// gpui 0.2.2 存在 `window.on_window_should_close(cx, f)`(返回 false 可阻止关闭),
// 因此源项目的关闭确认弹窗(SPEC 1.5)可以等价实现,不列入 KNOWN_DIFFERENCES。

impl AppShell {
    /// 在窗口根视图构建后调用(见 main.rs):注册"点关闭按钮先确认"。
    /// 确认 → `exit_confirmed = true` + `cx.quit()`;取消 → 不动作。
    pub fn register_close_guard(
        shell: &Entity<AppShell>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let weak = shell.downgrade();
        window.on_window_should_close(cx, move |window, cx: &mut gpui::App| {
            weak.update(cx, |this, cx| {
                if this.exit_confirmed {
                    true // 已确认,放行
                } else {
                    this.open_exit_confirm(window, cx);
                    false // 拦下本次关闭,等待确认
                }
            })
            .unwrap_or(true) // 实体已释放(不应发生)则放行
        });
    }
}

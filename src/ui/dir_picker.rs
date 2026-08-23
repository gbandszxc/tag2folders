//! DirPicker 目录选择组件。
//!
//! 结构:`Entity<DirPickerState>`(自含状态) + 每帧渲染函数。
//! - 主行:输入框(mono,左侧 FolderIcon、有值且未禁用时右侧清空按钮)+ 浏览按钮
//! - 浏览按钮:优先调 **gpui 原生目录对话框**(`prompt_for_paths`);
//!   打开失败时降级为**内置目录浏览模态**(browse_dirs 服务)
//! - 对外事件:`DirPickerEvent::Changed(path)` / `DirPickerEvent::Enter`
//!   (App/页面用 `cx.subscribe` 消费;ScanPage 的 Enter 快捷扫描靠后者)
//!
//! 页面持有 `Entity<DirPickerState>`,值直接读
//! `dp.read(cx).input.read(cx).value()`,无需在页面再存一份。
#![allow(dead_code)]

use std::time::Duration;


use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt, App, Context, Entity, EventEmitter, FocusHandle,
    SharedString, Subscription, Window, div, percentage, px, svg,
};
use gpui_component::input::{Input, InputState};
use tag2folders_lib::service::{self, DirEntry};

use crate::ui::theme;
use crate::ui::components::{Button, ButtonSize, ButtonVariant, Modal};
use crate::ui::service::{native_pick_directory, run_service_in};
use crate::ui::{Icon, icon_sized};

/// 对外事件。
#[derive(Debug, Clone)]
pub enum DirPickerEvent {
    /// 值变化(手输 / 清空 / 原生对话框选定 / 模态选定)
    Changed(String),
    /// 主输入框按下 Enter(源 onEnter;页面未传则无动作)
    Enter,
}

/// 降级目录浏览模态的状态。
struct BrowseState {
    open: bool,
    /// 当前浏览目录(browse_dirs 返回的 base_dir;'' = 根/家目录)
    current_path: String,
    entries: Vec<DirEntry>,
    loading: bool,
    /// 竞态 token:navigate 发起时 +1,回调比对丢弃过期响应
    token: u64,
}

pub struct DirPickerState {
    /// 主输入框(路径,mono)
    pub input: Entity<InputState>,
    /// 是否禁用整个组件
    pub disabled: bool,
    /// 可选 label
    pub label: Option<SharedString>,
    /// 可选错误文案
    pub error: Option<SharedString>,

    modal: BrowseState,
    /// 模态卡片键盘句柄(Escape 关闭)
    modal_focus: FocusHandle,
    /// 模态路径输入(mono, h 32;Enter 跳转 / Blur 重置回 currentPath)
    path_input: Entity<InputState>,
    /// 模态过滤输入(h 30)
    filter_input: Entity<InputState>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<DirPickerEvent> for DirPickerState {}

impl DirPickerState {
    /// `placeholder` 默认值由调用方给(约定默认 `请选择或输入目录路径...`)。
    pub fn new(
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
        });
        let path_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("输入路径后按 Enter 跳转...")
        });
        let filter_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("过滤当前目录下的子文件夹...")
        });
        let modal_focus = cx.focus_handle();

        let mut subs = Vec::new();
        // 主输入框:Change / PressEnter
        let input_for_sub = input.clone();
        subs.push(cx.subscribe(&input_for_sub, {
            let input_for_sub = input_for_sub.clone();
            move |_, _, ev: &gpui_component::input::InputEvent, cx| {
                match ev {
                    gpui_component::input::InputEvent::Change => {
                        let v = input_for_sub.read(cx).value().to_string();
                        cx.emit(DirPickerEvent::Changed(v));
                    }
                    gpui_component::input::InputEvent::PressEnter { .. } => {
                        cx.emit(DirPickerEvent::Enter);
                    }
                    _ => {}
                }
            }
        }));
        // 模态路径输入:Enter → navigate(输入框当前值);Blur → 重置回 currentPath
        subs.push(cx.subscribe_in(&path_input, window, |this, _, ev: &gpui_component::input::InputEvent, window, cx| {
            match ev {
                gpui_component::input::InputEvent::PressEnter { .. } => {
                    let target = this.path_input.read(cx).value().to_string();
                    this.navigate(target, window, cx);
                }
                gpui_component::input::InputEvent::Blur => {
                    let current = this.modal.current_path.clone();
                    let path_input = this.path_input.clone();
                    path_input.update(cx, |state, cx| state.set_value(current, window, cx));
                }
                _ => {}
            }
        }));

        Self {
            input,
            disabled: false,
            label: None,
            error: None,
            modal: BrowseState {
                open: false,
                current_path: String::new(),
                entries: Vec::new(),
                loading: false,
                token: 0,
            },
            modal_focus,
            path_input,
            filter_input,
            _subs: subs,
        }
    }

    /// 当前值(路径)。
    pub fn value(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    /// 编程设值(重置场景)。
    pub fn set_value(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let value: SharedString = value.to_string().into();
        let input = self.input.clone();
        input.update(cx, |state, cx| state.set_value(value, window, cx));
    }

    /// 清空(清空按钮,等价 onChange(''))。
    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_value("", window, cx);
    }

    // ── 浏览───────────────────────────────────────────────────────

    /// 浏览按钮点击:原生对话框优先,失败降级内置模态。
    pub fn browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        let receiver = native_pick_directory(cx);
        cx.spawn_in(window, async move |this, cx| {
            // 通道关闭视作打开失败
            let outcome: Result<Option<Vec<std::path::PathBuf>>, ()> =
                receiver.await.unwrap_or(Err(()));
            match outcome {
                Ok(Some(paths)) if !paths.is_empty() => {
                    let dir = paths[0].display().to_string();
                    let _ = this.update_in(cx, |state, window, cx| {
                        state.set_value(&dir, window, cx);
                    });
                }
                // 取消:不做事
                Ok(_) => {}
                // 打开失败 → 降级打开内置目录树模态
                Err(()) => {
                    let _ = this.update_in(cx, |state, window, cx| {
                        state.open_browse_modal(window, cx);
                    });
                }
            }
        })
        .detach();
    }

    /// 打开降级目录浏览模态:过滤重置 + navigate(value || '')。
    pub fn open_browse_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let filter_input = self.filter_input.clone();
        filter_input.update(cx, |state, cx| state.set_value("", window, cx));
        let start = self.value(cx);
        self.modal.open = true;
        self.modal_focus.focus(window);
        cx.notify();
        self.navigate(if start.is_empty() { String::new() } else { start }, window, cx);
    }

    /// 关闭模态。
    pub fn close_browse_modal(&mut self, cx: &mut Context<Self>) {
        self.modal.open = false;
        cx.notify();
    }

    /// navigate(path):loading → browse_dirs → 更新 current/entries;失败静默。
    pub fn navigate(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.modal.token += 1;
        let token = self.modal.token;
        self.modal.loading = true;
        cx.notify();
        run_service_in(
            window,
            cx,
            move || service::browse_dirs(path.clone()),
            move |this, result, window, cx| {
                if this.modal.token != token {
                    return; // 过期响应,丢弃
                }
                this.modal.loading = false;
                if let Ok(resp) = result {
                    this.modal.current_path = resp.base_dir.clone();
                    this.modal.entries = resp.entries;
                    // editingPath = base_dir(模态路径输入同步显示)
                    let path_input = this.path_input.clone();
                    let base = resp.base_dir.clone();
                    path_input.update(cx, |state, cx| state.set_value(base, window, cx));
                }
                cx.notify();
            },
        );
    }

    /// 模态"选择此目录":onChange(currentPath) + 关闭。
    fn confirm_browse_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.modal.current_path.clone();
        self.set_value(&path, window, cx);
        self.close_browse_modal(cx);
    }

    /// 过滤后的条目(name 小写子串匹配)。
    fn filtered_entries(&self, cx: &App) -> Vec<DirEntry> {
        let filter = self.filter_input.read(cx).value().to_lowercase();
        if filter.is_empty() {
            return self.modal.entries.clone();
        }
        self.modal
            .entries
            .iter()
            .filter(|e| e.name.to_lowercase().contains(&filter))
            .cloned()
            .collect()
    }
}

// ── 渲染 ────────────────────────────────────────────────────────────────────

/// 主行渲染(含可选 label / error)。
///
/// 键盘可达:清空按钮 `track_focus`(keyed state 句柄),Tab 循环可达,
/// 聚焦时 Enter/Space 由框架转发为 click,聚焦态文字色同 hover;
/// 浏览按钮由 Button 组件内置键盘焦点。
pub fn render_dir_picker(
    dp: &Entity<DirPickerState>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    // 集中提取 state 字段,尽早结束不可变借用(下方 use_keyed_state 需 &mut App)
    let (value, disabled, label, error, modal_open, input_entity) = {
        let state = dp.read(cx);
        (
            state.input.read(cx).value().to_string(),
            state.disabled,
            state.label.clone(),
            state.error.clone(),
            state.modal.open,
            state.input.clone(),
        )
    };
    let has_value = !value.is_empty();

    let icon_color = if has_value {
        theme::AMBER_600
    } else {
        theme::SLATE_400
    };

    let mut col = div().flex().flex_col().w_full();

    if let Some(label) = label {
        col = col.child(
            div()
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight(600.0))
                .text_color(theme::SLATE_700)
                .mb(px(6.0))
                .child(label),
        );
    }

    // 清空按钮的键盘焦点句柄(window 级 keyed state,按 id 持久化;
    // Tab 循环可达,聚焦时 Enter/Space 由框架转发为 click)
    let clear_focus = window
        .use_keyed_state("dir-clear", cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone();
    let clear_focused = clear_focus.is_focused(window);

    // 输入框:直接使用 Input 的 prefix 和 suffix，由 Input 内部 Flex 引擎自动垂直居中
    let input_field = {
        let dp2 = dp.clone();
        let mut input = Input::new(&input_entity);
        input.style().size.height = Some(px(38.0).into());
        input
            .flex_1()
            .min_w(px(0.0))
            .prefix(
                div()
                    .flex()
                    .items_center()
                    .pl(px(4.0))
                    .child(icon_sized(Icon::Folder, px(16.0)).text_color(icon_color)),
            )
            .when(has_value && !disabled, |this| {
                this.suffix(
                    div()
                        .id("dir-clear")
                        .flex()
                        .items_center()
                        .justify_center()
                        .p(px(2.0))
                        .rounded(theme::RADIUS_SM)
                        // 聚焦可见:文字/图标色提亮至 slate-600(同 hover)
                        .text_color(if clear_focused {
                            theme::SLATE_600
                        } else {
                            theme::SLATE_400
                        })
                        .cursor_pointer()
                        .hover(|st| st.text_color(theme::SLATE_600))
                        .track_focus(&clear_focus)
                        .child(icon_sized(Icon::X, px(14.0)))
                        .on_click(move |_, window, cx| {
                            dp2.update(cx, |state, cx| state.clear(window, cx));
                        }),
                )
            })
            .text_size(px(13.0))
            .font_family(theme::FONT_MONO)
            .disabled(disabled)
    };
    // 浏览按钮:secondary、h 38、px 16、weight 600、FolderOpenIcon 15 色 amber-700
    let browse_btn = {
        let dp2 = dp.clone();
        Button::new("dir-browse")
            .label("浏览...")
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Md)
            .h(px(38.0))
            .pad_x(px(16.0))
            .icon(Icon::FolderOpen, px(15.0))
            .disabled(disabled)
            .on_click(move |_, window, cx| {
                dp2.update(cx, |state, cx| state.browse(window, cx));
            })
    };

    let row = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(input_field)
        .child(browse_btn);
    col = col.child(row);

    if let Some(error) = error {
        col = col.child(
            div()
                .mt(px(4.0))
                .text_size(px(12.0))
                .text_color(theme::ROSE_600)
                .child(error),
        );
    }
    // 降级模态(打开时)
    let with_modal: gpui::AnyElement = if modal_open {
        render_browse_modal(dp, window, cx).into_any_element()
    } else {
        div().into_any_element()
    };

    div().child(col).child(with_modal)
}

/// 降级目录浏览模态。
///
/// 键盘可达:目录条目行 `track_focus`(keyed state 句柄,按 path 稳定 key 持久化,
/// 过滤后不漂移),Tab 循环可达,聚焦时 Enter/Space 由框架转发为 click,
/// 聚焦态 slate-100 底色;footer/主页/上一级按钮由 Button 组件内置键盘焦点。
fn render_browse_modal(
    dp: &Entity<DirPickerState>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    // 集中提取 state 字段,尽早结束不可变借用(循环内 use_keyed_state 需 &mut App)
    let (current_path, loading, entries, filter_text, at_root, path_input, filter_input, modal_focus) = {
        let state = dp.read(cx);
        let current_path = state.modal.current_path.clone();
        (
            current_path.clone(),
            state.modal.loading,
            state.filtered_entries(cx),
            state.filter_input.read(cx).value().to_string(),
            current_path.is_empty(),
            state.path_input.clone(),
            state.filter_input.clone(),
            state.modal_focus.clone(),
        )
    };

    // ── 路径导航行 ──
    let home_btn = {
        let dp2 = dp.clone();
        Button::new("browse-home")
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Sm)
            .icon(Icon::Home, px(14.0))
            .on_click(move |_, window, cx| {
                dp2.update(cx, |state, cx| state.navigate(String::new(), window, cx));
            })
    };
    let up_target = get_parent_path(&current_path);
    let up_btn = {
        let dp2 = dp.clone();
        Button::new("browse-up")
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Sm)
            .icon(Icon::ArrowUp, px(14.0))
            .disabled(at_root)
            .on_click(move |_, window, cx| {
                dp2.update(cx, |state, cx| state.navigate(up_target.clone(), window, cx));
            })
    };
    let nav_row = div()
        .flex()
        .gap(px(6.0))
        .child(home_btn)
        .child(up_btn)
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .child(
                    Input::new(&path_input)
                        .h(px(32.0))
                        .py(px(0.0))
                        .text_size(px(12.5))
                        .font_family(theme::FONT_MONO),
                ),
        );

    // ── 过滤输入(带左内嵌 SearchIcon)──
    let filter_row = div()
        .relative()
        .mt(px(10.0))
        .child(
            Input::new(&filter_input)
                .h(px(30.0))
                .py(px(0.0))
                .text_size(px(12.0))
                .pl(px(28.0))
                .bg(theme::SLATE_50),
        )
        .child(
            div()
                .absolute()
                .left(px(8.0))
                .top(px(0.0))
                .bottom(px(0.0))
                .flex()
                .items_center()
                .child(icon_sized(Icon::Search, px(13.0)).text_color(theme::SLATE_400)),
        );

    // ── 目录列表 ──
    let list: gpui::AnyElement = if loading {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .h(px(280.0))
            .text_size(px(13.0))
            // 文字用 slate-500 保证浅底对比度;旋转图标维持 slate-400
            .text_color(theme::SLATE_500)
            .child(
                svg()
                    .path(Icon::Refresh.path())
                    .size(px(16.0))
                    .text_color(theme::SLATE_400)
                    .with_animation(
                        "browse-loading-spin",
                        Animation::new(Duration::from_millis(theme::DURATION_SPIN_MS))
                            .repeat()
                            .with_easing(|t| t),
                        |el, delta| el.with_transformation(gpui::Transformation::rotate(percentage(delta))),
                    ),
            )
            .child("正在加载目录内容...")
            .into_any_element()
    } else if entries.is_empty() {
        let empty_text = if filter_text.is_empty() {
            "当前目录下无子文件夹"
        } else {
            "未找到匹配的子文件夹"
        };
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .h(px(280.0))
            .text_size(px(13.0))
            // 空态文字用 slate-500;Folder 图标(装饰)维持 slate-300
            .text_color(theme::SLATE_500)
            .child(icon_sized(Icon::Folder, px(24.0)).text_color(theme::SLATE_300))
            .child(empty_text)
            .into_any_element()
    } else {
        let mut list_el = div()
            .id("browse-entries")
            .h(px(280.0))
            .overflow_y_scroll()
            .bg(theme::BG_SURFACE)
            .border_1()
            .border_color(theme::BORDER_SUBTLE)
            .rounded(theme::RADIUS_MD);
        for (ix, entry) in entries.iter().enumerate() {
            let dp2 = dp.clone();
            let target = entry.path.clone();
            // 稳定 key:path 全局唯一,过滤后条目重排也不漂移(避免焦点/状态错位)
            let row_id = SharedString::from(format!("entry-{}", entry.path));
            // 键盘焦点句柄按同一稳定 key 持久化;Tab 可聚焦,Enter/Space 框架转发 click
            let focus_handle = window
                .use_keyed_state(row_id.clone(), cx, |_, cx| cx.focus_handle())
                .read(cx)
                .clone();
            let focused = focus_handle.is_focused(window);
            let row = div()
                .id(row_id)
                .flex()
                .items_center()
                .gap(px(10.0))
                .px(px(12.0))
                .py(px(8.0))
                .text_size(px(13.0))
                .text_color(theme::SLATE_800)
                .cursor_pointer()
                // 聚焦可见:slate-100 底色;悬浮时聚焦行维持该底色,其余行 slate-50
                .when(focused, |el| el.bg(theme::SLATE_100))
                .hover(move |st| {
                    if focused {
                        st.bg(theme::SLATE_100)
                    } else {
                        st.bg(theme::SLATE_50)
                    }
                })
                .track_focus(&focus_handle)
                .when(ix + 1 < entries.len(), |el| {
                    el.border_b_1().border_color(theme::SLATE_100)
                })
                .child(icon_sized(Icon::Folder, px(16.0)).text_color(theme::AMBER_500))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .font_weight(gpui::FontWeight(500.0))
                        .child(entry.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme::SLATE_500)
                        .child("进入 ›"),
                )
                .on_click(move |_, window, cx| {
                    dp2.update(cx, |state, cx| state.navigate(target.clone(), window, cx));
                });
            list_el = list_el.child(row);
        }
        list_el.into_any_element()
    };

    // ── 当前选择预览条 ──
    let preview = div()
        .mt(px(10.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(12.0))
        .py(px(8.0))
        .bg(theme::SLATE_50)
        .border_1()
        .border_color(theme::SLATE_200)
        .rounded(theme::RADIUS_SM)
        .text_size(px(12.0))
        .child(
            div()
                .font_weight(gpui::FontWeight(600.0))
                .text_color(theme::SLATE_500)
                .child("当前选择:"),
        )
        .child(if at_root {
            div().child("(根目录)").into_any_element()
        } else {
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .font_family(theme::FONT_MONO)
                .font_weight(gpui::FontWeight(500.0))
                .child(current_path.clone())
                .into_any_element()
        });

    // ── footer:左计数 + 取消 + 选择此目录 ──
    let footer = {
        let dp2 = dp.clone();
        let dp3 = dp.clone();
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme::SLATE_500)
                    .child(format!("共 {} 个子文件夹", entries.len())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        Button::new("browse-cancel")
                            .label("取消")
                            .variant(ButtonVariant::Ghost)
                            .on_click(move |_, _w, cx| {
                                dp2.update(cx, |state, cx| state.close_browse_modal(cx));
                            }),
                    )
                    .child(
                        Button::new("browse-confirm")
                            .label("选择此目录")
                            .variant(ButtonVariant::Primary)
                            .disabled(at_root)
                            .on_click(move |_, window, cx| {
                                dp3.update(cx, |state, cx| {
                                    state.confirm_browse_modal(window, cx)
                                });
                            }),
                    ),
            )
    };

    let dp_key = dp.clone();
    Modal::new("选择本地目录", move |_window, cx| {
        dp_key.update(cx, |state, cx| state.close_browse_modal(cx));
    })
    .title_icon(Icon::FolderOpen, px(18.0))
    .width(px(560.0))
    .footer(footer)
    .key_handler(modal_focus, {
        let dp2 = dp.clone();
        move |_e: &gpui::KeyDownEvent, _window, cx| {
            // Escape → 关闭模态(提升到卡片级,
            // 覆盖焦点在任意输入框时的 Escape)
            // 注:仅 escape 关闭;Enter 由路径输入的 PressEnter 事件处理
            if _e.keystroke.key == "escape" {
                dp2.update(cx, |state, cx| state.close_browse_modal(cx));
            }
        }
    })
    .child(nav_row)
    .child(filter_row)
    .child(list)
    .child(preview)
}

// ── getParentPath(路径拼接,跨平台分支)──────────────────────────────────────

pub fn get_parent_path(p: &str) -> String {
    if p.is_empty() {
        return String::new();
    }
    // Windows 盘根(^[A-Z]:[/\\]?$,忽略大小写)→ ''
    let bytes = p.as_bytes();
    if bytes.len() >= 2
        && bytes.len() <= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2..].iter().all(|c| *c == b'/' || *c == b'\\')
    {
        return String::new();
    }
    let normalized = p.replace('\\', "/");
    let trimmed = normalized.strip_suffix('/').unwrap_or(&normalized);
    match trimmed.rfind('/') {
        None => String::new(),
        Some(ix) => {
            let parent = &trimmed[..ix];
            // 若父为 `C:` 形式 → 返回 `C:\`
            if parent.len() == 2
                && parent.as_bytes()[0].is_ascii_alphabetic()
                && parent.as_bytes()[1] == b':'
            {
                format!("{parent}\\")
            } else {
                parent.to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::get_parent_path;

    /// getParentPath 各分支(路径拼接逻辑,纯字符串,跨平台一致)
    #[test]
    fn parent_path_branches() {
        assert_eq!(get_parent_path(""), "");
        // Windows 盘根 → ''
        assert_eq!(get_parent_path("C:\\"), "");
        assert_eq!(get_parent_path("c:/"), "");
        assert_eq!(get_parent_path("D:"), "");
        // POSIX:根 "/" → ''
        assert_eq!(get_parent_path("/"), "");
        // "/Users" 的父是 ''(顶层)
        assert_eq!(get_parent_path("/Users"), "");
        assert_eq!(get_parent_path("/Users/me"), "/Users");
        assert_eq!(get_parent_path("/Users/me/Music"), "/Users/me");
        // 尾部斜杠归一
        assert_eq!(get_parent_path("/Users/me/"), "/Users");
        // 反斜杠统一为 /:裸盘符 "C:" 形态补回 "C:\"(源 getParentPath 行为)
        assert_eq!(get_parent_path("C:\\Music"), "C:\\");
        // 其余父路径保持正斜杠(源逻辑仅对 ^[A-Z]:$ 特判)
        assert_eq!(get_parent_path("C:\\Music\\MP3"), "C:/Music");
        // 无分隔符 → ''
        assert_eq!(get_parent_path("Music"), "");
    }
}

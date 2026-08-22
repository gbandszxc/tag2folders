//! DirPicker 目录选择组件(SOURCE_SPEC 第 3 章)。
//!
//! 结构:`Entity<DirPickerState>`(自含状态) + 每帧渲染函数。
//! - 主行:输入框(mono,左侧 FolderIcon、有值且未禁用时右侧清空按钮)+ 浏览按钮
//! - 浏览按钮:优先调 **gpui 原生目录对话框**(`prompt_for_paths`);
//!   打开失败时降级为**内置目录浏览模态**(browse_dirs 服务,SPEC 3.3 逐项实现)
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
    /// 竞态 token:navigate 发起时 +1,回调比对丢弃过期响应(SPEC 3.3 navigate)
    token: u64,
}

pub struct DirPickerState {
    /// 主输入框(路径,mono)
    pub input: Entity<InputState>,
    /// 是否禁用整个组件
    pub disabled: bool,
    /// 可选 label(SPEC 3.1)
    pub label: Option<SharedString>,
    /// 可选错误文案(SPEC 3.1:fontSize 12、rose-600、marginTop 4)
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
    /// `placeholder` 默认值由调用方给(SPEC 3.1 默认 `请选择或输入目录路径...`)。
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
        // 模态路径输入:Enter → navigate(输入框当前值);Blur → 重置回 currentPath(SPEC 3.3)
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

    // ── 浏览(SPEC 3.2)───────────────────────────────────────────────────────

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

    /// 打开降级目录浏览模态:过滤重置 + navigate(value || '')(SPEC 3.3)。
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

    /// navigate(path):loading → browse_dirs → 更新 current/entries;失败静默(SPEC 3.3)。
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

    /// 过滤后的条目(name 小写子串匹配,SPEC 3.3)。
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
pub fn render_dir_picker(
    dp: &Entity<DirPickerState>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let state = dp.read(cx);
    let value = state.input.read(cx).value().to_string();
    let disabled = state.disabled;
    let has_value = !value.is_empty();

    let icon_color = if has_value {
        theme::AMBER_600
    } else {
        theme::SLATE_400
    };

    let mut col = div().flex().flex_col().w_full();

    if let Some(label) = state.label.clone() {
        col = col.child(
            div()
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight(600.0))
                .text_color(theme::SLATE_700)
                .mb(px(6.0))
                .child(label),
        );
    }

    // 输入框容器:relative + 左侧 FolderIcon 16 @ left 10 + 清空按钮 @ right 8
    // 先渲染 Input，再渲染 absolute 图标/清空，避免 Input 背景盖住图标
    let input_container = div()
        .relative()
        .flex_1()
        .min_w(px(0.0))
        .child(
            Input::new(&state.input)
                .h(px(38.0))
                .pl(px(34.0))
                .pr(if has_value && !disabled { px(32.0) } else { px(12.0) })
                .text_size(px(13.0))
                .font_family(theme::FONT_MONO)
                .disabled(disabled),
        )
        .child(
            div()
                .absolute()
                .left(px(10.0))
                .top(px(11.0))
                .child(icon_sized(Icon::Folder, px(16.0)).text_color(icon_color)),
        )
        .when(has_value && !disabled, |el| {
            let dp2 = dp.clone();
            el.child(
                div()
                    .id("dir-clear")
                    .absolute()
                    .right(px(8.0))
                    .top(px(10.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(4.0))
                    .rounded(theme::RADIUS_SM)
                    .text_color(theme::SLATE_400)
                    .cursor_pointer()
                    .hover(|st| st.text_color(theme::SLATE_600))
                    .child(icon_sized(Icon::X, px(14.0)).text_color(theme::SLATE_400))
                    .on_click(move |_, window, cx| {
                        dp2.update(cx, |state, cx| state.clear(window, cx));
                    }),
            )
        });

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
        .child(input_container)
        .child(browse_btn);
    col = col.child(row);

    if let Some(error) = state.error.clone() {
        col = col.child(
            div()
                .mt(px(4.0))
                .text_size(px(12.0))
                .text_color(theme::ROSE_600)
                .child(error),
        );
    }

    // 降级模态(打开时)
    let with_modal: gpui::AnyElement = if state.modal.open {
        render_browse_modal(dp, window, cx).into_any_element()
    } else {
        div().into_any_element()
    };

    div().child(col).child(with_modal)
}

/// 降级目录浏览模态(SPEC 3.3)。
fn render_browse_modal(
    dp: &Entity<DirPickerState>,
    _window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let state = dp.read(cx);
    let current_path = state.modal.current_path.clone();
    let loading = state.modal.loading;
    let entries = state.filtered_entries(cx);
    let filter_text = state.filter_input.read(cx).value().to_string();
    let at_root = current_path.is_empty();

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
                    Input::new(&state.path_input)
                        .h(px(32.0))
                        .text_size(px(12.5))
                        .font_family(theme::FONT_MONO),
                ),
        );

    // ── 过滤输入(带左内嵌 SearchIcon)──
    let filter_row = div()
        .relative()
        .mt(px(10.0))
        .child(
            div()
                .absolute()
                .left(px(8.0))
                .top(px(8.0))
                .child(icon_sized(Icon::Search, px(13.0)).text_color(theme::SLATE_400)),
        )
        .child(
            Input::new(&state.filter_input)
                .h(px(30.0))
                .text_size(px(12.0))
                .pl(px(28.0))
                .bg(theme::SLATE_50),
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
            .text_color(theme::SLATE_400)
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
            .text_color(theme::SLATE_400)
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
            let row = div()
                .id(SharedString::from(format!("entry-{ix}-{}", entry.name)))
                .flex()
                .items_center()
                .gap(px(10.0))
                .px(px(12.0))
                .py(px(8.0))
                .text_size(px(13.0))
                .text_color(theme::SLATE_800)
                .cursor_pointer()
                .hover(|st| st.bg(theme::SLATE_50))
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
                        .text_color(theme::SLATE_400)
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

    let modal_focus = state.modal_focus.clone();
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
            // Escape → 关闭模态(SPEC 3.3 路径输入 Escape;提升到卡片级,
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

// ── getParentPath(SPEC 3.4,逐分支照抄)──────────────────────────────────────

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

    /// SPEC 3.4 getParentPath 各分支(路径拼接逻辑,纯字符串,跨平台一致)
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

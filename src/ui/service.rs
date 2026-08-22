//! UI 侧服务调用辅助:把阻塞的服务层函数(`tag2folders_lib::service::*`)
//! 丢到后台线程执行,完成后回主线程更新实体并 `cx.notify()`。
//!
//! 这是**所有页面调用后端的统一入口**(后续页面 agent 直接复用,见
//! docs/UI_INTEGRATION.md):
//!
//! ```ignore
//! run_service(
//!     cx, // &mut Context<App>(或任何根实体 T)
//!     move || service::scan_directory(dir, Some(recursive)), // 阻塞工作(后台线程)
//!     |this, result, cx| {
//!         // 回到主线程:&mut App(实体)、Result<ScanResponse, ServiceError>、Context
//!         match result {
//!             Ok(resp) => { /* this.scan_state.files = resp.files; */ }
//!             Err(e) => { /* this.scan_state.error = Some(e.to_string()); */ }
//!         }
//!         cx.notify();
//!     },
//! );
//! ```
//!
//! 竞态防护(源项目的 token/abort 模式)需调用方自行实现:发起前递增
//! `request_token`,回调里比对 token 后再落地状态。
//!
//! 另外:服务函数本身要求 `Send + 'static`(返回值同样),这是后台线程的硬约束。

#![allow(dead_code)] // token 表/图标枚举/服务辅助为后续页面 agent 预留,当前未全部使用

use gpui::{Context, Window};

/// 在后台线程执行 `work`,完成后在主线程调用 `on_done(实体状态, 返回值, cx)`。
///
/// - `work` 与返回值 `R` 必须 `Send + 'static`(跨线程);
/// - `on_done` 在主线程执行,无需 Send;
/// - 任务随实体存活(WeakEntity 升级失败时静默丢弃,不会 panic)。
pub fn run_service<T, R>(
    cx: &mut Context<T>,
    work: impl FnOnce() -> R + Send + 'static,
    on_done: impl FnOnce(&mut T, R, &mut Context<T>) + 'static,
) where
    T: 'static,
    R: Send + 'static,
{
    let task = cx.background_executor().spawn(async move { work() });
    cx.spawn(async move |this, cx| {
        let result = task.await;
        let _ = this.update(cx, |state, cx| {
            on_done(state, result, cx);
            cx.notify();
        });
    })
    .detach();
}

/// [`run_service`] 的带窗口版本:回调额外拿到 `&mut Window`
/// (gpui-component 的 `InputState::set_value` 等需要窗口,DirPicker 用)。
pub fn run_service_in<T, R>(
    window: &Window,
    cx: &mut Context<T>,
    work: impl FnOnce() -> R + Send + 'static,
    on_done: impl FnOnce(&mut T, R, &mut Window, &mut Context<T>) + 'static,
) where
    T: 'static,
    R: Send + 'static,
{
    let task = cx.background_executor().spawn(async move { work() });
    cx.spawn_in(window, async move |this, cx| {
        let result = task.await;
        let _ = this.update_in(cx, |state, window, cx| {
            on_done(state, result, window, cx);
            cx.notify();
        });
    })
    .detach();
}

/// 便捷封装:直接返回 `Result<R, ServiceError>` 形态的工作函数,
/// `on_done` 收到 `Result`,错误已转为 `String`(`Display` 与源前端 toError 一致)。
pub fn run_service_result<T, R>(
    cx: &mut Context<T>,
    work: impl FnOnce() -> Result<R, tag2folders_lib::service::ServiceError> + Send + 'static,
    on_done: impl FnOnce(&mut T, Result<R, String>, &mut Context<T>) + 'static,
) where
    T: 'static,
    R: Send + 'static,
{
    run_service(
        cx,
        move || work().map_err(|e| e.to_string()),
        on_done,
    );
}

/// 原生目录选择对话框(gpui 自带,替代源 tauri plugin-dialog)。
///
/// 返回值:
/// - `Ok(Some(path))` → 用户选定目录
/// - `Ok(None)` → 用户取消(不做事)
/// - `Err(())` → 对话框打开失败(调用方应降级到内置目录浏览模态,见 DirPicker)
///
/// 用法(在拥有 `&mut App` 的回调里):
/// ```ignore
/// let rx = native_pick_directory(cx);
/// cx.spawn(async move |cx| {
///     match rx.await {
///         Ok(picked) => { /* cx.update(...) 更新实体 */ }
///         Err(()) => { /* 降级打开 DirPicker 内置模态 */ }
///     }
/// })
/// .detach();
/// ```
pub fn native_pick_directory(
    cx: &mut gpui::App,
) -> futures::channel::oneshot::Receiver<
    Result<Option<Vec<std::path::PathBuf>>, ()>,
> {
    // Receiver 的错误通道类型:prompt_for_paths 返回 Result<Option<Vec<PathBuf>>>(内层
    // anyhow::Result 已折叠)。此处统一映射为 Result<_, ()>:Err 表示"打开失败"。
    let (tx, rx) = futures::channel::oneshot::channel();
    let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: None,
    });
    cx.spawn(async move |_cx| {
        let outcome = receiver.await.map_err(|_| ()).and_then(|r| r.map_err(|_| ()));
        let _ = tx.send(outcome);
    })
    .detach();
    rx
}

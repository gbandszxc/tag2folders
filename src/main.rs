//! Tag2Folders GPUI 版入口。
//!
//! - 窗口参数照抄 SPEC 1.1:标题 `Tag2Folders`、1100×750、最小 900×600、可缩放
//! - `gpui_component::init` 最先调用(官方要求),随后把全局主题换成我们的
//!   设计 token(`ui::theme::apply_to_gpui_component`)
//! - 窗口根视图必须是 `gpui_component::Root`(组件库的 Dialog/Notification 层
//!   挂在 Root 上);业务根视图为 [`AppShell`]
//! - 资源源注册 `assets/`(图标经 AssetSource 加载,见 `ui::assets`)

mod app;
mod shot;
mod ui;

use std::cell::RefCell;
use std::rc::Rc;

use gpui::*;

use crate::app::AppShell;
use crate::ui::assets::Assets;

fn main() {
    Application::new()
        .with_assets(Assets)
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            // 换肤:把 gpui-component 色板映射为源项目设计 token(SPEC 第 6 章)
            ui::theme::apply_to_gpui_component(cx);

            let bounds = Bounds::centered(None, size(px(1100.), px(750.)), cx);
            // 截图取证模式需要把 AppShell 实体带出 open_window 闭包
            let shell_cell: Rc<RefCell<Option<Entity<AppShell>>>> = Rc::new(RefCell::new(None));
            let cell = shell_cell.clone();
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        window_min_size: Some(size(px(900.), px(600.))),
                        titlebar: Some(TitlebarOptions {
                            title: Some("Tag2Folders".into()),
                            appears_transparent: false,
                            ..Default::default()
                        }),
                        window_background: WindowBackgroundAppearance::Opaque,
                        ..Default::default()
                    },
                    |window, cx| {
                        let shell = cx.new(|cx| AppShell::new(window, cx));
                        // 窗口关闭确认(SPEC 1.5):must 在实体创建后注册
                        AppShell::register_close_guard(&shell, window, cx);
                        *cell.borrow_mut() = Some(shell.clone());
                        cx.new(|cx| gpui_component::Root::new(shell, window, cx))
                    },
                )
                .unwrap();
            cx.activate(true);

            // 截图取证(T2F_SHOT_* 环境变量;未设置时零开销)
            let shell = shell_cell.borrow().clone();
            if let Some(shell) = shell {
                shot::maybe_run_shot_session(cx, &window, shell);
            }
        });
}

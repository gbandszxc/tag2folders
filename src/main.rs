//! 最小 GPUI 壳：按 docs/SOURCE_SPEC.md 第 1.1 节的窗口参数打开单窗口。
//! - 标题 `Tag2Folders`，初始 1100×750，最小 900×600，可缩放
//! - 内容为占位 div（居中显示 "Tag2Folders GPUI - scaffold"），待 UI agent 替换
//! - `gpui_component::init` 先行初始化（主题/全局设置，官方要求最先调用），
//!   以便尽早验证 gpui-component 0.5.1 与 gpui 0.2.2 的组合可编译、可初始化

use gpui::*;

/// 脚手架占位根视图（后续由完整向导 UI 替换）
struct ScaffoldApp;

impl Render for ScaffoldApp {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            // 源应用根背景 --bg-app（SOURCE_SPEC 6.4）
            .bg(rgb(0xf8fafc))
            .text_color(rgb(0x0f172a))
            .child("Tag2Folders GPUI - scaffold")
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        gpui_component::init(cx);

        let bounds = Bounds::centered(None, size(px(1100.), px(750.)), cx);
        cx.open_window(
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
            |_, cx| cx.new(|_| ScaffoldApp),
        )
        .unwrap();
        cx.activate(true);
    });
}

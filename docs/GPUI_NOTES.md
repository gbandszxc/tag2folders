# GPUI 技术调研笔记(截至 2026-08-22)

> 调研目标:把 Tauri(React)桌面应用重写为纯 GPUI 桌面应用(macOS arm64 优先)。
> 应用形态:单窗口向导工具(左侧步骤栏 + 右侧内容区),需要:文本输入、下拉、复选、按钮组、可滚动固定表头表格、树视图、进度条、自动滚动日志控制台、模态确认框、目录选择。
> 所有结论均标注来源;查不到或未实测的明确标注。代码片段注明适用版本:
> - **[0.2.2]** = crates.io `gpui = "0.2.2"` 上已逐条验证(docs.rs 0.2.2 / 官方示例)
> - **[git main]** = zed 仓库 main 分支(2026-08-22,rev `fd82517a115d97a07835b52f0512b22b38e38ccf`),API 可能与 0.2.2 有差异
> - **[组合范式]** = 用已验证的原语拼装,个别签名需在脚手架阶段用 `cargo doc --open` 复核

---

## 0. TL;DR(决策摘要)

| 决策项 | 结论 |
|---|---|
| 依赖接入 | **crates.io `gpui = "0.2.2"`**(2025-10-22 发布,官方维护,docs.rs 全量文档)。不用 git 依赖(git main 已拆出 `gpui_platform`,未发布 crates.io,API 漂移) |
| MSRV | 官方无 rust-version 字段;zed workspace 用 edition 2024 ⇒ **Rust ≥ 1.85**,官方口径"最新 stable"。装 rustup stable 即可 |
| 组件库 | **采用 gpui-component(longbridge)`0.5.1`**(crates.io,依赖 registry gpui ^0.2.2,版本链吻合)。只取交互复杂件(Input/Select/Table/Tree/Dialog/Progress),装饰性 UI 纯 div 自绘 + 主题定制 |
| 文件对话框 | **gpui 0.2.2 自带官方原生对话框** `cx.prompt_for_paths(...)`,目录选择不需要 rfd |
| 最大风险 | ① gpui 处于 pre-1.0 且 Zed 2026 年收缩了社区向投入(锁定 0.2.2 缓解);② 中文 IME 需尽早实测;③ 首次编译 ~10 分钟级,CI/迭代节奏要适应 |

---

## 1. 依赖接入方式(最关键)

### 1.1 crates.io 发布状态【已验证】

- **gpui 已发布到 crates.io,官方维护**(发布者为 Zed 团队 Mikayla Maki)。版本史:
  - `0.1.0`(2022-06-23,占位,已 yanked)
  - `0.2.0`(2025-10-09)→ `0.2.1`(2025-10-14)→ **`0.2.2`(2025-10-22,当前最新)**
  - 来源:<https://crates.io/crates/gpui>(经 crates.io API 核对:`https://crates.io/api/v1/crates/gpui`)
- zed 仓库 main 分支的 `crates/gpui/Cargo.toml` 版本号**仍是 0.2.2**,即 crates.io 版本落后 main 不多,但 main 的 API 已有破坏性演进(见 1.3)。来源:<https://github.com/zed-industries/zed/blob/main/crates/gpui/Cargo.toml>
- 下载量 ~22 万,生态真实存在。docs.rs 有全量 API 文档:<https://docs.rs/gpui/0.2.2>

### 1.2 推荐的 Cargo.toml(方案 A,纯 crates.io)【推荐】

```toml
[package]
name = "tag2folders"
version = "0.1.0"
edition = "2021"   # 用 2024 也行(需 Rust >= 1.85);gpui 本体是 edition 2024

[dependencies]
gpui = "0.2.2"          # 官方,自带 macOS(Metal)平台层与 Application::new()
# gpui = { version = "0.2.2", features = [] }  # 默认 features: font-kit, wayland, x11, windows-manifest;
                                               # macOS-only 应用可关掉 wayland/x11 缩短编译(未验证收益,脚手架时试)

# 组件库(与上面同一 gpui 源,不会双版本冲突)——见第 7 节
gpui-component = "0.5.1"

anyhow = "1"
smol = "2"        # gpui 异步运行时基于 smol 生态;后台 IO 也可直接用 std 线程 + channel
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

要点:
- **不要把 crates.io 的 gpui-component 和 git 依赖的 gpui 混用**——gpui-component 0.5.1 在 crates.io 上声明的是 registry 依赖 `gpui ^0.2.2`(已核对其依赖清单:`https://crates.io/api/v1/crates/gpui-component/0.5.1/dependencies`);若你同时引 git 版 gpui,会出现两个不同源的 gpui 类型不兼容。gpui-component 仓库 main 的 workspace 才用 git 依赖(开发追新用)。来源:<https://github.com/longbridge/gpui-component/blob/main/Cargo.toml>
- 锁版本:提交 `Cargo.lock`,升级 gpui 需要人工评估(pre-1.0 破坏性变更常见,README 原话)。

### 1.3 方案 B:git 依赖(仅当需要 main 上的新能力)

zed main 的 README 现在推荐的接入方式是【git main,已验证】:

```toml
[dependencies]
gpui = { git = "https://github.com/zed-industries/zed", rev = "fd82517a115d97a07835b52f0512b22b38e38ccf" }  # 2026-08-22 main HEAD(经 git ls-remote 验证存在)
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit", "wayland", "x11"] }
```

来源:<https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md>

注意事项:
- **`gpui_platform` 尚未发布 crates.io**(2026-08-22 验证 `crates.io/api/v1/crates/gpui_platform` 返回 404)。main 分支把平台层(窗口/渲染/文本后端)拆进了 `gpui_platform`,zed 自带示例改用 `gpui_platform::application()` 启动,而 crates.io 0.2.2 仍是自包含的 `gpui::Application::new()`。两条路线 API 有漂移。
- git 依赖没有稳定 rev/tag 可跟(zed 仓库的 tag 是编辑器版本号,gpui 无独立 tag)。用 git 就必须自己锁 rev 并在升级时回归测试。
- rev 获取方法:`git ls-remote https://github.com/zed-industries/zed.git HEAD`。
- **对本项目的建议:不选方案 B。** 我们不需要 main 上的 WASM/新拆分;0.2.2 与 gpui-component 0.5.1 的组合是唯一"双 crate 同源"的稳定组合。

### 1.4 MSRV / 工具链【已验证】

- zed workspace 根 Cargo.toml:**无 `rust-version` 字段**,workspace `edition = "2024"` ⇒ 隐含 **Rust ≥ 1.85**。来源:<https://github.com/zed-industries/zed/blob/main/Cargo.toml>
- gpui README / docs.rs 口径:"pre-1.0,requires the latest stable Rust"。来源:<https://docs.rs/gpui/0.2.2>
- 建议:项目里放 `rust-toolchain.toml` 锁 `channel = "stable"`(脚手架时的具体小版本以本机 `rustc --version` 为准)。dev 机 darwin 25.5.0 arm64,直接 rustup stable 即可。

### 1.5 官方脚手架模板

- **`zed-industries/create-gpui-app`** 是官方脚手架:`cargo install create-gpui-app && create-gpui-app --name my-app`(支持 `--workspace`)。来源:<https://github.com/zed-industries/create-gpui-app>
- 其模板内容(templates/default)【已验证】:
  - `_Cargo.toml`:**git 依赖、无 rev** `gpui = { git = "https://github.com/zed-industries/zed" }`,另附注释掉的 smallvec;
  - `src/main.rs`:`Application::new().run(...)` + `cx.open_window(WindowOptions::default(), ...)` + `cx.new(...)`,一个绿色背景 HelloWorld。
- 风险:awesome-gpui 将其标记为 dormant(不活跃),且有 "can't find metal" 编译报错的 issue(#23)。模板又没锁 rev,跟着 main 跑随时可能撞上 gpui_platform 拆分。
- **用法建议:可以跑一次拿目录结构,然后把 Cargo.toml 改成上面方案 A。**
- 其他学习资源:官方 awesome 清单 <https://github.com/zed-industries/awesome-gpui>;社区教程 gpui-book <https://github.com/MatinAniss/gpui-book>(258 星);官方博客 ownership 文章 <https://zed.dev/blog/gpui-ownership>。

### 1.6 GPUI 官方文档现状

- 官网 <https://gpui.rs> 只有一页简介;官方口径是"文档和示例散落在 zed 的 crates 里(尤其 ui crate)"。
- 实际可用的参考:**docs.rs/gpui/0.2.2**(100% 条目有文档)+ **zed 仓库 `crates/gpui/examples/`**(~24 个官方示例,含 hello_world / input / data_table / popover / scrollable / uniform_list / window 等,是我们的主要 API 依据)。

---

## 2. 应用骨架

### 2.1 最小可运行 main.rs [0.2.2,组成件均已逐条验证]

```rust
use gpui::*;

struct WizardApp {
    step: usize,
}

impl Render for WizardApp {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .child(format!("当前步骤: {}", self.step))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1120.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(960.), px(640.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Tag2Folders".into()),
                    appears_transparent: false,          // true = 隐藏系统标题栏,自绘
                    ..Default::default()
                }),
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            |_, cx| cx.new(|_| WizardApp { step: 0 }),
        )
        .unwrap();
        cx.activate(true);
    });
}
```

已验证的依据:
- `Application::new()` 存在于 0.2.2:<https://docs.rs/gpui/0.2.2/gpui/struct.Application.html>(`pub fn new() -> Self`)
- `open_window(options: WindowOptions, build_root_view: impl FnOnce(&mut Window, &mut App) -> Entity<V>) -> Result<WindowHandle<V>>`:<https://docs.rs/gpui/0.2.2/gpui/struct.App.html>
- `WindowOptions` 全部 14 个字段 [0.2.2 已验证]:`window_bounds: Option<WindowBounds>`、`titlebar: Option<TitlebarOptions>`、`focus/show: bool`、`kind: WindowKind`(Normal/PopUp/Floating/Dialog)、`is_movable/is_resizable/is_minimizable: bool`、`display_id`、`window_background: WindowBackgroundAppearance`、`app_id`、`window_min_size: Option<Size<Pixels>>`、`window_decorations`(仅 Wayland)、`tabbing_identifier`。**没有 icon 字段。** 来源:<https://docs.rs/gpui/0.2.2/gpui/struct.WindowOptions.html>
- `TitlebarOptions { title: Option<SharedString>, appears_transparent: bool, traffic_light_position: Option<Point<Pixels>> }`(macOS 红绿灯按钮位置可自定义):<https://docs.rs/gpui/0.2.2/gpui/struct.TitlebarOptions.html>
- `WindowBounds::{Windowed, Maximized, Fullscreen}(Bounds<Pixels>)` + `Bounds::centered(None, size(px(800.), px(600.)), cx)`:官方 window.rs 示例;<https://docs.rs/gpui/0.2.2/gpui/enum.WindowBounds.html>
- `cx.activate(true)` / `cx.hide()`:官方 window.rs 示例。
- 标题栏策略:系统标题栏(默认)或 `appears_transparent: true` + 自绘(官方 window.rs 示例演示了 `titlebar: None` 完全自绘)。对本项目(向导工具):**直接用系统标题栏最省事**;若设计稿要求沉浸式,再开 appears_transparent。

### 2.2 退出方式 [0.2.2 + 官方示例已验证]

- 关窗口:`window.remove_window()`;退应用:`cx.quit()`(还有 `cx.shutdown()`、`on_app_quit` 钩子)。官方示例 `on_window_close_quit.rs` 演示"最后一个窗口关闭即退出"。
- cmd-q 绑定示例(官方 window.rs):`actions!(window, [Quit]);` + `KeyBinding::new("cmd-q", Quit, None)` + `cx.on_action(|_: &Quit, cx| cx.quit());`
- 说明:任务描述里提到的 `run_in_terminal` 是 Zed 编辑器的扩展 API,**gpui 桌面应用没有这个概念**,开发期就是 `cargo run`。

### 2.3 [git main] 的差异提醒

main 分支示例(`crates/gpui/examples/hello_world.rs`)开头是 `use gpui_platform::application;` + `application().run(...)`,且支持 wasm(`gpui_platform::web_init()`)。若看 main 上的示例代码,启动方式要换算回 0.2.2 的 `Application::new()`。来源:zed 仓库 examples 目录。

---

## 3. 状态管理与事件

### 3.1 核心模型(Entity / Context / notify)【已验证】

来源:官方博客 <https://zed.dev/blog/gpui-ownership>(GPUI 一切状态归 `App` 顶层所有):

```rust
// 创建实体:cx.new 返回类型化句柄 Entity<T>(类似 Rc)
let counter: Entity<Counter> = cx.new(|_cx| Counter { count: 0 });

// 更新实体:句柄.update(上下文, |状态, cx| ...) —— cx 是 Context<Counter>
counter.update(cx, |counter, cx| {
    counter.count += 1;
    cx.notify(); // 通知观察者(视图会重渲染)
});

// 读:entity.read(cx) -> &T
let n = observer.count_from(&observed.read(cx));
```

- 视图(实现了 `Render` 的实体)被 notify 后自动重绘;`cx.observe(&entity, |this, entity, cx| ...)` 可观察任意实体变化。
- 订阅生命周期:Subscription 要存住(通常 `self.subscriptions` vec 或字段),drop 即退订(gpui-component input 示例明确演示此点)。

### 3.2 事件(EventEmitter + emit + subscribe)

```rust
#[derive(Clone, Debug)]
struct ScanDone { found: usize }

impl EventEmitter<ScanDone> for Scanner {}   // 空实现即声明事件类型

// 发射(在 Context<Scanner> 里):
cx.emit(ScanDone { found: 42 });

// 订阅(经典签名,[0.2.2 组合范式,博客已验证 emit/事件部分]):
cx.subscribe(&scanner, |this, _scanner, event: &ScanDone, cx| {
    this.result = event.found;
    cx.notify();
}).detach();
```

- 注意:gpui-component 示例(git)里出现了 `cx.subscribe_in(&entity, window, move |this, _, ev: &InputEvent, _window, cx| ...)` 变体;0.2.2 精确签名以 docs.rs `Context::subscribe` 为准。来源:gpui-component examples/input。

### 3.3 异步:spawn / 后台线程 / 实时刷新进度

已验证的原语:
- `App::spawn(f)` 其中 `f: AsyncFnOnce(&mut AsyncApp) -> R`(0.2.2 已是 **async 闭包**写法 `async move |cx| {...}`):<https://docs.rs/gpui/0.2.2/gpui/struct.App.html>
- `cx.background_executor().spawn(fut) -> Task<T>`:后台线程池跑阻塞/重活(官方 image_gallery.rs 示例:`let task = cx.background_executor().spawn(fut).shared();`,配合 `futures::FutureExt`)
- `window.spawn(cx, async move |cx| { ... })`:闭包拿到 `AsyncWindowContext`(git main 示例 image_gallery.rs;0.2.2 同名 API 未逐条验证)
- 完成后刷新:`entity.update(&mut cx, |state, cx| { ...; cx.notify(); })`;或 `cx.on_next_frame(move |_, cx| cx.notify(entity))`(image_gallery.rs)

**"后台任务推进度 → UI 实时刷新"标准写法 [0.2.2 组合范式]**:

```rust
struct ScanState {
    progress: f32,           // 0.0 ~ 1.0
    logs: Vec<SharedString>,
}

impl ScanState {
    fn start(root: PathBuf, cx: &mut Context<Self>) {
        // 把 Entity 句柄克隆进异步闭包(强引用即可,应用生命周期内向导常驻)
        let state = cx.entity();
        cx.spawn(async move |cx| {
            let (tx, rx) = smol::channel::bounded::<(f32, String)>(64);

            // 1) 阻塞 IO 全部丢后台线程池
            cx.background_executor().spawn(async move {
                walk_dir(root, &tx).await;   // 内部按批 tx.send((progress, line)).await
            }).detach();

            // 2) 主线程消费进度,更新实体并触发重绘
            while let Ok((p, line)) = rx.recv().await {
                state.update(&mut cx, |state, cx| {
                    state.progress = p;
                    state.logs.push(line.into());
                    cx.notify(); // 每帧合并,UI 实时刷新
                })?;
            }
            anyhow::Ok(())
        })
        .detach();
    }
}
```

细节备注:
- `cx.entity()`(Context 上取自身句柄)、`Entity::update(&mut AsyncApp, ...)` 的 0.2.2 精确签名需 cargo doc 复核[组合范式];博客与示例确认语义无误。
- weak handle:docs.rs 0.2.2 存在 `WeakEntity<T>` 类型(索引可见);`downgrade()/upgrade()` 具体签名未逐条验证——**脚手架阶段用 cargo doc 确认**,或直接用强 Entity + Subscription 生命周期管理(官方示例主流做法)。
- 通道:gpui 生态惯用 `smol::channel` / `futures::channel`(gpui 依赖树里有 async-channel)。

---

## 4. 样式 API(div / Styled)

### 4.1 单位与换算【已验证】

- `px(f32) -> Pixels`:**逻辑像素(类似 CSS px)**,内部 f32 支持亚像素;渲染时才按 `scale(factor)` 转成 `ScaledPixels`(Retina 2x)。来源:<https://docs.rs/gpui/0.2.2/gpui/struct.Pixels.html>
- `rem(n) -> Rems`:相对**窗口字体大小**的单位,可用 `Window::set_rem_size` 改;`Rems::to_pixels(rem_size)` 换算。**默认 rem 数值文档未写**(浏览器惯例 16px 未被证实)——**建议本项目一律用 px(),避免换算不确定性**;Zed 内部用 rem 是为了 UI 字号缩放。来源:<https://docs.rs/gpui/0.2.2/gpui/struct.Rems.html>
- 颜色:`rgba(0x112233FF) -> Rgba`(`pub fn rgba(hex: u32) -> Rgba`,后两位是 alpha)已验证:<https://docs.rs/gpui/0.2.2/gpui/fn.rgba.html>;`rgb(0x2e7d32)` 在官方示例/模板中大量出现;`hsla(...)` 存在(Hsla 类型),具体参数签名未逐条验证。

### 4.2 Styled trait 常用方法清单

来源:zed main `crates/gpui/src/styled.rs` + 官方示例(与 0.2.2 基本一致,宏生成部分以 docs.rs 为准)。<https://docs.rs/gpui/0.2.2/gpui/trait.Styled.html>

- **布局(已验证存在)**:`flex()` / `block()` / `grid()` / `hidden()`;`flex_col()` `flex_row()`(+ `_reverse`);`flex_wrap()`/`flex_nowrap()`;`flex_1()` `flex_auto()` `flex_none()`;`flex_grow()/flex_grow_0/1` `flex_shrink()/flex_shrink_0/1` `flex_basis()`
- **对齐**:`justify_start/end/center/between/around/evenly`;`items_start/end/center/baseline/stretch`;`self_*`;`content_*`
- **Grid**:`grid_cols()/grid_rows()`(支持 min/max content)、`col_start/col_end/col_span`、`row_*`
- **尺寸**:`w()` `h()` `size()` `w_full()` `h_full()` `size_full()` `min_w()` `max_w()` 等(宏生成;`w_full/h_full/size_full` 在示例中高频出现已验证);`aspect_ratio()` `aspect_square()`
- **间距/内边距**(宏生成,Tailwind 风格):`p_2()` `px_2()` `py_1()` `m_2()` `mx_4()`……(`px_2()` 已在 uniform_list 示例验证);`gap()` 及 `gap_x/gap_y`(gap 系列宏生成,示例常见)
- **边框/圆角**:`border_1()` 等宽度系(宏生成)、`border_color()`(宏生成);`rounded_*` 系列(`rounded_md()` 在 hello_world 已验证;`rounded(px(4.))` 传值用法未逐条验证——脚手架时确认)
- **背景/文字**:`bg()` `text_color()` `text_bg()`;`font()` `font_family()` `font_features()` `font_weight()` `text_size()` `line_height()`;字号快捷 `text_xs/sm/base/lg/xl/2xl/3xl`;`underline()` `line_through()` `italic()`;`truncate()` `line_clamp()` `whitespace_nowrap()` `text_ellipsis()`
- **视觉效果**:`opacity()`(styled.rs 确认 + 官方 opacity.rs 示例)、`shadow_*` 系列(宏生成;hello_world 用了 shadow)、渐变/图案见官方 gradient.rs/pattern.rs 示例
- **滚动**:`overflow_scroll()`(scrollable.rs 示例验证)→ 纵向滚动需先 `.id("xxx")`(可交互元素必须有 ElementId);`overflow_y_scroll/overflow_x_scroll` 由宏生成存在,未逐条验证
- **交互态**:`cursor_pointer()`(已验证);`hover()/active()/focus()` 样式 refinement 与 `group` 悬浮联动:Zed 源码惯用 `.hover(|s| s.bg(...))` 写法,**0.2.2 签名未逐条验证,脚手架阶段以 docs.rs `StyleRefinement` 复核**;`.focus_visible` 有官方示例 focus_visible.rs
- **条件样式**:`.when(cond, |el| el.bg(...))`(popover.rs 示例已验证)
- **调试**:`.debug()` / `.debug_below()`(给容器描红框,排查布局神器,styled.rs 确认)
- **定位**:`relative()/absolute()/top()/left()/right()/bottom()/z_index()`(宏生成,具体名以 docs.rs 为准;data_table 示例用绝对定位画滚动条)

> 布局引擎是 taffy(gped Cargo.toml 依赖 `taffy = "=0.13.0"`),flexbox/grid 语义同 CSS,前端经验直接迁移。

---

## 5. 关键组件实现方式(逐项确认)

### 5.1 文本输入框 —— gpui **没有**自带 TextInput【已验证】

官方 `crates/gpui/examples/input.rs`(zed main)是**从零手写**的:示例内定义 `struct TextInput { focus_handle, content: SharedString, placeholder, selected_range: Range<usize>, marked_range(IME), last_layout, last_bounds, is_selecting }`,自己实现 Element 的 request_layout/prepaint/paint 生命周期、光标闪烁、选区绘制。gpui 只提供底层原语:

- 焦点:`FocusHandle` + `Focusable` + div 上 `.track_focus(&self.focus_handle(cx))`
- 输入处理 trait:`EntityInputHandler`(`selected_text_range() -> UTF16Selection`、`replace_text_in_range()`、`replace_and_mark_text_in_range()`(IME)、`bounds_for_range()`(IME 候选窗定位)、`character_index_for_point()`);paint 时 `window.handle_input(&focus_handle, ElementInputHandler::new(bounds, entity))` 注册
- 文本测量:`window.text_system().shape_line(...)` → `ShapedLine`,`line.x_for_index(i)` 可得任意字符偏移的 x 坐标(**"点击 chip 在光标处插入文本"就是改 `selected_range` + 字符串 splice,官方示例给了全套鼠标→offset 映射 `index_for_mouse_position`)**
- 键绑定:`actions!(input, [Backspace, SelectAll, ...])` + `KeyBinding::new("cmd-a", SelectAll, None)` + `.on_action(cx.listener(...))`
- grapheme 边界用外部 crate `unicode_segmentation`(不是 gpui 提供)

**结论:输入框不要自绘(≈官方示例 500+ 行,还要处理 IME/UTF-16),用 gpui-component 的 Input(底层 InputState 同样暴露 value/光标能力),详见第 7 节。**

### 5.2 可滚动容器 / 固定表头 / 虚拟化【已验证,官方 data_table.rs】

官方 `data_table.rs` 示例(10,000 行)给出的正是我们要的模式:

- **结构**:外层 `flex_col().overflow_hidden()` → 表头 div(固定列宽,不滚)→ 数据区 `uniform_list(...)`(只滚数据行)。
- **列宽**:`const FIELDS: [(&str, f32); 24]`(key, width) 固定像素列;行是 `TableRow(RenderOnce)`,每 cell `.w(px(w))` + `truncate()`。
- **uniform_list 签名 [0.2.2 已验证]**:
  ```rust
  uniform_list("entries", item_count, |range: Range<usize>, window: &mut Window, cx: &mut App| -> Vec<impl IntoElement> { ... })
  ```
  只为可见 range 构建元素 → 数千行必须虚拟化(官方万行示例即如此)。git main 上回调改包了 `cx.processor(...)`(示例现状),0.2.2 用上面三参数闭包。
- **滚动控制**:`UniformListScrollHandle` + `.track_scroll(&handle)`;读 `handle.offset()`(data_table 里的 `scroll_top()/scroll_height()`),写 `handle.set_offset(point(px(0.), -offset_y))`。自定义滚动条 = 绝对定位 div + `canvas()` 元素挂 MouseDown/Up/Move。
- **普通滚动容器**(日志控制台):`div().id("console").overflow_scroll()` + `.track_scroll(&scroll_handle)`;**自动滚到底**:日志追加后 `scroll_handle.set_offset(point(px(0.), f32::MAX))` 或按 content 高度计算[组合范式,scrollable.rs 只演示了基本滚动,自动到底的具体写法脚手架时验证]。
- 另有非等高虚拟化 `List`(docs.rs 索引确认存在 `List`/`UniformList` 两个类型;`ListState` API 未展开验证)。

### 5.3 模态框 / 浮层【已验证,官方 popover.rs】

- **原语**:`deferred(child)`(0.2.2 签名已验证:`pub fn deferred(child: impl IntoElement) -> Deferred`,把子元素提升到窗口最上层绘制)+ `anchored()` 定位。官方 popover.rs 模式:
  ```rust
  // 浮层 + 层级 + 锚定 + 防裁剪
  deferred(anchored().anchor(Anchor::TopLeft).snap_to_window_with_margin(px(8.))
      .child(popover_content))
      .priority(2)   // 数值大的在上,支持嵌套 deferred
  // 点击外部关闭
  .on_mouse_down_out(cx.listener(|this, _, _, cx| { this.open = false; cx.notify(); }))
  // 条件渲染
  .when(self.open, |el| el.child(...))
  ```
- **全屏遮罩模态(确认框)** [组合范式]:`deferred(div().absolute().size_full().bg(rgba(0x00000066)).flex().justify_center().items_center().child(card).on_mouse_down_out(...))` —— deferred 保证盖在所有内容上,遮罩用 absolute + 半透明 bg,卡片居中。**这是 Zed 内部 confirm 弹窗的同构做法(未在官方示例逐行核对,风险低)**。
- 原生确认框:`window.prompt(PromptLevel::Info, "Are you sure?", &[PromptButton::ok("确定"), PromptButton::cancel("取消")], |answer, ...| {...})`(官方 window.rs 示例验证,支持中文按钮文案)。轻量确认可直接用它,不必自绘。
- gpui-component 另有完整模态方案(第 7 节)。

### 5.4 复选框 / 下拉选择 —— 官方无组件,自绘或用 gpui-component

- docs.rs/gpui 0.2.2 顶层类型里**没有任何表单控件**(只有 div/List/Svg/Img 等基础件);Zed 自己的控件在未发布的 `ui` crate 里且以 GPL 风格耦合 workspace(HN 讨论中明确"the ui with components is a separate crate with GPL license",来源:<https://news.ycombinator.com/item?id=47003569>)。
- 自绘参考:checkbox = 可点击 div(`.id()`.on_click)+ 状态图形(SVG 或圆角边框+对勾字符)+ `cursor_pointer()`;下拉 = 按钮 + deferred/anchored 浮层列表 + `.on_mouse_down_out` 关闭(与 popover.rs 同构)。成本可控,但 Input 级别的复杂件不值得。
- gpui-component 提供 `checkbox` `radio` `switch` `select` 模块(已核对 docs.rs 模块表)。

### 5.5 进度条 / 树形视图 —— 官方无组件

- gpui examples 里的 `tree.rs` 是"深层嵌套元素演示",**不是树组件**;无 progress 组件。两者纯 div 实现:进度条 = 外层圆角 div + 内层 `.w(relative(w * progress))` 或 `.w_full()` 容器里绝对定位 `.w(fract(p))` [组合范式];树 = 递归 render + 缩进/展开状态(gpui-component 的 `tree` 模块可直接用)。
- 来源:examples 目录清单 <https://github.com/zed-industries/zed/tree/main/crates/gpui/examples>

### 5.6 字体加载 / 中文渲染

- **按 family name 引用系统字体可行 [机制已验证]**:TextSystem 有 `resolve_font(&Font)`(失败回退默认字体栈)和 `all_font_names() -> Vec<String>`(枚举 OS 全部字体名);macOS 后端是 font-kit(zed fork `zed-font-kit 0.14.1-zed`,gpui Cargo.toml 已验证)。用法:`.font_family("PingFang SC")` / `.font_family("Menlo")`(Styled::font_family 方法存在已验证)。**"PingFang SC""SF Mono""Menlo" 具体名字能否被 font-kit 命中:未实测,脚手架第一天就验证**(用 all_font_names() 打印确认)。
- **内嵌字体**:`TextSystem::add_fonts(fonts: Vec<Cow<'static, [u8]>>) -> Result<()>` [0.2.2 已验证] —— 传 `include_bytes!` 的字体数据。注意:**Zed 源码里的 `settings::load_embedded_fonts` 是 Zed 应用层函数,不在 gpui 里**;自己写一个 `fn load_fonts(cx: &App) { cx.text_system().add_fonts(vec![Cow::Borrowed(FONT)]) }` 即可 [组合范式]。
- **中文渲染**:字形级 CJK fallback 文档未明确(resolve_font 只描述字体级回退)。稳妥做法:UI 显式 `.font_family("PingFang SC")` 或注册内嵌中文字体;日志/代码区等宽用 "Menlo"(中文会走系统回退,需实测确认效果)。**未验证项,列入脚手架首日清单。**

### 5.7 组件清单速查表

| 需求 | gpui 0.2.2 官方 | 建议实现 |
|---|---|---|
| 文本输入(含光标/IME) | 无(官方手写示例 input.rs) | gpui-component `input` |
| 下拉选择 | 无 | gpui-component `select` 或自绘(popover 模式) |
| 复选框 | 无 | gpui-component `checkbox` 或自绘 |
| 按钮组 | 无(`button` 也无) | 自绘 div(简单)或 gpui-component `button` |
| 表格(滚动+固定表头+数千行) | 原语:uniform_list/overflow_scroll | 自绘表头+uniform_list(data_table.rs 模式)或 gpui-component `table` |
| 树形视图 | 无 | gpui-component `tree` 或自绘递归 |
| 进度条 | 无 | 自绘 div |
| 日志控制台(自动滚动) | 原语:id+overflow_scroll+ScrollHandle | 自绘 + set_offset;行数大时 uniform_list |
| 模态确认 | window.prompt(原生) + deferred 原语 | 原生 prompt / 自绘遮罩 / gpui-component `dialog` |
| 目录选择 | **prompt_for_paths(官方原生)** | 官方 API(第 6 节) |

---

## 6. 原生能力

### 6.1 官方文件对话框【0.2.2 已验证,重要】

gpui 0.2.2 **自带原生路径对话框**,不需要第三方:

```rust
// App 方法(docs.rs 0.2.2 逐条验证):
pub fn prompt_for_paths(&self, options: PathPromptOptions) -> Receiver<Result<Option<Vec<PathBuf>>>>
pub fn prompt_for_new_path(&self, directory: &Path, suggested_name: Option<&str>) -> Receiver<Result<Option<PathBuf>>>

// PathPromptOptions 字段:files: bool / directories: bool / multiple: bool / prompt: Option<SharedString>
```

用法 [组合范式]:选目录 = `PathPromptOptions { files: false, directories: true, multiple: false, .. }`,返回 `Receiver`,在 `cx.spawn` 的 async 块里 `.recv().await` / `.await` 拿结果(Receiver 的 await 语义脚手架时确认)。来源:<https://docs.rs/gpui/0.2.2/gpui/struct.App.html>、<https://docs.rs/gpui/0.2.2/gpui/struct.PathPromptOptions.html>

### 6.2 rfd 备选(仅在需要更深度定制时)

rfd 0.17.2:`rfd::FileDialog::pick_folder()` 是**同步阻塞**;`rfd::AsyncFileDialog::new().pick_folder().await` 返回 future。**macOS 注意**:官方文档说明 async 仅在"windowed environment like winit or SDL2"下真正异步,否则回落同步。gpui 有 NSWindow,大概率算 windowed——**未验证**。若非要用同步 API,必须 `cx.background_executor().spawn()` 丢后台线程,结果再回主线程更新实体。**本项目结论:用官方 prompt_for_paths,rfd 不引入。** 来源:<https://docs.rs/rfd/latest/rfd/>

### 6.3 其他原生能力 [0.2.2 docs.rs 验证]

- 菜单栏:`cx.set_menus(Vec<Menu>)`(有 set_menus.rs 示例);dock 菜单 `set_dock_menu`
- 系统通知(system_notifications.rs 示例);`cx.open_url` / `open_with_system`
- 窗口操作:`window.remove_window()` `window.resize(size)` `cx.hide()/activate()`

---

## 7. gpui-component(longbridge)评估

### 7.1 基本盘【已验证】

- crates.io `gpui-component = "0.5.1"`(2026-02-05 发布;0.5.0 2025-12-08,迭代约每 1-2 月),作者 Jason Lee(huacnlee),13.3k 星、活跃(awesome-gpui 标 active),Apache-2.0。来源:<https://crates.io/crates/gpui-component>、<https://github.com/longbridge/gpui-component>
- **依赖锁定关系**:0.5.1(crates.io)→ registry `gpui ^0.2.2` + `gpui-macros ^0.2.2`(已核依赖清单)。即它与我们主依赖完全同源同版本,无 rev 锁定问题。其仓库 main 用 git zed(无 rev)追新,当前开发 0.5.2 —— 跟仓库走才会踩 git 漂移,**我们只走 crates.io**。
- edition 2024(Rust ≥ 1.85)。可选 feature:webview / decimal / inspector / tree-sitter-languages(默认不开)。

### 7.2 提供的组件(0.5.1 docs.rs 模块全量核对,51 个模块)

`accordion, alert, animation, avatar, badge, breadcrumb, button, calendar, chart, checkbox, clipboard, collapsible, color_picker, date_picker, description_list, dialog, divider, dock, form, group_box, input, kbd, label, link, list, menu, notification, plot, popover, progress, radio, resizable, scroll, select, setting, sheet, sidebar, skeleton, slider, spinner, switch, tab, table, tag, text, theme, tooltip, tree`(另有根级 `TitleBar` 结构体)。来源:<https://docs.rs/gpui-component/0.5.1/gpui_component/>

对照我们的需求:**input(含 TextArea)/ select / checkbox / table(宣称虚拟化、固定+可调列、排序、可选 cell,支持数十万行)/ tree / dialog(模态)/ sheet(抽屉)/ progress / spinner / title bar / list / scroll(滚动条)/ sidebar** —— 全命中,还多出 Dock 布局、图表、通知等。

### 7.3 关键 API 形态(官方文档 + 示例已验证)

```rust
// 依赖(getting-started 给的 git 版;我们改用 crates.io 等价):
// gpui-component = "0.5.1"

fn main() {
    // 用 gpui_platform::application() 启动是 git 版写法;crates.io 0.2.2 下 Application::new() 即可
    Application::new().run(|cx: &mut App| {
        gpui_component::init(cx);   // 必须最先调用(主题/全局设置)——官方文档明确

        let bounds = Bounds::centered(None, size(px(1120.), px(760.)), cx);
        cx.open_window(WindowOptions { /* ... */ }, |window, cx| {
            let view = cx.new(|cx| WizardApp::new(window, cx));
            cx.new(|cx| Root::new(view, window, cx))  // 窗口第一层必须是 Root
        }).unwrap();
        cx.activate(true);
    });
}

// Input:状态在 Entity<InputState>,不是 props
struct WizardApp { input: Entity<InputState> /* , _subscriptions: Vec<Subscription> */ }
// 创建:cx.new(|cx| InputState::new(window, cx).placeholder("Enter your name"))
// 渲染:.child(Input::new(&self.input))
// 监听变化:cx.subscribe_in(&input_state, window, move |this, _, ev: &InputEvent, _window, cx| {
//     if let InputEvent::Change = ev {
//         let value = input_state.read(cx).value();
//         this.display_text = format!("Hello, {value}!").into();
//         cx.notify();
//     }
// })
// 来源:gpui-component examples/input(git main)

// 模态:window.open_dialog(cx, |dialog, _, _| dialog.title("确认").child(...))
// 且根视图 render 末尾追加:
//   .children(Root::render_dialog_layer(window, cx))
//   .children(Root::render_sheet_layer(window, cx))
// 来源:gpui-component examples/dialog_overlay
```

主题:`cx.theme()` 取色(`.primary/.background/.foreground`),尺寸 `.xsmall()~.large()`,变体 `.primary()/.danger()/.ghost()`。图标不内置(用 Lucide,需自带 SVG 资源 + gpui-component-assets,或完全自备)。来源:<https://longbridge.github.io/gpui-component/docs/getting-started>

### 7.4 结论:**采用,但"结构性采用"而非全家桶**

**建议:选。** 理由:

1. 官方 gpui 是"框架 + 原语",表单控件零提供;本项目 11 类控件中 Input/Select/Table/Tree/Modal 属于高交互复杂件,自绘 Input(IME/光标/选区)和虚拟化 Table 的工程量与风险完全不值得——这正是 gpui-component 的存在理由(社区共识,HN 讨论中 nu11ptr:"gpui-component now exists" 是继续选 gpui 的理由之一)。
2. 版本链干净:0.5.1 ↔ gpui 0.2.2 registry 锁定,不引入 git 漂移。
3. 与"1:1 复刻设计系统"不冲突的做法:
   - **交互件用它的**(Input/Select/Checkbox/Table/Tree/Dialog/Progress):通过 `theme` 模块整体替换色板/圆角/字号(gpui-component 主题是数据驱动可定制的,支持自定义 ThemeColor;程度需在脚手架期验证——**若某组件定制不到像素级,再降级为该组件自绘**,损失可控);
   - **装饰件自绘**(向导侧栏、卡片、按钮组、日志控制台):纯 div + px,本来就该自绘;
   - 不引入 gpui-component-assets(默认图标),用自己的 SVG。
4. 反面因素(诚实列出):依赖树大(lsp-types/markdown/tree-sitter 等是硬依赖,加剧首编译时长);样式若与它的默认风格冲突太多,改造成本会累积;升级节奏(0.x)有破坏性变更。缓解:锁 0.5.1、装饰层不依赖它、必要时逐组件替换。

---

## 8. 已知坑(macOS 视角)

1. **首编译时长**:社区实测 GPUI 应用首次 `cargo build` **10+ 分钟**(依赖树大),增量编译快;Zed 全仓从头 ~1 小时。对开发体验影响:新 clone/CI 冷缓存很慢,日常增量尚可。来源:<https://dev.to/dev-tngsh/building-a-desktop-time-app-in-rust-with-gpui-a-beginners-journey-181m>、<https://github.com/zed-industries/zed/discussions/17065>
2. **Metal / Xcode**:macOS 渲染走 Metal(gpui 构建期编译 Metal shader);**必须装 Xcode Command Line Tools**(gpui README 官方要求;Zed 自身构建还要求完整 Xcode + `xcode-select --switch`);有过 "metal shader compilation failed" 的环境类报错(create-gpui-app#23,多与系统更新相关);**macOS 26 上遇 Metal 工具链问题官方建议 `xcodebuild -downloadComponent Metal Toolchain`**;zed 有 `runtime_shaders` feature 把 shader 编译延迟到运行时(规避构建期 Metal 工具链问题)。来源:<https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md>、<https://github.com/zed-industries/create-gpui-app/issues/23>、<https://zed.dev/docs/development/macos>
3. **上游投入收缩(战略风险,2026)**:HN 线程披露 Zed 内部公告"GPUI development is getting some major brakes… focus on business relevant work in 2026",非 Zed 用例的 PR 被推迟;社区 fork `gpui-ce` 基本停滞(合并 1 个 PR 后停更);前 Zed 员工表示"gpui 很多设计以 Zed 编辑器为第一客户"。**对我们的含义:不要指望上游快速响应 issue/合入我们需要的通用化改动;锁定 0.2.2 + 源码可读可 fork 是实际保障(gpui Apache-2.0,代码量可控)。** 来源:<https://news.ycombinator.com/item?id=47003569>
4. **pre-1.0 破坏性变更**:README 原话"breaking changes are common"。必须:锁 Cargo.lock、升级人工评估、代码里少碰 gpui 冷僻内部 API。
5. **中文 IME**:框架层有完整钩子(EntityInputHandler 的 marked_range / replace_and_mark_text_in_range / bounds_for_range,input.rs 已验证),Zed 编辑器对中文输入法的兼容性历史 issue 不少(#28174 keymap 优先级、#34180 搜狗、#35700 回车上屏、#39608 预编辑删除、#41881 卡死、#56327 2025 年仍有干扰报告)——多数是编辑器 keymap 层问题,**向导式单行/多行输入风险低,但必须在脚手架第一周用系统拼音+搜狗实测 gpui-component Input**。来源:各 issue 链接见文末。
6. **窗口图标**:`WindowOptions` 无 icon 字段(全字段已核对)。macOS 应用图标属于 .app bundle(Info.plist/Assets),开发期 cargo run 出来的二进制没有图标是正常的。**已落地:`scripts/build-dmg.sh`** 完成 release 二进制 → .app(Info.plist + 图标)→ DMG 全流程,用法与细节见 docs/PACKAGING.md;图标沿用原项目 `docs/icon/raw.png`(assets/app-icon.png 为其副本),与重构前像素级一致。
7. **窗口 resize**:无需手写事件处理——布局由 taffy 自适应(`size_full` + flex);固定列宽表格在窗口变窄时需要自己决定截断/横向滚动(data_table 模式:外层 overflow_hidden + 行内 truncate)。窗口尺寸编程控制:`window.resize(size)`。
8. **git/main API 漂移**:网上教程/博客(2024-2025)与 main 示例混用旧/新 API(`cx.spawn` 闭包风格、`uniform_list` 回调、`gpui_platform::application()`),抄代码时以 docs.rs/0.2.2 为准。
9. **调试工具**:`.debug()`/.debug_below()` 布局描边;`#[gpui::test]` + TestAppContext 可写 UI 集成测试(官方 testing.rs 示例)。

---

## 9. 脚手架行动清单(建议顺序)

1. `mkdir tag2folders && cargo init`,依赖按第 1.2 节方案 A;`rust-toolchain.toml` 锁 stable。
2. 跑通 2.1 的 main.rs(0.2.2,系统标题栏,1120×760,min 960×640)。
3. **首日验证四件事**(全部标注过"未验证"):
   - `.font_family("PingFang SC")` / `"Menlo"` 是否命中(用 `all_font_names()` 打印);
   - 中文显示 + 中文 IME(系统拼音 + 搜狗)在 gpui-component Input 里的表现;
   - `prompt_for_paths` 选目录全流程(async 里 Receiver 的 await 写法);
   - 日志容器 `id + overflow_scroll + ScrollHandle` 自动滚到底的写法。
4. 引入 gpui-component:`init(cx)` + Root 包装 + 自定义主题色板(把设计系统 token 映射进 ThemeColor)。
5. 搭向导骨架:左步骤栏(自绘 div)+ 右内容区(flex_1 + 每步一个子视图 Entity)。
6. 逐个落组件:进度条/日志(自绘)→ Input/Select/Checkbox(gpui-component)→ Table(uniform_list 自绘或 gpui-component table)→ 树(gpui-component tree)→ 模态(deferred 自绘 / open_dialog)。
7. CI/开发节奏:预热 cargo 缓存(sccache 或共享 target),接受首编译 ~10min。

---

## 10. 来源索引

**官方**
- crates.io: <https://crates.io/crates/gpui>(API:crates.io/api/v1/crates/gpui)
- docs.rs: <https://docs.rs/gpui/0.2.2>(Application/WindowOptions/TitlebarOptions/WindowBounds/PathPromptOptions/App/Styled/Pixels/Rems/rgba/uniform_list/deferred/TextSystem 各页)
- gpui README(main): <https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md>
- zed 官方示例(本笔记代码依据): <https://github.com/zed-industries/zed/tree/main/crates/gpui/examples>(hello_world / input / data_table / popover / scrollable / uniform_list / window / on_window_close_quit 等)
- ownership 博客: <https://zed.dev/blog/gpui-ownership>
- macOS 构建要求: <https://zed.dev/docs/development/macos>
- 官网: <https://gpui.rs>;awesome 清单: <https://github.com/zed-industries/awesome-gpui>
- 官方脚手架: <https://github.com/zed-industries/create-gpui-app>(含 #23 metal issue)
- GPUI 2(2024 重写背景): <https://zed.dev/blog/gpui-2-on-preview>

**gpui-component**
- 仓库: <https://github.com/longbridge/gpui-component>;文档: <https://longbridge.github.io/gpui-component/docs/getting-started>
- crates.io: <https://crates.io/crates/gpui-component>(0.5.1,2026-02-05);docs.rs 模块表: <https://docs.rs/gpui-component/0.5.1/gpui_component/>
- 示例: examples/input、examples/dialog_overlay(git main)

**风险与社区**
- HN:GPUI 开发收缩: <https://news.ycombinator.com/item?id=47003569>
- 首编译时长: <https://dev.to/dev-tngsh/building-a-desktop-time-app-in-rust-with-gpui-a-beginners-journey-181m>;Zed 构建: <https://github.com/zed-industries/zed/discussions/17065>
- IME issues: <https://github.com/zed-industries/zed/issues/28174> / #34180 / #35700 / #39608 / #41881 / #56327
- rfd: <https://docs.rs/rfd/latest/rfd/>

**rev 验证记录**(git ls-remote,2026-08-22):zed main HEAD = `fd82517a115d97a07835b52f0512b22b38e38ccf`;gpui-component main HEAD = `5a5e2abc837b6d927e2b38e2097d2fbf39ebee77`。

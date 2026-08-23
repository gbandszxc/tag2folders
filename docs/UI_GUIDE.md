# UI 开发指南

> 讲解在本代码库里怎么组装 UI：加页面、调服务、弹窗、图标/颜色纪律，
> 以及已验证的 gpui 环境事实。基础组件 API 均有 doc 注释，本文只讲"怎么组装"。
> 行为/文案/token 规格见 `docs/SPEC.md`（基准线，改代码须同步）。

## 1. 现有资产清单

| 路径 | 内容 |
|---|---|
| `src/ui/theme.rs` | 全部设计 token(色/圆角/阴影/字体/时长)，常量名对应 CSS 变量 |
| `src/ui/icon.rs` | `Icon` 枚举 + `icon(Icon::X)` / `icon_sized(Icon::X, px(15.))` |
| `src/ui/components/button.rs` | `Button`(primary/secondary/outline/ghost/danger × sm/md/lg，loading/disabled/图标/左右图标位) |
| `src/ui/components/badge.rs` | `badge(variant)`、`badge_text`、`StatusBadge`(七种状态映射，含 `mapping_status_str(MappingStatus)`) |
| `src/ui/components/card.rs` | `Card`(title/subtitle/actions/padding 档位) |
| `src/ui/components/alert_bar.rs` | `AlertBar`(rose/amber/sky，支持 pre_wrap 多行错误) |
| `src/ui/components/progress_bar.rs` | `ProgressBar::new(current, total)` |
| `src/ui/components/step_nav.rs` | `StepNav`、`STEPS`、`step_nav_aside()` |
| `src/ui/components/modal.rs` | `Modal`(通用)+ `ConfirmModal`/`ConfirmOptions`(四 tone) |
| `src/ui/dir_picker.rs` | `DirPickerState`(Entity)+ `render_dir_picker` + `get_parent_path` |
| `src/ui/service.rs` | `run_service` / `run_service_in` / `run_service_result` / `native_pick_directory` |
| `src/app.rs` | 外壳 `AppShell` + 扫描/预览/进度三个页面 |

## 2. 怎么加一页

页面是**普通 struct，不是独立 Entity**，挂在 `AppShell` 上：

```rust
// src/app.rs
pub struct ScanPage {
    pub dir: Entity<DirPickerState>,     // 已建好的源目录选择
    pub recursive: bool,                 // 你自己的字段随便加
    pub loading: bool,
    pub error: Option<String>,
    // ...
}

impl ScanPage {
    fn new(window: &mut Window, cx: &mut Context<AppShell>) -> Self { /* cx.new 建 Entity */ }
}
```

渲染入口在 `AppShell::render_page` 的 `match self.current_step` 分支；页面内部
渲染写成 `AppShell` 的方法或页面自己的方法(拿 `&self`/`&mut self` + window + cx)。

页面内部需要 gpui-component 输入框(如模板输入、筛选关键词)：

```rust
pub template_input: Entity<InputState>,   // 构造：cx.new(|cx| InputState::new(window, cx).placeholder("{artist}/{album}/{title}.{ext}"))
// 渲染：Input::new(&state.template_input).h(px(38.)).font_family(theme::FONT_MONO)
// 取值：state.template_input.read(cx).value()
// 设值(需 window)：entity.update(cx, |s,cx| s.set_value(v, window, cx))
```

**事件回路**(页面字段是 Entity 时，订阅建立在 AppShell 层)：

```rust
// AppShell::new / ScanPage::new 里：
let dir = shell.scan.dir.clone();
shell._subs.push(cx.subscribe(&dir, |this, _entity, ev: &DirPickerEvent, cx| {
    match ev {
        DirPickerEvent::Changed(v) => { /* 源目录变更：作废已有扫描结果 */ cx.notify(); }
        DirPickerEvent::Enter => { /* Enter 快捷扫描 */ }
    }
}));
```

`InputState` 的事件同样:`cx.subscribe(&input_entity, |this, _, ev: &InputEvent, cx| ...)`,
变体 `Change / PressEnter{secondary} / Focus / Blur`;需要 window 的处理用
`cx.subscribe_in(&entity, window, |this, _, ev, window, cx| ...)`。

**订阅句柄随 reset 重建**:`AppShell::reset` 会重建页面结构体，订阅必须指向
新实体(旧 Subscription 随旧结构体一起丢弃)，见 `src/app.rs` 的 `_subs` 用法。

## 3. 怎么调服务(扫描/预览/整理/轮询)

阻塞的服务函数(`tag2folders_lib::service::*`)一律走后台线程，入口在
`src/ui/service.rs`:

```rust
use crate::ui::service::run_service;
use tag2folders_lib::service;

// 扫描(竞态 token 自管)
run_service(
    cx,                                                // &mut Context<AppShell>
    move || service::scan_directory(dir.clone(), Some(recursive)),
    |this, result, cx| match result {
        Ok(resp) => { this.scan.files = resp.files; /* ... */ }
        Err(e) => { this.scan.error = Some(e.to_string()); }
    },                                                 // 回调里记得 cx.notify()(run_service 已帮你调了一次)
);
```

- 回调**需要 window**(如更新 InputState)→ 用 `run_service_in(window, cx, work, |this, r, window, cx| ...)`;
- 错误已转 String → `run_service_result`;
- **竞态**：发起前 `self.scan_token += 1`，回调里 `if this.scan_token != token { return; }`
  (DirPicker::navigate 有现成示例)；
- **轮询**(进度页):`cx.spawn_in(window, async move |this, cx| { loop {
  cx.background_executor().timer(Duration::from_secs(1)).await;
  let snap = this.read_with(cx, |s, _| service::get_task_status(s.task_id.clone())).ok()??;
  ... }})`——或简单起见每秒一次 `run_service` + 终态置位；轮询异常静默重试不断轮询。

原生目录选择已在 DirPicker 内部接好(先原生，失败降级内置模态)，页面无需自调。

## 4. 怎么弹确认框

`AppShell.confirm: Option<PendingConfirm>` 单例槽 + `ConfirmModal`。现有两处
(重置/退出)；要弹新确认照抄：

```rust
let options = ConfirmOptions::new("message 必填")
    .title("标题").description("描述").tip("提示横幅")
    .confirm_text("确认重置").cancel_text("取消")
    .tone(ConfirmTone::Warning);   // Warning/Danger/Info/Primary,配色见 modal.rs
this.confirm = Some(PendingConfirm { options, action: ConfirmAction::Reset /* 加变体 */ });
this.confirm_focus.focus(window);  // autoFocus 语义:Escape=取消 / Enter=确认
cx.notify();
```

结果处理集中在 `AppShell::handle_confirm(ok, window, cx)`。

普通模态用 `components::Modal`;DirPicker 的浏览模态是最完整的参考实现。

## 5. 步骤解锁状态机

- 初始:`current_step = 1`，`max_unlocked_step = 1`;
- 扫描完成:`files.len() > 0` → `max_unlocked_step = max(prev, 2)`，否则锁回 1 且 `current_step = 1`;
- 预览页"开始执行整理"→ `max_unlocked_step = 3; current_step = 3`;
- 重置已在 `AppShell::reset` 实现(重建页面结构体 = 状态全清)；
  "完成并开启新任务"按钮直接调 `this.reset(window, cx)`。

## 6. 图标 / 颜色 / 间距纪律

- 数值一律用 `theme::*` 常量；禁止手写 hex；amber 系数值是历史定下的有效值，勿按色阶惯例"纠正"（详见 SPEC §8）；
- 图标:`icon_sized(Icon::Music, px(15.)).text_color(theme::AMBER_700)`——
  tint 机制 = 文本色，详见 `src/ui/icon.rs` 模块注释；
- 新图标：把 SVG 放 `assets/icons/`(24×24、stroke=currentColor、fill=none)，
  并在 `src/ui/assets.rs` 的 `EMBEDDED` 表 + `src/ui/icon.rs` 枚举里登记
  (内嵌优先，fs 回退)；
- CJK 字体已在根 div / gpui-component 主题双处设为 PingFang SC，等宽 Menlo，
  无需页面再设(除非局部覆盖)。

## 7. 已验证的环境事实(不用再踩)

- `on_window_should_close` 存在，关闭确认已实现(`register_close_guard`);
- div **没有** transform/transition(`.scale()` 不存在;hover 态切换瞬时)，
  需要 transform 的只有 `svg().with_transformation(...)`;
- `on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _: &mut Window, _: &mut App| ...)`:
  第一个参数必须给按键;闭包参数要显式标注类型(否则 HRTB 推导失败);
- `FluentBuilder::when/when_some` 只对 `IntoElement` 类型可用，**不能用在
  `.hover(|st| ...)` 闭包里**(StyleRefinement 不是 element)，闭包内用 if/else;
- `InputState::set_value` 需要 `&mut Window`——异步回调里用 `run_service_in` /
  `spawn_in` + `update_in`;
- 按钮的 `title`(悬浮提示)属性无 gpui 等价，未实现;
- 视口/滚动：外层 relative+size_full+overflow_hidden 约束 → workspace
  flex_col+min_h0+overflow_y_scroll;
- 图标 svg 遮罩：颜色必须 `.text_color()` 设在 svg 自身，不继承父;
- 字体 "PingFang SC" / "Menlo" 均在运行时验证可解析，中文正常;
- `gpui_component::init(cx)` → `ui::theme::apply_to_gpui_component(cx)` 顺序不可反，
  窗口根必须是 `gpui_component::Root`(Dialog/Sheet/Notification 层依赖)。

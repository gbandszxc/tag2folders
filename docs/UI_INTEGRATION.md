# UI 集成指南(给后续页面 agent:扫描页 / 预览页 / 进度页)

> 读者:接手 SPEC 4.1(ScanPage)、4.2(PreviewPage)、4.3(ProgressPage)的 agent。
> 前置阅读:`docs/SOURCE_SPEC.md` 对应章节 + 本文。基础组件 API 均有 doc 注释,
> 本文只讲"怎么组装"。

## 1. 现有资产清单

| 路径 | 内容 |
|---|---|
| `src/ui/theme.rs` | 全部设计 token(色/圆角/阴影/字体/时长),常量名对应 CSS 变量 |
| `src/ui/icon.rs` | `Icon` 枚举(32 个)+ `icon(Icon::X)` / `icon_sized(Icon::X, px(15.))` |
| `src/ui/components/button.rs` | `Button`(primary/secondary/outline/ghost/danger × sm/md/lg,loading/disabled/图标/左右图标位) |
| `src/ui/components/badge.rs` | `badge(variant)`、`badge_text`、`StatusBadge`(七种状态映射齐了,含 `mapping_status_str(MappingStatus)`) |
| `src/ui/components/card.rs` | `Card`(title/subtitle/actions/padding 档位) |
| `src/ui/components/alert_bar.rs` | `AlertBar`(rose/amber/sky,支持 pre_wrap 多行错误) |
| `src/ui/components/progress_bar.rs` | `ProgressBar::new(current, total)` |
| `src/ui/components/step_nav.rs` | `StepNav`、`STEPS`、`step_nav_aside()` |
| `src/ui/components/modal.rs` | `Modal`(通用)+ `ConfirmModal`/`ConfirmOptions`(四 tone) |
| `src/ui/dir_picker.rs` | `DirPickerState`(Entity)+ `render_dir_picker` + `get_parent_path` |
| `src/ui/service.rs` | `run_service` / `run_service_in` / `run_service_result` / `native_pick_directory` |
| `src/app.rs` | 外壳 `AppShell` + 三个页面结构体占位 |

## 2. 怎么加一页(以 ScanPage 为例)

页面是**普通 struct,不是独立 Entity**,挂在 `AppShell` 上:

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

渲染入口在 `AppShell::render_page` 的 `match self.current_step` 分支;页面内部
渲染写成 `AppShell` 的方法或页面自己的方法(拿 `&self`/`&mut self` + window + cx)。
**当前占位已接通 DirPicker**,替换占位卡片时保留这两处调用:

```rust
1 => card().title("扫描源目录").subtitle("...")
    .child(render_dir_picker(&self.scan.dir, window, app))  // app: &mut gpui::App
    ...
```

页面内部需要 gpui-component 输入框(如模板输入、筛选关键词):

```rust
pub template_input: Entity<InputState>,   // 构造:cx.new(|cx| InputState::new(window, cx).placeholder("{artist}/{album}/{title}.{ext}"))
// 渲染:Input::new(&state.template_input).h(px(38.)).font_family(theme::FONT_MONO)
// 取值:state.template_input.read(cx).value()
// 设值(需 window):entity.update(cx, |s,cx| s.set_value(v, window, cx))
```

**事件回路**(页面字段是 Entity 时,订阅建立在 AppShell 层):

```rust
// AppShell::new / ScanPage::new 里:
let dir = shell.scan.dir.clone();
shell._subs.push(cx.subscribe(&dir, |this, _entity, ev: &DirPickerEvent, cx| {
    match ev {
        DirPickerEvent::Changed(v) => { /* SPEC 4.1.8 输入变更效应 */ cx.notify(); }
        DirPickerEvent::Enter => { /* Enter 快捷扫描 */ }
    }
}));
```

`InputState` 的事件同样:`cx.subscribe(&input_entity, |this, _, ev: &InputEvent, cx| ...)`,
变体 `Change / PressEnter{secondary} / Focus / Blur`;需要 window 的处理用
`cx.subscribe_in(&entity, window, |this, _, ev, window, cx| ...)`。

## 3. 怎么调服务(扫描/预览/整理/轮询)

阻塞的服务函数(`tag2folders_lib::service::*`)一律走后台线程,三个入口在
`src/ui/service.rs`:

```rust
use crate::ui::service::run_service;
use tag2folders_lib::service;

// 扫描(SPEC 4.1.8,竞态 token 自管)
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
- **竞态**:发起前 `self.scan_token += 1`,回调里 `if this.scan_token != token { return; }`
  (DirPicker::navigate 有现成示例);
- **轮询**(进度页,SPEC 4.3.8):`cx.spawn_in(window, async move |this, cx| { loop {
  cx.background_executor().timer(Duration::from_secs(1)).await;
  let snap = this.read_with(cx, |s, _| service::get_task_status(s.task_id.clone())).ok()??;
  ... }})`——或简单起见每秒一次 `run_service` + 终态置位;断线静默重试语义照 SPEC。

原生目录选择已在 DirPicker 内部接好(先原生,失败降级内置模态),页面无需自调。

## 4. 怎么弹确认框

`AppShell.confirm: Option<PendingConfirm>` 单例槽 + `ConfirmModal`。已实现两处
(重置/退出);页面要弹新确认(如需要)照抄:

```rust
let options = ConfirmOptions::new("message 必填")
    .title("标题").description("描述").tip("提示横幅")
    .confirm_text("确认重置").cancel_text("取消")
    .tone(ConfirmTone::Warning);   // Warning/Danger/Info/Primary,配色见 SPEC 2.9
this.confirm = Some(PendingConfirm { options, action: ConfirmAction::Reset /* 加变体 */ });
this.confirm_focus.focus(window);  // autoFocus 语义:Escape=取消 / Enter=确认
cx.notify();
```

结果处理集中在 `AppShell::handle_confirm(ok, window, cx)`。
**注意 SPEC 7.1:全应用只有重置与退出两处确认,"开始执行整理"与"完成并开启新任务"
都不弹确认**,不要多加。

普通模态(如 PreviewPage 目录树)用 `components::Modal`;DirPicker 的浏览模态
是最完整的参考实现。

## 5. 步骤解锁状态机(SPEC 1.7)

- 扫描完成:`files.len() > 0` → `max_unlocked_step = max(prev, 2)`,否则锁回 1 且 `current_step = 1`;
- 预览页"开始执行整理"→ `max_unlocked_step = 3; current_step = 3`;
- 重置已在 `AppShell::reset` 实现(重建页面结构体 = resetKey 重挂载);
  "完成并开启新任务"按钮直接调 `this.reset(window, cx)`(不弹确认)。

## 6. 图标 / 颜色 / 间距纪律

- 一切数值**照抄 SPEC**,用 `theme::*` 常量;禁止手写 hex;
- 图标:`icon_sized(Icon::Music, px(15.)).text_color(theme::AMBER_700)`——
  tint 机制 = 文本色,详见 `src/ui/icon.rs` 模块注释;
- 新图标:把 SVG 放 `assets/icons/`(24×24、stroke=currentColor、fill=none),
  并在 `src/ui/assets.rs` 的 `EMBEDDED` 表 + `src/ui/icon.rs` 枚举里登记
  (内嵌优先,fs 回退);
- CJK 字体已在根 div / gpui-component 主题双处设为 PingFang SC,等宽 Menlo,
  无需页面再设(除非局部覆盖)。

## 7. 已验证的环境事实(不用再踩)

- `on_window_should_close` 存在,关闭确认已实现(`register_close_guard`);
- div **没有** transform/transition(`.scale()` 不存在;hover 态切换瞬时),
  需要 transform 的只有 `svg().with_transformation(...)`;
- `on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _: &mut Window, _: &mut App| ...)`:
  第一个参数必须给按键;闭包参数要显式标注类型(否则 HRTB 推导失败);
- `FluentBuilder::when/when_some` 只对 `IntoElement` 类型可用,**不能用在
  `.hover(|st| ...)` 闭包里**(StyleRefinement 不是 element),闭包内用 if/else;
- `InputState::set_value` 需要 `&mut Window`——异步回调里用 `run_service_in` /
  `spawn_in` + `update_in`;
- 按钮的 `title`(悬浮提示)属性无 gpui 等价,已在 KNOWN_DIFFERENCES 记录。

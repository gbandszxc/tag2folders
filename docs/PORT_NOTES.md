# GPUI 移植记录（PORT_NOTES）

> 本文件记录从源项目 `/Users/zxc/ProjectSpace/gitcode/tag2folders`（Tauri 2）
> 平移到本项目（GPUI）过程中所有与源代码的**有意分歧**。
> 错误文案、边界条件、测试用例一律原样保留，不在本文件讨论范围内。

## 1. 项目结构映射

| 源（src-tauri/src/） | 本项目（src/） | 说明 |
|---|---|---|
| `core/`（8 个文件） | `core/` | **逐字节复制，零改动**（已用 diff 校验，无任何 tauri 引用） |
| `task.rs` | `task.rs` | 去 tauri 事件发射（见 §2） |
| `commands.rs` | `service.rs` | Tauri 命令 → 普通函数（见 §3） |
| `lib.rs` | `lib.rs` | 去 tauri Builder，改为纯模块声明 |
| `main.rs` | `main.rs` | 最小 gpui 壳（占位 UI，待 UI agent 替换） |
| `examples/smoke.rs` | （未移植） | Windows 专用冒烟脚本，硬编码 `C:\Users\...` 路径，无法在 macOS 运行；如需要可后续改写为跨平台集成测试 |

## 2. task.rs 的分歧

- **移除 Tauri 事件通道**：源 `publish()` 同时做两件事——更新内存注册表快照 +
  `app.emit("progress://{task_id}", &event)`。GPUI 版删除 `AppHandle` 参数与
  emit 调用，只保留快照更新。依据：SOURCE_SPEC 5.3 明确"当前前端没有 listen
  事件通道，完全靠 get_task_status 每 1000ms 轮询"，事件通道本就是为迟到订阅者
  预留的死路径；GPUI 版 UI 沿用轮询。
- `run_organize()` 签名去掉首个 `app: AppHandle` 参数，其余逐行一致
  （推送时机、终态语义、"第一条错误即终止"语义均不变）。
- **线程模型不变**：源项目后台执行就是 `commands.rs` 里的 `std::thread::spawn`
  （并非 tauri::async_runtime），GPUI 版原样沿用。
- 快照结构 `ProgressEvent`/`TaskStatus`、终态 TTL 300s 惰性淘汰、容量上限 32
  （满时淘汰最旧终态）完全一致。

## 3. commands.rs → service.rs 的分歧

- 去掉 `#[tauri::command]`、`tauri::AppHandle`、`tauri::Emitter`；5 个命令
  改为普通 pub 函数：`scan_directory` / `generate_preview` / `start_organize` /
  `get_task_status` / `browse_dirs`。入参、返回结构与 SOURCE_SPEC 第 5 章一致。
- **错误类型**：源端错误为 `serde_json::Value`（三种形状：纯字符串、
  `{"template_errors": [...]}`、`{"preflight_errors": [...]}`）。改为枚举
  `ServiceError { Message(String), TemplateErrors(Vec<String>), PreflightErrors(Vec<String>) }`，
  与源端 JSON 形状一一对应；`Display` 输出与源前端 toError 规则一致
  （数组以 `\n` 连接，纯字符串原样），**所有错误文案逐字保留**。
- `exit_app` 命令不移植：GPUI 下退出由 UI 层 `cx.quit()` 承担
  （源前端的兜底 `getCurrentWindow().destroy()` 同理由 gpui 窗口管理覆盖）。
- `scan_directory` 的 `recursive: Option<bool>` 缺省 true 语义保留
  （SPEC 5.2）。

## 4. 依赖与构建

- 新增 UI 依赖：crates.io `gpui = "0.2.2"` + `gpui-component = "0.5.1"`
  （决策见 GPUI_NOTES §0/§1/§7，禁止 git 依赖混用）。
- 新增 `dirs = "5"`：供后续用数据目录持久化 task_id（替代源前端的
  localStorage `tag2folders_task_id`），脚手架阶段尚未使用。
- 其余依赖与源 src-tauri/Cargo.toml 完全一致（serde/serde_json/lofty 0.22/
  regex/uuid v4/dunce/filetime/unix-libc/windows-sys）。
- crate 布局：lib（`tag2folders_lib`，纯后端逻辑，无 gpui 依赖代码）+ bin
  （`tag2folders`，仅 main.rs 使用 gpui）。**若 gpui 编译受阻，
  `cargo test --lib` 仍可完整验证后端**。
- main.rs 已调用 `gpui_component::init(cx)`（官方要求最先调用），
  尽早验证组件库初始化链路；占位 UI 未用任何组件。

## 5. 首次编译：Metal shader 构建失败与处置（重要）

**现象**（首次 `cargo build`，依赖均正常解析，gpui-component 0.5.1 与
gpui 0.2.2 无任何版本冲突；仅 gpui 构建脚本失败）：

```text
cargo::error=metal shader compilation failed:
xcrun: error: unable to find utility "metal", not a developer tool or in PATH
warning: build failed, waiting for other jobs to finish...
```

**根因**：gpui 0.2.2 默认在**构建期**用 `xcrun -sdk macosx metal` 编译
`src/platform/mac/shaders.metal` 为 metallib。本机环境：

- macOS 26.5.2（darwin 25.5.0 arm64）
- `xcode-select -p` → `/Library/Developer/CommandLineTools`（**仅 CLT，无完整 Xcode**）
- `xcrun -f metal` → not found；`xcodebuild` → 需要 Xcode

即 GPUI_NOTES §8.2 预警的场景。官方建议的修复
`xcodebuild -downloadComponent Metal Toolchain` 在本机不可行（xcodebuild
要求安装完整 Xcode，约 7GB+，需 App Store/GUI）。

**处置**：给 gpui 开启上游自带的 `runtime_shaders` feature（Cargo.toml：
`gpui = { version = "0.2.2", features = ["runtime_shaders"] }`）：

- 依赖方案**未变**：同一 crate、同一版本 0.2.2、同一 registry 源；
  该 feature 是上游为此场景设计的开关（GPUI_NOTES §8.2 原文：
  "zed 有 runtime_shaders feature 把 shader 编译延迟到运行时（规避构建期
  Metal 工具链问题）"）。
- 机制：构建期只把生成的头与 shader 源码拼接进二进制（build.rs
  `emit_stitched_shaders`），运行期由 macOS 系统 Metal 框架的运行时编译器
  编译（`metal_renderer.rs`：`device.new_library_with_source(...)`），
  **不依赖任何开发者工具链**，功能等价，仅启动时多一次 shader 编译开销。
- **回退方式**：安装完整 Xcode（`xcode-select -s /Applications/Xcode.app`）
  或补齐 Metal Toolchain 后，删除该 feature 即恢复构建期编译。

若协调者不认可此处置，改为安装完整 Xcode 后移除该 feature 即可，
代码层无需任何改动。

**结果验证**（2026-08-22，本机 darwin 25.5.0 arm64 / macOS 26.5.2）：

- 首次 `cargo build`（失败于 metal）：88.75s（依赖已全部编完）
- 开启 runtime_shaders 后重跑 `cargo build`：18.44s，**成功**（exit 0）
- `cargo test`：**67 passed, 0 failed**（macOS 平台；源项目"71 个测试"
  含 5 个 `#[cfg(windows)]` 专属用例，在 macOS 不编译属预期）
- 冒烟：直接运行 `target/debug/tag2folders`，窗口正常打开（验证了
  运行期 Metal shader 编译与 `gpui_component::init` 均可用），进程无报错
- lofty 实际解析为 **0.22.4**（0.22 系最新补丁，无编译问题）
- 唯一警告：`block v0.1.6`、`proc-macro-error2 v2.0.1` 的 future-incompat
  提示（上游传递依赖，与本项目代码无关）

## 6. 待办 / 遗留

- task_id 持久化（dirs 数据目录）在 UI agent 接入时实现。
- 窗口关闭确认（源 onCloseRequested，SPEC 1.5）需 gpui 侧等价钩子，
  由 UI agent 处理。

## 7. D2(UI 基建)决策记录(2026-08-22)

### 7.1 SVG 图标着色机制结论(源码已验证)

读 gpui 0.2.2 `src/svg_renderer.rs` + `src/elements/svg.rs` 确认:gpui 渲染 SVG
的管线是 ① AssetSource 加载字节 → ② usvg/resvg 光栅化 → ③ **pixmap 降为纯
alpha 遮罩(颜色信息全部丢弃)** → ④ 绘制时用元素 `style.text.color` 上色。
因此:

- SVG 文件统一写 `stroke="currentColor"`(usvg 解析为不透明黑,只有 alpha 进遮罩);
- **图标换色 = `.text_color(色)`,无需按颜色生成 SVG 变体**;
- `assets/icons/*.svg` 32 个(SPEC 2.1 全量)+ `circle-x.svg`(gpui-component
  Input 清空按钮需要;`check.svg` 双用于 Checkbox)。AssetSource 为
  **编译期内嵌(include_str!)+ fs 回退**,二进制自包含。

### 7.2 组件采用策略(实际落地)

- **gpui-component 仅用于高交互件**:本轮实际使用 `Input`(gpui-component 0.5.1)、
  `InputState`、`Root`(窗口根视图必须);主题经 `Theme::global_mut(cx)` 覆盖
  `ThemeColor`(primary=amber-500、ring=amber-500、danger=rose-600、
  success=emerald-600、info=sky-600、radius=8px、字体 PingFang SC/Menlo 等,
  见 `ui::theme::apply_to_gpui_component`)。Checkbox/Select 留给页面 agent,
  资产(check.svg)已备好;
- 其余全部 div 自绘:Button/Badge/StatusBadge/Card/AlertBar/ProgressBar/StepNav/
  Modal/ConfirmModal(`src/ui/components/`),数值照抄 SPEC。

### 7.3 关键 API 事实(实测/源码核对)

- 窗口关闭拦截 `window.on_window_should_close(cx, f -> bool)` **存在**,关闭确认
  已实现(此前 GPUI_NOTES 未确认项落地);
- `on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, &mut Window, &mut App|)`
  必须显式按键 + 闭包参数全类型标注(否则 HRTB 推导失败);
- `FluentBuilder::when/when_some` 仅 `IntoElement` 可用,`.hover(|st|...)` 闭包内
  不可用(StyleRefinement 非 element),需 if/else;
- div 无 transform/transition(`.scale()` 不存在);动画降级清单见
  `docs/KNOWN_DIFFERENCES.md`;
- `Context::subscribe_in(&entity, window, ...)` 提供 window(Blur/焦点类处理需要);
  gpui-component `InputState::set_value` 强依赖 `&mut Window`,异步链路须
  `spawn_in` + `WeakEntity::update_in`;
- 字体运行时验证:"PingFang SC" / "Menlo" 均在 `all_font_names()` 命中,CJK 正常。

### 7.4 结构

- UI 全部在 bin 目标(`src/main.rs` + `src/app.rs` + `src/ui/**`),lib 保持纯逻辑;
  新增 `futures = "0.3"`(gpui 依赖树内已有,oneshot/Future 用);
- `src/service.rs` 仅加 `#[derive(Clone)] DirEntry`(UI 过滤需要,无行为变化);
- 窗口根 = `gpui_component::Root`(Dialog/Sheet/Notification 层依赖),业务根
  `AppShell` 挂其下;`gpui_component::init(cx)` → `ui::theme::apply_to_gpui_component(cx)`
  顺序不可反。

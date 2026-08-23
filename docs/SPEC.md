# Tag2Folders 应用规格（SPEC）

> 本文档描述**当前代码库的实现事实**，是功能开发与测试的基准线。
> 文中数值/文案/行为均提取自 `src/`，改动代码时请同步更新对应章节；两者冲突时以代码为准并回来修文档。
> UI 组装套路与 gpui 坑位见 `docs/UI_GUIDE.md`；打包见 `docs/PACKAGING.md`。

## 1. 产品概览

- 单窗口向导式桌面工具，三步整理音频文件：**扫描 → 模板预览 → 执行整理**。
- 窗口：标题 `Tag2Folders`，1100×750（最小 900×600），可缩放，系统标题栏，版本徽章 `v2.0.1`。
- 技术栈：Rust（edition 2021，≥1.85）、gpui 0.2.2 + gpui-component 0.5.1（均 crates.io，**禁止混入 git 依赖的 gpui**，会双版本冲突）；gpui 开启 `runtime_shaders` feature（构建期无需 `xcrun metal`，装好完整 Xcode 后可移除）。
- crate 布局：`lib`（`tag2folders_lib`，纯逻辑，无 gpui）+ `bin`（`tag2folders`，UI）。`cargo test --lib` 可独立验证后端。

## 2. 架构

```
src/
├── main.rs        gpui 入口：Application → open_window → gpui_component::Root(AppShell)
│                  初始化顺序（不可反）：gpui_component::init(cx) → theme::apply_to_gpui_component(cx)
├── lib.rs         tag2folders_lib：core / task / service 三模块
│   ├── core/      业务核心（纯函数，无 IO 状态）
│   │   ├── scanner.rs        目录扫描
│   │   ├── metadata.rs       音频元数据提取（lofty）
│   │   ├── template.rs       模板校验与渲染
│   │   ├── preview.rs        预览映射生成（三遍算法）
│   │   ├── organizer.rs      冲突消解 + 预检 + 执行
│   │   ├── path_security.rs  源/目标目录安全校验
│   │   └── path_util.rs      路径工具（跨平台语义）
│   ├── task.rs    任务快照注册表 + 后台执行线程
│   └── service.rs UI 直接调用的服务函数（scan/preview/organize/status/browse）
├── app.rs         AppShell：状态机 + 三页结构体 + 全部页面渲染
├── shot.rs        截图取证（T2F_SHOT_* 环境变量，正常启动零开销）
└── ui/
    ├── theme.rs        设计 token 全表 + gpui-component 主题接管
    ├── icon.rs         Icon 枚举（32 个，Lucide 风格）
    ├── assets.rs       AssetSource（include_str! 内嵌 + fs 回退）
    ├── dir_picker.rs   目录输入组件（原生对话框 → 内置浏览模态降级）
    ├── service.rs      run_service* 后台线程 helpers
    └── components/     button / badge / card / alert_bar / progress_bar / step_nav / modal
```

**UI 状态架构约定**（详见 `src/app.rs` 模块注释与 UI_GUIDE）：

- 根实体 `AppShell`（实现 Render）持有全部向导状态：`current_step` / `max_unlocked_step` / 三个页面结构体 / 确认弹窗槽。
- 页面是**普通 struct**（非 Entity），字段直接挂在 AppShell 上；高交互控件持有独立 Entity（`Entity<DirPickerState>`、`Entity<InputState>`），事件回路在 `wire_page_subscriptions` 建立。
- `AppShell::reset` **重建页面结构体**（等价重挂载，内部状态与订阅全部丢弃重建），并归位步骤状态、清 taskId。
- 三页状态常驻：切页不丢页面内部状态（struct 字段常驻 + render 按 `current_step` 切换）。
- **竞态 token**：`scan_token` / `preview_token` / `progress_token` 挂 AppShell（跨 reset 单调递增），发起时快照、回调比对，过期响应丢弃。

## 3. 数据契约（`core` 类型，serde snake_case）

```rust
pub struct AudioMetadata {          // 元数据（勿改字段名，UI 按此消费）
    path, ext, artist, album, title, track, year, genre: String,
    readable: bool,
    error: String,                  // 读取错误信息，可空串（#[serde(default)]）
}
pub enum OrganizeMode { Move, Copy }            // 序列化 "move" | "copy"
pub enum MappingStatus { Ok, Conflict, BatchConflict, MissingMetadata,
                         Unreadable, BoundaryError, WriteError }   // snake_case
pub struct FileMappingItem {
    source: String,          // 源文件绝对路径
    target: String,          // 渲染的原始目标路径（冲突消解前）
    final_target: String,    // 冲突消解后的最终计划路径（映射表显示此字段）
    relative_target: String, // 相对 target_dir 的显示用路径
    status: MappingStatus,
    conflict: bool,          // 磁盘已存在同名目标（或链式重命名）
    batch_conflict: bool,    // 批内目标碰撞（或文件-目录冲突）
}
pub struct PreviewRequest { files: Vec<AudioMetadata>, template, target_dir, mode }
pub struct PreviewResponse { template, target_dir, total, mappings,
                             template_errors: Vec<String>, directory_tree: Value }
pub struct ProgressEvent { task_id, status, current, total, current_file, message }
```

- **元数据兜底值**（`metadata.rs` 常量，`missing_metadata` 判定依赖确切字符串，勿改动）：
  `Unknown Artist` / `Unknown Album` / `Unknown Title` / `0`（track）/ `Unknown Year` / `Unknown Genre`。
- **directory_tree**：嵌套 JSON 对象，目录名 → 子树；特殊键 `__files__` → 该目录直接包含的文件名数组。目录组件恰好叫 `__files__` 时存为 `__files__\0`（空字节在所有文件系统路径组件中非法，永不冲突），UI 展示时解码还原。
- **ServiceError**（`service.rs`）：`Message(String)` / `TemplateErrors(Vec<String>)` / `PreflightErrors(Vec<String>)`；数组类错误 `Display` 以 `\n` 连接多行展示。

## 4. 业务核心（core/）

### 4.1 scanner

- 支持的扩展名（小写、含点）：`.mp3 .flac .ogg .m4a .wav .aac .wma .ape .opus`。
- 递归/单层扫描；结果按路径字符串排序；无权限目录静默跳过。
- `ScanError`：`Directory not found: {p}` / `Not a directory: {p}` / `Permission denied: {p}`。

### 4.2 metadata（lofty）

- 查找链：lofty 标签 → WAV 的 RIFF LIST/INFO 子块（`INAM/IART/IPRD/IPRT/ICRD/IGNR` → title/artist/album/track/year/genre）→ 兜底值。
- artist 键序 `TrackArtist → AlbumArtist`；year 键序 `Year → RecordingDate`；均取首条 trim 后非空文本。
- 标题完全缺失时用文件名主干；音轨号 `3/12` 规范化为 `3`。
- 解析失败 → `readable=false` + `error` 信息 + 全兜底字段（**不返回 Err**）。

### 4.3 template

- 占位符：`{artist} {album} {title} {track} {year} {genre} {ext}`，正则 `\{(\w+)\}`（Unicode `\w`）；未知占位符校验时报错（渲染时原样保留）。
- track 为纯数字且不足两位时左补零（`1` → `01`；`0` → `00`）。
- 逐段清洗：非法字符 `[<>:"/\|?* 控制字符]` → `_`；Windows 保留名（`CON PRN AUX NUL COM1-9 LPT1-9`，不看大小写与扩展名）追加 `_`；去除尾部点/空格（Win32 会静默丢弃）；字面量段保留 `.`/`..` 供边界检测。反斜杠归一为 `/`，去前导 `/`。
- 校验错误文案（测试锁定）：
  - `Template must not be empty.`
  - `Unsupported placeholder(s): ['xxx']. Supported: ['album', 'artist', 'ext', 'genre', 'title', 'track', 'year'].`（列表按字母序、Python repr 风格单引号）

### 4.4 preview（三遍算法，纯只读，不创建任何文件/目录）

1. **Pass 1**：逐文件渲染相对目标 → 拼接绝对目标。`resolve_lenient` 仅用于边界验证（resolve 后必须位于 target_root 之内且 ≠ target_root 自身）；**存储 normpath 后的路径以保留用户大小写意图**（resolve 在大小写不敏感盘上会把大小写重命名改写回磁盘既有拼写）。
2. **可整理条目筛选**（不可整理条目不参与冲突消解，避免抢占 `_1` 后缀槽）：排除 boundary 失败、unreadable、move 模式源父目录不可写/源重复、copy 模式纯大小写重命名。
3. **Pass 2**：`plan_targets` 消解磁盘冲突（目标已存在或为悬空 symlink → 追加 `_1`/`_2`… 后缀；目标等于自身源的原地条目不加后缀）与批内碰撞（先到先得）。
4. **文件-目录祖先冲突检测**：两个最终目标互为祖先/后代（`foo.mp3` vs `foo.mp3/bar.mp3`）→ 相关条目 `write_error`，剔除后**重跑** plan_targets。
5. **Pass 3** 状态判定（优先级从高到低）：
   `boundary_error` → `unreadable` → `write_error`（文件-目录冲突 / 目标祖先不可写或为文件或悬空 symlink / move 源父不可写 / move 重复源 / copy 大小写重命名）→ `conflict`（原始目标磁盘已存在，排除自身；悬空 symlink 视作已占用）→ `batch_conflict`（可整理条目间原始目标碰撞）→ `missing_metadata`（artist/album/year/genre 任一等于兜底值）→ `ok`。
   另有**链式重命名**浮出：ok/missing_metadata 但 final_target ≠ 原始渲染路径（本应占的槽被批内他条抢占）→ 改判 `conflict`。
6. 冲突改名后再次校验边界（消解器可能生成逃逸目标目录的路径）。
7. `build_directory_tree`：仅纳入会被执行的映射（剔除 boundary_error/unreadable/write_error），使目录预览只展示真正会创建的文件。

### 4.5 organizer

- `plan_targets(raws, sources)`：磁盘占用 = 文件存在 **或** 悬空 symlink；`claim_key` 在大小写不敏感文件系统（macOS/Windows）上折叠大小写。
- `preflight_check`（执行前只读预检，任一失败整批拒绝）。错误文案全表（测试锁定）：

| # | 文案 |
|---|---|
| 1 | `Source not found: {path}` |
| 2 | `Source is not a file: {path}` |
| 3 | `Source is not readable: {path}` |
| 4 | `Source parent directory is not writable (move requires write+execute on parent): {path}` |
| 5 | `Duplicate source in move batch (file can only be moved once): {path}` |
| 6 | `Case-only copy is not supported: source and destination are the same file on this filesystem: {src} -> {dst}` |
| 7 | `Target escapes the target directory: {final_target}` |
| 8 | `Target resolves to the target directory itself (not a valid file path): {final_target}` |
| 9 | `Duplicate final target in batch: {final_target}` |
| 10 | `Target path conflicts with another target in batch (file-vs-directory collision): {final_target}` |
| 11 | `Cannot determine write access for: {path}` |
| 12 | `Target ancestor is a broken symlink: {ancestor}. Cannot create path: {path}` |
| 13 | `Target ancestor is not a directory (it is a file): {ancestor}. Cannot create path: {path}` |
| 14 | `No write+execute permission for directory: {path}` |

- `organize_file(source, planned_target, mode)`：
  - 精确原地（normpath 字符串相等）→ 直接成功，不碰文件系统；
  - copy 模式纯大小写重命名 → 失败 `Case-only rename is not supported in copy mode: ...`；
  - move 模式纯大小写重命名 → 直接过（不走后缀消解，保证显示名更新）；
  - 执行期竞态冲突 → 现场追加 `_1` 后缀；
  - `mkdir -p` 目标父目录；move = `rename`（跨卷失败回退 copy+delete）；copy = 内容+权限位+尽力保留 mtime/atime；
  - PermissionDenied 错误格式化为 `Permission denied: {err}`，其余为 io 错误原文。

### 4.6 path_security / path_util

- `safe_resolve(raw, context)`：trim → 空 `"{context} must not be empty."`；含 `..` 段（正反斜杠均查）`"{context} contains disallowed path traversal components."`；resolve 后非绝对 `"{context} could not be resolved to an absolute path."`。
- `validate_source_dir` context = `Source directory`；`validate_target_dir` context = `Target directory`，另加存在但非目录 → `Target directory path exists but is not a directory.`
- `path_util` 提供跨平台语义：`normpath`（纯词法）、`resolve_lenient`（resolve(strict=False)）、`is_within`、`paths_equal_ci`（Windows 大小写不敏感）、`dir_writable_executable`（W_OK+X_OK）、`samefile` 等。

## 5. 任务系统（task.rs）

- `create_task(total)` → UUID v4 task_id，注册初始快照（pending, current=0）。
- 注册表：进程内 `Mutex<HashMap>`；**容量上限 32**（满时淘汰最旧终态）；**终态 TTL 300s 惰性淘汰**（get 时清理）。
- `run_organize`（后台 `std::thread::spawn` 执行，逐条映射）：
  1. 处理前：`{running, current: i-1, current_file: 源路径, message: ""}`
  2. 单条失败：`{error, current: i-1, current_file: 源路径, message: "Failed: {src}: {错误}"}`，**任务立即终止**（第一条错误即终止，不跳过继续）
  3. 单条成功：`{running, current: i, message: "Processed {i}/{total}"}`
  4. 全部完成：`{done, current: total, current_file: "", message: "Completed {N} file(s)."}`
- `get_snapshot(task_id)`：终态超 TTL 视同不存在；服务层包成 `Task not found: {task_id}` 错误。
- UI 消费方式：**轮询** `get_task_status`，1s 一次；无事件通道。

## 6. 服务层 API（service.rs，UI 直接调用）

| 函数 | 入参 | 返回 / 错误 |
|---|---|---|
| `scan_directory` | `(source_dir: String, recursive: Option<bool>)`，recursive 缺省 true | `ScanResponse { source_dir, total, files }`；`Message`（路径校验/扫描错误文案） |
| `generate_preview` | `PreviewRequest` | `PreviewResponse`；模板错误 `TemplateErrors`、校验错误 `Message` |
| `start_organize` | `(mappings, mode, target_dir)` | `{task_id, total}`；空批次 `No file mappings provided.`；预检失败 `PreflightErrors`；通过后 spawn 后台线程立即返回 |
| `get_task_status` | `(task_id)` | `ProgressEvent`；`Task not found: {id}` |
| `browse_dirs` | `(path)`，空串 = 根（Windows 盘符列表 / 其他平台 `$HOME`） | `{base_dir, entries: [{name, path}]}`；仅子目录、按名排序、无权限静默跳过；不存在 `路径不存在：{path}`（中文） |

阻塞函数，UI 层一律经 `run_service*` 丢后台线程。

## 7. UI 外壳与状态机（app.rs）

### 7.1 布局

- 根：纵向 flex、`bg-app`（#f8fafc）、字体 PingFang SC / 基准 14px / 行高 1.5。
- 顶栏（58px，白底、下边框）：品牌区（34×34 amber-500 圆角方块 Tag 图标 + `Tag2Folders` + 副标题 `音频文件智能整理 · 扫描 → 预览 → 执行`）｜右侧 `v2.0.1` 徽章 + 重置按钮（ghost sm）。
- 左步骤栏：固定 230px 白底；三步骤（`扫描文件/选择源目录与提取标签`、`模板预览/规划命名与结构方案`、`执行整理/批量安全归档与监控`）；38×38 瓦片分态（done=emerald+Check / active=amber+阴影 / dimmed=opacity 0.5）；连接线随解锁变 amber-400；点击仅 ≤ max_unlocked 可达。
- 工作区：flex 1、纵向滚动、padding 24、内容 max-width 1080 居中。

### 7.2 步骤状态机

- 初始 `current_step=1`、`max_unlocked_step=1`。
- 扫描完成：有文件 → `max_unlocked = max(prev, 2)`；无文件 → 锁回 1 且回步骤 1。
- 预览页"开始执行整理" → `max_unlocked=3; current_step=3`（**无二次确认弹窗**，移动模式的防护 = 预览页静态警告条 + 进度页"准备开始"文案）。
- "返回扫描" / "下一步" 直接切页，无解锁检查。
- `reset`（重置确认 / "完成并开启新任务"，均不再二次确认）：回步骤 1、max_unlocked=1、三 token+1 丢弃在途请求、清 taskId（内存+持久化文件）、重建三页。

### 7.3 确认弹窗（全应用仅两处）

| 触发 | title | message | description / tip | 按钮 |
|---|---|---|---|---|
| 顶栏重置 | `确认重置全部数据?` | `确定要清空当前的扫描结果、整理模板配置并重新开始吗?` | tip: `若当前有正在后台执行的文件整理任务,重置将断开界面追踪。` | `确认重置`/`取消`，Warning |
| 窗口关闭·有任务 | `确认退出应用?` | `确定要退出 Tag2Folders 吗?` | desc: `当前有正在进行或未完成的文件整理任务,退出将中断处理。` tip: `建议等待任务整理完成后再退出应用。` | `确认退出`/`取消`，Warning |
| 窗口关闭·无任务 | 同上 | 同上 | desc: `退出后当前未保存的配置与扫描缓存将被清除。` | 同上 |

- Escape = 取消、Enter = 确认（confirm_focus autoFocus）、点遮罩 = 取消。
- **has_running_task**：task_id 非空 **且** 快照未到终态（尚无快照视为进行中）。终态任务不报"有任务"。
- 窗口关闭拦截：`window.on_window_should_close`；确认退出 → `exit_confirmed=true` + `std::process::exit(0)`。

### 7.4 task_id 持久化与重连

- 文件：数据目录 `<data_dir>/tag2folders/state.json`，内容仅 `{"task_id": "..."}`；读写失败全部静默。
- `start_organize` 成功 → 先写盘再更新界面；reset / 新整理批次 → 删除文件。
- 启动重连：读取持久化 task_id → `get_task_status` 探测；任务存活 → `started=true` 恢复轮询（停留步骤 1）；已过期 → 静默清空（含文件）。

### 7.5 扫描页（步骤 1）

- 结构：扫描源目录卡 →（错误条 / 空结果提示）→ 结果卡（看板计数 + 筛选栏 + 表格 + 截断提示）→ 底部导航条。
- 源目录 DirPicker，placeholder `例如 D:\Music 或 /Users/me/Music`；递归复选框**默认勾选**（gpui-component Checkbox）；开始扫描按钮 primary（loading 文案 `正在扫描…`），**输入为空禁用**；主输入框 Enter = 快捷扫描。
- 输入变更效应（目录或递归变化，同值不触发）：清本页结果与筛选词、token+1 丢弃在途响应、App 级扫描数据同步清空（锁回步骤 1）。
- 扫描失败：rose 错误条 + 清结果；成功但 0 文件：sky 提示条 `未发现音频文件。请检查目录路径，或尝试开启「递归扫描子目录」后重新扫描。`
- 看板胶囊 4 枚：总文件数(amber) / 可读取(emerald) / 不可读取(rose) / 筛选结果(slate，仅有筛选词时)。
- 筛选：6 字段单选胶囊（文件名/艺术家/专辑/标题/年份/流派）+ 关键词输入 + 清空按钮（有词或非默认字段时显示）；匹配 = **大小写不敏感子串**，filename 字段只匹配 basename（`/` 与 `\` 切分取末段）。
- 文件表格 5 列：文件名 30% / 艺术家 18% / 专辑 20% / 标题 22% / 状态 10%（StatusBadge sm：ok/unreadable）；固定表头 + 表体容器内滚动（max 480）+ 外层横向滚动（min 560）；**最多渲染 200 行**，超出提示 `仅显示前 200 条，共 {N} 条。可使用筛选缩小范围。`；行 hover 无斑马纹、不可点击。
- 底部导航：左侧计数（`已筛选 x / y 个文件` / `共 N 个音频文件`）；右侧 `下一步：设置模板`（有筛选词时 `（N 个）`），无文件禁用。**带筛选词点击下一步 = 提交筛选子集为 App 级数据**（下游预览只用被筛过的文件）。

### 7.6 预览页（步骤 2）

- 无扫描数据：amber 警告条 `尚未扫描任何文件，请先完成扫描步骤。` + `前往扫描` 按钮。
- 整理配置卡（`整理配置` / `设置目标目录与命名模板，点击占位符即可插入`）：
  - 目标目录 DirPicker（label `目标目录`，placeholder 动态：`留空则整理到源目录` / `留空则整理到源目录（{sourceDir}）`；**留空 = 整理到源目录**）；
  - 命名模板：mono 输入框，**默认值 = placeholder = `{album}/{track}. {title}.{ext}`**；7 枚占位符芯片（`{artist}`艺术家/`{album}`专辑/`{title}`标题/`{track}`音轨号/`{year}`年份/`{genre}`流派/`{ext}`后缀），点击插入——输入框聚焦中插到内部光标处、未聚焦追加到末尾，随后聚焦输入框；
  - 操作模式分段 toggle：`复制（保留源文件）` / `移动（删除源文件）`，**默认 Move**；选中 move 时下方显示警告条（`移动模式不可逆：`加粗 + 后续文案）；
  - 错误条（rose，pre-wrap 多行）：模板错误 Ok 路径以 `；` 连接、Err 路径（ServiceError Display）以 `\n` 连接；
  - `生成预览` 按钮（loading `生成预览中…`），`loading || 无文件 || 模板空白` 禁用。
- **表单变更效应**（模板/目标目录/模式任一变化，同值不触发）：作废在途响应 + 清预览结果与 App 级整理批次（"开始执行整理"随之消失）。
- 生成预览：`effective_target = 目标目录.trim() || sourceDir`；发起前先清旧结果（失败时旧计划不可执行）。
- 结果区（mappings 非空才整体显示）：
  - 统计 6 卡：文件总数 / 正常 / 冲突（conflict+batch_conflict）/ 缺失信息 / 不可读 / 越界+写入受阻；flex wrap、每卡 min-width 150；
  - Tabs 分段切换：`详细映射列表`（带总数徽章）/ `目录树层级预览`（激活时右侧附注 `点击文件夹可展开 / 折叠`）；
  - 映射表 3 列：源文件（basename）38% / 目标路径（**final_target 完整路径**）46% / 最终状态（StatusBadge sm）16%；最多渲染 **300 行**（`仅显示前 300 条映射，共 {N} 条。`）；冲突行**无特殊底色**（以 amber 徽章表达）；行不可点击；
  - 目录树：头部工具栏（Layers 图标 + `目标目录结构` + 过滤输入 `过滤文件...` + `全部折叠/全部展开` 切换）；主体 min 140 / max 420 容器内滚动；**默认展开 depth < 2**；目录行（缩进 depth×20+6、数量徽标 `(直接子项数)`）点击切换开合；文件行缩进 (depth+1)×20+8、mono 12.5；**过滤仅作用于文件名**（小写子串，整组无匹配则不渲染该组）；同层排序 = 字典序（serde_json BTreeMap）；
  - 底部导航：`返回扫描`（outline）｜`开始执行整理（{N} 个文件）`，N = 总数 − unreadable − boundary_error − write_error，**N=0 禁用**。
- 开始执行整理：剔除 `unreadable / boundary_error / write_error` 三类映射（预检会整批拒绝），整理参数交 App 级（target = resolvedTargetDir || 目标目录 || sourceDir），清旧 taskId，解锁步骤 3。
- App 级整理批次复位时 `organize_mode` 回 **Move**（与页面默认一致）。

### 7.7 进度页（步骤 3）

- 无映射：amber 警告条 `没有待处理的文件，请先完成扫描和预览步骤。`（无按钮）。
- 任务概览卡：操作模式徽章（`移动（删除源文件）`/`复制（保留源文件）`）/ 目标目录 mono chip（空显示 `（未设置）`）/ 待处理总数（amber 大数字）。
- 准备开始卡（未发起）：说明文字（`将移动/复制 N 个文件到目标目录，…`，计数加粗 amber）+ errMsg（发起失败时，rose）+ `开始执行` 按钮；`starting` 防双击；**发起失败 started 回 false 可重试**。
- 执行进度卡（进行中）：百分比大字（32px amber-800，`round(current/total*100)`，无快照或 total=0 → 0）+ `x / y 已处理`（无快照 `等待任务开始…`）+ 进度条（轨道 12 圆角 full slate-100、填充 amber-500）+ 当前文件条（脉冲圆点 + `正在处理` + mono 文件名）。
- 完成横幅（done）：emerald，`整理完成` + `共处理 {N} 个文件，任务已成功结束。` + `完成并开启新任务`（= 直接 reset，不弹确认）。
- 失败横幅（error 终态）：rose，`任务执行失败` + 错误正文（pre-wrap；message 为空时显示 `执行出错`）+ 同款完成按钮。
- 实时日志卡（`实时日志` + TERMINAL 徽章）：slate-950 控制台、mono 12 / 行高 1.8、max 260；行着色：`[n/total]` 前缀 amber-400 + 正文 sky-300，非括号行 slate-400；`current_file` 行与上一行相同不重复追加，message 行不去重；**缓冲上限 300**（丢最旧）；**滚动锚定**：距底 ≤40px 时新日志自动跟底，上翻不打扰。
- 轮询：1s 一次 `get_task_status`；查询失败**静默重试不断轮询**；done/error 停止；token 失效（reset/新任务）自杀。

## 8. 设计 token（theme.rs）

- 颜色常量命名对应 CSS 变量（`--slate-50` → `SLATE_50`）。色板：slate 50-950、amber 50-900、emerald/rose/sky 50-200+500-700。
- ⚠️ **amber 系为历史定下的有效值，勿按色阶惯例"纠正"**：`AMBER_500 = #f59e0b`（品牌主色）、`AMBER_400 = #ffc533` 等，全 UI 配色按现值调过。`amber-950` 不存在，芯片悬浮文字用 `INHERITED_TEXT = #0f172a`。
- 功能色：`BG_APP #f8fafc`、`BG_SURFACE #fff`、`BORDER_SUBTLE #e2e8f0`、`BORDER_DEFAULT #cbd5e1`、`BORDER_FOCUS #ffae00`（硬编码）、遮罩 `rgba(15,23,42,0.55)`、聚焦光晕 `rgba(255,174,0,0.2)`。
- 圆角 4/6/8/12/16/full；阴影 xs→xl + 组件硬编码阴影（主按钮/品牌方块/激活瓦片/确认弹窗/底部条，均见 theme.rs 函数）。
- 字体：sans `PingFang SC`、mono `Menlo`（运行时已验证可解析）、基准 14px / 1.5。
- 时长参考：150/200/300ms、spin 1s、pulse 2s、进度 250ms（gpui 无 CSS 过渡，多数静态）。
- gpui-component 主题接管（`apply_to_gpui_component`）：primary=amber-500、primary 前景=slate-800、danger=rose-600、success=emerald-600、info=sky-600、ring=amber-500、radius 8px、shadow=false。

## 9. 组件清单（ui/）

| 组件 | 要点 |
|---|---|
| `Button` | variant primary/secondary/outline/ghost/danger × size sm/md/lg；loading = 禁用+旋转图标；disabled opacity 0.55；可覆写 height/pad_x/text_size |
| `badge` / `BadgeVariant` | emerald/amber/rose/sky/slate 三色胶囊；`StatusBadge` 七状态映射：ok=正常(emerald)、conflict=磁盘冲突(amber)、batch_conflict=批内冲突(amber)、missing_metadata=缺失信息(sky)、unreadable=不可读(slate)、boundary_error=路径越界(rose)、write_error=写入受阻(rose)；未知值原样 slate |
| `Card` | title/subtitle/actions + padding 档位 none/sm/md/lg = 0 / 12×16 / 18×22 / 24×28；白底圆角 12 |
| `AlertBar` | rose/amber/sky 三变体内联提示条；支持 pre_wrap 多行 |
| `ProgressBar` | `new(current, total)`，amber-500 填充 |
| `StepNav` | STEPS 常量 + 分态瓦片 + 连接线；`step_nav_aside()` 230px 容器 |
| `Modal` / `ConfirmModal` | ConfirmOptions 默认：confirm `确定`/cancel `取消`/tone Warning/width 460；四 tone（Warning/Danger/Info/Primary）；Escape=取消、Enter=确认 |
| `DirPicker` | 输入框(mono h38 + Folder 图标 + 清空按钮) + `浏览...` 按钮；**原生对话框优先，打开失败降级内置浏览模态**；模态：主页/上一级按钮、路径输入（Enter 跳转 / Escape 关闭 / Blur 回显）、过滤（name 小写子串）、条目整行点击=进入、footer `共 N 个子文件夹` + `取消` + `选择此目录`（currentPath 为空禁用）+ 当前选择预览条 |
| `Icon` | 32 枚举；SVG 24×24 stroke=currentColor；**着色 = `.text_color()`（alpha 遮罩机制）**；assets 33 个 svg（check 双用、circle-x 为组件库清空按钮） |

## 10. 边界行为速查

| 项 | 值 / 行为 |
|---|---|
| 扫描表截断 | 200 行（UI 截断，数据完整） |
| 映射表截断 | 300 行 |
| 日志缓冲 | 300 条，丢最旧；current_file 行连续去重 |
| 任务终态保留 | 300s（TTL 惰性淘汰）；注册表容量 32 |
| 轮询间隔 | 1s；查询失败静默重试 |
| 任务失败语义 | 第一条错误即终止（非逐文件跳过） |
| 竞态防护 | scan/preview/progress 三 token，回调比对丢弃过期响应 |
| 大小写不敏感盘 | claim_key 折叠大小写；copy 模式纯大小写重命名被拒；move 模式放行 |
| 悬空 symlink | 视作磁盘已占用（冲突消解与可写探测都检查 `is_symlink`） |
| Windows 特例 | 保留名转义、尾部点/空格清洗、盘符根浏览、路径组件大小写不敏感比较 |
| 空目标目录 | 整理到源目录（effectiveTarget 回落） |
| 移动默认 | **预览页默认 Move**（注意：这是有意的产品默认，非 bug） |
| 确认弹窗 | 仅重置与退出两处；其余操作（开始整理/完成新任务）不弹确认 |

## 11. 截图取证（shot.rs）

- 环境变量：`T2F_SHOT_STATES`（`empty,scan,preview,preview_tree,progress` 任选）+ `T2F_SHOT_DIR`；未设置零开销。
- ScreenCaptureKit 截自有窗口（无需屏幕录制权限）；`WAIT_ON_SCREEN 8s + SETTLE 1.2s`。
- 演示态注入走**真实** `service::generate_preview` 计算（`AppShell::setup_shot_state`），保证截图忠实于实际代码路径。

## 12. 测试基线

`cargo test` = lib 67 + bin 13。错误文案（模板校验、预检、路径校验、扫描）与关键行为（筛选、树开合、日志去重、task_id 持久化往返、冲突消解）均由单测锁定；改文案须同步用例。

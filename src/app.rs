//! 应用外壳:顶栏 + 左步骤栏 + 右工作区 + 状态机 + 重置确认。
//!
//! ## 状态架构约定
//!
//! - 根实体为 [`AppShell`](实现 Render),持有全部向导状态:
//!   `current_step` / `max_unlocked_step` / 三个页面结构体 / 待挂载的 Modal 状态;
//! - **页面是普通 struct(非独立 Entity)**,字段直接挂在 [`AppShell`] 上;
//!   页面内部需要高交互控件时,持有对应 `Entity`(如 `Entity<DirPickerState>`、
//!   `Entity<InputState>`),在页面构造函数里 `cx.new` + `cx.subscribe` 建立
//!   事件回路(订阅句柄 `Subscription` 存进 AppShell,随 reset 一起丢弃重建);
//! - [`AppShell::reset`] **重建页面结构体**清空三页内部状态,并归位
//!   step/max_unlocked/清理 taskId;
//! - "三个页面状态常驻"(切页不丢页面内部状态)由"struct 字段常驻 + render
//!   按 current_step 切换"满足。

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt, Context, DefiniteLength, Entity, FocusHandle, HighlightStyle, Pixels,
    ScrollHandle, SharedString, StyledText, Subscription, Window, div, px,
};

use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState};
use serde::{Deserialize, Serialize};
use tag2folders_lib::core::preview::PreviewRequest;
use tag2folders_lib::core::{AudioMetadata, FileMappingItem, MappingStatus, OrganizeMode};
use tag2folders_lib::service;
use tag2folders_lib::task::{ProgressEvent, TaskStatus};

use crate::ui::components::{
    AlertBar, AlertVariant, BadgeVariant, Button, ButtonSize, ButtonVariant, CardPadding,
    ConfirmModal, ConfirmOptions, ConfirmTone, ProgressBar, StatusBadge, StatusBadgeSize, StepNav,
    badge, card, step_nav_aside,
};
use crate::ui::dir_picker::{DirPickerEvent, DirPickerState, render_dir_picker};
use crate::ui::service::{run_service_in, run_service_result};
use crate::ui::theme;
use crate::ui::{Icon, icon_16, icon_sized};

// ── 页面──────────────────────────────────────────

/// 扫描页快速筛选字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterField {
    Filename,
    Artist,
    Album,
    Title,
    Year,
    Genre,
}

impl FilterField {
    /// (字段, 胶囊文案)全表,顺序固定。
    pub const ALL: [(FilterField, &'static str); 6] = [
        (FilterField::Filename, "文件名"),
        (FilterField::Artist, "艺术家"),
        (FilterField::Album, "专辑"),
        (FilterField::Title, "标题"),
        (FilterField::Year, "年份"),
        (FilterField::Genre, "流派"),
    ];

    /// 小写字段 key。
    pub fn key(self) -> &'static str {
        match self {
            FilterField::Filename => "filename",
            FilterField::Artist => "artist",
            FilterField::Album => "album",
            FilterField::Title => "title",
            FilterField::Year => "year",
            FilterField::Genre => "genre",
        }
    }

    /// 取文件行上对应字段的字符串值;`filename` 取 path 的 basename(源:
    /// `f.path.split(/[/\\]/).pop() ?? ''`,其余字段取元数据属性)。
    fn value_of(self, f: &AudioMetadata) -> &str {
        match self {
            FilterField::Filename => basename(&f.path),
            FilterField::Artist => &f.artist,
            FilterField::Album => &f.album,
            FilterField::Title => &f.title,
            FilterField::Year => &f.year,
            FilterField::Genre => &f.genre,
        }
    }
}

/// JS `p.split(/[/\\]/).pop() ?? p`:按 `/` 与 `\` 切分取最后一段。
fn basename(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

/// 筛选匹配规则:大小写不敏感的子串包含。`kw_lower` 已 trim+lowercase。
fn matches_filter(field: FilterField, kw_lower: &str, f: &AudioMetadata) -> bool {
    field.value_of(f).to_lowercase().contains(kw_lower)
}

/// 表格最多渲染行数(纯 UI 截断,父级数据完整)。
const TABLE_LIMIT: usize = 200;

/// 表格体容器最大高度:表头固定行 + 表体容器内滚动(gpui 无 position:sticky)。
const TABLE_BODY_MAX_H: Pixels = px(480.0);

// ── 预览页常量────────────────────────────────────

/// 模板默认值 / 输入框占位文案(默认 `{album}/{track}. {title}.{ext}`，track 不足两位补零)。
const DEFAULT_TEMPLATE: &str = "{album}/{track}. {title}.{ext}";

/// 占位符芯片全表。
const PLACEHOLDERS: [(&str, &str); 7] = [
    ("{artist}", "艺术家"),
    ("{album}", "专辑"),
    ("{title}", "标题"),
    ("{track}", "音轨号"),
    ("{year}", "年份"),
    ("{genre}", "流派"),
    ("{ext}", "后缀"),
];

/// 映射表最多渲染行数。
const PREVIEW_TABLE_LIMIT: usize = 300;

/// 目录树默认展开深度。
const TREE_INITIAL_EXPANDED_DEPTH: usize = 2;

/// 目录树哨兵键(子树内直接文件列表)与转义形式(见 preview.rs build_directory_tree)。
const TREE_SENTINEL: &str = "__files__";
const TREE_ESCAPED_SENTINEL: &str = "__files__\u{0}";

/// 目录树主体最大/最小高度。
const TREE_BODY_MAX_H: Pixels = px(420.0);
const TREE_BODY_MIN_H: Pixels = px(140.0);

// ── 进度页常量───────────────────────────────────────────

/// 日志缓冲上限。
const LOG_CAP: usize = 300;

/// 日志控制台最大高度。
const LOG_CONSOLE_MAX_H: Pixels = px(260.0);

/// 滚动锚定阈值。
const LOG_BOTTOM_ANCHOR: Pixels = px(40.0);

/// 任务状态轮询间隔。
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// task_id 持久化文件:数据目录 `<data>/tag2folders/state.json`
/// (源 localStorage `tag2folders_task_id` 的桌面等价物,仅存 task_id)。
const STATE_DIR: &str = "tag2folders";
const STATE_FILE: &str = "state.json";

/// 预览结果区视图切换。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewTab {
    /// 详细映射列表
    List,
    /// 目录树层级预览
    Tree,
}

/// 步骤 1:扫描文件。页面为普通 struct(状态挂在 AppShell 上),
/// 高交互控件(DirPicker/筛选输入)持有独立 Entity。
pub struct ScanPage {
    /// 源目录选择(值 = `dir.read(cx).value()`)
    pub dir: Entity<DirPickerState>,
    /// 页面级 sourceDir 镜像(仅用于"值未变化则不触发作废"的 React setState 语义)
    pub source_dir: String,
    /// 递归扫描子目录(默认勾选)
    pub recursive: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub files: Vec<AudioMetadata>,
    pub has_scanned: bool,
    /// 当前筛选字段(默认 filename)
    pub filter_field: FilterField,
    /// 筛选关键词(gpui-component InputState)
    pub filter_input: Entity<InputState>,
}

impl ScanPage {
    fn new(window: &mut Window, cx: &mut Context<AppShell>) -> Self {
        let dir = cx.new(|cx| {
            DirPickerState::new("例如 D:\\Music 或 /Users/me/Music", window, cx)
        });
        let filter_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("输入关键词筛选…"));
        Self {
            dir,
            source_dir: String::new(),
            recursive: true,
            loading: false,
            error: None,
            files: Vec::new(),
            has_scanned: false,
            filter_field: FilterField::Filename,
            filter_input,
        }
    }

    /// 筛选结果:关键词 trim 后为空 → 原样返回;否则按当前字段做大小写不敏感
    /// 子串匹配。
    fn filtered_files(&self, cx: &gpui::App) -> Vec<AudioMetadata> {
        let kw_raw = self.filter_input.read(cx).value();
        let kw = kw_raw.trim().to_lowercase();
        if kw.is_empty() {
            return self.files.clone();
        }
        self.files
            .iter()
            .filter(|f| matches_filter(self.filter_field, &kw, f))
            .cloned()
            .collect()
    }
}

/// 步骤 2:模板预览。页面为普通 struct(状态挂在 AppShell 上),
/// 高交互控件(目标目录 DirPicker / 模板输入 / 树过滤输入)持有独立 Entity。
pub struct PreviewPage {
    /// 目标目录选择(placeholder 动态:留空则整理到源目录(源目录))
    pub dir: Entity<DirPickerState>,
    /// 命名模板输入(mono;默认值与 placeholder 均为 `{album}/{track}. {title}.{ext}`)
    pub template_input: Entity<InputState>,
    /// 模板输入框是否聚焦(占位符插入的分支条件,等价源 `document.activeElement === el`)
    pub template_focused: bool,
    /// 当前悬浮的占位符芯片下标(源 PlaceholderChip 自带 hovered 态;gpui 需上提)
    pub hovered_chip: Option<usize>,
    /// 操作模式(默认 move;表单变更之一)
    pub mode: OrganizeMode,
    pub loading: bool,
    pub error: Option<String>,
    pub mappings: Vec<FileMappingItem>,
    pub directory_tree: serde_json::Value,
    pub resolved_target_dir: String,
    /// 结果区视图切换(list=映射表 / tree=目录树)
    pub active_tab: PreviewTab,
    /// 目录树过滤输入
    pub tree_filter: Entity<InputState>,
    /// 目录树全部展开开关(源 expandAll;翻转 = 源 key 变更强制重挂载、重置全部开合)
    pub tree_expand_all: bool,
    /// 被用户手动切换过开合的树节点(与默认开合相反;expandAll 翻转时清空)
    pub tree_toggled: HashSet<String>,
    // ── 表单值镜像:React useEffect 依赖数组"同值不触发 effect"的等价实现 ──
    pub form_template: String,
    pub form_target_dir: String,
}

impl PreviewPage {
    fn new(window: &mut Window, cx: &mut Context<AppShell>) -> Self {
        let dir = cx.new(|cx| {
            let mut s = DirPickerState::new("留空则整理到源目录", window, cx);
            s.label = Some("目标目录".into());
            s
        });
        let template_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(DEFAULT_TEMPLATE)
                .placeholder(DEFAULT_TEMPLATE)
        });
        let tree_filter = cx.new(|cx| InputState::new(window, cx).placeholder("过滤文件..."));
        Self {
            dir,
            template_input,
            template_focused: false,
            hovered_chip: None,
            mode: OrganizeMode::Move,
            loading: false,
            error: None,
            mappings: Vec::new(),
            directory_tree: empty_tree(),
            resolved_target_dir: String::new(),
            active_tab: PreviewTab::List,
            tree_filter,
            tree_expand_all: true,
            tree_toggled: HashSet::new(),
            form_template: DEFAULT_TEMPLATE.to_string(),
            form_target_dir: String::new(),
        }
    }
}

/// 空目录树(`{}`)。
fn empty_tree() -> serde_json::Value {
    serde_json::Value::Object(Default::default())
}

/// 开始整理前剔除的状态(后端预检会整批拒绝这三类)。
fn is_organizable(m: &FileMappingItem) -> bool {
    !matches!(
        m.status,
        MappingStatus::Unreadable | MappingStatus::BoundaryError | MappingStatus::WriteError
    )
}

/// 树节点默认开合(默认展开 0、1 层);用户手动切换过的节点取反,
/// expandAll 翻转后重置。
fn tree_node_open(expand_all: bool, user_toggled: bool, depth: usize) -> bool {
    let default_open = expand_all && depth < TREE_INITIAL_EXPANDED_DEPTH;
    if user_toggled {
        !default_open
    } else {
        default_open
    }
}

/// 目录树过滤:文件名小写子串匹配;空过滤全通过。
fn tree_file_matches(file: &str, filter_lower: &str) -> bool {
    filter_lower.is_empty() || file.to_lowercase().contains(&filter_lower.to_lowercase())
}

/// 转义哨兵键解码:目录组件恰好叫 `__files__` 时后端存为 `__files__\0`
/// (preview.rs build_directory_tree),展示时还原。
fn decode_tree_key(key: &str) -> &str {
    if key == TREE_ESCAPED_SENTINEL {
        TREE_SENTINEL
    } else {
        key
    }
}

/// 步骤 3:执行整理。页面为普通 struct(状态挂在
/// AppShell 上);轮询循环与 task_id 持久化在 AppShell 层——token 必须跨
/// 页面重建存续(同 scan_token/preview_token,见 progress_token 字段)。
pub struct ProgressPage {
    /// 最新任务快照(源 progress:ProgressEvent|null)
    pub progress: Option<ProgressEvent>,
    /// 任务已发起(startOrganize 成功或重连)后为 true(源 started;
    /// 控制"准备开始"卡与进度区/终态横幅的互斥)
    pub started: bool,
    /// 实时日志行
    pub log: Vec<String>,
    /// 日志控制台滚动句柄(滚动锚定:新日志到达且停留在底部附近时自动跟随)
    pub log_scroll: ScrollHandle,
    /// 任务已完成(源 done;终态 status=done)
    pub done: bool,
    /// 错误消息(源 errMsg;startOrganize 失败或任务 error 终态)
    pub error: Option<String>,
    /// "开始执行"防双击(源 startingRef)
    pub starting: bool,
}

impl ProgressPage {
    fn new() -> Self {
        Self {
            progress: None,
            started: false,
            log: Vec::new(),
            log_scroll: ScrollHandle::new(),
            done: false,
            error: None,
            starting: false,
        }
    }
}

// ── task_id 持久化(源 localStorage `tag2folders_task_id` 的桌面等价物)──

/// state.json 内容(仅存 task_id)。
#[derive(Debug, Serialize, Deserialize)]
struct PersistedState {
    task_id: String,
}

fn state_file_path() -> Option<PathBuf> {
    dirs::data_dir().map(|base| base.join(STATE_DIR).join(STATE_FILE))
}

/// 读取持久化的 task_id;任何失败(缺文件/内容损坏/无数据目录)静默返回空串。
fn load_persisted_task_id() -> String {
    state_file_path()
        .map(|path| read_state_file(&path))
        .unwrap_or_default()
}

fn read_state_file(path: &std::path::Path) -> String {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<PersistedState>(&text).ok())
        .map(|s| s.task_id)
        .unwrap_or_default()
}

/// 写入 task_id;写盘失败静默忽略(任务仍在内存注册表内,本轮会话可追踪)。
fn persist_task_id(id: &str) {
    if let Some(path) = state_file_path() {
        write_state_file(&path, id);
    }
}

fn write_state_file(path: &std::path::Path, id: &str) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let state = PersistedState {
        task_id: id.to_string(),
    };
    if let Ok(text) = serde_json::to_string(&state) {
        let _ = std::fs::write(path, text);
    }
}

/// 清除持久化的 task_id(源 setTaskId('') → localStorage.removeItem)。
fn clear_persisted_task_id() {
    if let Some(path) = state_file_path() {
        let _ = std::fs::remove_file(path);
    }
}

// ── 进度页纯逻辑辅助─────────────────────────

/// pct = progress && total>0 ? round(current/total*100) : 0。
fn task_percent(progress: Option<&ProgressEvent>) -> usize {
    progress
        .and_then(|p| {
            (p.total > 0).then(|| ((p.current as f32 / p.total as f32) * 100.0).round() as usize)
        })
        .unwrap_or(0)
}

/// 日志行颜色分级:源正则 `^(\[[^\]]*\])\s*(.*)$` 的
/// 等价实现——行首 `[...]`(首个 `]` 即闭括号)为前缀,`\s*` 消耗其后的
/// 全部空白;返回 (前缀含括号, 余文)。不匹配返回 None(整行 slate-400)。
fn split_log_line(line: &str) -> Option<(&str, &str)> {
    if !line.starts_with('[') {
        return None;
    }
    let close = line.find(']')?;
    let prefix = &line[..=close];
    let rest = line[close + 1..].trim_start();
    Some((prefix, rest))
}

/// 追加 message 日志行。
fn append_log_line(log: &mut Vec<String>, line: String) {
    log.push(line);
    if log.len() > LOG_CAP {
        log.remove(0);
    }
}

/// 追加 current_file 日志行。
fn append_log_line_dedup(log: &mut Vec<String>, line: String) {
    if log.last().map(|l| l == &line).unwrap_or(false) {
        return;
    }
    append_log_line(log, line);
}

/// 日志控制台是否停留在底部附近(距底 ≤40px,滚动锚定阈值)。
/// 依据 gpui ScrollHandle:max_offset.height + offset.y 即距底像素
/// (内容未溢出时为 0,天然视为"在底部",与源 atBottomRef 初始 true 一致)。
fn log_is_at_bottom(handle: &ScrollHandle) -> bool {
    (handle.max_offset().height + handle.offset().y) < LOG_BOTTOM_ANCHOR
}

// ── 确认弹窗状态 ─────────────────────────────────────────────────────────────

#[derive(Debug)]
enum ConfirmAction {
    /// 顶栏"重置"
    Reset,
    /// 窗口关闭
    Exit,
}

struct PendingConfirm {
    options: ConfirmOptions,
    action: ConfirmAction,
}

// ── 根实体 ───────────────────────────────────────────────────────────────────

pub struct AppShell {
    /// 当前步骤 1|2|3
    current_step: usize,
    /// 已解锁的最大步骤 1|2|3(点击规则:只能访问 ≤ 此值)
    max_unlocked_step: usize,
    /// 重置计数(源 resetKey;重建页面即等价重挂载,计数仅供诊断/动画 key)
    reset_key: u32,

    /// App 级扫描数据(源 App.tsx `scannedFiles`,handleScanComplete 写入;
    /// 预览页消费:generate_preview 的入参文件列表)
    pub scanned_files: Vec<AudioMetadata>,
    /// App 级源目录(源 App.tsx `sourceDir`;注意是提交值:扫描成功 = trim 后
    /// 输入、"下一步"带筛选 = 页面原值、作废/失败 = '')
    pub source_dir: String,

    /// App 级整理批次(源 App.tsx `mappings`):预览页"开始执行整理"写入,
    /// 表单变更/重扫描即作废(onClearOrganize)。**给 D5 进度页**:以
    /// `start_organize(organize_mappings, organize_mode, organize_target_dir)` 发起任务。
    pub organize_mappings: Vec<FileMappingItem>,
    /// App 级整理模式(源 organizeMode;作废/重扫复位 copy)
    pub organize_mode: OrganizeMode,
    /// App 级整理目标目录(源 targetDir;= resolvedTargetDir || targetDir || sourceDir)
    pub organize_target_dir: String,
    /// 进行中整理任务 id(源 App.tsx taskId / localStorage `tag2folders_task_id`
    /// 的内存部分;onOrganize 与 reset 置 '')。start_organize 成功后写入 +
    /// 数据目录持久化;get_task_status 轮询凭它追踪;退出确认的
    /// has_running_task 以本字段非空且任务未终态为准
    pub task_id: String,

    pub scan: ScanPage,
    pub preview: PreviewPage,
    pub progress: ProgressPage,

    /// 扫描竞态 token:输入变更/递归切换时 +1,在途响应比对后丢弃。
    /// 放 AppShell(而非 ScanPage)是因为 reset 会重建页面结构体,token 必须
    /// 跨重建单调递增,才能丢弃"重置前发起"的在途扫描(等价源卸载时 token+1)。
    scan_token: u64,

    /// 预览请求竞态 token:表单变更/新预览/reset 时 +1,
    /// 回调比对后丢弃过期响应。与 scan_token 同理放 AppShell 跨页面重建。
    preview_token: u64,

    /// 任务发起/轮询竞态 token:reset、新任务发起、
    /// 重连时 +1。start_organize 回调比对后丢弃过期响应;轮询循环比对后自杀。
    /// 与 scan_token 同理放 AppShell 跨页面重建。
    progress_token: u64,

    /// 待挂载的确认弹窗(单例:重置/退出)
    confirm: Option<PendingConfirm>,
    confirm_focus: FocusHandle,
    /// 全局根容器焦点句柄(用于快捷键分发)
    pub focus_handle: FocusHandle,
    /// 退出已确认(允许本次关窗)
    exit_confirmed: bool,
    _subs: Vec<Subscription>,
}

impl AppShell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let confirm_focus = cx.focus_handle();
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);
        let mut shell = Self {
            current_step: 1,
            max_unlocked_step: 1,
            reset_key: 0,
            scanned_files: Vec::new(),
            source_dir: String::new(),
            organize_mappings: Vec::new(),
            organize_mode: OrganizeMode::Move,
            organize_target_dir: String::new(),
            task_id: String::new(),
            scan: ScanPage::new(window, cx),
            preview: PreviewPage::new(window, cx),
            progress: ProgressPage::new(),
            confirm: None,
            confirm_focus,
            focus_handle,
            exit_confirmed: false,
            scan_token: 0,
            preview_token: 0,
            progress_token: 0,
            _subs: Vec::new(),
        };

        // 扫描页/预览页事件回路(见 wire_page_subscriptions)
        shell.wire_page_subscriptions(window, cx);

        // 任务重连:读取持久化 task_id,
        // 任务仍在注册表内(终态未超 300s 被淘汰)→ 恢复轮询追踪(页面状态
        // 保持步骤 1,与源刷新后 currentStep=1、mappings 丢失一致);
        // 已过期/不存在 → 静默清空(含持久化文件),停留步骤 1。
        shell.task_id = load_persisted_task_id();
        if !shell.task_id.is_empty() {
            if service::get_task_status(shell.task_id.clone()).is_ok() {
                shell.progress.started = true;
                shell.start_progress_polling(cx);
            } else {
                shell.task_id.clear();
                clear_persisted_task_id();
            }
        }

        shell
    }

    // ── 步骤状态机───────────────────────────────────────────────

    /// (重)建立页面事件回路。`new` 与 `reset` 都要调用:页面结构体重建后,
    /// 订阅必须指向新实体(订阅句柄随 reset 一起丢弃重建)。
    fn wire_page_subscriptions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._subs.clear();

        // 扫描页:DirPicker 事件
        let scan_dir = self.scan.dir.clone();
        self._subs.push(cx.subscribe_in(
            &scan_dir,
            window,
            |this, _entity, ev: &DirPickerEvent, window, cx| match ev {
                // 输入变更效应:清本页结果、作废在途响应、同步清空 App 级扫描数据
                DirPickerEvent::Changed(v) => this.on_scan_dir_changed(v.clone(), window, cx),
                // Enter 快捷扫描(仅主输入框会发出该事件;浏览模态内 Enter 走导航、
                // 复选框不发出,天然等价源容器级 onKeyDown 的排除规则)
                DirPickerEvent::Enter => this.handle_scan(window, cx),
            },
        ));
        // 筛选关键词变化 → 重算筛选(纯渲染态,只需重绘)
        let scan_filter = self.scan.filter_input.clone();
        self._subs.push(cx.subscribe(
            &scan_filter,
            |_this, _entity, _ev: &InputEvent, cx| cx.notify(),
        ));
        // 预览页:目标目录变化 → 作废已有预览结果
        let preview_dir = self.preview.dir.clone();
        self._subs.push(cx.subscribe(
            &preview_dir,
            |this, _entity, ev: &DirPickerEvent, cx| {
                if let DirPickerEvent::Changed(v) = ev {
                    this.on_preview_target_changed(v.clone(), cx);
                }
            },
        ));
        // 模板输入:Change → 作废;Focus/Blur → 维护光标插入的分支条件
        let preview_template = self.preview.template_input.clone();
        self._subs.push(cx.subscribe(
            &preview_template,
            |this, _entity, ev: &InputEvent, cx| match ev {
                InputEvent::Change => this.on_preview_template_changed(cx),
                InputEvent::Focus => {
                    this.preview.template_focused = true;
                    cx.notify();
                }
                InputEvent::Blur => {
                    this.preview.template_focused = false;
                    cx.notify();
                }
                _ => {}
            },
        ));
        // 目录树过滤:纯渲染态,只需重绘
        let tree_filter = self.preview.tree_filter.clone();
        self._subs.push(cx.subscribe(
            &tree_filter,
            |_this, _entity, _ev: &InputEvent, cx| cx.notify(),
        ));
    }

    fn go_to_step(&mut self, step: usize, cx: &mut Context<Self>) {
        if step <= self.max_unlocked_step {
            self.current_step = step;
            cx.notify();
        }
    }

    // ── 扫描页逻辑─────────────────────────────────────

    /// 开始扫描:token 竞态防护 + 后台线程执行(重 IO 不阻塞 UI)。
    /// 回调带 window:handleScanComplete 需更新预览页目标目录 placeholder(InputState)。
    fn handle_scan(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let raw = self.scan.dir.read(cx).value(cx);
        let source_dir = raw.trim().to_string();
        if source_dir.is_empty() {
            return;
        }
        let recursive = self.scan.recursive;
        let token = self.scan_token;
        self.scan.loading = true;
        self.scan.error = None;
        cx.notify();

        let work_dir = source_dir.clone();
        run_service_in(
            _window,
            cx,
            move || service::scan_directory(work_dir.clone(), Some(recursive)),
            move |this, result, window, cx| {
                // 在途时输入已变更(或已重置)→ 丢弃本次响应
                if this.scan_token != token {
                    return;
                }
                this.scan.loading = false;
                match result {
                    Ok(resp) => {
                        this.scan.files = resp.files.clone();
                        this.scan.has_scanned = true;
                        this.handle_scan_complete(resp.files, source_dir, window, cx);
                    }
                    Err(msg) => {
                        // 失败:清旧结果,旧表格与"下一步"不可用;App 级数据同步清空
                        this.scan.error = Some(msg.to_string());
                        this.scan.files = Vec::new();
                        this.scan.has_scanned = true;
                        this.handle_scan_complete(Vec::new(), String::new(), window, cx);
                    }
                }
            },
        );
    }

    /// 源目录输入变化(手输/清空/原生对话框/浏览模态选定)。
    /// 值未变化不触发作废(React setState 同值不触发 effect 的语义)。
    fn on_scan_dir_changed(&mut self, new_dir: String, window: &mut Window, cx: &mut Context<Self>) {
        if new_dir == self.scan.source_dir {
            return;
        }
        self.scan.source_dir = new_dir;
        self.on_scan_input_changed(window, cx);
    }

    /// 输入变更效应:
    /// 清本页 files/error/loading/hasScanned/filterKeyword、token+1 丢弃在途响应,
    /// 并 `onScanComplete([], '')` 同步清空 App 级扫描数据(锁回步骤 1)。
    fn on_scan_input_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scan.files.clear();
        self.scan.error = None;
        self.scan.loading = false;
        self.scan.has_scanned = false;
        self.scan_token += 1;
        // 清筛选关键词(InputState 设值需要 window)
        let filter_input = self.scan.filter_input.clone();
        filter_input.update(cx, |state, cx| state.set_value("", window, cx));
        self.handle_scan_complete(Vec::new(), String::new(), window, cx);
    }

    /// App 级 handleScanComplete:写入 scannedFiles/sourceDir、
    /// 解锁状态机推进(有数据 → max_unlocked ≥ 2;无数据锁回 1 并回步骤 1)。
    /// 同时清 App 级整理批次(源:setMappings([]) + setOrganizeMode('copy') +
    /// setTargetDir('');**页面本地状态保留不清**——三页常驻,源亦如此)。
    pub fn handle_scan_complete(
        &mut self,
        files: Vec<AudioMetadata>,
        dir: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let has_files = !files.is_empty();
        self.scanned_files = files;
        self.source_dir = dir;
        self.clear_organize_batch();
        // 预览页目标目录 placeholder 动态拼源目录
        // (`留空则整理到源目录（{sourceDir}）`;清空时回落基础文案)
        let placeholder = if self.source_dir.is_empty() {
            "留空则整理到源目录".to_string()
        } else {
            format!("留空则整理到源目录（{}）", self.source_dir)
        };
        let preview_dir_input = self.preview.dir.read(cx).input.clone();
        preview_dir_input.update(cx, |state, cx| {
            state.set_placeholder(placeholder, window, cx);
        });
        if has_files {
            self.max_unlocked_step = self.max_unlocked_step.max(2);
        } else {
            self.max_unlocked_step = 1;
            self.current_step = 1;
        }
        cx.notify();
    }

    /// "下一步:设置模板":有筛选词时先把筛选子集
    /// 提交为 App 级数据(下游预览只用被筛过的文件),再切到步骤 2。
    fn handle_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keyword = self.scan.filter_input.read(cx).value().to_string();
        if !keyword.trim().is_empty() {
            let filtered = self.scan.filtered_files(cx);
            let source_dir = self.scan.dir.read(cx).value(cx);
            self.handle_scan_complete(filtered, source_dir, window, cx);
        }
        // onNext = setCurrentStep(2),无解锁检查(源行为)
        self.current_step = 2;
        cx.notify();
    }

    /// 全量重置。
    /// 语义:回步骤 1、max_unlocked=1、清空页面数据(重建页面结构体)、清
    /// taskId(内存 + 数据目录持久化文件)并停止任务轮询。
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.current_step = 1;
        self.max_unlocked_step = 1;
        self.reset_key += 1;
        // 丢弃"重置前发起"的在途扫描/预览/整理发起,并停掉运行中的轮询循环
        // (源:页面卸载 stopPolling + token 自增丢在途响应)
        self.scan_token += 1;
        self.preview_token += 1;
        self.progress_token += 1;
        self.scanned_files.clear();
        self.source_dir.clear();
        self.clear_organize_batch();
        self.task_id.clear();
        // 源 handleReset → setTaskId('') → localStorage.removeItem
        clear_persisted_task_id();
        // 重建页面 = 源 resetKey 强制重挂载(内部状态与订阅全部丢弃重建)
        self.scan = ScanPage::new(window, cx);
        self.preview = PreviewPage::new(window, cx);
        self.progress = ProgressPage::new();
        self.wire_page_subscriptions(window, cx);
        cx.notify();
    }

    // ── 预览页逻辑─────────────────────────────

    /// App 级 onClearOrganize(源 App.tsx 默认为 copy，本项目改为 move):mappings=[]、organizeMode='move'、
    /// targetDir=''。预览失败/表单变更/重扫描时调用,防止旧计划被执行。
    fn clear_organize_batch(&mut self) {
        self.organize_mappings.clear();
        self.organize_mode = OrganizeMode::Move;
        self.organize_target_dir.clear();
    }

    /// 清页面本地预览结果(mappings/directoryTree/resolvedTargetDir)。
    fn clear_preview_results(&mut self) {
        self.preview.mappings.clear();
        self.preview.directory_tree = empty_tree();
        self.preview.resolved_target_dir.clear();
    }

    /// 表单变更效应:
    /// 作废在途响应 + 清本地预览结果 + onClearOrganize("开始执行整理"随之消失)。
    fn invalidate_preview_results(&mut self, cx: &mut Context<Self>) {
        self.preview_token += 1; // abort:在途响应过期
        self.preview.loading = false;
        self.clear_preview_results();
        self.clear_organize_batch();
        cx.notify();
    }

    /// 模板输入变化(React setState 同值不触发 effect 的语义)。
    fn on_preview_template_changed(&mut self, cx: &mut Context<Self>) {
        let v = self.preview.template_input.read(cx).value().to_string();
        if v == self.preview.form_template {
            return;
        }
        self.preview.form_template = v;
        self.invalidate_preview_results(cx);
    }

    /// 目标目录变化(手输/清空/原生对话框/浏览模态选定;同值不触发)。
    fn on_preview_target_changed(&mut self, new_dir: String, cx: &mut Context<Self>) {
        if new_dir == self.preview.form_target_dir {
            return;
        }
        self.preview.form_target_dir = new_dir;
        self.invalidate_preview_results(cx);
    }

    /// 操作模式切换(点击已激活项无效果,React setState 同值语义)。
    fn set_preview_mode(&mut self, mode: OrganizeMode, cx: &mut Context<Self>) {
        if mode == self.preview.mode {
            return;
        }
        self.preview.mode = mode;
        self.invalidate_preview_results(cx);
    }

    /// 点击占位符芯片:聚焦中 → 在**光标位置**
    /// 插入(gpui-component InputState::insert 取内部光标,插入后光标落在
    /// 插入文本末尾,等价源 setSelectionRange);未聚焦 → 追加到末尾。随后
    /// 聚焦输入框(源 requestAnimationFrame focus)。
    fn insert_placeholder(&mut self, tag: &str, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.preview.template_focused;
        let input = self.preview.template_input.clone();
        input.update(cx, |state, cx| {
            if focused {
                state.insert(tag.to_string(), window, cx);
            } else {
                let appended = format!("{}{tag}", state.value());
                state.set_value(appended, window, cx);
            }
            state.focus(window, cx);
        });
    }

    /// 生成预览。
    fn handle_preview(&mut self, cx: &mut Context<Self>) {
        let template_raw = self.preview.template_input.read(cx).value().to_string();
        if template_raw.trim().is_empty() {
            return;
        }
        // 中止上一个在途请求 + 先作废旧预览(新请求失败时旧计划不可执行;
        // 亦清 App 级整理批次,防止 /progress 执行过期计划)
        self.preview_token += 1;
        let token = self.preview_token;
        self.clear_preview_results();
        self.clear_organize_batch();
        self.preview.loading = true;
        self.preview.error = None;
        cx.notify();

        // effectiveTarget = targetDir.trim() || sourceDir(留空 → 整理到源目录)
        let target_raw = self.preview.dir.read(cx).value(cx);
        let effective_target = {
            let t = target_raw.trim();
            if t.is_empty() {
                self.source_dir.clone()
            } else {
                t.to_string()
            }
        };
        let files = self.scanned_files.clone();
        let template = template_raw.trim().to_string();
        let mode = self.preview.mode;

        run_service_result(
            cx,
            move || {
                service::generate_preview(PreviewRequest {
                    files,
                    template,
                    target_dir: effective_target,
                    mode,
                })
            },
            move |this, result, cx| {
                if this.preview_token != token {
                    return; // 过期响应(表单已变更/已重置),丢弃
                }
                this.preview.loading = false;
                match result {
                    Ok(resp) => {
                        if !resp.template_errors.is_empty() {
                            // Ok 路径模板错误:'；' 连接
                            this.preview.error = Some(resp.template_errors.join("；"));
                            this.clear_preview_results();
                        } else {
                            this.preview.mappings = resp.mappings;
                            this.preview.directory_tree = resp.directory_tree;
                            this.preview.resolved_target_dir = resp.target_dir;
                        }
                    }
                    // Err 路径:模板校验错(TemplateErrors,前端 toError 以 '\n'
                    // 连接)或目标目录校验字符串——文案由 ServiceError::Display 给出
                    Err(msg) => this.preview.error = Some(msg),
                }
                cx.notify();
            },
        );
    }

    /// 开始执行整理:剔除 unreadable /
    /// boundary_error / write_error 三类必然被预检整批拒绝的映射,把整理参数
    /// 交给 App 级状态(D5 进度页消费),解锁并进入步骤 3。
    /// **无二次确认弹窗**。
    fn handle_start_organize(&mut self, cx: &mut Context<Self>) {
        let organizable: Vec<FileMappingItem> = self
            .preview
            .mappings
            .iter()
            .filter(|m| is_organizable(m))
            .cloned()
            .collect();
        // onOrganize(m, mode, resolvedTargetDir || targetDir || sourceDir)
        let target_dir = if !self.preview.resolved_target_dir.is_empty() {
            self.preview.resolved_target_dir.clone()
        } else {
            let raw = self.preview.dir.read(cx).value(cx);
            if !raw.is_empty() {
                raw
            } else {
                self.source_dir.clone()
            }
        };
        self.organize_mappings = organizable;
        self.organize_mode = self.preview.mode;
        self.organize_target_dir = target_dir;
        // 源 onOrganize 内 setTaskId('')(含 localStorage.removeItem)。
        // 旧任务的轮询循环(若有)继续追踪旧 task_id,与源 setInterval
        // 捕获 id 的行为一致(见 start_progress_polling 注释)。
        self.task_id.clear();
        clear_persisted_task_id();
        // onNext = setMaxUnlockedStep(3) + setCurrentStep(3)
        self.max_unlocked_step = 3;
        self.current_step = 3;
        cx.notify();
    }

    /// 返回扫描(onBack = setCurrentStep(1),源无解锁检查)。
    fn go_back_to_scan(&mut self, cx: &mut Context<Self>) {
        self.current_step = 1;
        cx.notify();
    }

    // ── 进度页逻辑──

    /// "开始执行":starting 防双击;置 started、清
    /// done/error/log/progress → `start_organize(mappings, mode, target_dir)`
    /// (后台线程)→ 成功:写 task_id 并持久化(先持久化再更新界面,POST 在途
    /// 时用户切走任务也保持可重连)、启动轮询;失败:started 回 false(按钮
    /// 重现,可重试同批次)、错误进"准备开始"卡。
    fn handle_progress_start(&mut self, cx: &mut Context<Self>) {
        if self.progress.starting {
            return; // 源 startingRef:双击在首次置位生效前直达这里的防护
        }
        self.progress.starting = true;
        self.progress.started = true;
        self.progress.done = false;
        self.progress.error = None;
        self.progress.log.clear();
        self.progress.progress = None;
        self.progress_token += 1; // 使旧轮询循环失效(源 startPolling 先 stopPolling)
        let token = self.progress_token;
        cx.notify();

        let mappings = self.organize_mappings.clone();
        let mode = self.organize_mode;
        let target_dir = self.organize_target_dir.clone();
        run_service_result(
            cx,
            move || service::start_organize(mappings, mode, target_dir),
            move |this, result, cx| {
                // 重置/新发起后到达的过期响应 → 丢弃
                if this.progress_token != token {
                    return;
                }
                this.progress.starting = false;
                match result {
                    Ok(resp) => {
                        this.task_id = resp.task_id.clone();
                        persist_task_id(&resp.task_id);
                        this.start_progress_polling(cx);
                    }
                    Err(msg) => {
                        this.progress.started = false;
                        this.progress.error = Some(msg);
                    }
                }
                cx.notify();
            },
        );
    }

    /// 启动任务状态轮询:每 1000ms 调一次
    /// `get_task_status(task_id)`(id 在启动时捕获,与源 setInterval 闭包一致
    /// ——onOrganize 清空 task_id 不影响已运行循环对旧任务的追踪);快照落地
    /// 见 [`Self::apply_task_snapshot`];done/error 终态停止;查询失败静默
    /// 重试不中断(源 catch 语义)。token 防串:reset/新发起使本循环自杀。
    /// get_task_status 为内存注册表读取(锁 + 小结构克隆,微秒级),直接在
    /// 主线程 async 上下文执行。
    fn start_progress_polling(&mut self, cx: &mut Context<Self>) {
        let task_id = self.task_id.clone();
        if task_id.is_empty() {
            return;
        }
        self.progress_token += 1;
        let token = self.progress_token;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let result = service::get_task_status(task_id.clone());
                let keep_going = this.update(cx, |this, cx| {
                    if this.progress_token != token {
                        return false; // 已重置/被新循环取代 → 自杀
                    }
                    let terminal = this.apply_task_snapshot(result);
                    cx.notify();
                    !terminal
                });
                match keep_going {
                    Ok(true) => {}
                    _ => return, // 终态、实体已释放或 token 失效
                }
            }
        })
        .detach();
    }

    /// 单次快照落地:更新 progress;追加日志行
    /// (current_file 非空 → `[current/total] basename` 连续相同不重复,
    /// 否则 message 非空 → 追加 message;滚动锚定);status=done → done=true,
    /// status=error → errMsg = message || `执行出错`,两者均返回 true(停轮询)。
    fn apply_task_snapshot(&mut self, data: Result<ProgressEvent, service::ServiceError>) -> bool {
        let Ok(data) = data else {
            // 任务不存在/查询失败:静默重试,不中断轮询
            return false;
        };
        // 滚动锚定:以追加前的滚动位置判断用户是否停留在底部附近
        let at_bottom = log_is_at_bottom(&self.progress.log_scroll);
        if !data.current_file.is_empty() {
            let line = format!(
                "[{}/{}] {}",
                data.current,
                data.total,
                basename(&data.current_file)
            );
            append_log_line_dedup(&mut self.progress.log, line);
        } else if !data.message.is_empty() {
            append_log_line(&mut self.progress.log, data.message.clone());
        }
        if at_bottom {
            // 新日志到达且用户位于底部附近 → 自动滚到底;上翻阅读不打扰
            self.progress.log_scroll.scroll_to_bottom();
        }
        self.progress.progress = Some(data.clone());
        match data.status {
            TaskStatus::Done => {
                self.progress.done = true;
                true
            }
            TaskStatus::Error => {
                self.progress.error = Some(if data.message.is_empty() {
                    "执行出错".to_string()
                } else {
                    data.message.clone()
                });
                true
            }
            _ => false,
        }
    }

    /// 是否有进行中/未完成的整理任务。
    /// 判定:task_id 非空 且 快照未到终态(done/error);尚无快照(刚发起/
    /// 恢复后未轮到)视为进行中。终态任务不会因退出中断任何处理,不报"有任务"。
    fn has_running_task(&self) -> bool {
        if self.task_id.is_empty() {
            return false;
        }
        !matches!(
            self.progress.progress.as_ref().map(|p| p.status),
            Some(TaskStatus::Done) | Some(TaskStatus::Error)
        )
    }

    // ── 确认弹窗 ─────────────────────────────────────────────────────────────

    fn open_reset_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let options = ConfirmOptions::new("确定要清空当前的扫描结果、整理模板配置并重新开始吗?")
            .title("确认重置全部数据?")
            .tip("若当前有正在后台执行的文件整理任务,重置将断开界面追踪。")
            .confirm_text("确认重置")
            .cancel_text("取消")
            .tone(ConfirmTone::Warning);
        self.confirm = Some(PendingConfirm {
            options,
            action: ConfirmAction::Reset,
        });
        self.confirm_focus.focus(window);
        cx.notify();
    }

    fn open_exit_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (description, tip) = if self.has_running_task() {
            (
                Some("当前有正在进行或未完成的文件整理任务,退出将中断处理。"),
                Some("建议等待任务整理完成后再退出应用。"),
            )
        } else {
            (Some("退出后当前未保存的配置与扫描缓存将被清除。"), None)
        };
        let mut options = ConfirmOptions::new("确定要退出 Tag2Folders 吗?")
            .title("确认退出应用?")
            .confirm_text("确认退出")
            .cancel_text("取消")
            .tone(ConfirmTone::Warning);
        if let Some(d) = description {
            options = options.description(d);
        }
        if let Some(t) = tip {
            options = options.tip(t);
        }
        self.confirm = Some(PendingConfirm {
            options,
            action: ConfirmAction::Exit,
        });
        self.confirm_focus.focus(window);
        cx.notify();
    }
    fn handle_confirm(&mut self, ok: bool, window: &mut Window, cx: &mut Context<Self>) {
        let pending = match self.confirm.take() {
            Some(p) => p,
            None => return,
        };
        if ok {
            match pending.action {
                ConfirmAction::Reset => self.reset(window, cx),
                ConfirmAction::Exit => {
                    self.exit_confirmed = true;
                    std::process::exit(0);
                }
            }
        }
        // 取消:不做任何事
        cx.notify();
    }

    // ── 渲染 ─────────────────────────────────────────────────────────────────
    fn render_header(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(58.0))
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            // 源 padding 0 clamp(16px, 3vw, 32px) → 取中值 24(已知差异)
            .px(px(24.0))
            .bg(theme::BG_SURFACE)
            .border_b_1()
            .border_color(theme::BORDER_SUBTLE)
            // 品牌区:gap 12
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .min_w(px(0.0))
                    .child(
                        div()
                            .size(px(34.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(10.0))
                            .bg(theme::AMBER_500)
                            .shadow(theme::shadow_brand_tile())
                            .child(icon_16(Icon::Tag).size(px(18.0)).text_color(theme::SLATE_800)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_size(px(15.5))
                                    .font_weight(gpui::FontWeight(700.0))
                                    .text_color(theme::SLATE_900)
                                    .child("Tag2Folders"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(theme::SLATE_500)
                                    .truncate()
                                    .child("音频文件智能整理 · 扫描 → 预览 → 执行"),
                            ),
                    ),
            )
            // 右侧操作区:gap 10
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(10.0))
                    // 版本徽章:badge-amber + padding 4px 10px + fontSize 11.5
                    .child(
                        badge(BadgeVariant::Amber)
                            .px(px(10.0))
                            .py(px(4.0))
                            .text_size(px(11.5))
                            .child("v2.0.1"),
                    )
                    // 重置按钮:ghost sm + RefreshIcon 14(title 提示见已知差异)
                    .child(
                        Button::new("header-reset")
                            .label("重置")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .icon(Icon::Refresh, px(14.0))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_reset_confirm(window, cx);
                            })),
                    ),
            )
    }

    /// 当前页渲染(步骤 1/2/3)。
    fn render_page(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        match self.current_step {
            1 => self.render_scan_page(window, cx).into_any_element(),
            2 => self.render_preview_page(window, cx).into_any_element(),
            _ => self.render_progress_page(window, cx).into_any_element(),
        }
    }

    // ── 进度页渲染────────────────────────────

    /// 步骤 3 完整页面:无任务数据警告 → 任务概览卡 →(未发起)准备开始卡 /
    /// (进行中)执行进度卡 / 终态横幅(完成/失败)→ 实时日志控制台。
    fn render_progress_page(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let mappings = &self.organize_mappings;
        let no_mappings = mappings.is_empty();
        let started = self.progress.started;
        let done = self.progress.done;
        let mode = self.organize_mode;
        let target_dir = self.organize_target_dir.clone();

        // 4.3.1 无任务数据警告(noMappings;整页唯一内容,无按钮)
        if no_mappings {
            return div()
                .flex()
                .items_start()
                .gap(px(10.0))
                .px(px(16.0))
                .py(px(12.0))
                .bg(theme::AMBER_50)
                .border_1()
                .border_color(theme::AMBER_200)
                .rounded(theme::RADIUS_LG)
                .child(
                    div().flex_none().mt(px(1.0)).child(
                        icon_sized(Icon::AlertCircle, px(16.0)).text_color(theme::AMBER_600),
                    ),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(theme::AMBER_800)
                        .child("没有待处理的文件，请先完成扫描和预览步骤。"),
                )
                .into_any_element();
        }

        // ── 事件处理器 ──
        let on_start = cx.listener(
            |this, _e: &gpui::ClickEvent, _window, cx| this.handle_progress_start(cx),
        );

        // ── 4.3.2 任务概览卡片 ──
        let mode_badge_text = match mode {
            OrganizeMode::Move => "移动（删除源文件）",
            OrganizeMode::Copy => "复制（保留源文件）",
        };
        let overview = card()
            .title("任务概览")
            .subtitle("整理任务的模式、目标与待处理数量")
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    // 源 gap '14px 32px'(行 14 / 列 32);gpui 无法分离 → 取 32
                    .gap(px(32.0))
                    .child(
                        div()
                            .child(overview_label("操作模式"))
                            .child(
                                badge(BadgeVariant::Amber)
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .text_size(px(12.0))
                                    .child(mode_badge_text),
                            ),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_grow()
                            .flex_shrink()
                            .flex_basis(px(220.0))
                            .child(overview_label("目标目录"))
                            .child(
                                // 等宽 chip:slate-100 底、padding 4 10、圆角 6、
                                // 12.5 slate-700、truncate(title 悬浮提示为已知差异)
                                div()
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(12.5))
                                    .text_color(theme::SLATE_700)
                                    .bg(theme::SLATE_100)
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .rounded(theme::RADIUS_SM)
                                    .max_w(gpui::relative(1.0))
                                    .truncate()
                                    .child(if target_dir.is_empty() {
                                        "（未设置）".to_string()
                                    } else {
                                        target_dir.clone()
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .child(overview_label("待处理总数"))
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .text_size(px(18.0))
                                            .font_weight(gpui::FontWeight(700.0))
                                            .text_color(theme::AMBER_800)
                                            .child(mappings.len().to_string()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_weight(gpui::FontWeight(500.0))
                                            .text_color(theme::SLATE_500)
                                            .child("个文件"),
                                    ),
                            ),
                    ),
            );

        let mut page = div().flex().flex_col().w_full().child(overview);

        // ── 4.3.3 准备开始卡片(!started)──
        if !started {
            let (mode_word, suffix) = match mode {
                OrganizeMode::Move => ("移动", "，完成后源文件将被删除。"),
                OrganizeMode::Copy => ("复制", "，源文件将保留。"),
            };
            let head = format!("将{mode_word} ");
            let count = mappings.len().to_string();
            let text = format!("{head}{count} 个文件到目标目录{suffix}");
            // 加粗计数(源 <strong color amber-800>)按字节区间高亮
            let range = head.len()..head.len() + count.len();
            let mut prep = card()
                .map(|el| el.mt(px(16.0)))
                .title("准备开始")
                .subtitle("确认无误后开始执行整理任务")
                .child(
                    div()
                        .mb(px(16.0))
                        .text_size(px(13.5))
                        .text_color(theme::SLATE_600)
                        .child(
                            StyledText::new(text).with_highlights(vec![(
                                range,
                                HighlightStyle {
                                    font_weight: Some(gpui::FontWeight(700.0)),
                                    color: Some(theme::AMBER_800.into()),
                                    ..Default::default()
                                },
                            )]),
                        ),
                );
            // errMsg 提示(rose;startOrganize 失败后的重试场景)
            if let Some(err) = self.progress.error.clone() {
                prep = prep.child(
                    AlertBar::new(AlertVariant::Rose, err)
                        .icon(Icon::AlertCircle)
                        .mb(px(16.0)),
                );
            }
            prep = prep.child(
                Button::new("progress-start")
                    .label("开始执行")
                    .variant(ButtonVariant::Primary)
                    .size(ButtonSize::Lg)
                    .icon(Icon::Play, px(15.0))
                    .on_click(on_start),
            );
            page = page.child(prep);
        }

        // ── 4.3.4 执行进度卡片(started && !done && 无错误)──
        if started && !done && self.progress.error.is_none() {
            let pct = task_percent(self.progress.progress.as_ref());
            let current_file = self
                .progress
                .progress
                .as_ref()
                .map(|p| p.current_file.clone())
                .unwrap_or_default();
            let (cur, total) = self
                .progress
                .progress
                .as_ref()
                .map(|p| (p.current, p.total))
                .unwrap_or((0, 0));

            let mut running = card()
                .map(|el| el.mt(px(16.0)))
                .title("执行进度")
                .subtitle("任务进行中，请勿关闭窗口")
                // 头行:百分比大字 + 已处理计数(底对齐、两端)
                .child(
                    div()
                        .mb(px(10.0))
                        .flex()
                        .items_end()
                        .justify_between()
                        .gap(px(12.0))
                        .child(
                            div()
                                .text_size(px(32.0))
                                .font_weight(gpui::FontWeight(700.0))
                                .text_color(theme::AMBER_800)
                                .line_height(gpui::relative(1.0))
                                .child(format!("{pct}%")),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(theme::SLATE_600)
                                .child(if self.progress.progress.is_some() {
                                    format!("{cur} / {total} 已处理")
                                } else {
                                    "等待任务开始…".to_string()
                                }),
                        ),
                )
                .child(ProgressBar::new(cur, total));

            // 当前文件条(current_file 非空时):脉冲圆点 + 正在处理 + 文件名
            if !current_file.is_empty() {
                running = running.child(
                    div()
                        .mt(px(12.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(12.0))
                        .py(px(7.0))
                        .bg(theme::AMBER_50)
                        .border_1()
                        .border_color(theme::AMBER_200)
                        .rounded(theme::RADIUS_MD)
                        .child(
                            // 脉冲圆点 7×7 amber-500(animate-pulse:1↔0.6,2s)
                            div()
                                .size(px(7.0))
                                .flex_none()
                                .rounded(theme::RADIUS_FULL)
                                .bg(theme::AMBER_500)
                                .with_animation(
                                    SharedString::from("progress-current-pulse"),
                                    Animation::new(Duration::from_millis(
                                        theme::DURATION_PULSE_MS,
                                    ))
                                    .repeat()
                                    .with_easing(|t| 1.0 - (2.0 * t - 1.0).abs()),
                                    |el, eased| el.opacity(0.6 + 0.4 * eased),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(11.5))
                                .font_weight(gpui::FontWeight(600.0))
                                .text_color(theme::AMBER_800)
                                .child("正在处理"),
                        )
                        .child(
                            icon_sized(Icon::FileAudio, px(14.0)).text_color(theme::AMBER_600),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .font_family(theme::FONT_MONO)
                                .text_size(px(12.0))
                                .text_color(theme::AMBER_900)
                                .truncate()
                                .child(current_file),
                        ),
                );
            }
            page = page.child(running);
        }

        // ── 4.3.5 完成横幅(started && done)──
        if started && done {
            let total = self
                .progress
                .progress
                .as_ref()
                .map(|p| p.total)
                .unwrap_or(mappings.len());
            let on_finish = cx.listener(
                |this, _e: &gpui::ClickEvent, window, cx| {
                    // onFinish = handleReset(false):不弹确认,直接全量重置
                    this.reset(window, cx);
                },
            );
            page = page.child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .items_start()
                    .gap(px(14.0))
                    .px(px(22.0))
                    .py(px(20.0))
                    .bg(theme::EMERALD_50)
                    .border_1()
                    .border_color(theme::EMERALD_200)
                    .rounded(theme::RADIUS_LG)
                    .child(
                        div().flex_none().mt(px(2.0)).child(
                            icon_sized(Icon::CheckCircle, px(24.0))
                                .text_color(theme::EMERALD_600),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(gpui::FontWeight(700.0))
                                    .text_color(theme::EMERALD_700)
                                    .child("整理完成"),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(12.5))
                                    .text_color(theme::EMERALD_700)
                                    .opacity(0.85)
                                    .child(format!(
                                        "共处理 {total} 个文件，任务已成功结束。"
                                    )),
                            )
                            .child(
                                div().mt(px(14.0)).child(
                                    Button::new("progress-finish-done")
                                        .label("完成并开启新任务")
                                        .variant(ButtonVariant::Primary)
                                        .icon(Icon::Sparkles, px(15.0))
                                        .on_click(on_finish),
                                ),
                            ),
                    ),
            );
        }

        // ── 4.3.6 失败横幅(started && errMsg && !done)──
        if started && !done {
            if let Some(err) = self.progress.error.clone() {
                let on_finish = cx.listener(
                    |this, _e: &gpui::ClickEvent, window, cx| {
                        this.reset(window, cx);
                    },
                );
                page = page.child(
                    div()
                        .mt(px(16.0))
                        .flex()
                        .items_start()
                        .gap(px(14.0))
                        .px(px(22.0))
                        .py(px(20.0))
                        .bg(theme::ROSE_50)
                        .border_1()
                        .border_color(theme::ROSE_200)
                        .rounded(theme::RADIUS_LG)
                        .child(
                            div().flex_none().mt(px(2.0)).child(
                                icon_sized(Icon::AlertCircle, px(24.0))
                                    .text_color(theme::ROSE_600),
                            ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .text_size(px(16.0))
                                        .font_weight(gpui::FontWeight(700.0))
                                        .text_color(theme::ROSE_700)
                                        .child("任务执行失败"),
                                )
                                .child(
                                    // rose-800 未定义陷阱 → #0f172a;
                                    // pre-wrap:多行错误(如 preflight_errors)换行展示
                                    div()
                                        .mt(px(4.0))
                                        .text_size(px(12.5))
                                        .text_color(theme::INHERITED_TEXT)
                                        .child(err),
                                )
                                .child(
                                    div().mt(px(14.0)).child(
                                        Button::new("progress-finish-fail")
                                            .label("完成并开启新任务")
                                            .variant(ButtonVariant::Primary)
                                            .icon(Icon::Sparkles, px(15.0))
                                            .on_click(on_finish),
                                    ),
                                ),
                        ),
                );
            }
        }

        // ── 4.3.7 实时日志控制台(started && log 非空)──
        if started && !self.progress.log.is_empty() {
            let mut console = div()
                .id("progress-log")
                .track_scroll(&self.progress.log_scroll)
                .max_h(LOG_CONSOLE_MAX_H)
                .overflow_y_scroll()
                .bg(theme::SLATE_950) // #020617
                .rounded(theme::RADIUS_MD)
                .px(px(14.0))
                .py(px(12.0))
                .font_family(theme::FONT_MONO)
                .text_size(px(12.0))
                .line_height(gpui::relative(1.8))
                .text_color(theme::SLATE_300);
            for (ix, line) in self.progress.log.iter().enumerate() {
                console = console.child(render_log_line(ix, line));
            }
            page = page.child(
                card()
                    .map(|el| el.mt(px(16.0)))
                    .title("实时日志")
                    .actions(
                        badge(BadgeVariant::Slate)
                            .text_size(px(10.5))
                            .child("TERMINAL"),
                    )
                    .child(console),
            );
        }

        page.into_any_element()
    }

    // ── 扫描页渲染──────────────────────────────

    /// 步骤 1 完整页面:配置卡(源目录/递归/开始扫描)→ 错误/空结果提示 →
    /// 结果卡(看板计数/筛选/文件表格)→ 底部导航条。
    fn render_scan_page(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        // 状态快照(渲染期间仅不可变借用;事件经 cx.listener 捕获)
        let dir_empty = self.scan.dir.read(cx).value(cx).trim().is_empty();
        let loading = self.scan.loading;
        let error = self.scan.error.clone();
        let has_scanned = self.scan.has_scanned;
        let files_empty = self.scan.files.is_empty();
        let keyword = self.scan.filter_input.read(cx).value().to_string();
        let filter_active = !keyword.trim().is_empty();
        let filtered = self.scan.filtered_files(cx);
        let readable_count = self.scan.files.iter().filter(|f| f.readable).count();
        let unreadable_count = self.scan.files.len() - readable_count;
        let filter_field = self.scan.filter_field;
        let show_clear = !keyword.is_empty() || filter_field != FilterField::Filename;

        // 事件处理器(全部先于 &mut App 重借用创建)
        let on_scan = cx.listener(
            |this, _e: &gpui::ClickEvent, window, cx| this.handle_scan(window, cx),
        );
        let on_recursive = cx.listener(|this, checked: &bool, window, cx| {
            this.scan.recursive = *checked;
            // recursive 变化同样触发输入变更效应
            this.on_scan_input_changed(window, cx);
        });
        let on_clear_filter_bar = cx.listener(
            |this, _e: &gpui::ClickEvent, window, cx| this.clear_scan_filter(window, cx),
        );
        let on_clear_filter_empty = cx.listener(
            |this, _e: &gpui::ClickEvent, window, cx| this.clear_scan_filter(window, cx),
        );
        let on_next = cx.listener(
            |this, _e: &gpui::ClickEvent, window, cx| this.handle_next(window, cx),
        );

        // 筛选栏:前缀文字 + 字段胶囊(单选) + 关键词输入 + 清空
        let mut filter_bar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .px(px(20.0))
            .py(px(12.0))
            .border_t_1()
            .border_b_1()
            .border_color(theme::BORDER_SUBTLE)
            .bg(theme::SLATE_50)
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight(600.0))
                    .text_color(theme::SLATE_500)
                    .child("快速筛选"),
            );
        for (field, label) in FilterField::ALL {
            let selected = filter_field == field;
            let on_field = cx.listener(|this, f: &FilterField, _window, cx| {
                this.scan.filter_field = *f;
                cx.notify();
            });
            filter_bar = filter_bar.child(
                div()
                    .id(SharedString::from(format!("filter-field-{}", field.key())))
                    .px(px(12.0))
                    .py(px(4.0))
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight(if selected { 600.0 } else { 500.0 }))
                    .rounded(theme::RADIUS_FULL)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .when(selected, |el| {
                        el.bg(theme::AMBER_500)
                            .text_color(theme::SLATE_800)
                            .hover(|st| st.bg(theme::AMBER_600))
                    })
                    .when(!selected, |el| {
                        el.bg(theme::SLATE_200)
                            .text_color(theme::SLATE_600)
                            .hover(|st| st.bg(theme::SLATE_300).text_color(theme::SLATE_800))
                    })
                    .child(label)
                    .on_click(move |_, window, cx| on_field(&field, window, cx)),
            );
        }
        // 关键词输入:左内嵌 SearchIcon 13 @ left 9、h 32、fontSize 12.5、pl 28 — 先渲染Input再渲染icon避免被盖
        filter_bar = filter_bar
            .child(
                div()
                    .relative()
                    .flex_grow()
                    .flex_shrink()
                    .flex_basis(px(160.0))
                    .min_w(px(140.0))
                    .child(
                        Input::new(&self.scan.filter_input)
                            .h(px(32.0))
                            .py(px(0.0))
                            .pl(px(28.0))
                            .text_size(px(12.5)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(9.0))
                            .top(px(0.0))
                            .bottom(px(0.0))
                            .flex()
                            .items_center()
                            .child(
                                icon_sized(Icon::Search, px(13.0)).text_color(theme::SLATE_500),
                            ),
                    ),
            )
            .when(show_clear, |el| {
                el.child(
                    Button::new("filter-clear")
                        .label("清空")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .icon(Icon::X, px(13.0))
                        .on_click(on_clear_filter_bar),
                )
            });

        // 配置卡——render_dir_picker 需要 &mut App,最后借用 cx
        let app: &mut gpui::App = cx;
        let config_card = card()
            .title("扫描源目录")
            .subtitle("选择包含音频文件的文件夹，扫描并读取标签信息")
            .child(render_dir_picker(&self.scan.dir, window, app))
            .child(
                // 按钮行:marginTop 14、两端对齐、gap 12、可换行
                div()
                    .mt(px(14.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        // 递归复选框:label 13px slate-600(默认勾选)
                        Checkbox::new("scan-recursive")
                            .label("递归扫描子目录")
                            .checked(self.scan.recursive)
                            .text_size(px(13.0))
                            .text_color(theme::SLATE_600)
                            .on_click(on_recursive),
                    )
                    .child(
                        // 开始扫描:primary + MusicIcon 15;与输入/浏览同高38保持三者协调
                        Button::new("scan-start")
                            .label(if loading { "正在扫描…" } else { "开始扫描" })
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Md)
                            .h(px(38.0))
                            .pad_x(px(20.0))
                            .icon(Icon::Music, px(15.0))
                            .loading(loading)
                            .disabled(dir_empty)
                            .on_click(on_scan),
                    ),
            );

        // 页面骨架
        let mut page = div().flex().flex_col().w_full().child(config_card);

        // 错误提示
        if let Some(err) = error.clone() {
            page = page.child(AlertBar::new(AlertVariant::Rose, err).mt(px(12.0)));
        }
        // 空结果提示:hasScanned && !loading && !error && 0 文件
        if has_scanned && !loading && error.is_none() && files_empty {
            page = page.child(
                AlertBar::new(
                    AlertVariant::Sky,
                    "未发现音频文件。请检查目录路径，或尝试开启「递归扫描子目录」后重新扫描。",
                )
                .mt(px(12.0))
                .pad_x(px(16.0))
                .pad_y(px(12.0)),
            );
        }

        // 结果区
        if !files_empty {
            // 看板计数行
            let pills_row = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .px(px(20.0))
                .pt(px(16.0))
                .pb(px(14.0))
                .child(stat_pill(
                    BadgeVariant::Amber,
                    Icon::FileAudio,
                    "总文件数",
                    self.scan.files.len(),
                ))
                .child(stat_pill(
                    BadgeVariant::Emerald,
                    Icon::Music,
                    "可读取",
                    readable_count,
                ))
                .child(stat_pill(
                    BadgeVariant::Rose,
                    Icon::AlertTriangle,
                    "不可读取",
                    unreadable_count,
                ))
                .when(filter_active, |el| {
                    el.child(stat_pill(
                        BadgeVariant::Slate,
                        Icon::Search,
                        "筛选结果",
                        filtered.len(),
                    ))
                });

            // 表格区:无匹配空态 / 表格 + 截断提示
            let display: &Vec<AudioMetadata> = if filter_active {
                &filtered
            } else {
                &self.scan.files
            };
            let table_area: gpui::AnyElement = if display.is_empty() {
                // 空态:SearchIcon 26 + 文案 + 清空筛选按钮
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .py(px(40.0))
                    .text_color(theme::SLATE_500)
                    .child(icon_sized(Icon::Search, px(26.0)).text_color(theme::SLATE_400))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .child("没有匹配筛选条件的文件"),
                    )
                    .child(
                        Button::new("filter-clear-empty")
                            .label("清空筛选")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .on_click(on_clear_filter_empty),
                    )
                    .into_any_element()
            } else {
                render_scan_table(display).into_any_element()
            };

            let mut stats_card = card()
                .padding(CardPadding::None)
                .map(|el| el.mt(px(16.0)))
                .child(pills_row)
                .child(filter_bar)
                .child(table_area);
            // 截断提示行
            if display.len() > TABLE_LIMIT {
                stats_card = stats_card.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(20.0))
                        .py(px(10.0))
                        .text_size(px(12.5))
                        .text_color(theme::SLATE_600)
                        .child(icon_sized(Icon::Info, px(13.0)).text_color(theme::SLATE_500))
                        .child(format!(
                            "仅显示前 {TABLE_LIMIT} 条，共 {} 条。可使用筛选缩小范围。",
                            display.len()
                        )),
                );
            }
            page = page.child(stats_card);

            // 底部导航条
            page = page.child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .px(px(16.0))
                    .py(px(12.0))
                    .bg(theme::BG_SURFACE)
                    .border_1()
                    .border_color(theme::BORDER_SUBTLE)
                    .rounded(theme::RADIUS_LG)
                    .shadow(theme::shadow_sticky_bar())
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::SLATE_500)
                            .child(if filter_active {
                                format!("已筛选 {} / {} 个文件", filtered.len(), self.scan.files.len())
                            } else {
                                format!("共 {} 个音频文件", self.scan.files.len())
                            }),
                    )
                    .child(
                        Button::new("scan-next")
                            .label(if filter_active {
                                format!("下一步：设置模板（{} 个）", filtered.len())
                            } else {
                                "下一步：设置模板".to_string()
                            })
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Md)
                            .h(px(38.0))
                            .pad_x(px(20.0))
                            .icon(Icon::ArrowRight, px(15.0))
                            .icon_right()
                            .disabled(files_empty)
                            .on_click(on_next),
                    ),
            );
        }
        page.into_any_element()
    }

    /// 清空筛选:keyword='' 且 field='filename'。
    fn clear_scan_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scan.filter_field = FilterField::Filename;
        let filter_input = self.scan.filter_input.clone();
        filter_input.update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    // ── 预览页渲染──────────────────────────────

    /// 步骤 2 完整页面:无文件警告 → 整理配置卡(目标目录 / 命名模板+占位符
    /// chips / 操作模式 toggle+移动警告 / 错误条 / 生成预览按钮)→ 结果区
    /// (统计 StatCard / Tabs / 映射表或目录树 / 底部导航)。
    fn render_preview_page(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let no_files = self.scanned_files.is_empty();
        let loading = self.preview.loading;
        let error = self.preview.error.clone();
        let mode = self.preview.mode;
        let template = self.preview.template_input.read(cx).value().to_string();
        let has_results = !self.preview.mappings.is_empty();

        // ── 事件处理器(全部先于 &mut App 重借用创建)──
        let on_go_scan = cx.listener(
            |this, _e: &gpui::ClickEvent, _window, cx| this.go_back_to_scan(cx),
        );
        let on_back = cx.listener(
            |this, _e: &gpui::ClickEvent, _window, cx| this.go_back_to_scan(cx),
        );
        let on_preview = cx.listener(
            |this, _e: &gpui::ClickEvent, _window, cx| this.handle_preview(cx),
        );
        let on_start = cx.listener(
            |this, _e: &gpui::ClickEvent, _window, cx| this.handle_start_organize(cx),
        );
        let on_copy = cx.listener(|this, _e: &gpui::ClickEvent, _window, cx| {
            this.set_preview_mode(OrganizeMode::Copy, cx);
        });
        let on_move = cx.listener(|this, _e: &gpui::ClickEvent, _window, cx| {
            this.set_preview_mode(OrganizeMode::Move, cx);
        });
        let on_tab_list = cx.listener(|this, _e: &gpui::ClickEvent, _window, cx| {
            this.preview.active_tab = PreviewTab::List;
            cx.notify();
        });
        let on_tab_tree = cx.listener(|this, _e: &gpui::ClickEvent, _window, cx| {
            this.preview.active_tab = PreviewTab::Tree;
            cx.notify();
        });

        // ── 4.2.2-2 命名模板:输入框 + 占位符芯片行 ──
        let mut chips_row = div()
            .mt(px(8.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme::SLATE_500)
                    .child("插入占位符："),
            );
        let hovered_chip = self.preview.hovered_chip;
        for (ix, (tag, label)) in PLACEHOLDERS.iter().enumerate() {
            let (tag, label) = (*tag, *label);
            let on_insert =
                cx.listener(move |this, _e: &gpui::ClickEvent, window, cx| {
                    this.preview.hovered_chip = None;
                    this.insert_placeholder(tag, window, cx);
                });
            let on_chip_hover = cx.listener(move |this, hovered: &bool, _window, cx| {
                this.preview.hovered_chip = hovered.then_some(ix);
                cx.notify();
            });
            chips_row = chips_row.child(placeholder_chip(
                ix,
                tag,
                label,
                hovered_chip == Some(ix),
                on_insert,
                on_chip_hover,
            ));
        }
        let template_block = div()
            .child(field_label("命名模板"))
            .child({
                let mut input = Input::new(&self.preview.template_input);
                input.style().size.height = Some(px(38.0).into());
                input
                    .px(px(12.0))
                    .text_size(px(13.0))
                    .font_family(theme::FONT_MONO)
            })
            .child(chips_row);
        // ── 4.2.2-3 操作模式 toggle + 移动警告条 ──
        let mut mode_block = div()
            .child(field_label("操作模式"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .p(px(3.0))
                    .gap(px(2.0))
                    .bg(theme::SLATE_100)
                    .rounded(theme::RADIUS_MD)
                    .child(segment_btn(
                        "mode-copy",
                        "复制（保留源文件）",
                        Icon::Copy,
                        mode == OrganizeMode::Copy,
                        if mode == OrganizeMode::Copy {
                            theme::AMBER_800
                        } else {
                            theme::SLATE_600
                        },
                        None,
                        on_copy,
                    ))
                    .child(segment_btn(
                        "mode-move",
                        "移动（删除源文件）",
                        Icon::ArrowRight,
                        mode == OrganizeMode::Move,
                        if mode == OrganizeMode::Move {
                            theme::AMBER_800
                        } else {
                            theme::SLATE_600
                        },
                        None,
                        on_move,
                    )),
            );
        if mode == OrganizeMode::Move {
            mode_block = mode_block.child(move_mode_warning());
        }

        // ── 4.2.2-5 生成预览按钮行(右对齐)──
        let button_row = div().flex().justify_end().child(
            Button::new("preview-generate")
                .label(if loading { "生成预览中…" } else { "生成预览" })
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Md)
                .h(px(38.0))
                .pad_x(px(20.0))
                .icon(Icon::Eye, px(15.0))
                .loading(loading)
                .disabled(loading || no_files || template.trim().is_empty())
                .on_click(on_preview),
        );
        // ── 4.2.1 无文件警告(noFiles;先于配置卡渲染,marginBottom 16)──
        let no_files_bar = no_files.then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .px(px(16.0))
                .py(px(12.0))
                .bg(theme::AMBER_50)
                .border_1()
                .border_color(theme::AMBER_200)
                .rounded(theme::RADIUS_LG)
                .mb(px(16.0))
                .child(
                    div().flex_none().child(
                        icon_sized(Icon::AlertTriangle, px(16.0)).text_color(theme::AMBER_600),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(px(13.0))
                        .text_color(theme::AMBER_800)
                        .child("尚未扫描任何文件，请先完成扫描步骤。"),
                )
                .child(
                    Button::new("preview-go-scan")
                        .label("前往扫描")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .on_click(on_go_scan),
                )
        });

        // ── 结果区(4.2.4,mappings 非空才整体显示)──
        let results: Option<gpui::AnyElement> = has_results.then(|| {
            let mappings = &self.preview.mappings;
            let count = |st: MappingStatus| mappings.iter().filter(|m| m.status == st).count();
            let ok_count = count(MappingStatus::Ok);
            let conflict_count =
                count(MappingStatus::Conflict) + count(MappingStatus::BatchConflict);
            let missing_count = count(MappingStatus::MissingMetadata);
            let unreadable_count = count(MappingStatus::Unreadable);
            let boundary_count = count(MappingStatus::BoundaryError);
            let write_count = count(MappingStatus::WriteError);
            let blocked_count = boundary_count + write_count;
            let organizable_count =
                mappings.len() - unreadable_count - boundary_count - write_count;

            // 统计 StatCard 网格(flex wrap + 每卡 min-width 150)
            let stats_grid = div()
                .mt(px(18.0))
                .flex()
                .flex_wrap()
                .gap(px(10.0))
                .child(stat_card(
                    "文件总数",
                    mappings.len(),
                    Icon::FileAudio,
                    theme::AMBER_300,
                    theme::AMBER_100,
                    theme::AMBER_700,
                    theme::AMBER_800,
                ))
                .child(stat_card(
                    "正常",
                    ok_count,
                    Icon::CheckCircle,
                    theme::EMERALD_200,
                    theme::EMERALD_50,
                    theme::EMERALD_600,
                    theme::EMERALD_700,
                ))
                .child(stat_card(
                    "冲突",
                    conflict_count,
                    Icon::AlertTriangle,
                    theme::AMBER_200,
                    theme::AMBER_50,
                    theme::AMBER_600,
                    theme::AMBER_700,
                ))
                .child(stat_card(
                    "缺失信息",
                    missing_count,
                    Icon::Info,
                    theme::SKY_200,
                    theme::SKY_50,
                    theme::SKY_600,
                    theme::SKY_700,
                ))
                .child(stat_card(
                    "不可读",
                    unreadable_count,
                    Icon::XCircle,
                    theme::BORDER_SUBTLE,
                    theme::SLATE_100,
                    theme::SLATE_600,
                    theme::SLATE_900,
                ))
                .child(stat_card(
                    "越界/写入受阻",
                    blocked_count,
                    Icon::AlertCircle,
                    theme::ROSE_200,
                    theme::ROSE_50,
                    theme::ROSE_600,
                    theme::ROSE_700,
                ));

            // Tab 切换行
            let tab_row = div()
                .mt(px(16.0))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .p(px(3.0))
                        .gap(px(2.0))
                        .bg(theme::SLATE_100)
                        .rounded(theme::RADIUS_MD)
                        .child(segment_btn(
                            "tab-list",
                            "详细映射列表",
                            Icon::FileAudio,
                            self.preview.active_tab == PreviewTab::List,
                            if self.preview.active_tab == PreviewTab::List {
                                theme::AMBER_700
                            } else {
                                theme::SLATE_600
                            },
                            Some(mappings.len()),
                            on_tab_list,
                        ))
                        .child(segment_btn(
                            "tab-tree",
                            "目录树层级预览",
                            Icon::Layers,
                            self.preview.active_tab == PreviewTab::Tree,
                            if self.preview.active_tab == PreviewTab::Tree {
                                theme::AMBER_700
                            } else {
                                theme::SLATE_600
                            },
                            None,
                            on_tab_tree,
                        )),
                )
                .when(self.preview.active_tab == PreviewTab::Tree, |el| {
                    el.child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::SLATE_500)
                            .child("点击文件夹可展开 / 折叠"),
                    )
                });

            // list / tree 视图
            let content: gpui::AnyElement = match self.preview.active_tab {
                PreviewTab::List => {
                    let mut table_card = card()
                        .padding(CardPadding::None)
                        .map(|el| el.mt(px(12.0)))
                        .child(render_mapping_table(mappings));
                    if mappings.len() > PREVIEW_TABLE_LIMIT {
                        table_card = table_card.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .px(px(20.0))
                                .py(px(10.0))
                                .text_size(px(12.5))
                                .text_color(theme::SLATE_600)
                                .child(
                                    icon_sized(Icon::Info, px(13.0)).text_color(theme::SLATE_500),
                                )
                                .child(format!(
                                    "仅显示前 {PREVIEW_TABLE_LIMIT} 条映射，共 {} 条。",
                                    mappings.len()
                                )),
                        );
                    }
                    table_card.into_any_element()
                }
                PreviewTab::Tree => div()
                    .mt(px(12.0))
                    .child(self.render_directory_tree(cx))
                    .into_any_element(),
            };

            // 底部导航条
            let bottom_nav = div()
                .mt(px(16.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .px(px(16.0))
                .py(px(12.0))
                .bg(theme::BG_SURFACE)
                .border_1()
                .border_color(theme::BORDER_SUBTLE)
                .rounded(theme::RADIUS_LG)
                .shadow(theme::shadow_sticky_bar())
                .child(
                    Button::new("preview-back")
                        .label("返回扫描")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Md)
                        .h(px(38.0))
                        .pad_x(px(16.0))
                        .icon(Icon::ArrowLeft, px(14.0))
                        .on_click(on_back),
                )
                .child(
                    Button::new("preview-start")
                        .label(format!("开始执行整理（{organizable_count} 个文件）"))
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Md)
                        .h(px(38.0))
                        .pad_x(px(20.0))
                        .icon(Icon::ArrowRight, px(15.0))
                        .icon_right()
                        .disabled(organizable_count == 0)
                        .on_click(on_start),
                );
            div()
                .child(stats_grid)
                .child(tab_row)
                .child(content)
                .child(bottom_nav)
                .into_any_element()
        });

        // ── 整理配置卡(render_dir_picker 需要 &mut App,最后借用 cx)──
        let app: &mut gpui::App = cx;
        let mut config_body = div()
            .flex()
            .flex_col()
            .gap(px(18.0))
            .child(render_dir_picker(&self.preview.dir, window, app))
            .child(template_block)
            .child(mode_block);
        // 4.2.2-4 错误提示(rose,pre-wrap 多行)
        if let Some(err) = error {
            config_body = config_body.child(
                AlertBar::new(AlertVariant::Rose, err)
                    .icon(Icon::AlertCircle)
                    .pre_wrap(true),
            );
        }
        let config_card = card()
            .title("整理配置")
            .subtitle("设置目标目录与命名模板，点击占位符即可插入")
            .child(config_body.child(button_row));

        // ── 页面骨架(警告条 → 配置卡 → 结果区)──
        let mut page = div().flex().flex_col().w_full();
        if let Some(bar) = no_files_bar {
            page = page.child(bar);
        }
        page = page.child(config_card);
        if let Some(results) = results {
            page = page.child(results);
        }
        page.into_any_element()
    }

    // ── 目录树渲染──────────────────────

    /// 目录树组件外壳:头部工具栏(Layers 标题 / 过滤输入 / 全部折叠切换)+
    /// 主体(容器内滚动、min 140 / max 420、bg slate-50)+ 空态。
    fn render_directory_tree(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let filter_raw = self.preview.tree_filter.read(cx).value().to_string();
        let filter_lower = filter_raw.to_lowercase();

        // "全部折叠"/"全部展开"切换:翻转 expandAll 并清空用户开合记录
        // (各节点回到默认开合)
        let on_expand_toggle = cx.listener(|this, _e: &gpui::ClickEvent, _window, cx| {
            this.preview.tree_expand_all = !this.preview.tree_expand_all;
            this.preview.tree_toggled.clear();
            cx.notify();
        });
        let show_tree_filter_clear = !filter_raw.is_empty();
        let on_clear_tree_filter = cx.listener(|this, _e: &gpui::ClickEvent, window, cx| {
            let tree_filter = this.preview.tree_filter.clone();
            tree_filter.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
            cx.notify();
        });
        // 主体:根层遍历(子目录对象键 + `__files__` 文件组;serde_json Map 为
        // BTreeMap,同层目录/文件按字典序排列)
        let tree = &self.preview.directory_tree;
        let root_empty = tree.as_object().map(|o| o.is_empty()).unwrap_or(true);
        let body_inner: gpui::AnyElement = if root_empty {
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .py(px(32.0))
                .text_color(theme::SLATE_500)
                .child(icon_sized(Icon::Folder, px(28.0)).text_color(theme::SLATE_400))
                .child(div().text_size(px(13.0)).child("暂无目录结构数据"))
                .into_any_element()
        } else {
            let mut rows = div().flex().flex_col();
            if let Some(obj) = tree.as_object() {
                for (k, v) in obj {
                    if k == TREE_SENTINEL {
                        let files = tree_files_of(v);
                        rows = rows.child(render_tree_files(&files, 0, &filter_lower));
                    } else {
                        rows = rows.child(self.render_tree_node(
                            &format!("root/{k}"),
                            decode_tree_key(k),
                            v,
                            0,
                            &filter_lower,
                            cx,
                        ));
                    }
                }
            }
            rows.into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .bg(theme::BG_SURFACE)
            .border_1()
            .border_color(theme::BORDER_SUBTLE)
            .rounded(theme::RADIUS_LG)
            .overflow_hidden()
            .child(
                // 头部工具栏:padding 10 16、下边框 subtle、bg slate-50、两端对齐
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .px(px(16.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme::BORDER_SUBTLE)
                    .bg(theme::SLATE_50)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(icon_sized(Icon::Layers, px(15.0)).text_color(theme::AMBER_700))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(gpui::FontWeight(600.0))
                                    .text_color(theme::SLATE_700)
                                    .child("目标目录结构"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            // 过滤输入:w 140、h 28、fontSize 12、pl 24、
                            // 左内嵌 SearchIcon 12 @ left 7，有内容时显示清空 X 按钮
                            .child(
                                div()
                                    .relative()
                                    .w(px(140.0))
                                    .child(
                                        Input::new(&self.preview.tree_filter)
                                            .h(px(28.0))
                                            .py(px(0.0))
                                            .text_size(px(12.0))
                                            .pl(px(24.0))
                                            .pr(if show_tree_filter_clear { px(22.0) } else { px(8.0) }),
                                    )
                                    .child(
                                        div()
                                            .absolute()
                                            .left(px(7.0))
                                            .top(px(0.0))
                                            .bottom(px(0.0))
                                            .flex()
                                            .items_center()
                                            .child(
                                                icon_sized(Icon::Search, px(12.0))
                                                    .text_color(theme::SLATE_500),
                                            ),
                                    )
                                    .when(show_tree_filter_clear, |el| {
                                        el.child(
                                            div()
                                                .absolute()
                                                .right(px(4.0))
                                                .top(px(0.0))
                                                .bottom(px(0.0))
                                                .flex()
                                                .items_center()
                                                .child(
                                                    div()
                                                        .id("tree-filter-clear")
                                                        .p(px(2.0))
                                                        .rounded(theme::RADIUS_XS)
                                                        .cursor_pointer()
                                                        .hover(|st| st.bg(theme::SLATE_200))
                                                        .child(
                                                            icon_sized(Icon::X, px(11.0))
                                                                .text_color(theme::SLATE_500),
                                                        )
                                                        .on_click(
                                                            move |e: &gpui::ClickEvent, window, cx| {
                                                                on_clear_tree_filter(e, window, cx);
                                                            },
                                                        ),
                                                ),
                                        )
                                    }),
                            )
                            .child(
                                Button::new("tree-expand-toggle")
                                    .label(if self.preview.tree_expand_all {
                                        "全部折叠"
                                    } else {
                                        "全部展开"
                                    })
                                    .icon(
                                        if self.preview.tree_expand_all {
                                            Icon::Folder
                                        } else {
                                            Icon::FolderOpen
                                        },
                                        px(12.0),
                                    )
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::Sm)
                                    .text_size(px(12.0))
                                    .pad_x(px(8.0))
                                    .pad_y(px(4.0))
                                    .on_click(on_expand_toggle),
                            ),
                    ),
            )
            .child(
                // 主体:padding 12 14、min 140 / max 420、容器内滚动、bg slate-50
                div()
                    .id("preview-tree-body")
                    .px(px(14.0))
                    .py(px(12.0))
                    .min_h(TREE_BODY_MIN_H)
                    .max_h(TREE_BODY_MAX_H)
                    .overflow_y_scroll()
                    .bg(theme::SLATE_50)
                    .child(body_inner),
            )
    }

    /// 目录节点行(递归):箭头 + 文件夹图标 + 目录名 + 数量徽标 `(直接子项数)`;
    /// 展开时先渲染子目录、后渲染 `__files__` 文件组。
    fn render_tree_node(
        &self,
        path_key: &str,
        name: &str,
        node: &serde_json::Value,
        depth: usize,
        filter_lower: &str,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let path_key = path_key.to_string();
        let Some(obj) = node.as_object() else {
            return div();
        };
        let files = obj.get(TREE_SENTINEL).map(tree_files_of).unwrap_or_default();
        let subdirs: Vec<(&String, &serde_json::Value)> =
            obj.iter().filter(|(k, _)| k.as_str() != TREE_SENTINEL).collect();
        let total_items = files.len() + subdirs.len();

        let user_toggled = self.preview.tree_toggled.contains(&path_key);
        let open = tree_node_open(self.preview.tree_expand_all, user_toggled, depth);

        // 点击行 → 切换开合(记录与默认相反的节点)
        let toggle_key = path_key.clone();
        let on_toggle = cx.listener(move |this, _e: &gpui::ClickEvent, _window, cx| {
            if !this.preview.tree_toggled.insert(toggle_key.clone()) {
                this.preview.tree_toggled.remove(&toggle_key);
            }
            cx.notify();
        });

        // 目录行:padding 5px 12px 5px (depth*20+6)、fontSize 13、weight 600、
        // slate-800、圆角 6、hover slate-100;箭头 14 slate-400;
        // FolderOpen/Folder 16 amber-500;数量徽标 11 slate-400
        let row = div()
            .id(SharedString::from(format!("tree-dir-{path_key}")))
            .flex()
            .items_center()
            .gap(px(6.0))
            .pl(px(depth as f32 * 20.0 + 6.0))
            .pr(px(12.0))
            .py(px(5.0))
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight(600.0))
            .text_color(theme::SLATE_800)
            .rounded(theme::RADIUS_SM)
            .cursor_pointer()
            .hover(|st| st.bg(theme::SLATE_100))
            .child(
                icon_sized(
                    if open { Icon::ChevronDown } else { Icon::ChevronRight },
                    px(14.0),
                )
                .text_color(theme::SLATE_500),
            )
            .child(
                icon_sized(if open { Icon::FolderOpen } else { Icon::Folder }, px(16.0))
                    .text_color(theme::AMBER_500),
            )
            .child(div().child(name.to_string()))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme::SLATE_500)
                    .ml(px(4.0))
                    .child(format!("({total_items})")),
            )
            .on_click(move |e: &gpui::ClickEvent, window, cx| on_toggle(e, window, cx));

        let mut node_el = div().flex().flex_col().child(row);
        if open {
            let mut children = div().flex().flex_col();
            for (k, v) in subdirs {
                let child_key = format!("{path_key}/{k}");
                children = children.child(self.render_tree_node(
                    &child_key,
                    decode_tree_key(k),
                    v,
                    depth + 1,
                    filter_lower,
                    cx,
                ));
            }
            if !files.is_empty() {
                children = children.child(render_tree_files(&files, depth, filter_lower));
            }
            node_el = node_el.child(children);
        }
        node_el
    }
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 键盘快捷键监听:支持 Cmd+1/2/3(macOS) 或 Ctrl+1/2/3(Windows/Linux) 切换向导步骤
        let on_key_down = cx.listener(|this, event: &gpui::KeyDownEvent, _window, cx| {
            let is_primary_mod =
                event.keystroke.modifiers.platform || event.keystroke.modifiers.control;
            if is_primary_mod && !event.keystroke.modifiers.alt {
                let target_step = match event.keystroke.key.as_str() {
                    "1" => Some(1),
                    "2" => Some(2),
                    "3" => Some(3),
                    _ => None,
                };
                if let Some(step) = target_step {
                    this.go_to_step(step, cx);
                }
            }
        });

        // 根容器:100vh、纵向 flex、bg-app
        let shell = div()
            .id("app-shell")
            .track_focus(&self.focus_handle)
            .on_key_down(on_key_down)
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::BG_APP)
            .text_color(theme::TEXT_PRIMARY)
            .font_family(theme::FONT_SANS)
            .line_height(gpui::relative(theme::LINE_HEIGHT_BASE))
            .text_size(theme::FONT_SIZE_BASE)
            // 顶栏
            .child(self.render_header(window, cx))
            // 中段:aside + main
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(step_nav_aside().child({
                        // 步骤点击:仅 ≤ max_unlocked_step 可达
                        let on_step = cx.listener(|this, step: &usize, _window, cx| {
                            this.go_to_step(*step, cx);
                        });
                        StepNav::new(
                            self.current_step,
                            self.max_unlocked_step,
                            move |step, _e, window, cx| on_step(&step, window, cx),
                        )
                    }))
                    .child(
                        // 右工作区:flex 1、纵向滚动、padding clamp(16,2.5vw,32) → 24
                        div()
                            .id("workspace-scroll")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .p(px(24.0))
                            .child(
                                div()
                                    .max_w(px(1080.0))
                                    .w_full()
                                    .mx_auto()
                                    .child(self.render_page(window, cx)),
                            ),
                    ),
            );
        // 确认弹窗(单例,deferred 遮罩盖在最上层)
        let confirm_el: gpui::AnyElement = match &self.confirm {
            Some(pending) => {
                let on_result = cx.listener(|this, ok: &bool, window, cx| {
                    this.handle_confirm(*ok, window, cx);
                });
                ConfirmModal::new(
                    pending.options.clone(),
                    self.confirm_focus.clone(),
                    move |ok, window, cx| on_result(&ok, window, cx),
                )
                .into_any_element()
            }
            None => div().into_any_element(),
        };

        div().relative().size_full().overflow_hidden().child(shell).child(confirm_el)
    }
}

// ── 扫描页渲染辅助 ───────────────────────────────────────────────────────────

/// 看板计数胶囊:badge 加强版,padding 6px 12px、fontSize 12、
/// gap 7;label opacity 0.75 / weight 500,数值 13.5 / weight 700。
fn stat_pill(variant: BadgeVariant, icon: Icon, label: &str, value: usize) -> gpui::Div {
    let (_, fg, _) = variant.colors();
    badge(variant)
        .gap(px(7.0))
        .px(px(12.0))
        .py(px(6.0))
        .text_size(px(12.0))
        .child(icon_sized(icon, px(13.0)).text_color(fg))
        .child(
            div()
                .opacity(0.75)
                .font_weight(gpui::FontWeight(500.0))
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(13.5))
                .font_weight(gpui::FontWeight(700.0))
                .child(value.to_string()),
        )
}

/// 表格列定义:(列名, 宽度百分比)。
const SCAN_TABLE_COLS: [(&str, f32); 5] = [
    ("文件名", 0.30),
    ("艺术家", 0.18),
    ("专辑", 0.20),
    ("标题", 0.22),
    ("状态", 0.10),
];

/// 表头单元格:padding 10px 12px、weight 600、slate-600、bg slate-50。
fn scan_header_cell(width: f32, label: &str) -> gpui::Div {
    div()
        .w(DefiniteLength::Fraction(width))
        .px(px(12.0))
        .py(px(10.0))
        .text_size(px(12.5))
        .font_weight(gpui::FontWeight(600.0))
        .text_color(theme::SLATE_600)
        .whitespace_nowrap()
        .child(label.to_string())
}

/// 正文单元格:padding 9px 12px、slate-700、内容 truncate。
fn scan_text_cell(width: f32, text: &str) -> gpui::Div {
    div()
        .w(DefiniteLength::Fraction(width))
        .px(px(12.0))
        .py(px(9.0))
        .text_size(px(12.5))
        .text_color(theme::SLATE_700)
        .child(div().truncate().child(text.to_string()))
}

/// 文件表格:外层水平滚动(表 minWidth 560)、固定表头、
/// 表体容器内垂直滚动(≤200 行直接构建,不虚拟化);行 hover 底色 slate-50、
/// 无斑马纹/无选中态;状态列 StatusBadge sm(ok/unreadable)。
fn render_scan_table(display: &[AudioMetadata]) -> impl IntoElement {
    let header = div()
        .flex()
        .bg(theme::SLATE_50)
        .border_b_1()
        .border_color(theme::BORDER_SUBTLE)
        .child(scan_header_cell(SCAN_TABLE_COLS[0].1, SCAN_TABLE_COLS[0].0))
        .child(scan_header_cell(SCAN_TABLE_COLS[1].1, SCAN_TABLE_COLS[1].0))
        .child(scan_header_cell(SCAN_TABLE_COLS[2].1, SCAN_TABLE_COLS[2].0))
        .child(scan_header_cell(SCAN_TABLE_COLS[3].1, SCAN_TABLE_COLS[3].0))
        .child(scan_header_cell(SCAN_TABLE_COLS[4].1, SCAN_TABLE_COLS[4].0));

    let shown_count = display.len().min(TABLE_LIMIT);
    let mut body = div()
        .id("scan-table-body")
        .flex()
        .flex_col()
        .max_h(TABLE_BODY_MAX_H)
        .overflow_y_scroll();
    for (ix, f) in display.iter().take(TABLE_LIMIT).enumerate() {
        let status: &'static str = if f.readable { "ok" } else { "unreadable" };
        let row = div()
            .id(SharedString::from(format!("scan-row-{ix}")))
            .flex()
            .items_center()
            .hover(|st| st.bg(theme::SLATE_50))
            .when(ix + 1 < shown_count, |el| {
                el.border_b_1().border_color(theme::SLATE_100)
            })
            .child(scan_text_cell(SCAN_TABLE_COLS[0].1, basename(&f.path)))
            .child(scan_text_cell(SCAN_TABLE_COLS[1].1, &f.artist))
            .child(scan_text_cell(SCAN_TABLE_COLS[2].1, &f.album))
            .child(scan_text_cell(SCAN_TABLE_COLS[3].1, &f.title))
            .child(
                div()
                    .w(DefiniteLength::Fraction(SCAN_TABLE_COLS[4].1))
                    .px(px(12.0))
                    .py(px(9.0))
                    .child(StatusBadge::new(status).size(StatusBadgeSize::Sm)),
            );
        body = body.child(row);
    }

    // 外层水平滚动 + 表宽下限 560(源 minWidth 560 / overflowX auto)
    div()
        .id("scan-table-scroll")
        .overflow_x_scroll()
        .child(div().flex().flex_col().min_w(px(560.0)).child(header).child(body))
}

// ── 预览页渲染辅助────────────────────────────

/// 表单字段标签:fontSize 13、weight 600、slate-700、marginBottom 6。
fn field_label(text: &str) -> gpui::Div {
    div()
        .text_size(px(13.0))
        .font_weight(gpui::FontWeight(600.0))
        .text_color(theme::SLATE_700)
        .mb(px(6.0))
        .child(text.to_string())
}

// ── 进度页渲染辅助────────────────────────────────

/// 概览卡字段小标签:fontSize 11.5、slate-500、marginBottom 4。
fn overview_label(text: &str) -> gpui::Div {
    div()
        .text_size(px(11.5))
        .text_color(theme::SLATE_500)
        .mb(px(4.0))
        .child(text.to_string())
}

/// 日志行:`[n/total]` 前缀琥珀 amber-400 #ffc533、
/// 正文天蓝 sky-300 #bae6fd;不匹配整行 slate-400。
fn render_log_line(ix: usize, line: &str) -> gpui::Stateful<gpui::Div> {
    let row = div().id(SharedString::from(format!("log-line-{ix}")));
    match split_log_line(line) {
        Some((prefix, rest)) => row
            .flex()
            .child(div().text_color(theme::AMBER_400).child(prefix.to_string()))
            .child(div().text_color(theme::SKY_300).child(rest.to_string())),
        None => row.text_color(theme::SLATE_400).child(line.to_string()),
    }
}

/// 分段控件按钮:
/// 激活 = weight 600 + amber-800 + 白底 + 圆角 6 + shadow-xs;
/// 未激活 = weight 500 + slate-600 + 透明底。可选计数徽章(list Tab)。
fn segment_btn(
    id: &str,
    label: &str,
    icon: Icon,
    active: bool,
    icon_color: gpui::Rgba,
    badge: Option<usize>,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let mut btn = div()
        .id(SharedString::from(id.to_string()))
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(14.0))
        .py(px(6.0))
        .text_size(px(13.0))
        .font_weight(gpui::FontWeight(if active { 600.0 } else { 500.0 }))
        .rounded(theme::RADIUS_SM)
        .whitespace_nowrap()
        .cursor_pointer()
        .child(icon_sized(icon, px(14.0)).text_color(icon_color))
        .child(div().child(label.to_string()))
        .on_click(move |e: &gpui::ClickEvent, window, cx| on_click(e, window, cx));
    if active {
        btn = btn
            .bg(theme::BG_SURFACE)
            .text_color(theme::AMBER_800)
            .shadow(theme::shadow_xs());
    } else {
        btn = btn.text_color(theme::SLATE_600);
    }
    if let Some(n) = badge {
        btn = btn.child(
            div()
                .text_size(px(11.0))
                .px(px(6.0))
                .py(px(1.0))
                .rounded(theme::RADIUS_FULL)
                .font_weight(gpui::FontWeight(600.0))
                .when(active, |el| {
                    el.bg(theme::AMBER_200).text_color(theme::AMBER_900)
                })
                .when(!active, |el| {
                    el.bg(theme::SLATE_200).text_color(theme::SLATE_600)
                })
                .child(n.to_string()),
        );
    }
    btn
}

/// 占位符芯片:TagIcon 12 + 等宽 tag(weight 600)+
/// 中文小标 11(常态 slate-500 / 悬浮 amber-800);悬浮 = amber-400 边框 +
/// amber-100 底 + 深色文字(#0f172a)。
/// hover 逐子元素变色在 gpui 需页面持有 hover 状态(on_hover 上提实现)。
fn placeholder_chip(
    ix: usize,
    tag: &str,
    label: &str,
    hovered: bool,
    on_insert: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
    on_hover: impl Fn(&bool, &mut Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let (border, bg, text, icon_color, label_color) = if hovered {
        (
            theme::AMBER_400,
            theme::AMBER_100,
            theme::INHERITED_TEXT,
            theme::AMBER_700,
            theme::AMBER_800,
        )
    } else {
        (
            theme::SLATE_200,
            theme::SLATE_50,
            theme::SLATE_700,
            theme::SLATE_500,
            theme::SLATE_500,
        )
    };
    div()
        .id(SharedString::from(format!("ph-chip-{ix}")))
        .flex()
        .items_center()
        .gap(px(5.0))
        .px(px(8.0))
        .py(px(3.0))
        .text_size(px(12.0))
        .rounded(theme::RADIUS_SM)
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_color(text)
        .cursor_pointer()
        .child(icon_sized(Icon::Tag, px(12.0)).text_color(icon_color))
        .child(
            div()
                .font_family(theme::FONT_MONO)
                .font_weight(gpui::FontWeight(600.0))
                .child(tag.to_string()),
        )
        .child(
            div()
                .text_size(px(11.0))
                .opacity(0.9)
                .text_color(label_color)
                .child(label.to_string()),
        )
        .on_click(move |e: &gpui::ClickEvent, window, cx| on_insert(e, window, cx))
        .on_hover(move |hovered: &bool, window, cx| on_hover(hovered, window, cx))
}

/// 移动模式警告条:amber-50 底 / amber-200 边框、圆角 8、
/// padding 10 14;AlertTriangle 16 amber-600;文字 12.5 amber-800、行高 1.6,
/// "移动模式不可逆："加粗(StyledText 高亮,等价源 `<strong>`)。
fn move_mode_warning() -> gpui::Div {
    const PREFIX: &str = "移动模式不可逆：";
    let text = format!(
        "{PREFIX}执行后源文件将从原目录删除。请再次确认目标目录与命名模板正确，且源文件已做好备份。"
    );
    div()
        .flex()
        .items_start()
        .gap(px(10.0))
        .px(px(14.0))
        .py(px(10.0))
        .bg(theme::AMBER_50)
        .border_1()
        .border_color(theme::AMBER_200)
        .rounded(theme::RADIUS_MD)
        .mt(px(10.0))
        .child(
            div().flex_none().mt(px(2.0)).child(
                icon_sized(Icon::AlertTriangle, px(16.0)).text_color(theme::AMBER_600),
            ),
        )
        .child(
            div()
                .text_size(px(12.5))
                .text_color(theme::AMBER_800)
                .line_height(gpui::relative(1.6))
                .child(StyledText::new(text).with_highlights(vec![(
                    0..PREFIX.len(),
                    HighlightStyle {
                        font_weight: Some(gpui::FontWeight(700.0)),
                        ..Default::default()
                    },
                )])),
        )
}

/// StatCard:白底、圆角 12、阴影 xs、padding 16 18、
/// 左列(标题 12/500/slate-500 + 数值 22/700/变体色)+ 右侧 38px 图标方块
/// (变体底色/图标色);边框色随变体。
fn stat_card(
    title: &str,
    value: usize,
    icon: Icon,
    border: gpui::Rgba,
    icon_bg: gpui::Rgba,
    icon_color: gpui::Rgba,
    val_color: gpui::Rgba,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .flex_1()
        .min_w(px(150.0))
        .px(px(18.0))
        .py(px(16.0))
        .bg(theme::BG_SURFACE)
        .border_1()
        .border_color(border)
        .rounded(theme::RADIUS_LG)
        .shadow(theme::shadow_xs())
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .min_w(px(0.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight(500.0))
                        .text_color(theme::SLATE_500)
                        .whitespace_nowrap()
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(px(22.0))
                        .font_weight(gpui::FontWeight(700.0))
                        .text_color(val_color)
                        .child(value.to_string()),
                ),
        )
        .child(
            div()
                .size(px(38.0))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(theme::RADIUS_MD)
                .bg(icon_bg)
                .child(icon_sized(icon, px(18.0)).text_color(icon_color)),
        )
}

/// 映射表列定义:(列名, 宽度百分比)。
const PREVIEW_TABLE_COLS: [(&str, f32); 3] = [("源文件", 0.38), ("目标路径", 0.46), ("最终状态", 0.16)];

/// 映射表:源文件(basename)38% / 目标路径(final_target 完整
/// 路径)46% / 最终状态(StatusBadge sm)16%;表 minWidth 560 + 外层水平滚动、
/// 固定表头 + 表体容器内滚动(同扫描页);行 hover 底色
/// slate-50、无斑马纹;**冲突行无特殊底色**(以 amber 徽章表达,源行为);
/// 行不可点击。最多渲染 300 行(截断提示由调用方追加)。
fn render_mapping_table(mappings: &[FileMappingItem]) -> impl IntoElement {
    let header = div()
        .flex()
        .bg(theme::SLATE_50)
        .border_b_1()
        .border_color(theme::BORDER_SUBTLE)
        .child(scan_header_cell(PREVIEW_TABLE_COLS[0].1, PREVIEW_TABLE_COLS[0].0))
        .child(scan_header_cell(PREVIEW_TABLE_COLS[1].1, PREVIEW_TABLE_COLS[1].0))
        .child(scan_header_cell(PREVIEW_TABLE_COLS[2].1, PREVIEW_TABLE_COLS[2].0));

    let shown_count = mappings.len().min(PREVIEW_TABLE_LIMIT);
    let mut body = div()
        .id("preview-table-body")
        .flex()
        .flex_col()
        .max_h(TABLE_BODY_MAX_H)
        .overflow_y_scroll();
    for (ix, m) in mappings.iter().take(PREVIEW_TABLE_LIMIT).enumerate() {
        let row = div()
            .id(SharedString::from(format!("preview-row-{ix}")))
            .flex()
            .items_center()
            .hover(|st| st.bg(theme::SLATE_50))
            .when(ix + 1 < shown_count, |el| {
                el.border_b_1().border_color(theme::SLATE_100)
            })
            .child(scan_text_cell(PREVIEW_TABLE_COLS[0].1, basename(&m.source)))
            .child(scan_text_cell(PREVIEW_TABLE_COLS[1].1, &m.final_target))
            .child(
                div()
                    .w(DefiniteLength::Fraction(PREVIEW_TABLE_COLS[2].1))
                    .px(px(12.0))
                    .py(px(9.0))
                    .child(StatusBadge::from_mapping_status(m.status).size(StatusBadgeSize::Sm)),
            );
        body = body.child(row);
    }

    div()
        .id("preview-table-scroll")
        .overflow_x_scroll()
        .child(div().flex().flex_col().min_w(px(560.0)).child(header).child(body))
}

/// 取 `__files__` 哨兵键下的文件名数组。
fn tree_files_of(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// 文件行组(`__files__` 叶子):FileAudioIcon 14 amber-600 +
/// 等宽文件名 12.5 slate-700、truncate、hover slate-100;缩进 (depth+1)*20+8;
/// **整组无过滤匹配则不渲染**(目录节点不过滤,仅文件)。
fn render_tree_files(files: &[String], depth: usize, filter_lower: &str) -> gpui::Div {
    let mut col = div().flex().flex_col();
    let mut any = false;
    for file in files {
        if !tree_file_matches(file, filter_lower) {
            continue;
        }
        any = true;
        col = col.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .pl(px((depth as f32 + 1.0) * 20.0 + 8.0))
                .pr(px(12.0))
                .py(px(4.0))
                .text_size(px(12.5))
                .font_family(theme::FONT_MONO)
                .text_color(theme::SLATE_700)
                .rounded(theme::RADIUS_XS)
                .hover(|st| st.bg(theme::SLATE_100))
                .child(icon_sized(Icon::FileAudio, px(14.0)).text_color(theme::AMBER_600))
                .child(div().flex_1().min_w(px(0.0)).truncate().child(file.clone())),
        );
    }
    if any {
        col
    } else {
        div()
    }
}


// ── 窗口关闭确认挂接 ─────────────────────────────────────────────────────────
//
// 基于 gpui 0.2.2 的 `window.on_window_should_close(cx, f)`(返回 false 可阻止关闭)。

impl AppShell {
    /// 在窗口根视图构建后调用(见 main.rs):注册"点关闭按钮先确认"。
    /// 确认 → `exit_confirmed = true` + `cx.quit()`;取消 → 不动作。
    pub fn register_close_guard(
        shell: &Entity<AppShell>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let weak = shell.downgrade();
        window.on_window_should_close(cx, move |window, cx: &mut gpui::App| {
            weak.update(cx, |this, cx| {
                if this.exit_confirmed {
                    true // 已确认,放行
                } else {
                    this.open_exit_confirm(window, cx);
                    false // 拦下本次关闭,等待确认
                }
            })
            .unwrap_or(true) // 实体已释放(不应发生)则放行
        });
    }
}

// ── 截图取证(T1 视觉证据;正常启动路径不经过)──────────────────────────────

/// 演示源/目标目录(仅截图态使用;目标目录在注入时按需创建)。
const SHOT_SOURCE_DIR: &str = "/tmp/t2f-shots/music";
const SHOT_TARGET_DIR: &str = "/tmp/t2f-shots/target";

impl AppShell {
    /// 构造 5 个演示文件的元数据(3 位艺术家;1 个不可读,字段为兜底值)。
    fn shot_files() -> Vec<AudioMetadata> {
        let mk = |file: &str,
                  artist: &str,
                  album: &str,
                  title: &str,
                  track: &str,
                  year: &str,
                  genre: &str,
                  readable: bool,
                  error: &str| AudioMetadata {
            path: format!("{SHOT_SOURCE_DIR}/{file}"),
            ext: file.rsplit('.').next().unwrap_or("").to_string(),
            artist: artist.into(),
            album: album.into(),
            title: title.into(),
            track: track.into(),
            year: year.into(),
            genre: genre.into(),
            readable,
            error: error.into(),
        };
        vec![
            mk("陈奕迅 - 陪你度过漫长岁月.mp3", "陈奕迅", "准备中", "陪你度过漫长岁月", "03", "2015", "Pop", true, ""),
            mk("陈奕迅 - 十年.flac", "陈奕迅", "黑·白·灰", "十年", "07", "2003", "Pop", true, ""),
            mk("王菲 - 匆匆那年.flac", "王菲", "匆匆那年", "匆匆那年", "01", "2014", "原声带", true, ""),
            mk("周杰伦 - 晴天.mp3", "周杰伦", "叶惠美", "晴天", "04", "2003", "Mandopop", true, ""),
            mk("track05_corrupt.ogg", "Unknown Artist", "Unknown Album", "track05_corrupt", "0", "Unknown Year", "Unknown Genre", false, "Not a supported audio format"),
        ]
    }

    /// [取证专用]把向导直接置为指定演示态。预览/进度态的映射与目录树都经
    /// 真实 `service::generate_preview` 计算,保证截图忠实于实际代码路径。
    /// 返回 false 表示状态名不被识别。调用方:`src/shot.rs`(T2F_SHOT_* 模式)。
    #[doc(hidden)]
    pub fn setup_shot_state(
        &mut self,
        state: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // 预览/进度共用的整理批次(真实预览计算,Move 模式)
        let preview_bundle = if state.starts_with("preview") || state == "progress" {
            std::fs::create_dir_all(SHOT_TARGET_DIR).ok();
            let req = PreviewRequest {
                files: Self::shot_files(),
                template: DEFAULT_TEMPLATE.to_string(),
                target_dir: SHOT_TARGET_DIR.to_string(),
                mode: OrganizeMode::Move,
            };
            match service::generate_preview(req) {
                Ok(resp) => Some(resp),
                Err(err) => {
                    eprintln!("[shot] generate_preview 失败: {err}");
                    None
                }
            }
        } else {
            None
        };

        match state {
            // 初始空态:构造默认即步骤 1 空表单,无需注入
            "empty" => {}

            // 步骤 1:已扫描出 5 个文件的表格态
            "scan" => {
                let files = Self::shot_files();
                self.scan.dir.update(cx, |s, cx| {
                    s.set_value(SHOT_SOURCE_DIR, window, cx);
                });
                self.scan.files.clone_from(&files);
                self.scan.has_scanned = true;
                self.scan.source_dir = SHOT_SOURCE_DIR.to_string();
                self.scanned_files = files;
                self.source_dir = SHOT_SOURCE_DIR.to_string();
            }

            // 步骤 2:含映射表/目录树/统计卡的完整预览态(真实预览计算)
            "preview" | "preview_tree" => {
                let Some(resp) = preview_bundle else { return false };
                self.scan.dir.update(cx, |s, cx| {
                    s.set_value(SHOT_SOURCE_DIR, window, cx);
                });
                self.scan.files = Self::shot_files();
                self.scan.has_scanned = true;
                self.scan.source_dir = SHOT_SOURCE_DIR.to_string();
                self.scanned_files = Self::shot_files();
                self.source_dir = SHOT_SOURCE_DIR.to_string();

                self.preview.dir.update(cx, |s, cx| {
                    s.set_value(SHOT_TARGET_DIR, window, cx);
                });
                self.preview.mode = OrganizeMode::Move;
                self.preview.mappings = resp.mappings.clone();
                self.preview.directory_tree = resp.directory_tree;
                self.preview.resolved_target_dir = resp.target_dir.clone();
                self.preview.active_tab = if state == "preview_tree" {
                    PreviewTab::Tree
                } else {
                    PreviewTab::List
                };
                self.preview.form_template = DEFAULT_TEMPLATE.to_string();
                self.preview.form_target_dir = SHOT_TARGET_DIR.to_string();
                self.current_step = 2;
                self.max_unlocked_step = 2;
            }

            // 步骤 3:进度 60%(3/5)+ 若干日志行的执行态
            "progress" => {
                let Some(resp) = preview_bundle else { return false };
                let total = resp.mappings.len().max(1);
                let current = (total * 3) / 5; // 60%
                let mappings = resp.mappings.clone();
                let mut log = Vec::new();
                for (idx, m) in mappings.iter().enumerate().take(current) {
                    // 忠实于 apply_task_snapshot 的行形状:每文件两条
                    // `[start_i/total]` 与 `[done_i/total]`(current_file 恒非空)
                    let name = basename(&m.source);
                    log.push(format!("[{}/{}] {}", idx, total, name));
                    log.push(format!("[{}/{}] {}", idx + 1, total, name));
                }
                self.organize_mappings = mappings.clone();
                self.organize_mode = OrganizeMode::Move;
                self.organize_target_dir = SHOT_TARGET_DIR.to_string();
                self.progress.started = true;
                self.progress.progress = Some(ProgressEvent {
                    task_id: "shot-task".to_string(),
                    status: TaskStatus::Running,
                    current,
                    total,
                    current_file: mappings
                        .get(current)
                        .map(|m| m.source.clone())
                        .unwrap_or_default(),
                    message: format!("Processed {current}/{total}"),
                });
                self.progress.log = log;
                self.current_step = 3;
                self.max_unlocked_step = 3;
            }

            _ => return false,
        }
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(path: &str, artist: &str, year: &str, genre: &str, readable: bool) -> AudioMetadata {
        AudioMetadata {
            path: path.into(),
            ext: "mp3".into(),
            artist: artist.into(),
            album: "Unknown Album".into(),
            title: "Unknown Title".into(),
            track: "0".into(),
            year: year.into(),
            genre: genre.into(),
            readable,
            error: String::new(),
        }
    }

    /// JS `p.split(/[/\\]/).pop()` 语义:两种分隔符、无分隔符、空串
    #[test]
    fn basename_splits_on_both_separators() {
        assert_eq!(basename("/Users/me/a.mp3"), "a.mp3");
        assert_eq!(basename("C:\\Music\\b.flac"), "b.flac");
        assert_eq!(basename("plain.mp3"), "plain.mp3");
        assert_eq!(basename(""), "");
    }


    /// 筛选匹配:大小写不敏感子串;filename 字段只匹配 basename
    #[test]
    fn filter_matches_case_insensitive_substring() {
        let f = meta("/m/Beat It.mp3", "Michael Jackson", "1982", "Pop", true);
        assert!(matches_filter(FilterField::Filename, "beat", &f));
        assert!(!matches_filter(FilterField::Filename, "/m/", &f)); // 只看 basename
        assert!(matches_filter(FilterField::Artist, "jackson", &f)); // 入参约定已 lowercase
        assert!(matches_filter(FilterField::Album, "unknown", &f));
        assert!(matches_filter(FilterField::Year, "19", &f));
        assert!(!matches_filter(FilterField::Genre, "rock", &f));
        // 反斜杠路径的 basename
        let w = meta("C:\\Music\\Song.mp3", "X", "2000", "Pop", false);
        assert!(matches_filter(FilterField::Filename, "song", &w));
    }

    // ── 预览页──────────────────────────────────────────────────

    fn mapping(status: MappingStatus) -> FileMappingItem {
        FileMappingItem {
            source: "/tmp/a.mp3".into(),
            target: "/out/A/a.mp3".into(),
            final_target: "/out/A/a.mp3".into(),
            relative_target: "A/a.mp3".into(),
            status,
            conflict: false,
            batch_conflict: false,
        }
    }

    /// 开始整理前剔除三类不可执行映射:unreadable /
    /// boundary_error / write_error;保留 ok/conflict/batch_conflict/missing_metadata
    #[test]
    fn organizable_filter_excludes_blocked_statuses() {
        let mappings = [
            mapping(MappingStatus::Ok),
            mapping(MappingStatus::Conflict),
            mapping(MappingStatus::BatchConflict),
            mapping(MappingStatus::MissingMetadata),
            mapping(MappingStatus::Unreadable),
            mapping(MappingStatus::BoundaryError),
            mapping(MappingStatus::WriteError),
        ];
        let organizable: Vec<_> = mappings.iter().filter(|m| is_organizable(m)).collect();
        assert_eq!(organizable.len(), 4);
        assert!(organizable.iter().all(|m| is_organizable(m)));
    }

    /// 目录树节点开合:默认展开 0/1 层;用户切换取反;
    /// expandAll 关闭后默认全收起(用户记录同时被清空,由调用方保证)
    #[test]
    fn tree_node_open_follows_default_and_user_toggle() {
        // expandAll=true:0/1 层默认展开,2 层默认收起
        assert!(tree_node_open(true, false, 0));
        assert!(tree_node_open(true, false, 1));
        assert!(!tree_node_open(true, false, 2));
        // 用户切换 → 取反
        assert!(!tree_node_open(true, true, 0));
        assert!(tree_node_open(true, true, 2));
        // expandAll=false:全部默认收起
        assert!(!tree_node_open(false, false, 0));
        assert!(!tree_node_open(false, false, 1));
    }

    /// 目录树过滤:文件名小写子串匹配;空过滤全通过
    #[test]
    fn tree_file_filter_is_case_insensitive_substring() {
        assert!(tree_file_matches("Song.mp3", ""));
        assert!(tree_file_matches("Song.mp3", "song"));
        assert!(tree_file_matches("Song.mp3", "MP3"));
        assert!(!tree_file_matches("Song.mp3", "flac"));
    }

    /// 转义哨兵键解码:目录组件恰好叫 `__files__` 时还原展示名
    #[test]
    fn tree_key_decoding_restores_escaped_sentinel() {
        assert_eq!(decode_tree_key("__files__\u{0}"), "__files__");
        assert_eq!(decode_tree_key("Artist"), "Artist");
        assert_eq!(decode_tree_key("__files__"), "__files__");
    }

    // ── 进度页──────────────────────────────────────────────────

    /// 日志行颜色分级:`[...]` 前缀 + 正文;`\s*` 消耗全部空白;
    /// 不匹配(无 `[` 开头/无闭括号)→ None;空括号 `[]` 是合法前缀
    #[test]
    fn log_line_splits_bracket_prefix() {
        assert_eq!(split_log_line("[2/5] a.mp3"), Some(("[2/5]", "a.mp3")));
        assert_eq!(split_log_line("[2/5]   a.mp3"), Some(("[2/5]", "a.mp3")));
        assert_eq!(split_log_line("Processed 1/3"), None);
        assert_eq!(split_log_line("[unclosed"), None);
        assert_eq!(split_log_line("[]"), Some(("[]", "")));
        assert_eq!(split_log_line(""), None);
    }

    /// 日志去重与上限:current_file 行仅连续相同去重,
    /// message 行无去重;缓冲上限 300 丢弃最旧
    #[test]
    fn log_dedup_and_cap() {
        let mut log = Vec::new();
        for _ in 0..5 {
            append_log_line_dedup(&mut log, "[1/2] a.mp3".to_string());
        }
        assert_eq!(log, vec!["[1/2] a.mp3"]);
        append_log_line_dedup(&mut log, "[2/2] b.mp3".to_string());
        // 非连续重复必须保留(仅比对最后一行)
        append_log_line_dedup(&mut log, "[1/2] a.mp3".to_string());
        assert_eq!(log.len(), 3);

        // message 分支无去重(源:无条件追加)
        let mut m = Vec::new();
        append_log_line(&mut m, "Completed 2 file(s).".to_string());
        append_log_line(&mut m, "Completed 2 file(s).".to_string());
        assert_eq!(m.len(), 2);

        // 上限 300,丢弃最旧
        let mut big = Vec::new();
        for i in 0..(LOG_CAP + 10) {
            append_log_line(&mut big, format!("line {i}"));
        }
        assert_eq!(big.len(), LOG_CAP);
        assert_eq!(big.first().unwrap(), "line 10");
        assert_eq!(big.last().unwrap(), &format!("line {}", LOG_CAP + 9));
    }

    /// pct = round(current/total*100);无快照或 total=0 → 0
    #[test]
    fn task_percent_rounds_and_defaults_zero() {
        let ev = |cur: usize, total: usize| ProgressEvent {
            task_id: "t".into(),
            status: TaskStatus::Running,
            current: cur,
            total,
            current_file: String::new(),
            message: String::new(),
        };
        assert_eq!(task_percent(None), 0);
        assert_eq!(task_percent(Some(&ev(0, 3))), 0);
        assert_eq!(task_percent(Some(&ev(1, 3))), 33); // 33.33 → 33
        assert_eq!(task_percent(Some(&ev(2, 3))), 67); // 66.67 → 67
        assert_eq!(task_percent(Some(&ev(3, 3))), 100);
        assert_eq!(task_percent(Some(&ev(5, 0))), 0);
    }

    /// task_id 持久化往返 + 静默失败语义(缺文件/损坏内容 → 空串)
    #[test]
    fn task_id_state_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("t2f-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(STATE_FILE);

        assert_eq!(read_state_file(&path), ""); // 缺文件
        write_state_file(&path, "abc-def");
        assert_eq!(read_state_file(&path), "abc-def");
        write_state_file(&path, "");
        assert_eq!(read_state_file(&path), "");
        // 损坏内容 → 静默空串
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(read_state_file(&path), "");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

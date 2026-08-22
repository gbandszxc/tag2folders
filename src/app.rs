//! 应用外壳(SOURCE_SPEC 第 1 章):顶栏 + 左步骤栏 + 右工作区 + 状态机 + 重置确认。
//!
//! ## 状态架构(给后续页面 agent 的约定)
//!
//! - 根实体为 [`AppShell`](实现 Render),持有全部向导状态:
//!   `current_step` / `max_unlocked_step` / 三个页面结构体 / 待挂载的 Modal 状态;
//! - **页面是普通 struct(非独立 Entity)**,字段直接挂在 [`AppShell`] 上;
//!   页面内部需要高交互控件时,持有对应 `Entity`(如 `Entity<DirPickerState>`、
//!   `Entity<InputState>`),在页面构造函数里 `cx.new` + `cx.subscribe` 建立
//!   事件回路(订阅句柄 `Subscription` 存进 AppShell,随 reset 一起丢弃重建);
//! - `reset_key` 语义:源项目通过 React `key` 强制重挂载三页清空内部状态;
//!   GPUI 版 [`AppShell::reset`] 直接**重建页面结构体**(等价重挂载),并归位
//!   step/max_unlocked/清理 taskId;
//! - "三个页面始终挂载"的源语义(仅 display 切换、保留状态)由"struct 字段
//!   常驻 + render 按 current_step 切换"天然满足。

use gpui::prelude::*;
use gpui::{
    Context, DefiniteLength, Entity, FocusHandle, Pixels, SharedString, Subscription, Window, div,
    px,
};

use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState};
use tag2folders_lib::core::AudioMetadata;
use tag2folders_lib::service;

use crate::ui::components::{
    AlertBar, AlertVariant, BadgeVariant, Button, ButtonSize, ButtonVariant, CardPadding,
    ConfirmModal, ConfirmOptions, ConfirmTone, StatusBadge, StatusBadgeSize, StepNav, badge, card,
    step_nav_aside,
};
use crate::ui::dir_picker::{DirPickerEvent, DirPickerState, render_dir_picker};
use crate::ui::service::run_service_result;
use crate::ui::theme;
use crate::ui::{Icon, icon_16, icon_sized};

// ── 页面(后续页面 agent 在此扩展)──────────────────────────────────────────

/// 筛选字段(SOURCE_SPEC 4.1.5 FILTER_FIELDS;key 与源前端 filterField 值一致)。
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

    /// 源前端字段 key。
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

/// 筛选匹配规则(SPEC 4.1.5):大小写不敏感的子串包含。`kw_lower` 已 trim+lowercase。
fn matches_filter(field: FilterField, kw_lower: &str, f: &AudioMetadata) -> bool {
    field.value_of(f).to_lowercase().contains(kw_lower)
}

/// 表格最多渲染行数(SOURCE_SPEC 4.1.6 TABLE_LIMIT=200;纯 UI 截断,父级数据完整)。
const TABLE_LIMIT: usize = 200;

/// 表格体容器最大高度:源为整页滚动 + sticky 表头,GPUI 版改为容器内滚动 +
/// 固定表头行(见 docs/KNOWN_DIFFERENCES.md)。
const TABLE_BODY_MAX_H: Pixels = px(480.0);

/// 步骤 1:扫描文件(SOURCE_SPEC 4.1)。页面为普通 struct(状态挂在 AppShell 上),
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
    /// 子串匹配(SPEC 4.1.5,与源 filteredFiles 一致)。
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

/// 步骤 2:模板预览(数据结构占位)。页面 agent 参考 SPEC 4.2。
pub struct PreviewPage {
    /// 目标目录选择
    pub dir: Entity<DirPickerState>,
    // TODO(页面 agent):template/targetDir/mode/mappings/directoryTree 等
}

impl PreviewPage {
    fn new(window: &mut Window, cx: &mut Context<AppShell>) -> Self {
        let dir = cx.new(|cx| {
            let mut s = DirPickerState::new("留空则整理到源目录", window, cx);
            s.label = Some("目标目录".into());
            s
        });
        Self { dir }
    }
}

/// 步骤 3:执行整理(占位)。页面 agent 参考 SPEC 4.3(轮询/日志/进度)。
pub struct ProgressPage {
    // TODO(页面 agent):progress/started/log/done/errMsg/taskId(持久化)
}

impl ProgressPage {
    fn new() -> Self {
        Self {}
    }

    /// 是否存在进行中/未完成的整理任务(退出确认用,SPEC 1.5)。
    pub fn has_running_task(&self) -> bool {
        false // TODO(页面 agent):taskId 非空时 true
    }
}

// ── 确认弹窗状态 ─────────────────────────────────────────────────────────────

enum ConfirmAction {
    /// 顶栏"重置"(SPEC 1.4)
    Reset,
    /// 窗口关闭(SPEC 1.5;description/tip 视任务状态两变体)
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
    /// 已解锁的最大步骤 1|2|3(点击规则:只能访问 ≤ 此值,SPEC 1.6/1.7)
    max_unlocked_step: usize,
    /// 重置计数(源 resetKey;重建页面即等价重挂载,计数仅供诊断/动画 key)
    reset_key: u32,

    /// App 级扫描数据(源 App.tsx `scannedFiles`,handleScanComplete 写入;
    /// 预览页 D4 消费:generate_preview 的入参文件列表)
    #[allow(dead_code)] // D4(预览页)接入后移除
    pub scanned_files: Vec<AudioMetadata>,
    /// App 级源目录(源 App.tsx `sourceDir`;注意是提交值:扫描成功 = trim 后
    /// 输入、"下一步"带筛选 = 页面原值、作废/失败 = '')
    #[allow(dead_code)] // D4(预览页)接入后移除
    pub source_dir: String,

    pub scan: ScanPage,
    pub preview: PreviewPage,
    pub progress: ProgressPage,

    /// 扫描竞态 token(SPEC 4.1.8):输入变更/递归切换时 +1,在途响应比对后丢弃。
    /// 放 AppShell(而非 ScanPage)是因为 reset 会重建页面结构体,token 必须
    /// 跨重建单调递增,才能丢弃"重置前发起"的在途扫描(等价源卸载时 token+1)。
    scan_token: u64,

    /// 待挂载的确认弹窗(单例:重置/退出)
    confirm: Option<PendingConfirm>,
    confirm_focus: FocusHandle,
    /// 退出已确认(允许本次关窗)
    exit_confirmed: bool,

    _subs: Vec<Subscription>,
}

impl AppShell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let confirm_focus = cx.focus_handle();

        let mut shell = Self {
            current_step: 1,
            max_unlocked_step: 1,
            reset_key: 0,
            scanned_files: Vec::new(),
            source_dir: String::new(),
            scan: ScanPage::new(window, cx),
            preview: PreviewPage::new(window, cx),
            progress: ProgressPage::new(),
            confirm: None,
            confirm_focus,
            exit_confirmed: false,
            scan_token: 0,
            _subs: Vec::new(),
        };

        // 扫描页/预览页事件回路(见 wire_page_subscriptions)
        shell.wire_page_subscriptions(window, cx);

        shell
    }

    // ── 步骤状态机(SPEC 1.7)───────────────────────────────────────────────

    /// (重)建立页面事件回路。`new` 与 `reset` 都要调用:页面结构体重建后,
    /// 订阅必须指向新实体(订阅句柄随 reset 一起丢弃重建,见 UI_INTEGRATION §2)。
    fn wire_page_subscriptions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._subs.clear();

        // 扫描页:DirPicker 事件(SPEC 4.1.8)
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
        // 预览页(占位):目录变化仅重绘
        let preview_dir = self.preview.dir.clone();
        self._subs.push(cx.subscribe(
            &preview_dir,
            |_this, _entity, ev: &DirPickerEvent, cx| {
                if let DirPickerEvent::Changed(_) = ev {
                    cx.notify();
                }
            },
        ));
    }

    fn go_to_step(&mut self, step: usize, cx: &mut Context<Self>) {
        if step <= self.max_unlocked_step {
            self.current_step = step;
            cx.notify();
        }
    }

    // ── 扫描页逻辑(SOURCE_SPEC 4.1.8)─────────────────────────────────────

    /// 开始扫描:token 竞态防护 + 后台线程执行(重 IO 不阻塞 UI)。
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
        run_service_result(
            cx,
            move || service::scan_directory(work_dir.clone(), Some(recursive)),
            move |this, result, cx| {
                // 在途时输入已变更(或已重置)→ 丢弃本次响应
                if this.scan_token != token {
                    return;
                }
                this.scan.loading = false;
                match result {
                    Ok(resp) => {
                        this.scan.files = resp.files.clone();
                        this.scan.has_scanned = true;
                        this.handle_scan_complete(resp.files, source_dir, cx);
                    }
                    Err(msg) => {
                        // 失败:清旧结果,旧表格与"下一步"不可用;App 级数据同步清空
                        this.scan.error = Some(msg);
                        this.scan.files = Vec::new();
                        this.scan.has_scanned = true;
                        this.handle_scan_complete(Vec::new(), String::new(), cx);
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

    /// 输入变更效应(SPEC 4.1.8 useEffect [sourceDir, recursive]):
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
        self.handle_scan_complete(Vec::new(), String::new(), cx);
    }

    /// App 级 handleScanComplete(SOURCE_SPEC 1.7):写入 scannedFiles/sourceDir、
    /// 解锁状态机推进(有数据 → max_unlocked ≥ 2;无数据锁回 1 并回步骤 1)。
    pub fn handle_scan_complete(
        &mut self,
        files: Vec<AudioMetadata>,
        dir: String,
        cx: &mut Context<Self>,
    ) {
        let has_files = !files.is_empty();
        self.scanned_files = files;
        self.source_dir = dir;
        // TODO(D4 预览页 agent):源 handleScanComplete 同时清 App 级
        // mappings、organizeMode='copy'、targetDir=''(页面本地状态保留不清)
        if has_files {
            self.max_unlocked_step = self.max_unlocked_step.max(2);
        } else {
            self.max_unlocked_step = 1;
            self.current_step = 1;
        }
        cx.notify();
    }

    /// "下一步:设置模板"(SPEC 4.1.7 handleNext):有筛选词时先把筛选子集
    /// 提交为 App 级数据(下游预览只用被筛过的文件),再切到步骤 2。
    fn handle_next(&mut self, cx: &mut Context<Self>) {
        let keyword = self.scan.filter_input.read(cx).value().to_string();
        if !keyword.trim().is_empty() {
            let filtered = self.scan.filtered_files(cx);
            let source_dir = self.scan.dir.read(cx).value(cx);
            self.handle_scan_complete(filtered, source_dir, cx);
        }
        // onNext = setCurrentStep(2),无解锁检查(源行为)
        self.current_step = 2;
        cx.notify();
    }

    /// 全量重置(SPEC 1.4 确认后 / SPEC 4.3.5 "完成并开启新任务"直接调用)。
    /// 语义:回步骤 1、max_unlocked=1、清空页面数据(重建页面结构体)、清 taskId。
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.current_step = 1;
        self.max_unlocked_step = 1;
        self.reset_key += 1;
        // 丢弃"重置前发起"的在途扫描(等价源卸载时 scanTokenRef += 1)
        self.scan_token += 1;
        self.scanned_files.clear();
        self.source_dir.clear();
        // 重建页面 = 源 resetKey 强制重挂载(内部状态与订阅全部丢弃重建)
        self.scan = ScanPage::new(window, cx);
        self.preview = PreviewPage::new(window, cx);
        self.progress = ProgressPage::new();
        self.wire_page_subscriptions(window, cx);
        // TODO(页面 agent):taskId 清除(含持久化)
        cx.notify();
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
        let (description, tip) = if self.progress.has_running_task() {
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
                    cx.quit();
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
                            .text_color(theme::SLATE_800)
                            .shadow(theme::shadow_brand_tile())
                            .child(icon_16(Icon::Tag).size(px(18.0))),
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

    /// 当前页渲染。步骤 1 已实现;步骤 2/3 仍为占位(页面 agent 替换)。
    fn render_page(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        match self.current_step {
            1 => self.render_scan_page(window, cx).into_any_element(),
            2 => {
                let app: &mut gpui::App = cx;
                card()
                    .title("整理配置")
                    .subtitle("设置目标目录与命名模板,点击占位符即可插入")
                    .child(render_dir_picker(&self.preview.dir, window, app))
                    .child(
                        div()
                            .mt(px(14.0))
                            .text_size(px(13.0))
                            .text_color(theme::SLATE_500)
                            .child("预览页占位 —— 由页面 agent 实现(SPEC 4.2)"),
                    )
                    .into_any_element()
            }
            _ => card()
                .title("任务概览")
                .subtitle("整理任务的模式、目标与待处理数量")
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(theme::SLATE_500)
                        .child("进度页占位 —— 由页面 agent 实现(SPEC 4.3)"),
                )
                .into_any_element(),
        }
    }

    // ── 扫描页渲染(SOURCE_SPEC 4.1.1 ~ 4.1.7)──────────────────────────────

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
            // recursive 变化同样触发输入变更效应(SPEC 4.1.8 useEffect 依赖)
            this.on_scan_input_changed(window, cx);
        });
        let on_clear_filter_bar = cx.listener(
            |this, _e: &gpui::ClickEvent, window, cx| this.clear_scan_filter(window, cx),
        );
        let on_clear_filter_empty = cx.listener(
            |this, _e: &gpui::ClickEvent, window, cx| this.clear_scan_filter(window, cx),
        );
        let on_next = cx.listener(
            |this, _e: &gpui::ClickEvent, _window, cx| this.handle_next(cx),
        );

        // 筛选栏(SPEC 4.1.5):前缀文字 + 字段胶囊(单选) + 关键词输入 + 清空
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
                        el.bg(theme::AMBER_500).text_color(theme::SLATE_800)
                    })
                    .when(!selected, |el| {
                        el.bg(theme::SLATE_200).text_color(theme::SLATE_600)
                    })
                    .child(label)
                    .on_click(move |_, window, cx| on_field(&field, window, cx)),
            );
        }
        // 关键词输入:左内嵌 SearchIcon 13 @ left 9、h 32、fontSize 12.5、pl 28
        filter_bar = filter_bar
            .child(
                div()
                    .relative()
                    .flex_grow()
                    .flex_shrink()
                    .flex_basis(px(160.0))
                    .min_w(px(140.0))
                    .child(
                        div()
                            .absolute()
                            .left(px(9.0))
                            .top(px(9.0))
                            .child(
                                icon_sized(Icon::Search, px(13.0)).text_color(theme::SLATE_400),
                            ),
                    )
                    .child(
                        Input::new(&self.scan.filter_input)
                            .h(px(32.0))
                            .pl(px(28.0))
                            .text_size(px(12.5)),
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

        // 配置卡(SPEC 4.1.1)——render_dir_picker 需要 &mut App,最后借用 cx
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
                        // 开始扫描:primary lg + MusicIcon 15;loading 禁用+文案切换
                        Button::new("scan-start")
                            .label(if loading { "正在扫描…" } else { "开始扫描" })
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Lg)
                            .icon(Icon::Music, px(15.0))
                            .loading(loading)
                            .disabled(dir_empty)
                            .on_click(on_scan),
                    ),
            );

        // 页面骨架
        let mut page = div().flex().flex_col().w_full().child(config_card);

        // 错误提示(SPEC 4.1.2)
        if let Some(err) = error.clone() {
            page = page.child(AlertBar::new(AlertVariant::Rose, err).mt(px(12.0)));
        }
        // 空结果提示(SPEC 4.1.3):hasScanned && !loading && !error && 0 文件
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

        // 结果区(SPEC 4.1.4 ~ 4.1.7,files 非空才整体显示)
        if !files_empty {
            // 看板计数行(SPEC 4.1.4)
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

            // 表格区(SPEC 4.1.6):无匹配空态 / 表格 + 截断提示
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
                    .text_color(theme::SLATE_400)
                    .child(icon_sized(Icon::Search, px(26.0)).text_color(theme::SLATE_300))
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
            // 截断提示行(SPEC 4.1.6 表尾)
            if display.len() > TABLE_LIMIT {
                stats_card = stats_card.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(20.0))
                        .py(px(10.0))
                        .text_size(px(12.0))
                        .text_color(theme::SLATE_500)
                        .child(icon_sized(Icon::Info, px(13.0)).text_color(theme::SLATE_400))
                        .child(format!(
                            "仅显示前 {TABLE_LIMIT} 条，共 {} 条。可使用筛选缩小范围。",
                            display.len()
                        )),
                );
            }
            page = page.child(stats_card);

            // 底部导航条(SPEC 4.1.7;源为 sticky bottom,GPUI 无 sticky → 常规流)
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
                            .size(ButtonSize::Lg)
                            .icon(Icon::ArrowRight, px(15.0))
                            .icon_right()
                            .disabled(files_empty)
                            .on_click(on_next),
                    ),
            );
        }

        page.into_any_element()
    }

    /// 清空筛选(SPEC 4.1.5):keyword='' 且 field='filename'。
    fn clear_scan_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scan.filter_field = FilterField::Filename;
        let filter_input = self.scan.filter_input.clone();
        filter_input.update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 根容器:100vh、纵向 flex、bg-app(SPEC 1.2)
        let shell = div()
            .id("app-shell")
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
            // 中段:aside + main(SPEC 1.2)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(step_nav_aside().child({
                        // 步骤点击:仅 ≤ max_unlocked_step 可达(SPEC 1.6)
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
                        // 右工作区:flex 1、内滚、padding clamp(16,2.5vw,32) → 24
                        div()
                            .id("workspace-scroll")
                            .flex()
                            .flex_1()
                            .min_w(px(0.0))
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

        div().child(shell).child(confirm_el)
    }
}

// ── 扫描页渲染辅助 ───────────────────────────────────────────────────────────

/// 看板计数胶囊(SPEC 4.1.4 StatPill):badge 加强版,padding 6px 12px、fontSize 12、
/// gap 7;label opacity 0.75 / weight 500,数值 13.5 / weight 700。
fn stat_pill(variant: BadgeVariant, icon: Icon, label: &str, value: usize) -> gpui::Div {
    badge(variant)
        .gap(px(7.0))
        .px(px(12.0))
        .py(px(6.0))
        .text_size(px(12.0))
        .child(icon_sized(icon, px(13.0)))
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

/// 表格列定义(SPEC 4.1.6):(列名, 宽度百分比)。
const SCAN_TABLE_COLS: [(&str, f32); 5] = [
    ("文件名", 0.30),
    ("艺术家", 0.18),
    ("专辑", 0.20),
    ("标题", 0.22),
    ("状态", 0.10),
];

/// 表头单元格:padding 10px 12px、weight 600、slate-600、bg slate-50(SPEC 2.7)。
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

/// 正文单元格:padding 9px 12px、slate-700、内容 truncate(SPEC 2.7)。
fn scan_text_cell(width: f32, text: &str) -> gpui::Div {
    div()
        .w(DefiniteLength::Fraction(width))
        .px(px(12.0))
        .py(px(9.0))
        .text_size(px(12.5))
        .text_color(theme::SLATE_700)
        .child(div().truncate().child(text.to_string()))
}

/// 文件表格(SPEC 4.1.6):外层水平滚动(表 minWidth 560)、固定表头、
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

    /// 筛选匹配:大小写不敏感子串;filename 字段只匹配 basename(SPEC 4.1.5)
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
}

// ── 窗口关闭确认挂接 ─────────────────────────────────────────────────────────
//
// gpui 0.2.2 存在 `window.on_window_should_close(cx, f)`(返回 false 可阻止关闭),
// 因此源项目的关闭确认弹窗(SPEC 1.5)可以等价实现,不列入 KNOWN_DIFFERENCES。

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

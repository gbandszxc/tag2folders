---
name: Tag2Folders
description: 基于音频元数据标签自动整理文件的桌面工具（Rust + GPUI）
colors:
  # Primary — Workbench Amber（历史生效值，勿按色阶惯例纠正）
  primary: "#f59e0b"
  primary-hover: "#d97706"
  primary-active: "#b45309"
  primary-glow: "#ffc533"
  primary-line: "#ffdc80"
  primary-soft: "#fef3c7"
  primary-softer: "#fffbeb"
  primary-ink: "#7d4600"
  primary-ink-soft: "#b36900"
  primary-ring: "#ffae00"
  on-primary: "#1e293b"
  # Neutral — slate
  ink: "#0f172a"
  ink-body: "#334155"
  ink-soft: "#475569"
  ink-muted: "#64748b"
  ink-faint: "#94a3b8"
  line-default: "#cbd5e1"
  line-subtle: "#e2e8f0"
  surface-muted: "#f1f5f9"
  app-bg: "#f8fafc"
  surface: "#ffffff"
  console-bg: "#020617"
  # Semantic
  success: "#059669"
  success-soft: "#ecfdf5"
  success-line: "#a7f3d0"
  danger: "#e11d48"
  danger-soft: "#fff1f2"
  danger-line: "#fecdd3"
  info: "#0284c7"
  info-soft: "#f0f9ff"
  info-line: "#bae6fd"
typography:
  display:
    fontFamily: "PingFang SC, -apple-system, sans-serif"
    fontSize: "32px"
    fontWeight: 700
    lineHeight: 1.0
  headline:
    fontFamily: "PingFang SC, -apple-system, sans-serif"
    fontSize: "16px"
    fontWeight: 700
    lineHeight: 1.5
  title:
    fontFamily: "PingFang SC, -apple-system, sans-serif"
    fontSize: "15px"
    fontWeight: 600
    lineHeight: 1.5
  body:
    fontFamily: "PingFang SC, -apple-system, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "PingFang SC, -apple-system, sans-serif"
    fontSize: "11.5px"
    fontWeight: 600
    lineHeight: 1.4
  mono:
    fontFamily: "Menlo, ui-monospace, monospace"
    fontSize: "12.5px"
    fontWeight: 400
    lineHeight: 1.5
rounded:
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "12px"
  xl: "16px"
  full: "9999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "20px"
  xxl: "24px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    rounded: "{rounded.md}"
    padding: "8px 14px"
    typography: "{typography.body}"
  button-primary-hover:
    backgroundColor: "{colors.primary-hover}"
    textColor: "{colors.ink}"
  button-secondary:
    backgroundColor: "{colors.surface-muted}"
    textColor: "{colors.ink-body}"
    rounded: "{rounded.md}"
    padding: "8px 14px"
  button-outline:
    backgroundColor: "transparent"
    textColor: "{colors.ink-body}"
    rounded: "{rounded.md}"
    padding: "8px 14px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.ink-soft}"
    rounded: "{rounded.md}"
    padding: "8px 14px"
  badge-amber:
    backgroundColor: "{colors.primary-soft}"
    textColor: "{colors.primary-ink}"
    rounded: "{rounded.full}"
    padding: "2px 8px"
  card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    padding: "18px 22px"
  input:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.md}"
    padding: "8px 12px"
    height: "38px"
  segment-active:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.primary-ink-soft}"
    rounded: "{rounded.sm}"
    padding: "6px 14px"
  alert-amber:
    backgroundColor: "{colors.primary-softer}"
    textColor: "{colors.primary-ink-soft}"
    rounded: "{rounded.md}"
    padding: "10px 14px"
---

# Design System: Tag2Folders

> 权威实现来源：`src/ui/theme.rs`（token 全表）与 `src/app.rs` / `src/ui/components/*`（组件）。本文是它们的可读快照，冲突时以代码为准并回来同步。token 全部经 `theme::*` 常量消费，**禁止手写 hex**。

## Overview

**Creative North Star: "The Amber Workbench"**

一块冷静、干净、井然的整理工作台：slate 灰承担全部结构性底色，琥珀（amber）是台面上唯一的暖光，只照在品牌、激活态、主动作和进度上。氛围是"温和可靠"——不是极客控制台，也不是消费级玩具，而是一位让人放心把文件交给它的助手：颜色几乎只花在状态与动作上，装饰极度节省。

信息密度中等偏高：表格、日志控制台、目录树都为"批量文件操作"的效率服务，行高与内边距紧凑（表格行 9×12、树行 5×12）。运行时（gpui）没有 CSS 过渡，所有状态切换都是瞬时的——系统因此更依赖"色块 + 字重"的清晰表达，而不是动效叙事；全局仅保留 loading 旋转与进行中脉冲圆点两个动效。

**Key Characteristics:**

- 冷静 slate 打底；琥珀只出现在品牌 / 激活 / 主动作 / 进度 / 警告
- 卡片化信息分区 + 左侧 230px 步骤栏构成向导骨架
- 状态即颜色：emerald=成功、rose=错误、sky=信息、amber=警告（与品牌共用）
- 等宽字体承载一切"文件系统真相"（路径 / 模板 / 日志）
- 无过渡依赖：瞬时态切换 + 两个功能性动效（spin / pulse）

## Colors

一句话：一张 slate 灰的工作台上，一盏琥珀灯——语义色只报状态，不抢品牌。

### Primary

- **Workbench Amber** (#f59e0b)：品牌主色。主按钮底、激活步骤瓦片、进度条填充、选中筛选胶囊、目录树文件夹图标、进度百分比大字（后者用 amber-800）。
- **Amber Deep**（hover #d97706 / active #b45309）：主按钮的悬浮与按下梯度。
- **Amber Soft**（#fef3c7 底 + #7d4600 字 + #ffdc80 边）：amber 徽章与选中分段控件的浅色三件套。
- **Amber Wash** (#fffbeb)：警告横幅与"当前处理"条的低饱和底。
- **Focus Ring** (#ffae00)：输入框聚焦边框（硬编码历史值，非 amber-500）。

### Neutral

- **Ink**（#0f172a 主文字 / #334155 正文 / #475569 ghost 文字 / #64748b 次要 / #94a3b8 占位与图标）：五级文字灰阶。
- **Paper** (#ffffff)：卡片、面板、弹窗的唯一表面色。
- **Workbench Grey** (#f8fafc)：应用底色（= slate-50），与 Paper 形成"台面 vs 卡片"两级。
- **Muted Grey** (#f1f5f9)：表头底、分段控件轨道、次级 chip 底。
- **Line**（subtle #e2e8f0 / default #cbd5e1）：两级 1px 边框；输入框用 default，卡片分隔用 subtle。
- **Console Black** (#020617)：日志控制台专用深底，全应用唯一深色表面。

### Tertiary

- **Signal Green**（#059669 / soft #ecfdf5 / line #a7f3d0）：成功横幅、可读取、emerald 徽章。
- **Signal Red**（#e11d48 / soft #fff1f2 / line #fecdd3）：错误条、失败横幅、不可读取、danger 按钮。
- **Signal Blue**（#0284c7 / soft #f0f9ff / line #bae6fd）：信息提示条、缺失信息徽章；日志正文用 #bae6fd。

### Named Rules

**The One Warm Accent Rule.** 琥珀是唯一的暖色声音。新 UI 不得引入第二暖色系；状态表达只能从 emerald / rose / sky 三组语义色取用。

**The Don't-Retune Rule.** amber 系数值是历史定下的生效值（如 amber-500 = #f59e0b、amber-400 = #ffc533），与常见 Tailwind 色阶不同。禁止按色阶惯例"纠正"——全 UI 配色按现值调过（theme.rs 有同款警告）。

## Typography

**Display/Body Font:** PingFang SC（macOS 系统中文主力，运行时已验证可解析）
**Mono Font:** Menlo（路径 / 模板 / 日志 / 目录树文件名）

**Character:** 单一字族 + 单一等宽的极简配对；层级完全靠字号与字重（400/500/600/700 四档）表达，不靠换字体。

### Hierarchy

- **Display**（700, 32px, lh 1.0）：进度百分比大字，全应用唯一 display 用法。
- **Headline**（700, 16px, lh 1.5）：完成/失败横幅标题、模态标题。
- **Title**（600, 15–15.5px, lh 1.5）：卡片标题、品牌标题。
- **Body**（400–500, 12.5–14px, lh 1.5）：正文与控件文字；基准 14px / 1.5。
- **Label**（500–600, 11–12.5px, lh 1.4）：徽章、字段标签、表格表头（表头 12.5/600）。
- **Mono**（400–600, 12–13px, lh 1.5–1.8）：路径 13、树文件名 12.5、日志 12/1.8；模板输入 13。

### Named Rules

**The Mono-Truth Rule.** 凡是文件系统的内容（路径、文件名、命名模板、日志行）一律 Menlo；界面语言一律 PingFang SC。两族不混用于同一内容。

## Layout

单窗口 1100×750（最小 900×600），不可再分的向导骨架：

- **顶栏** 58px：品牌区（34×34 amber 圆角方块图标 + 标题/副标题）｜版本徽章 + 重置。
- **左步骤栏** 230px 固定：三步骤 + 连接线，白底右边框。
- **工作区** 弹性滚动：padding 24，内容 max-width 1080 居中；卡片纵向堆叠，区块间距 16px。
- **底部导航条**：两端对齐（统计/返回在左，主动作在右），白底圆角 12 + 上抛阴影。

间距节奏 4 / 8 / 12 / 16 / 20 / 24（4px 基数）；弹窗宽 460（确认）/ 520（通用）/ 560（目录浏览），遮罩 rgba(15,23,42,0.55)。

## Elevation & Depth

环境光式（ambient）：**静止表面零阴影**，深度只作为"状态提示"出现——悬浮、弹层、激活、贴底导航。全部阴影定义在 `theme.rs` 的 shadow 函数里，色基 rgba(15,23,42,x)。

### Shadow Vocabulary

- **shadow-xs**（0 1px 2px rgba(15,23,42,0.05)）：卡片、激活分段按钮——最常用。
- **主按钮**：常态 0 1px 2px rgba(0,0,0,0.05)，hover 0 2px 4px rgba(0,0,0,0.08)。
- **品牌方块**：0 1px 3px rgba(0,0,0,0.1)；**激活步骤瓦片**：0 1px 4px rgba(217,133,0,0.25)（带琥珀色温的光）。
- **确认弹窗**：0 12px 36px rgba(15,23,42,0.16)；**通用模态**：shadow-xl。
- **底部导航条**：0 -6px 16px rgba(15,23,42,0.05)（负 y 上抛）。

### Named Rules

**The Flat-At-Rest Rule.** 静止即平面。新增表面不得自带阴影；阴影只随悬浮/弹层/激活态出现。

## Shapes

圆角六级：4（树文件行）/ 6（sm 按钮、chip、树目录行）/ 8（按钮、输入框、提示条、日志控制台）/ 12（卡片、横幅、导航条）/ 16（模态）/ full（徽章、胶囊、进度条、圆点）。边框一律 1px（subtle/default 两级）。无描边渐变、无裁切异形；"圆形"只出现在徽章胶囊与 7px 脉冲圆点。

## Components

每个组件先给一句"性格"，再给关键规格；完整行为规格见 docs/SPEC.md §9。

### Buttons

克制而清晰：色块即语义，按下即确认。
- **Shape:** 圆角 8px（sm 6px）；尺寸 sm 5×10/12px、md 8×14/13px、lg 10×20/14px。
- **Primary:** amber-500 底 + slate-800 字（#1e293b，非纯白）+ 600 字重；hover amber-600、active amber-700。
- **Secondary / Outline / Ghost / Danger:** slate-100 底 / 透明底灰边 / 纯透明灰字 / rose-50 底 rose-600 字（hover 反白）。
- **States:** disabled = opacity 0.55 + 禁悬浮；loading = 禁用 + 前置旋转图标（1s 线性）；图标可左可右（icon_right）。

### Badges & StatusBadge

信息的"标签纸"：小、圆、三色套件（soft 底 + 深字 + line 边）。
- **Shape:** full 胶囊，padding 2×8，11px/600；加强版（版本/模式徽章）4×10、11.5–12px。
- **StatusBadge:** 七状态映射——正常(emerald)/磁盘冲突·批内冲突(amber)/缺失信息(sky)/不可读(slate)/路径越界·写入受阻(rose)；图标 12–13px；未知状态原样显示。

### Cards / Containers

台面上的"托盘"：白底、12px 圆角、1px subtle 边、shadow-xs、overflow hidden。
- 头部（标题 15/600 + 副标题 12/slate-500 + 右侧动作区）padding 14×20 + 下边框；主体 padding 档位 none/sm/md/lg = 0 / 12×16 / 18×22 / 24×28。

### Inputs / Fields

文件系统的入口用 mono：路径与模板输入 Menlo 13px、高 38、8px 圆角、1px slate-300 边。
- **Focus:** 边框 amber-500（组件库 ring）；原生 3px 光晕未复刻（历史差异）。
- **内嵌图标:** 左侧 Folder/Search 图标（absolute，left 8–10）；有值时右侧清空按钮。
- **Disabled:** slate-100 底 + slate-400 字。

### Segment Control（分段胶囊）

单选的安静表达：slate-100 轨道（padding 3）+ 白底激活片（600 字重 amber-800 字 + shadow-xs + 6px 圆角）；用于操作模式 toggle 与结果区 Tabs；激活 Tab 可带计数徽章（amber-200 底 amber-900 字）。

### Alert Bars

没有 toast，一切就地说话：rose（错误，可 pre-wrap 多行）/ amber（警告，如"移动模式不可逆"）/ sky（信息，如空扫描结果）；图标 15px + 文字 12.5px + 8px 圆角。

### Step Nav（签名组件）

向导的骨架与进度本身：38×38 图标瓦片四态——done（emerald-50 底 emerald-600 Check 图标）/ active（amber-500 底 + 琥珀光晕阴影 + 700 字重）/ dimmed（opacity 0.5）/ 默认（slate-100 底）；步骤间 2px 连接线随解锁从 slate-100 变 amber-400；激活项右侧 7px 脉冲圆点（2s，opacity 1↔0.6）。

### Directory Tree

档案柜抽屉：目录行（chevron + Folder/FolderOpen amber-500 图标 + 名称 13/600 + `(N)` 计数）缩进 depth×20+6；文件行 mono 12.5 + FileAudio amber-600 图标缩进 (depth+1)×20+8；默认展开 depth<2；头部带过滤输入与"全部折叠/展开"。

## Do's and Don'ts

### Do:

- **Do** 一律用 `theme::*` 常量取色/圆角/阴影；新间距落在 4/8/12/16/20/24 节奏上。
- **Do** 状态语义固定：emerald=成功、rose=错误、sky=信息、amber=警告与品牌。
- **Do** 新图标走 `assets/icons/`（24×24、stroke=currentColor、fill=none）并在 `icon.rs`/`assets.rs` 登记；着色用 `.text_color()`（SVG alpha 遮罩机制）。
- **Do** 文件系统内容（路径/文件名/模板/日志）一律 mono。
- **Do** 改 token 时同步 theme.rs → 本文档 frontmatter。

### Don't:

- **Don't** 按色阶惯例"修正" amber 值（The Don't-Retune Rule）；也 不要引入第二暖色系。
- **Don't** 引入 toast / 全局通知——反馈一律内联 AlertBar。
- **Don't** 给冲突行/警告行加特殊底色——冲突用徽章表达，行保持中性。
- **Don't** 为装饰加阴影或动效（The Flat-At-Rest Rule；无过渡是运行时事实，不要伪造）。
- **Don't** 手写 hex 或绕过 theme 常量造新灰色。

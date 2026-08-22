# Tag2Folders 源项目 UI 规格文档（SOURCE_SPEC）

> 本文是对源项目 `/Users/zxc/ProjectSpace/gitcode/tag2folders`（Tauri 2 + React 18 桌面应用）的**逐行逆向规格**，作为 GPUI 重写的唯一 UI 依据。目标：**样式与功能均与源应用完全一致**。
> 所有中文文案均为源码原样抄录；所有颜色/尺寸/间距均为源码中的字面值。
> 源码版本：`2.0.1`（frontend/package.json 与 tauri.conf.json 一致）。

**极其重要的色彩陷阱（必读）**：源 `index.css` 中 `--amber-*` 变量被**声明了两次**，第二次（"Warning: Amber" 语义块，位于文件后部）覆盖了第一次的部分值。CSS 后声明生效，因此**运行时的有效值**为：

| 变量 | 首次声明 | **有效值（第二次覆盖后）** |
|---|---|---|
| `--amber-50` | #fffdf5 | **#fffbeb** |
| `--amber-100` | #fff8e6 | **#fef3c7** |
| `--amber-200` | #ffedba | **#fde68a** |
| `--amber-300` | #ffdc80 | #ffdc80（未覆盖） |
| `--amber-400` | #ffc533 | #ffc533（未覆盖） |
| `--amber-500` | #ffae00 | **#f59e0b**（品牌主色实际被覆盖！） |
| `--amber-600` | #f59e00 | **#d97706** |
| `--amber-700` | #d98500 | **#b45309** |
| `--amber-800` | #b36900 | #b36900（未覆盖） |
| `--amber-900` | #7d4600 | #7d4600（未覆盖） |

即：凡源码写 `var(--amber-500)` 之处，实际渲染为 `#f59e0b` 而非 DESIGN.md 宣称的 `#FFAE00`。GPUI 重写必须使用**有效值**（本表右列）才能与源应用像素一致。硬编码的 `#ffae00` 仅出现在 `--border-focus: #ffae00` 和输入框聚焦光晕 `rgba(255, 174, 0, 0.2)` 中。另外 `--indigo-*` 别名未被覆盖，全部保持首次值（`--indigo-500/600 = #ffae00`），但页面组件实际只用 `--amber-*`。`--amber-950` **不存在**（见 7.9 未定义变量陷阱）。

---

## 1. 应用外壳

### 1.1 窗口（tauri.conf.json）

- `productName`: `Tag2Folders`；`identifier`: `com.gbandszxc.tag2folders`；`version`: `2.0.1`
- 单窗口：`title: "Tag2Folders"`，`width: 1100`，`height: 750`，`minWidth: 900`，`minHeight: 600`，`resizable: true`，`fullscreen: false`
- CSP: `null`；已启用插件：`tauri_plugin_dialog`（原生目录选择对话框）；capabilities: `core:default`, `core:window:default`, `dialog:default`

### 1.2 根布局（App.tsx）

- 根容器：`height: 100vh; display: flex; flex-direction: column; background: var(--bg-app)`（#f8fafc）
- 结构：`header`（顶栏）→ `div[flex:1, display:flex, minHeight:0]`（`aside` 侧栏 + `main` 工作区）
- `#root` 为 `height:100%; display:flex; flex-direction:column`（index.css）；`html { font-size: 14px; line-height: 1.5; font-family: var(--font-sans); color: var(--text-primary) }`；`body { user-select: none; overflow-x: hidden }`（仅 `input/textarea/[contenteditable]/.selectable/code/pre/.mono-text` 允许选中文本）
- 全局滚动条：宽高 6px；thumb `var(--slate-300)`(#cbd5e1) 圆角 full，hover `var(--slate-400)`(#94a3b8)；track 透明
- 全局焦点环：`:focus-visible { outline: 2px solid var(--amber-500)(#f59e0b); outline-offset: 1px }`

### 1.3 顶栏 header

- 高度 `58px`，`flexShrink: 0`，`display:flex; align-items:center; justify-content:space-between; gap:16; padding: 0 clamp(16px, 3vw, 32px)`
- 背景 `var(--bg-surface)`(#ffffff)，下边框 `1px solid var(--border-subtle)`(#e2e8f0)，`zIndex: 10`
- **品牌区**（左侧，gap 12）：
  - 图标方块：34×34px，圆角 10px，背景 `var(--amber-500)`(#f59e0b)，前景色 `#1e293b`，阴影 `0 1px 3px rgba(0,0,0,0.1)`，内含 `TagIcon size=18`
  - 标题：`Tag2Folders`，fontSize 15.5，fontWeight 700，color `var(--slate-900)`(#0f172a)，letterSpacing -0.01em
  - 副标题：`音频文件智能整理 · 扫描 → 预览 → 执行`，fontSize 11.5，color `var(--slate-500)`(#64748b)，超长省略（nowrap+ellipsis）
- **右侧操作区**（gap 10，flexShrink 0）：
  - 版本徽章：`<span class="badge badge-amber" style="padding:4px 10px; font-size:11.5">v2.0.1</span>`。badge-amber 有效样式：背景 `var(--amber-100)`(#fef3c7)，文字 `var(--amber-900)`(#7d4600)，边框 1px `var(--amber-300)`(#ffdc80)，fontWeight 600，圆角 full
  - 重置按钮：`<Button variant="ghost" size="sm" icon={<RefreshIcon size={14}/>} title="清空所有数据并重新开始">重置</Button>`

### 1.4 重置按钮的确认弹窗（App.tsx handleReset(true)）

唯一调用 `useConfirm` 的两个位置之一。弹窗参数（ConfirmModal 逐字）：

- `title: '确认重置全部数据？'`
- `message: '确定要清空当前的扫描结果、整理模板配置并重新开始吗？'`
- `tip: '若当前有正在后台执行的文件整理任务，重置将断开界面追踪。'`
- `confirmText: '确认重置'`，`cancelText: '取消'`，`tone: 'warning'`
- 确认后执行：currentStep=1、maxUnlockedStep=1、sourceDir=''、scannedFiles=[]、mappings=[]、organizeMode='copy'、targetDir=''、taskId 清除（含 localStorage）、`resetKey+1`（三个页面以 `key` 重挂载，内部状态全清）
- 取消则不做任何事

### 1.5 窗口关闭确认（App.tsx onCloseRequested，仅 Tauri 环境）

- 注册 `getCurrentWindow().onCloseRequested`，`event.preventDefault()` 阻止直接关闭；`isClosingRef` 防重入
- `hasRunningTask = Boolean(taskIdRef.current)`（存在进行中/未完成整理任务）
- 弹窗参数：
  - `title: '确认退出应用？'`，`message: '确定要退出 Tag2Folders 吗？'`
  - 有任务时 `description: '当前有正在进行或未完成的文件整理任务，退出将中断处理。'`，`tip: '建议等待任务整理完成后再退出应用。'`
  - 无任务时 `description: '退出后当前未保存的配置与扫描缓存将被清除。'`，`tip: undefined`
  - `confirmText: '确认退出'`，`cancelText: '取消'`，`tone: 'warning'`
- 确认 → `exitApp()`（invoke `exit_app`；失败则 `getCurrentWindow().destroy()`）

### 1.6 左侧步骤向导栏（aside）

- 宽度 `clamp(210px, 22vw, 250px)`，flexShrink 0，背景 `var(--bg-surface)`(#ffffff)，右边框 1px `var(--border-subtle)`(#e2e8f0)，`padding: 20px 14px`，纵向 flex，`overflowY: auto`
- 内部列容器 `gap: 2px`
- 三个步骤（`STEPS` 常量，顺序固定）：

| num | label | desc（副标题） | icon |
|---|---|---|---|
| 1 | `扫描文件` | `选择源目录与提取标签` | MusicIcon |
| 2 | `模板预览` | `规划命名与结构方案` | EyeIcon |
| 3 | `执行整理` | `批量安全归档与监控` | PlayIcon |

- 步骤间连接线（每两个步骤之间，非最后一个后）：`marginLeft: 30, height: 24, width: 2, marginTop: 2, marginBottom: 2`，颜色：`step.num < maxUnlockedStep` ? `var(--amber-400)`(#ffc533) : `var(--slate-100)`(#f1f5f9)

**StepItem 状态机**（`current`=当前步骤，`max`=maxUnlockedStep，均 1|2|3）：

- `done = step.num < current`（已完成）
- `active = step.num === current`（进行中）
- `unlocked = step.num <= max`（已解锁）
- `dimmed = !unlocked`（未解锁，整项 opacity 0.5）

**38×38 图标瓦片（tile，圆角 10px）分态样式**：

| 状态 | 背景 | 图标色 | 其他 |
|---|---|---|---|
| done | `var(--emerald-50)`#ecfdf5 | `var(--emerald-600)`#059669 | 边框 1px `var(--emerald-200)`#a7f3d0；图标换成 `CheckIcon size=18` |
| active | `var(--amber-500)`#f59e0b | `#1e293b` | fontWeight 700；阴影 `0 1px 4px rgba(217,133,0,0.25)`；显示原步骤图标 |
| dimmed | `var(--slate-50)`#f8fafc | `var(--slate-300)`#cbd5e1 | — |
| 默认（未激活已解锁，如"下一步"） | `var(--slate-100)`#f1f5f9 | `var(--slate-500)`#64748b | — |

- 瓦片过渡：`transition: all var(--transition-fast)`(150ms)
- 步骤项整行：`role="button"`，`tabIndex = unlocked ? 0 : -1`，active 时 `aria-current="step"`；`padding: 9px 12px`，圆角 `var(--radius-lg)`(12px)，`gap: 12`；hover（仅 unlocked && !active）背景 `var(--slate-50)`；`transition: background 150ms, opacity 150ms`
- 标题行：fontSize 13，fontWeight active?700:600，颜色 dimmed?`var(--slate-400)`:active?`var(--amber-900)`(#7d4600):`var(--slate-800)`(#1e293b)
- 副标题：fontSize 11，颜色 dimmed?`var(--slate-400)`:`var(--slate-500)`，marginTop 2，单行省略
- **右侧状态徽标**（flexShrink 0）四选一：
  - active：8×8 圆点，背景 `var(--amber-500)`(#f59e0b)，class `animate-pulse`（透明度 1↔0.6，2s 循环）
  - done：文字 `已完成`，fontSize 11，fontWeight 600，color `var(--emerald-600)`(#059669)
  - unlocked 非 active：`ChevronRightIcon size=15 color=var(--slate-400)`
  - 未解锁：文字 `未解锁`，fontSize 11，color `var(--slate-400)`

**点击/键盘规则**：仅 unlocked 可点（cursor: pointer；未解锁 cursor: default）；点击 → `setCurrentStep(num)`；键盘 Enter 或 Space（仅 unlocked）→ 同点击（preventDefault）。

### 1.7 步骤解锁状态机（App.tsx）

- 初始：`currentStep=1`，`maxUnlockedStep=1`
- `handleScanComplete(files, dir)`（ScanPage 扫描完成回调）：`files.length > 0` → `maxUnlockedStep = max(prev, 2)`；否则 `maxUnlockedStep=1` 且 `currentStep=1`。同时写入 scannedFiles/sourceDir，并清空 mappings、organizeMode='copy'、targetDir=''
- PreviewPage `onNext`（点击"开始执行整理"）→ `maxUnlockedStep=3; currentStep=3`，并经 `onOrganize` 保存 mappings/mode/targetDir、清空 taskId
- `handleReset`（见 1.4）→ 全部归位

### 1.8 右侧工作区（main）

- `flex: 1, minWidth: 0, overflowY: auto, padding: clamp(16px, 2.5vw, 32px)`
- 内容容器：`maxWidth: 1080; margin: 0 auto`（水平居中）
- **三个页面始终挂载**，仅切换 `display: block/none`（保留各页内部状态）；外层 div 的 `key` 含 `resetKey`（`scan-${n}` / `preview-${n}` / `progress-${n}`），重置时全部重新挂载
- 进入某页时播放动画：`scaleUp 220ms cubic-bezier(0.16, 1, 0.3, 1)`（从 `opacity:0; scale(0.97) translateY(4px)` 到正常）
- 任务 ID 持久化：`localStorage['tag2folders_task_id']`（set 时写入，清空时 removeItem；初始 state 从 localStorage 读取）

---

## 2. 通用组件清单（frontend/src/components/CommonUI.tsx）

### 2.1 图标系统（Icon）

- **全部为内联 SVG，非 emoji/字符**。统一：`viewBox="0 0 24 24"`、`fill="none"`、`stroke=color`（默认 `currentColor`）、`stroke-width="2"`、`stroke-linecap="round"`、`stroke-linejoin="round"`、默认尺寸 16px、`flex-shrink: 0`
- 共 32 个导出图标。GPUI 重写需按下表路径数据绘制（`d` 属性原文）：

| 图标名 | 形状 | 用途 | path 数据 |
|---|---|---|---|
| FolderIcon | 关闭的文件夹 | DirPicker 输入框左侧/弹窗列表/目录树 | `M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z` |
| FolderOpenIcon | 打开的文件夹 | 浏览按钮、弹窗标题、目录树展开态 | `m6 14 1.45-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.55 6a2 2 0 0 1-1.94 1.5H4a2 2 0 0 1-2-2V5c0-1.1.9-2 2-2h3.93a2 2 0 0 1 1.66.9l.82 1.2a2 2 0 0 0 1.66.9H18a2 2 0 0 1 2 2v2` |
| MusicIcon | 音符（双符头） | 步骤1瓦片、扫描按钮、可读取徽章 | path `M9 18V5l12-2v13` + circle(6,18,r3) + circle(18,16,r3) |
| PlayIcon | 播放三角 | 步骤3瓦片、开始执行按钮 | polygon `6 3 20 12 6 21 6 3` |
| RefreshIcon | 双向循环箭头 | 重置按钮、loading 旋转 | 4 段：`M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8`；`M21 3v5h-5`；`M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16`；`M8 16H3v5` |
| ArrowRightIcon | 右箭头 | 下一步按钮、移动模式图标 | `M5 12h14` + `m12 5 7 7-7 7` |
| ArrowLeftIcon | 左箭头 | 返回扫描按钮 | `M19 12H5` + `m12 19-7-7 7-7` |
| ArrowUpIcon | 上箭头 | 目录弹窗"上一级" | `m5 12 7-7 7 7` + `M12 19V5` |
| CheckIcon | 对勾 | 步骤完成瓦片 | polyline `20 6 9 17 4 12` |
| CheckCircleIcon | 圆+对勾 | ok 状态徽章、成功横幅 | `M22 11.08V12a10 10 0 1 1-5.93-9.14` + polyline `22 4 12 14.01 9 11.01` |
| AlertTriangleIcon | 三角+感叹号 | 冲突/警告 | `m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z` + line(12,9→12,13) + line(12,17→12.01,17) |
| AlertCircleIcon | 圆+感叹号 | 错误/rose 状态 | circle(12,12,r10) + line(12,8→12,12) + line(12,16→12.01,16) |
| XCircleIcon | 圆+叉 | unreadable 状态 | circle(12,12,r10) + `m15 9-6 6` + `m9 9 6 6` |
| XIcon | 叉号 | 清空/关闭按钮 | `M18 6 6 18` + `m6 6 12 12` |
| CopyIcon | 双叠矩形 | 复制模式 | rect(8,8,14,14,rx2) + `M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2` |
| SearchIcon | 放大镜 | 筛选/过滤输入 | circle(11,11,r8) + `m21 21-4.3-4.3` |
| TagIcon | 标签+圆点 | 品牌图标、占位符芯片 | `M12 2H2v10l9.29 9.29c.94.94 2.48.94 3.42 0l6.58-6.58c.94-.94.94-2.48 0-3.42L12 2Z` + `M7 7h.01` |
| EyeIcon | 眼睛 | 步骤2瓦片、生成预览按钮 | `M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z` + circle(12,12,r3) |
| LayersIcon | 三层叠片 | 目录树标题、目录树 Tab | polygon `12 2 2 7 12 12 22 7 12 2` + polyline `2 17 12 22 22 17` + polyline `2 12 12 17 22 12` |
| SettingsIcon | 齿轮 | （已导出但页面未使用） | 长 path（齿轮轮廓）+ circle(12,12,r3) |
| ChevronRightIcon | 右尖角 | 步骤导航、树折叠箭头 | `m9 18 6-6-6-6` |
| ChevronDownIcon | 下尖角 | 树展开箭头 | `m6 9 6 6 6-6` |
| SparklesIcon | 四芒星光 | "完成并开启新任务"按钮 | `m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z` + `M5 3v4` + `M19 17v4` + `M3 5h4` + `M17 19h4` |
| FileIcon | 文档 | （已导出但页面未使用） | `M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z` + `M14 2v4a2 2 0 0 0 2 2h4` |
| FileAudioIcon | 文档+波形 | 文件数徽章、当前文件、映射 Tab | `M17.5 22h.5a2 2 0 0 0 2-2V7l-5-5H6a2 2 0 0 0-2 2v3` + `M14 2v4a2 2 0 0 0 2 2h4` + `M2 17a2 2 0 0 0 2-2v4a2 2 0 0 0-2-2Z` + `M5 14v6` + `M8 11v12` + `M11 13v8` |
| HomeIcon | 房屋 | 目录弹窗"根目录" | `m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z` + polyline `9 22 9 12 15 12 15 22` |
| LockIcon | 挂锁 | （已导出但页面未使用） | rect(3,11,18,11,rx2) + `M7 11V7a5 5 0 0 1 10 0v4` |
| TerminalIcon | 终端提示符 | （已导出但页面未使用） | polyline `4 17 10 11 4 5` + line(12,19→20,19) |
| ExternalLinkIcon | 外链 | （已导出但页面未使用） | `M15 3h6v6` + `M10 14 21 3` + `M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6` |
| TrashIcon | 垃圾桶 | （已导出但页面未使用） | `M3 6h18` + `M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6` + `M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2` |
| FilterIcon | 漏斗 | （已导出但页面未使用） | polygon `22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3` |
| InfoIcon | 圆+i | info 状态徽章、提示 | circle(12,12,r10) + `M12 16v-4` + `M12 8h.01` |

### 2.2 Button

- Props：`variant: 'primary'|'secondary'|'outline'|'ghost'|'danger'|'success'`（默认 `'secondary'`）；`size: 'sm'|'md'|'lg'`（默认 md）；`icon`（ReactNode，左侧）；`iconPosition: 'left'|'right'`（默认 left）；`loading`；原生 button 属性
- **变体怪癖**：`variant='success'` 实际套用 CSS 类 `badge-emerald`（非 btn- 类），效果为浅绿胶囊底。源码中无调用方使用 success 变体
- 结构：`[loading 时: 旋转 RefreshIcon(sm 尺寸 12 / 其他 14, marginRight 4 当有文字)] [非 loading 且 iconPosition=left: icon] children [非 loading 且 iconPosition=right: icon]`
- `loading=true` 时：禁用 + 前置旋转图标（`animate-spin`：1s 线性无限旋转）
- 基类 `.btn`：inline-flex、居中、gap 6、padding `8px 14px`、fontSize 13、fontWeight 500、圆角 `var(--radius-md)`(8px)、1px 透明边框、line-height 1.25、nowrap、`transition: all 150ms`、cursor pointer
- 尺寸：`.btn-sm` padding `5px 10px`、fontSize 12、圆角 6px；`.btn-lg` padding `10px 20px`、fontSize 14、fontWeight 600、圆角 12px；（`.btn-icon` padding 8 正方形，未使用）
- **禁用态**：`opacity: 0.55; cursor: not-allowed; pointer-events: none`（hover 样式全部经 `:not(:disabled)` 限定）

| 变体 | 常态 | 悬浮 | 按下 |
|---|---|---|---|
| primary | bg `var(--amber-500)`#f59e0b；文字 `#1e293b`；weight 600；边框 `var(--amber-600)`#d97706；阴影 `0 1px 2px rgba(0,0,0,0.05)` | bg `var(--amber-600)`#d97706；文字 `#0f172a`；边框 `var(--amber-700)`#b45309；阴影 `0 2px 4px rgba(0,0,0,0.08)` | bg `var(--amber-700)`#b45309；文字 `#ffffff`；边框 `var(--amber-800)`#b36900；`translateY(1px)` |
| secondary | bg `var(--slate-100)`#f1f5f9；文字 `var(--slate-700)`#334155；边框 `var(--slate-200)`#e2e8f0 | bg #e2e8f0；文字 `var(--slate-900)`#0f172a；边框 `var(--slate-300)`#cbd5e1 | 无特殊 |
| outline | 透明底；文字 `var(--slate-700)`；边框 `var(--border-default)`#cbd5e1 | bg `var(--slate-50)`#f8fafc；文字 `var(--slate-900)`；边框 `var(--slate-400)`#94a3b8 | 无特殊 |
| ghost | 透明底；文字 `var(--slate-600)`#475569；透明边框 | bg `var(--slate-100)`；文字 `var(--slate-900)` | 无特殊 |
| danger | bg `var(--rose-50)`#fff1f2；文字 `var(--rose-600)`#e11d48；边框 `var(--rose-200)`#fecdd3 | bg #e11d48；文字 `#ffffff`；边框 #e11d48 | 无特殊 |

### 2.3 Badge（CSS 类，非组件）

`.badge`：inline-flex、居中、gap 4、padding `2px 8px`、fontSize 11、fontWeight 600、圆角 9999px、line-height 1.4、nowrap。

| 类 | 背景 | 文字 | 边框 |
|---|---|---|---|
| badge-emerald | `var(--emerald-50)`#ecfdf5 | `var(--emerald-700)`#047857 | `var(--emerald-200)`#a7f3d0 |
| badge-amber / badge-indigo（同规则） | `var(--amber-100)`#fef3c7 | `var(--amber-900)`#7d4600 | `var(--amber-300)`#ffdc80（weight 600） |
| badge-rose | `var(--rose-50)`#fff1f2 | `var(--rose-700)`#be123c | `var(--rose-200)`#fecdd3 |
| badge-sky | `var(--sky-50)`#f0f9ff | `var(--sky-700)`#0369a1 | `var(--sky-200)`#bae6fd |
| badge-slate | `var(--slate-100)`#f1f5f9 | `var(--slate-700)`#334155 | `var(--slate-200)`#e2e8f0 |

### 2.4 StatusBadge（状态徽章组件）

Props：`status`（字符串，七种合法值或任意未知）、`size: 'sm'|'md'`（默认 md）、`showIcon`（默认 true）。

`STATUS_CONFIG` 映射（label 原文）：

| status | label | 颜色变体 | 图标 |
|---|---|---|---|
| `ok` | `正常` | emerald | CheckCircleIcon |
| `conflict` | `磁盘冲突` | amber | AlertTriangleIcon |
| `batch_conflict` | `批内冲突` | amber | AlertTriangleIcon |
| `missing_metadata` | `缺失信息` | sky | InfoIcon |
| `unreadable` | `不可读` | slate | XCircleIcon |
| `boundary_error` | `路径越界` | rose | AlertCircleIcon |
| `write_error` | `写入受阻` | rose | AlertCircleIcon |
| 未知值 | 原样显示 status | slate | InfoIcon |

- 视觉：badge 类基础上，md：padding `2px 8px`、fontSize 12、图标 13px；sm：padding `1px 6px`、fontSize 11、图标 12px；fontWeight 600、圆角 9999px、gap 4
- `title` 属性 = label（悬浮显示）
- 怪癖：sm 时 className 附加 `btn-sm`（对 span 无效果，可忽略）

### 2.5 Card

Props：`title`、`subtitle`、`actions`/`extra`（同义，右侧动作区）、`children`、`padding: 'none'|'sm'|'md'|'lg'`（默认 md）、`headerStyle`、`bodyStyle`。

- 卡片：背景 `var(--bg-surface)`#ffffff、圆角 `var(--radius-lg)`12px、边框 1px `var(--border-subtle)`#e2e8f0、阴影 `var(--shadow-xs)`、overflow hidden
- 头部（title/subtitle/actions 任一存在时）：padding `14px 20px`、下边框 1px subtle、flex 两端对齐 gap 12；title：fontSize 15、fontWeight 600、color `var(--slate-800)`#1e293b；subtitle：fontSize 12、color `var(--slate-500)`、marginTop 2
- 主体 padding 映射：`none: '0'`，`sm: '12px 16px'`，`md: '18px 22px'`，`lg: '24px 28px'`
- 相关 CSS 类：`.card-padded` padding `20px 24px`；`.card-compact` padding `14px 18px`（未在组件中使用）

### 2.6 输入框（CSS：.input-base / .input-mono）

- `.input-base`：宽 100%、padding `8px 12px`、fontSize 13、文字 `var(--text-primary)`、背景 `var(--bg-surface)`#ffffff、边框 1px `var(--border-default)`#cbd5e1、圆角 8px、无边框聚焦线（outline none）、`transition: border-color 150ms, box-shadow 150ms`
- hover（非禁用）：边框 `var(--slate-400)`#94a3b8
- **聚焦**：边框 `var(--amber-500)`#f59e0b + `box-shadow: 0 0 0 3px rgba(255, 174, 0, 0.2)`
- 禁用：背景 `var(--slate-100)`、文字 `var(--slate-400)`、cursor not-allowed
- `.input-mono`：等宽字体 `var(--font-mono)`、fontSize 12.5px

### 2.7 表格（CSS：.modern-table）

- `width:100%`、`border-collapse: separate; border-spacing:0`、fontSize 12.5
- `th`：padding `10px 12px`、左对齐、fontWeight 600、color `var(--slate-600)`、背景 `var(--slate-50)`#f8fafc、下边框 1px subtle、nowrap；页面里 th 额外 `position:sticky; top:0; z-index:1`（容器内滚动时固定表头）
- `td`：padding `9px 12px`、color `var(--slate-700)`、下边框 1px `var(--slate-100)`#f1f5f9（**无斑马纹**；行 hover 背景为 `var(--slate-50)`）；末行 td 无下边框；td 背景过渡 150ms
- 页面用法：`tableLayout: fixed`，`minWidth: 560`，外层 `overflowX: auto`

### 2.8 Modal

Props：`isOpen`、`onClose`、`title`、`children`、`footer`、`width`（默认 520）。

- 关闭时返回 null（不渲染）
- 遮罩 `.modal-overlay`：fixed 全屏、z-index 1000、背景 `var(--bg-overlay)`=rgba(15,23,42,0.55)、`backdrop-filter: blur(4px)`、flex 居中、padding 20px、动画 fadeIn 150ms ease-out
- 内容 `.modal-content`：width prop、`maxWidth: 92vw`、`maxHeight: 86vh`、背景 `var(--bg-surface)`、圆角 `var(--radius-xl)`16px、边框 1px subtle、阴影 `var(--shadow-xl)`、纵向 flex、动画 scaleUp 200ms cubic-bezier(0.16,1,0.3,1)
- 头部：padding `16px 20px`、下边框 subtle；标题 fontSize 16、weight 600、`var(--slate-900)`；右侧关闭按钮：ghost 样式、padding 6、圆角 6px、色 `var(--slate-400)`、`XIcon size=18`
- 主体：`flex:1; overflowY:auto; padding: 18px 20px`
- 底部（有 footer 时）：padding `14px 20px`、上边框 subtle、背景 `var(--slate-50)`、底部圆角 `0 0 16px 16px`、右对齐、gap 10
- **点击遮罩空白处（mousedown 且 target===遮罩）→ onClose**；内容区 mousedown stopPropagation
- 无 Escape 处理（仅 ConfirmModal 有）

### 2.9 ConfirmModal + ConfirmProvider + useConfirm

`ConfirmOptions`：`title?`、`message`（必填）、`description?`、`tip?`、`confirmText?`（默认 `确定`）、`cancelText?`（默认 `取消`）、`tone?: 'warning'|'danger'|'info'|'primary'`（默认 warning）、`icon?`、`width?`（默认 460）。`useConfirm()` 返回 `(options | string) => Promise<boolean>`；传字符串等价 `{message}`。

- 遮罩：同 modal-overlay 但 `z-index: 1100`；loading 时点遮罩不关闭
- 内容卡片：width 460（默认）、maxWidth 92vw、圆角 16px、背景白、边框 subtle、阴影 `0 12px 36px rgba(15, 23, 42, 0.16)`、动画 scaleUp 180ms
- 正文区 padding `22px 24px 18px`；横向布局：左侧 40×40 语气图标徽章（圆角 12px）+ 右侧内容
- 语气配置：

| tone | 图标 | 徽章底 | 徽章边框 | 确认按钮变体 |
|---|---|---|---|---|
| warning | AlertTriangleIcon 20 色 `var(--amber-800)`#b36900 | `var(--amber-100)`#fef3c7 | `var(--amber-300)`#ffdc80 | primary |
| danger | AlertCircleIcon 20 色 `var(--rose-600)`#e11d48 | `var(--rose-100)`#ffe4e6 | `var(--rose-200)`#fecdd3 | danger |
| info | InfoIcon 20 色 `var(--sky-600)`#0284c7 | `var(--sky-100)`#e0f2fe | `var(--sky-200)`#bae6fd | primary |
| primary | CheckCircleIcon 20 色 `var(--amber-800)` | `var(--amber-100)` | `var(--amber-300)` | primary |

- 标题：fontSize 15.5、weight 700、`var(--slate-900)`、letterSpacing -0.01em、marginBottom 6
- message：fontSize 13.5、line-height 1.55、`var(--slate-700)`、weight 500
- description：fontSize 12.5、line-height 1.5、`var(--slate-500)`、marginTop 6
- tip 横幅（可选）：marginTop 14、padding `10px 14px`、背景 `var(--amber-50)`#fffbeb、边框 1px `var(--amber-200)`#fde68a、圆角 8px、fontSize 12、line-height 1.5、色 `var(--amber-900)`#7d4600、前置 InfoIcon 15 色 `var(--amber-700)`#b45309
- 底部：padding `12px 20px`、背景 `var(--slate-50)`、上边框 subtle、右对齐 gap 10；取消=Button secondary minWidth 76；确认=Button（变体随 tone）minWidth 88、**autoFocus**（Enter 默认触发确认）
- **Escape 键 → onCancel**（window 级 keydown 监听，preventDefault）
- 点击遮罩 → onCancel（loading 时除外）
- Provider 实现单例弹窗；确认 resolve(true)、取消/Escape resolve(false)

### 2.10 Tabs

Props：`tabs/items`（互为别名）、`active/activeKey`、`onChange(key)`、`size: 'sm'|'md'`。TabItem：`{id|key, label, icon?, badge?}`。

- 容器：inline-flex、padding 3px、背景 `var(--slate-100)`#f1f5f9、圆角 8px、gap 2（**分段控件/toggle 风格**）
- 按钮：md padding `6px 14px` fontSize 13（sm `4px 10px` / 12）；激活：fontWeight 600、色 `var(--amber-800)`#b36900、背景白、圆角 6px、阴影 `var(--shadow-xs)`；未激活：fontWeight 500、色 `var(--slate-600)`、透明底；无边框；过渡 150ms
- 图标：激活时色 `var(--amber-700)`#b45309，未激活继承文字色
- badge：fontSize 11、padding `1px 6px`、圆角 full、weight 600；激活 bg `var(--amber-200)`#fde68a 色 `var(--amber-900)`#7d4600；未激活 bg `var(--slate-200)` 色 `var(--slate-600)`

### 2.11 StatCard

Props：`title/label`（label 优先）、`value`、`subtitle?`、`icon?`、`variant/tone`（tone 优先，默认 default）、`compact?`、`trend?`、`onClick?`。

- 卡片：白底、圆角 compact?8px:12px、阴影 `var(--shadow-xs)`、`transition: all 150ms`；padding compact `10px 14px` : `16px 18px`；左右布局（左侧文字列 + 右侧图标）
- 标题：fontSize compact 11:12、weight 500、`var(--slate-500)`、nowrap
- 数值：fontSize compact 18:22、weight 700、letterSpacing -0.02em、颜色随变体；trend 附注 fontSize 11 slate-500
- subtitle：fontSize 11、`var(--slate-400)`、truncate
- 图标容器：compact 30:38px 正方形、圆角 8px、变体底色/图标色
- 变体色板（cardBg 均为白、仅边框/图标/数值色不同）：

| variant | border | iconBg | iconColor | valColor |
|---|---|---|---|---|
| default | `var(--border-subtle)`#e2e8f0 | `var(--slate-100)`#f1f5f9 | `var(--slate-600)`#475569 | `var(--slate-900)`#0f172a |
| muted | subtle | slate-100 | `var(--slate-400)`#94a3b8 | `var(--slate-600)` |
| primary | `var(--amber-300)`#ffdc80 | `var(--amber-100)`#fef3c7 | `var(--amber-700)`#b45309 | `var(--amber-800)`#b36900 |
| success | `var(--emerald-200)`#a7f3d0 | `var(--emerald-50)`#ecfdf5 | `var(--emerald-600)`#059669 | `var(--emerald-700)`#047857 |
| warning | `var(--amber-200)`#fde68a | `var(--amber-50)`#fffbeb | `var(--amber-600)`#d97706 | `var(--amber-700)`#b45309 |
| danger | `var(--rose-200)`#fecdd3 | `var(--rose-50)`#fff1f2 | `var(--rose-600)`#e11d48 | `var(--rose-700)`#be123c |
| info | `var(--sky-200)`#bae6fd | `var(--sky-50)`#f0f9ff | `var(--sky-600)`#0284c7 | `var(--sky-700)`#0369a1 |

### 2.12 PlaceholderChip（占位符芯片）

Props：`label?`、`tag?`、`description?`、`onInsert?`、`onClick?`（onInsert 优先）、`active?`。

- tag 规范化：若不以 `{` 开头且 `}` 结尾则包上 `{}`；显示为 `{artist}` 等
- 内置标签字典（`DEFAULT_TAG_LABELS`，raw 与带花括号两种键同值）：`{artist}`→艺术家/音频的艺术家/歌手标签；`{album}`→专辑/音频所属专辑名称；`{title}`→标题/音频曲目名；`{track}`→音轨号/音轨序号 (如 01, 02)；`{year}`→年份/发行年份 (如 2024)；`{genre}`→流派/曲风流派 (如 Pop, Rock)；`{ext}`→后缀/原始音频文件扩展名 (如 mp3, flac)
- 悬浮 title 提示：有描述时 `"{label}: {desc} (点击插入)"`，否则 `"点击插入 {tag}"`
- 视觉：inline-flex、gap 5、padding `3px 8px`、fontSize 12、圆角 6px、过渡 `all 150ms ease`、cursor pointer、userSelect none
  - 常态：边框 1px `var(--slate-200)`#e2e8f0、背景 `var(--slate-50)`#f8fafc、文字 `var(--slate-700)`、TagIcon 12 色 `var(--slate-400)`
  - 悬浮/active：边框 `var(--amber-400)`#ffc533、背景 active?`var(--amber-200)`#fde68a:`var(--amber-100)`#fef3c7；TagIcon 色 `var(--amber-700)`#b45309；**文字色 `var(--amber-950)` —— 该变量未定义，实际继承祖先色（≈`#0f172a`）**；中文小标 fontSize 11、色 `var(--amber-800)`#b36900（常态 `var(--slate-500)`）
  - tag 文本用等宽字体、weight 600
- 点击 → `onInsert(formattedTag)`（带花括号形式）

### 2.13 DirectoryTreeView（目录树组件）

Props：`tree`（DirectoryTree）、`rootName`（默认 `输出目录结构`）、`initialExpandedDepth`（默认 2）、`maxHeight`（默认 360）、`searchFilter`（默认 ''）、`emptyText`（默认 `暂无目录结构数据`）。

- 外壳：card 样式、白底、圆角 12px、边框 subtle
- **头部工具栏**：padding `10px 16px`、下边框 subtle、背景 `var(--slate-50)`、两端对齐 gap 12
  - 左：LayersIcon 15 色 `var(--amber-700)`#b45309 + 标题 fontSize 13 weight 600 `var(--slate-700)`（显示 rootName，PreviewPage 传 `目标目录结构`）
  - 右：过滤输入框（width 140、padding `4px 8px 4px 24px`、fontSize 12、height 28、圆角 6px、placeholder `过滤文件...`、左内嵌 SearchIcon 12 色 slate-400）+ 折叠切换按钮（btn btn-ghost btn-sm、fontSize 11、padding `4px 8px`、文本 `全部折叠`/`全部展开` 交替）
- **主体**：padding `12px 14px`、maxHeight prop、minHeight 140、overflowY auto、背景 `var(--slate-50)`
- 空态：FolderIcon 28 色 slate-300 + emptyText fontSize 13，居中，色 slate-400
- `expandAll` 布尔状态：切换时以 key 变更强制重建整棵树；初始每个目录节点 `isOpen = defaultOpen && depth < initialExpandedDepth`（即默认展开 0、1 层）
- **目录行**：`padding: 5px 12px 5px (depth*20+6)`、fontSize 13、fontWeight 600、色 `var(--slate-800)`、圆角 6px、cursor pointer、hover 背景 `var(--slate-100)`（100ms 过渡）
  - 前缀箭头：ChevronDownIcon 14（展开）/ ChevronRightIcon 14（折叠），色 `var(--slate-400)`
  - 文件夹图标：FolderOpenIcon 16（展开）/ FolderIcon 16（折叠），色 `var(--amber-500)`#f59e0b
  - 目录名 + 数量徽标 `({直接子文件数+子目录数})`：fontSize 11、色 `var(--slate-400)`、weight 400、marginLeft 4
  - 点击行 → 切换 isOpen
- **文件行**（`__files__` 叶子组）：`padding: 4px 12px 4px ((depth+1)*20+8)`、fontSize 12.5、等宽字体、色 `var(--slate-700)`、圆角 4px、hover 背景 slate-100；前缀 FileAudioIcon 14 色 `var(--amber-600)`#d97706；文件名 truncate + title 全名
- 过滤：文件名**小写子串**匹配（`f.toLowerCase().includes(filter.toLowerCase())`）；整组无匹配则该组不渲染；目录节点不做过滤（仅文件）
- 渲染顺序：先子目录（对象键序），后 `__files__` 文件列表；根层 depth=0
- 备注：源码中 `ESCAPED_SENTINEL='__files__\x00'` 与 decodedName 逻辑为死代码，实际键名就是 `__files__`

---

## 3. DirPicker 组件（frontend/src/components/DirPicker.tsx）

### 3.1 Props 与布局

`value`、`onChange(path)`、`onEnter?`（页面未使用，ScanPage 靠 Enter 冒泡实现快捷扫描）、`placeholder`（默认 `请选择或输入目录路径...`）、`disabled`（默认 false）、`label?`、`error?`、`autoFocus?`、`showInput`（默认 true）。

- 外层：宽 100%
- 可选 label：fontSize 13、weight 600、色 `var(--slate-700)`、marginBottom 6
- **主行**：flex、gap 8
  - 输入框容器（relative、flex 1）：
    - 左侧 FolderIcon 16 @ left:10，色：有值 `var(--amber-600)`#d97706 / 无值 `var(--slate-400)`#94a3b8
    - 输入框：`input-base input-mono`，`paddingLeft: 34`，`paddingRight: 有值且未禁用 ? 32 : 12`，`height: 38`，`fontSize: 13`；受控值 = value；手输直接 onChange
    - 清空按钮（仅有值且未禁用时显示）：绝对定位 right:8、无边框、色 `var(--slate-400)`（hover `var(--slate-600)`）、padding 4、圆角 6px、内含 XIcon 14、title `清空路径`、点击 `onChange('')`
  - **浏览按钮**：`Button variant="secondary"`、`height: 38`、`padding: 0 16px`、`fontWeight: 600`、icon FolderOpenIcon 15 色 `var(--amber-700)`#b45309、文本 `浏览...`
- 可选 error 文本：fontSize 12、色 `var(--rose-600)`#e11d48、marginTop 4
- 输入框内 **Enter → onEnter?.()**（若未传则无动作；ScanPage 的容器级 Enter 处理依赖事件冒泡，见 4.2）

### 3.2 浏览按钮行为（handleBrowseClick）

1. `disabled` 时直接返回
2. 调用原生目录选择对话框：`open({ directory: true, multiple: false })`（@tauri-apps/plugin-dialog）；返回字符串 → `onChange(dir)`；返回 null（用户取消）→ 不做事
3. **异常（非 Tauri 环境或权限失败）→ 降级打开内置目录树模态框**

### 3.3 降级目录树模态框

数据源：Tauri 命令 `browse_dirs(path)` → `{ base_dir: string, entries: [{name, path}] }`。

- Modal：`title` = FolderOpenIcon 18 色 `var(--amber-700)` + `选择本地目录`；`width: 560`
- **打开时**：`filterText` 重置为 ''，`navigate(value || '')`
- `navigate(path)`：loading=true → 调 browse_dirs → 成功：`currentPath = res.base_dir`、`editingPath = res.base_dir`、`entries = res.entries`；失败：**静默保持当前状态**；finally loading=false
- **底部 footer**（flex）：左侧计数 `共 {过滤后条目数} 个子文件夹`（fontSize 12、slate-500）；`Button ghost 取消`；`Button primary 选择此目录`（disabled 当 currentPath 为空）→ `onChange(currentPath)` 并关闭
- **路径导航行**（flex gap 6）：
  - 主页按钮：secondary sm，仅 HomeIcon 14（无文字），title `返回根目录 / 盘符`，点击 `navigate('')`
  - 上一级按钮：secondary sm，仅 ArrowUpIcon 14，title `返回上一级目录`，`disabled = currentPath === ''`，点击 `navigate(getParentPath(currentPath))`
  - 路径输入框：`input-base input-mono`、height 32、fontSize 12.5、placeholder `输入路径后按 Enter 跳转...`；**Enter → navigate(editingPath)**；**Escape → 关闭模态框**；onBlur 将 editingPath 重置回 currentPath
- **过滤输入**：placeholder `过滤当前目录下的子文件夹...`、height 30、fontSize 12、paddingLeft 28、背景 `var(--slate-50)`、左内嵌 SearchIcon 13 @ left:8,top:8；过滤规则：条目 name 小写子串匹配
- **目录列表**：height 280、overflowY auto、边框 subtle、圆角 8px、白底
  - loading：RefreshIcon 16 旋转 + `正在加载目录内容...`（色 slate-400、fontSize 13、居中）
  - 空：FolderIcon 24 色 slate-300 + 有过滤词 ? `未找到匹配的子文件夹` : `当前目录下无子文件夹`
  - 条目行（key=entry.path）：flex、gap 10、padding `8px 12px`、下边框 1px `var(--slate-100)`、cursor pointer、fontSize 13、色 `var(--slate-800)`、hover 背景 `var(--slate-50)`（100ms）、离开回透明
    - FolderIcon 16 色 `var(--amber-500)`#f59e0b
    - 名称（truncate、flex 1、weight 500）
    - 右侧提示 `进入 ›`（fontSize 11、色 slate-400）
    - **整行点击 → navigate(entry.path)**（进入该子目录，不是选中）
- **当前选择预览条**：padding `8px 12px`、背景 slate-50、圆角 6px、边框 1px `var(--slate-200)`、fontSize 12；前缀 `当前选择:`（slate-500、weight 600）；值为等宽字体、truncate、weight 500、title=完整路径；空时显示 `(根目录)`

### 3.4 getParentPath 逻辑（路径拼接）

- 空串 → `''`
- Windows 盘根（正则 `^[A-Z]:[/\\]?$`，忽略大小写）→ `''`
- 反斜杠统一为 `/` → 去尾部 `/` → 取最后一个 `/` 前；无 `/` → `''`
- 若父为 `C:` 形式 → 返回 `C:\`（补反斜杠）
- macOS/Linux 上逐级向上最终到 `/`

---

## 4. 三个页面的逐区块规格

### 4.1 ScanPage（步骤 1：扫描文件）

**Props**：`onScanComplete(files, sourceDir)`、`onNext()`。本地状态：sourceDir（''）、recursive（true）、loading、error、files、hasScanned、filterField（'filename'）、filterKeyword（''）。

#### 4.1.1 扫描源目录卡片

- `<Card title="扫描源目录" subtitle="选择包含音频文件的文件夹，扫描并读取标签信息">`
- DirPicker（无 label），placeholder：`例如 D:\Music 或 /Users/me/Music`
- 按钮行（marginTop 14、两端对齐、gap 12、可换行）：
  - 左：`<label>` 复选框 `递归扫描子目录`（默认勾选；fontSize 13、色 `var(--slate-600)`、cursor pointer；原生 checkbox）
  - 右：`<Button variant="primary" size="lg" icon={<MusicIcon size={15}/>} loading={loading} disabled={!sourceDir.trim()}>`，文本 loading ? `正在扫描…` : `开始扫描`

#### 4.1.2 错误提示（error 非空时）

- 容器：flex、gap 10、padding `10px 14px`、背景 `var(--rose-50)`、边框 1px `var(--rose-200)`、圆角 8px、marginTop 12
- AlertTriangleIcon 15 色 `var(--rose-600)`；文本 fontSize 12.5、色 `var(--rose-800)`（≈#9f1239 之外的 token，`--rose-800` 未定义 → 见 7.9 陷阱）、wordBreak break-word

#### 4.1.3 空结果提示（hasScanned && !loading && !error && files.length===0）

- 背景 `var(--sky-50)`、边框 `var(--sky-200)`、圆角 8px、marginTop 12、padding `12px 16px`
- InfoIcon 15 色 `var(--sky-600)`；文本 fontSize 12.5 色 `var(--sky-800)`（未定义 token，见 7.9）：`未发现音频文件。请检查目录路径，或尝试开启「递归扫描子目录」后重新扫描。`

#### 4.1.4 看板计数（files.length > 0 时整体显示）

- 外层 card（marginTop 16、paddingTop 16、overflow visible）
- **StatPill 行**（padding `0 20px 14px`、flex wrap gap 8）。StatPill = `badge badge-{tone}` 加强版：padding `6px 12px`、fontSize 12、gap 7；label opacity 0.75 weight 500；数值 strong fontSize 13.5、tabular-nums：

| 文案 | tone | 图标 | 值 |
|---|---|---|---|
| `总文件数` | amber | FileAudioIcon 13 | files.length |
| `可读取` | emerald | MusicIcon 13 | files.filter(readable).length |
| `不可读取` | rose | AlertTriangleIcon 13 | files.length - 可读取 |
| `筛选结果`（仅有筛选词时） | slate | SearchIcon 13 | filteredFiles.length |

#### 4.1.5 筛选栏

- 容器：padding `12px 20px`、上下边框 subtle、背景 `var(--slate-50)`、flex wrap gap 8
- 前缀文字：`快速筛选`（fontSize 12、weight 600、`var(--slate-500)`）
- **字段胶囊按钮**（FILTER_FIELDS，单选）：`文件名(filename)`、`艺术家(artist)`、`专辑(album)`、`标题(title)`、`年份(year)`、`流派(genre)`
  - 样式：padding `4px 12px`、fontSize 12、圆角 full、无边框、nowrap、过渡 150ms；选中：weight 600、背景 `var(--amber-500)`#f59e0b、文字 `#1e293b`；未选中：weight 500、背景 `var(--slate-200)`#e2e8f0、文字 `var(--slate-600)`
- **关键词输入**：flex `1 1 160px`、minWidth 140、`input-base`、paddingLeft 28（左内嵌 SearchIcon 13 @ left:9 垂直居中）、height 32、fontSize 12.5、placeholder `输入关键词筛选…`
- **清空按钮**（仅 `filterKeyword || filterField !== 'filename'` 时显示）：ghost sm + XIcon 13，文本 `清空`；点击重置 keyword='' 且 field='filename'
- **过滤匹配规则**：关键词 `trim().toLowerCase()`；`filename` 字段取 `path` 按 `/` 或 `\` 切分的最后一段（basename），其他字段取对应属性字符串；匹配 = `值.toLowerCase().includes(kw)`（**大小写不敏感的子串包含**）；关键词为空白时不过滤

#### 4.1.6 文件表格

- **无匹配空态**（有筛选词且结果为 0）：SearchIcon 26 色 slate-300 + `没有匹配筛选条件的文件`（fontSize 13）+ `Button outline sm 清空筛选`（同清空逻辑），居中 padding `40px 16px`、色 slate-400、gap 8
- **表格**：`modern-table`、tableLayout fixed、minWidth 560、外层 overflowX auto；列（sticky 表头）：

| 列名 | 宽度 | 内容 |
|---|---|---|
| `文件名` | 30% | basename(path)，truncate，title=完整 path |
| `艺术家` | 18% | f.artist，truncate，title=完整值 |
| `专辑` | 20% | f.album，truncate，title |
| `标题` | 22% | f.title，truncate，title |
| `状态` | 10% | `StatusBadge status={f.readable ? 'ok' : 'unreadable'} size="sm"` |

- **最多渲染 200 行**（TABLE_LIMIT=200）；超出时表尾提示行（padding `10px 20px`、fontSize 12、slate-500、InfoIcon 13 slate-400）：`仅显示前 200 条，共 {displayFiles.length} 条。可使用筛选缩小范围。`
- 行无点击展开、无选中态（不可交互，仅 hover 底色）

#### 4.1.7 底部导航条（sticky）

- `position: sticky; bottom: 0; zIndex: 2`、marginTop 16、padding `12px 16px`、白底、边框 subtle、圆角 12px、阴影 `0 -6px 16px rgba(15,23,42,0.05)`、两端对齐 gap 12
- 左侧统计（fontSize 12、slate-500）：有筛选 `已筛选 {filteredFiles.length} / {files.length} 个文件`，否则 `共 {files.length} 个音频文件`
- 右按钮：`primary lg iconPosition=right ArrowRightIcon 15`，disabled=`files.length === 0`，文本：`下一步：设置模板` + 有筛选时追加 `（{filteredFiles.length} 个）`
- **handleNext**：若有关键词 → 先 `onScanComplete(filteredFiles, sourceDir)`（把筛选结果提交为父级数据）再 `onNext()`；否则直接 onNext()

#### 4.1.8 扫描逻辑与竞态

- `handleScan`：`sourceDir` 空白则返回；记 token → loading=true、error='' → `scanDirectory(sourceDir.trim(), recursive)`
  - 成功且 token 未过期：files=result.files、hasScanned=true、`onScanComplete(result.files, sourceDir.trim())`
  - 失败且未过期：error=消息、files=[]、hasScanned=true、`onScanComplete([], '')`（父级数据同步清空，"下一步"不可用）
- **输入变更效应**（sourceDir 或 recursive 变化，首次挂载除外）：清 files/error/loading/hasScanned/filterKeyword，token+1（丢弃在途响应），`onScanComplete([], '')`
- 组件卸载：token+1
- **Enter 快捷键**：包裹卡片的 div `onKeyDown`：Enter 且目标非 button、不在 `.modal-overlay` 内、非 checkbox/radio → 触发 `handleScan()`

### 4.2 PreviewPage（步骤 2：模板预览）

**Props**：`scannedFiles`、`sourceDir`、`onOrganize(mappings, mode, targetDir)`、`onClearOrganize()`、`onBack()`、`onNext()`。本地状态：template（初始 `'{artist}/{album}/{title}.{ext}'`）、targetDir（''）、mode（'copy'）、loading、error、mappings（[]）、directoryTree（{}）、resolvedTargetDir（''）、activeTab（'list'）。`noFiles = scannedFiles.length === 0`。

#### 4.2.1 无文件警告（noFiles 时）

- 背景 `var(--amber-50)`、边框 `var(--amber-200)`、圆角 12px、marginBottom 16、padding `12px 16px`、flex gap 10
- AlertTriangleIcon 16 色 `var(--amber-600)`；文本 fontSize 13 色 `var(--amber-800)`：`尚未扫描任何文件，请先完成扫描步骤。`
- 右侧 `Button outline sm 前往扫描` → onBack()

#### 4.2.2 整理配置卡片

`<Card title="整理配置" subtitle="设置目标目录与命名模板，点击占位符即可插入">`，内部纵向 flex gap 18：

1. **DirPicker**：label `目标目录`；placeholder 动态：`留空则整理到源目录` +（sourceDir 非空时）`（{sourceDir}）`
2. **命名模板**：
   - 标签 `命名模板`（fontSize 13、weight 600、slate-700、marginBottom 6）
   - 输入框：`input-base input-mono`、height 38、fontSize 13、placeholder `{artist}/{album}/{title}.{ext}`、受控值 template
   - 芯片行（marginTop 8、wrap gap 6）：前缀 `插入占位符：`（fontSize 12、slate-500）+ 7 个 PlaceholderChip：`{artist}` 艺术家、`{album}` 专辑、`{title}` 标题、`{track}` 音轨号、`{year}` 年份、`{genre}` 流派、`{ext}` 后缀
   - **insertPlaceholder(tag) 光标逻辑**：pos = 输入框聚焦中 ? `selectionStart` : `template.length`；在 pos 处插入 tag；随后 `requestAnimationFrame` 中重新 focus 输入框并把光标设为 `pos + tag.length`
3. **操作模式**（toggle）：
   - 标签 `操作模式`
   - 容器：inline-flex、padding 3、gap 2、背景 `var(--slate-100)`、圆角 8px
   - 两按钮：`copy` → CopyIcon 14 + `复制（保留源文件）`；`move` → ArrowRightIcon 14 + `移动（删除源文件）`
   - 激活态：weight 600、色 `var(--amber-800)`、白底、圆角 6px、阴影 shadow-xs；未激活：weight 500、色 `var(--slate-600)`、透明底（与 Tabs 组件同款分段样式）
   - **move 模式警告条**（mode==='move' 时显示于 toggle 下方 marginTop 10）：背景 `var(--amber-50)`、边框 `var(--amber-200)`、圆角 8px、padding `10px 14px`；AlertTriangleIcon 16 色 `var(--amber-600)`；文本 fontSize 12.5、色 `var(--amber-800)`、line-height 1.6，原文：`**移动模式不可逆：**执行后源文件将从原目录删除。请再次确认目标目录与命名模板正确，且源文件已做好备份。`（"移动模式不可逆："为 strong 加粗）
4. **错误提示**（error 非空）：同 ScanPage rose 样式，但 `whiteSpace: pre-wrap`（支持多行，template_errors 以 `；` 连接或命令错误以 `\n` 连接）
5. **预览按钮行**（右对齐）：`Button primary lg icon=EyeIcon 15 loading`，disabled = `loading || noFiles || !template.trim()`；文本 loading ? `生成预览中…` : `生成预览`

#### 4.2.3 生成预览逻辑（handlePreview）

- template 空白直接返回；中止上一个在途请求（AbortController；本地 IPC 实际不可中止，靠调用方丢弃过期响应）
- 请求前先清空 mappings/directoryTree/resolvedTargetDir 并 `onClearOrganize()`（防止失败后旧计划被执行）
- `effectiveTarget = targetDir.trim() || sourceDir`（目标目录留空 → 整理到源目录）
- `generatePreview(scannedFiles, template.trim(), effectiveTarget, mode)`
- 响应 `template_errors.length > 0` → error = errors.join(`'；'`)，mappings 清空；否则写入 mappings、directoryTree、resolvedTargetDir=result.target_dir
- **表单变更效应**（template/targetDir/mode 变化，首次挂载除外）：中止在途、清空 mappings/directoryTree/resolvedTargetDir、`onClearOrganize()`（"开始执行整理"随之消失）

#### 4.2.4 预览结果区（mappings.length > 0 时）

- **统计 StatCard 网格**：`grid-template-columns: repeat(auto-fit, minmax(150px, 1fr))`、gap 10、marginTop 18：

| title | variant | icon(18) | 值 |
|---|---|---|---|
| `文件总数` | primary | FileAudioIcon | mappings.length |
| `正常` | success | CheckCircleIcon | ok 数 |
| `冲突` | warning | AlertTriangleIcon | conflict+batch_conflict 数 |
| `缺失信息` | info | InfoIcon | missing_metadata 数 |
| `不可读` | default | XCircleIcon | unreadable 数 |
| `越界/写入受阻` | danger | AlertCircleIcon | boundary_error+write_error 数 |

- **Tab 切换行**（marginTop 16、gap 10）：Tabs 两项：`list` = `详细映射列表`（FileAudioIcon 14，badge=mappings.length）；`tree` = `目录树层级预览`（LayersIcon 14）；tree 激活时右侧附注 `点击文件夹可展开 / 折叠`（fontSize 12、slate-500）
- **list 表格**（card、marginTop 12、overflow visible）：modern-table、fixed、minWidth 560；列：`源文件` 38%（basename(source)，title=source）、`目标路径` 46%（**显示 `final_target` 完整路径**，truncate，title=final_target）、`最终状态` 16%（StatusBadge sm）。冲突行**无特殊高亮底色**——冲突以 amber StatusBadge（`磁盘冲突`/`批内冲突`）表达；**行不可点击/无展开**。最多 300 行（TABLE_LIMIT=300），超出提示：`仅显示前 300 条映射，共 {mappings.length} 条。`
- **tree 视图**：`<DirectoryTreeView tree={directoryTree} rootName="目标目录结构" maxHeight={420} />`
- **底部 sticky 导航条**（样式同 ScanPage 4.1.7）：
  - 左：`Button outline icon=ArrowLeftIcon 14 返回扫描` → onBack()
  - 右：`Button primary lg iconPosition=right ArrowRightIcon 15`，disabled = `organizableCount === 0`；文本 `开始执行整理（{organizableCount} 个文件）`
  - `organizableCount = mappings.length - unreadable数 - boundary_error数 - write_error数`

#### 4.2.5 开始整理（handleStartOrganize）

- **过滤映射**：剔除 status 为 `unreadable`、`boundary_error`、`write_error` 的项（这些会被后端预检整批拒绝），保留其余（含 conflict/batch_conflict/missing_metadata/ok）
- `onOrganize(过滤后mappings, mode, resolvedTargetDir || targetDir || sourceDir)` → `onNext()`（进入步骤 3，解锁 maxUnlockedStep=3）
- **注意：此按钮没有二次确认弹窗**。移动模式的防护 = 4.2.2 的静态警告条 + 步骤 3"准备开始"卡片的文案确认（源码事实，见 7.1）

### 4.3 ProgressPage（步骤 3：执行整理）

**Props**：`mappings`、`mode`、`targetDir`、`taskId`（持久化于 App/localStorage）、`onTaskIdChange(id)`、`onFinish()`。本地状态：progress（ProgressEvent|null）、started（初始 = `persistedTaskId !== ''`）、log（string[]）、done、errMsg。`noMappings = mappings.length === 0`。

#### 4.3.1 无任务数据警告（noMappings）

- amber-50/amber-200、圆角 12px、padding `12px 16px`；AlertCircleIcon 16 色 `var(--amber-600)`；文本 fontSize 13 色 `var(--amber-800)`：`没有待处理的文件，请先完成扫描和预览步骤。`（无按钮）

#### 4.3.2 任务概览卡片

`<Card title="任务概览" subtitle="整理任务的模式、目标与待处理数量">`；内容 flex wrap、`gap: '14px 32px'`：

- **操作模式**：小标签 `操作模式`（fontSize 11.5、slate-500、marginBottom 4）→ badge-amber（fontSize 12、padding `4px 10px`）：move → `移动（删除源文件）`；copy → `复制（保留源文件）`
- **目标目录**：小标签 `目标目录` → 等宽 chip（背景 slate-100、padding `4px 10px`、圆角 6px、fontSize 12.5、slate-700、truncate、title=完整路径）；空时显示 `（未设置）`
- **待处理总数**：小标签 `待处理总数` → 数值 fontSize 18、weight 700、色 `var(--amber-800)`、tabular-nums：`{mappings.length} 个文件`（"个文件" fontSize 12、weight 500、slate-500）

#### 4.3.3 准备开始卡片（!started）

`<Card title="准备开始" subtitle="确认无误后开始执行整理任务" style={{marginTop:16}}>`：

- 说明文字（fontSize 13.5、slate-600、marginBottom 16）：`将` + (move?`移动`:`复制`) + ` ` + `<strong color amber-800>{mappings.length}</strong>` + ` 个文件到目标目录` + (move?`，完成后源文件将被删除。`:`，源文件将保留。`)
- errMsg 提示（rose 样式，同前，marginBottom 16）
- `Button primary lg icon=PlayIcon 15` 文本 `开始执行`

**handleStart**：`startingRef` 防双击；置 started=true、done=false、errMsg=''、log=[]、progress=null → `startOrganize(mappings, mode, targetDir)` → 成功：`taskIdRef=res.task_id`、`onTaskIdChange(res.task_id)`（先持久化再判断挂载状态）、`startPolling(res.task_id)`；失败：started 回 false（按钮重新出现可重试）、errMsg=错误消息

#### 4.3.4 执行进度卡片（started && !done && !errMsg）

`<Card title="执行进度" subtitle="任务进行中，请勿关闭窗口" style={{marginTop:16}}>`：

- 头行（flex 底对齐、两端、gap 12、marginBottom 10）：
  - **百分比大字**：fontSize 32、weight 700、letterSpacing -0.02em、色 `var(--amber-800)`、tabular-nums、lineHeight 1；`pct = progress && total>0 ? round(current/total*100) : 0`
  - 右侧计数（fontSize 13、slate-600、tabular-nums）：有 progress ? `{current} / {total} 已处理` : `等待任务开始…`
- **进度条**：轨道 height 12、圆角 full、背景 `var(--slate-100)`、overflow hidden；填充 div（class `progress-bar-fill`）：宽度 100%、`transform: scaleX(pct/100)`（clamp 0..100）、背景 `var(--amber-500)`#f59e0b、`transform-origin: left`、`transition: transform 250ms cubic-bezier(0.16,1,0.3,1)`、圆角 full
- **当前文件条**（progress.current_file 非空时；marginTop 12、padding `7px 12px`、背景 amber-50、边框 amber-200、圆角 8px、flex gap 8）：
  - 脉冲圆点 7×7、`var(--amber-500)`、animate-pulse
  - `正在处理`（fontSize 11.5、weight 600、amber-800）
  - FileAudioIcon 14 色 `var(--amber-600)`
  - 文件名（等宽、fontSize 12、色 `var(--amber-900)`#7d4600、truncate、title=完整路径）

#### 4.3.5 完成横幅（started && done）

- 容器：emerald-50 底、emerald-200 边框、圆角 12px、marginTop 16、padding `20px 22px`、gap 14
- CheckCircleIcon 24 色 `var(--emerald-600)`
- 标题 `整理完成`（fontSize 16、weight 700、emerald-700）
- 副文（fontSize 12.5、emerald-700、opacity 0.85）：`共处理 {progress?.total ?? mappings.length} 个文件，任务已成功结束。`
- `Button primary icon=SparklesIcon 15` 文本 `完成并开启新任务` → `onFinish()` = App 的 `handleReset(false)`（**不弹确认**，直接全量重置回步骤 1）

#### 4.3.6 失败横幅（started && errMsg && !done）

- rose-50/rose-200、圆角 12px、padding `20px 22px`
- AlertCircleIcon 24 色 `var(--rose-600)`
- 标题 `任务执行失败`（fontSize 16、weight 700、rose-700）
- errMsg 正文（fontSize 12.5、rose-800、pre-wrap、break-word）
- 同款 `完成并开启新任务` 按钮

#### 4.3.7 实时日志控制台（started && log.length > 0）

`<Card title="实时日志" extra={<span class="badge badge-slate" style={{fontSize:10.5}}>TERMINAL</span>}>`：

- 容器：背景 `var(--slate-950)`#020617、圆角 8px、padding `12px 14px`、等宽字体、fontSize 12、line-height 1.8、maxHeight 260、overflowY auto、基础文字色 `var(--slate-300)`#cbd5e1
- **颜色分级（LogLine）**：正则 `^(\[[^\]]*\])\s*(.*)$` 匹配 → 前缀 `[n/total]` 色 `var(--amber-400)`#ffc533，其余正文色 `var(--sky-300)`#bae6fd；不匹配的行色 `var(--slate-400)`#94a3b8
- **滚动锚定**：`onScroll` 时 `atBottomRef = (scrollHeight - scrollTop - clientHeight < 40)`；新日志到达时**仅当 atBottom 为 true 才自动滚到底**（用户上翻阅读时不打扰）
- 日志行来源（轮询循环内）：
  - `current_file` 非空 → `[{current}/{total}] {basename(current_file)}`；与上一行相同则不重复追加
  - 否则 `message` 非空 → 追加 message
  - 缓冲上限 300 条（LOG_CAP，超出丢弃最旧）

#### 4.3.8 轮询协议（前端消费方式）

- `startPolling(id)`：`setInterval(1000ms)` 循环调 `getTaskStatus(id)`
- 每次响应：更新 progress、追加日志；`status === 'done'` → done=true 并停止轮询；`status === 'error'` → errMsg = `data.message || '执行出错'` 并停止；网络/调用异常 → **继续轮询不中断**
- 卸载时停止轮询；`mountedRef` 防卸载后更新
- **重连**：挂载时若 `persistedTaskId` 非空 → 直接 startPolling；组件已挂载期间 taskId 到达且未 started/未 done/无轮询 → 重连（覆盖"点开始后立刻切页再切回"场景）；页面刷新后 App 从 localStorage 恢复 taskId，ProgressPage 据此恢复进度视图

---

## 5. 数据与命令层

### 5.1 TypeScript 类型（frontend/src/api/types.ts，逐字段）

```typescript
export interface AudioFileInfo {
  path: string       // 绝对路径
  ext: string        // 扩展名（如 mp3）
  artist: string     // 无标签时为 "Unknown Artist"
  album: string      // 兜底 "Unknown Album"
  title: string      // 兜底 "Unknown Title"
  track: string      // 兜底 "0"（有值时如 "01"）
  year: string       // 兜底 "Unknown Year"
  genre: string      // 兜底 "Unknown Genre"
  readable: boolean
  error: string      // 读取错误信息，可空串
}

export interface ScanResponse {
  source_dir: string
  total: number
  files: AudioFileInfo[]
}

export interface FileMappingItem {
  source: string
  target: string           // 渲染的原始目标路径（冲突消解前）
  final_target: string     // 冲突消解后的最终计划路径（UI 表格显示此字段）
  relative_target: string  // 相对 target_dir 的显示用路径
  status: 'ok' | 'conflict' | 'batch_conflict' | 'missing_metadata'
        | 'unreadable' | 'boundary_error' | 'write_error'
  conflict: boolean        // 磁盘上已存在同名目标
  batch_conflict: boolean  // 批内目标碰撞
}

export interface PreviewResponse {
  template: string
  target_dir: string
  total: number
  mappings: FileMappingItem[]
  template_errors: string[]
  directory_tree: DirectoryTree
}

export type DirectoryTree = { [key: string]: DirectoryTree | string[] }
// 目录名 → 子树；特殊键 "__files__" → 该目录直接包含的文件名数组

export interface OrganizeStartResponse {
  task_id: string
  total: number
}

export interface ProgressEvent {
  task_id: string
  status: 'pending' | 'running' | 'done' | 'error'
  current: number
  total: number
  current_file: string
  message: string
}

// client.ts 中追加（不在 types.ts）：
export interface DirEntry { name: string; path: string }
export interface BrowseResponse { base_dir: string; entries: DirEntry[] }
```

兜底常量（metadata.rs）：`Unknown Artist` / `Unknown Album` / `Unknown Title` / `0`(track) / `Unknown Year` / `Unknown Genre`。

### 5.2 Tauri 命令（6 个；invoke 参数名为 camelCase）

**错误规范化（toError）**：Err 值为 string → `Error(string)`；为对象且含 `template_errors` 或 `preflight_errors` 数组 → `Error(arr.join('\n'))`；其他 → `Error(JSON.stringify(raw))`。

#### 1) scan_directory
- 参数：`{ sourceDir: string, recursive?: boolean }`（recursive 缺省 true）
- 返回：`ScanResponse`
- 错误（字符串）：`Source directory must not be empty.` / `Source directory contains disallowed path traversal components.` / `Source directory could not be resolved to an absolute path.` / 扫描 IO 错误文本
- 行为：规范化源路径（拒绝 `..`、要求绝对路径）→ 递归/单层扫描音频文件 → 批量提取元数据

#### 2) generate_preview
- 参数：`{ req: { files: AudioFileInfo[], template: string, target_dir: string, mode: 'move'|'copy' } }`（req 内为 snake_case）
- 返回：`PreviewResponse`（`directory_tree` 为任意 JSON 对象）
- 错误：模板错误 → 对象 `{ template_errors: string[] }`；校验错误 → 字符串（如 `Target directory must not be empty.` 等，同 5.2.1 格式但 context 为 `Target directory`；另有 `Target directory path exists but is not a directory.`）
- 模板错误文案（template.rs）：`Template must not be empty.`；`Unsupported placeholder(s): ['xxx']. Supported: ['album', 'artist', 'ext', 'genre', 'title', 'track', 'year'].`（列表为 Python 风格单引号表示，unsupported 排序后输出；Supported 集合 = artist/album/title/track/year/genre/ext 七个，顺序为常量数组序）

#### 3) start_organize
- 参数：`{ mappings: FileMappingItem[], mode: 'move'|'copy', targetDir: string }`
- 返回：`{ task_id: string, total: number }`
- 错误：`No file mappings provided.`（空批次）；目标目录校验字符串；预检失败 → 对象 `{ preflight_errors: string[] }`（多条以 `\n` 连接展示）
- 预检错误文案（organizer.rs，英文原文）：
  - `Source not found: {path}`
  - `Source is not a file: {path}`
  - `Source is not readable: {path}`
  - `Source parent directory is not writable (move requires write+execute on parent): {path}`
  - `Duplicate source in move batch (file can only be moved once): {path}`
  - `Case-only copy is not supported: source and destination are the same file on this filesystem: {src} -> {dst}`
  - `Target escapes the target directory: {final_target}`
  - `Target resolves to the target directory itself (not a valid file path): {final_target}`
  - `Duplicate final target in batch: {final_target}`
  - `Target path conflicts with another target in batch (file-vs-directory collision): {final_target}`
  - `Cannot determine write access for: {path}`
  - `Target ancestor is a broken symlink: {ancestor}. Cannot create path: {path}`
  - `Target ancestor is not a directory (it is a file): {ancestor}. Cannot create path: {path}`
  - `No write+execute permission for directory: {path}`
- 行为：全部预检通过后创建任务（UUID v4）、spawn 后台线程执行、立即返回 task_id

#### 4) get_task_status
- 参数：`{ taskId: string }`
- 返回：`ProgressEvent`（当前快照）
- 错误：`Task not found: {task_id}`（任务不存在或终态超 300s 被淘汰）

#### 5) browse_dirs
- 参数：`{ path: string }`（空串 = 根）
- 返回：`{ base_dir: string, entries: DirEntry[] }`
- 错误：`路径不存在：{path}`（中文）
- 行为：path 为空 → Windows 返回盘符列表（`C:\`…，base_dir 为空串）；其他平台返回 `$HOME`。读取目录仅保留子目录（文件忽略），按文件名排序，无权限项静默跳过。base_dir 为规范化后的绝对路径

#### 6) exit_app
- 无参数；`app.exit(0)`；前端失败时兜底 `getCurrentWindow().destroy()`

### 5.3 后台任务协议（src-tauri/src/task.rs）

- **事件通道**：`progress://{task_id}`，payload = ProgressEvent（同 5.1 结构，serde snake_case：status 序列化为 `pending`/`running`/`done`/`error`）
- **推送时机**（run_organize 循环，逐条映射）：
  1. 处理前：`{status: running, current: i-1, total, current_file: source, message: ""}`（i 从 1 起）
  2. 失败（终止任务）：`{status: error, current: i-1, total, current_file: source, message: "Failed: {source}: {错误信息}"}`——任务立即停止，后续文件不再处理
  3. 单条成功：`{status: running, current: i, total, current_file: source, message: "Processed {i}/{total}"}`
  4. 全部完成：`{status: done, current: total, total, current_file: "", message: "Completed {total} file(s)."}`
- 每次事件同时更新内存注册表快照（供 get_task_status 读取）
- 注册表：终态任务 TTL 300s 惰性淘汰；容量上限 32（满时淘汰最旧终态任务）
- **重要事实：当前前端没有 listen 事件通道，完全靠 `get_task_status` 每 1000ms 轮询**。事件通道是为迟到订阅者预留的（架构注释提及），GPUI 版可沿用轮询或改订阅，但 UI 行为（1s 刷新粒度、断线重连语义）应保持
- 任务恢复：task_id 存 localStorage(`tag2folders_task_id`)，刷新/切页后凭它轮询快照恢复

---

## 6. 设计 token 总表（index.css，标注有效值）

### 6.1 中性色（Slate）

| token | hex |
|---|---|
| --slate-50 | #f8fafc |
| --slate-100 | #f1f5f9 |
| --slate-200 | #e2e8f0 |
| --slate-300 | #cbd5e1 |
| --slate-400 | #94a3b8 |
| --slate-500 | #64748b |
| --slate-600 | #475569 |
| --slate-700 | #334155 |
| --slate-800 | #1e293b |
| --slate-900 | #0f172a |
| --slate-950 | #020617 |

### 6.2 琥珀色（amber，**有效值**，见文首陷阱表）

`--amber-50:#fffbeb`、`--amber-100:#fef3c7`、`--amber-200:#fde68a`、`--amber-300:#ffdc80`、`--amber-400:#ffc533`、`--amber-500:#f59e0b`、`--amber-600:#d97706`、`--amber-700:#b45309`、`--amber-800:#b36900`、`--amber-900:#7d4600`。`--amber-950` 不存在。`--indigo-50..900` = 首次声明值（50:#fffdf5, 100:#fff8e6, 200:#ffedba, 300:#ffdc80, 400:#ffc533, 500:#ffae00, 600:#ffae00, 700:#d98500, 800:#b36900, 900:#7d4600），未被组件使用。

### 6.3 语义色

| 组 | 50 | 100 | 200 | 500 | 600 | 700 |
|---|---|---|---|---|---|---|
| emerald | #ecfdf5 | #d1fae5 | #a7f3d0 | #10b981 | #059669 | #047857 |
| rose | #fff1f2 | #ffe4e6 | #fecdd3 | #f43f5e | #e11d48 | #be123c |
| sky | #f0f9ff | #e0f2fe | #bae6fd | #0ea5e9 | #0284c7 | #0369a1 |

（sky-300 #bae6fd 亦被定义并用于日志正文色。emerald-300/400、rose-300/400 等未定义。）

### 6.4 功能映射

| token | 值 |
|---|---|
| --bg-app | #f8fafc |
| --bg-surface | #ffffff |
| --bg-subtle | #f1f5f9 |
| --bg-muted | #e2e8f0 |
| --bg-overlay | rgba(15, 23, 42, 0.55) |
| --border-subtle | #e2e8f0 |
| --border-default | #cbd5e1 |
| --border-focus | #ffae00 |
| --text-primary | #0f172a |
| --text-secondary | #334155 |
| --text-muted | #64748b |
| --text-tertiary | #94a3b8 |
| --text-on-primary | #ffffff |

### 6.5 字体

- `--font-sans: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "WenQuanYi Micro Hei", sans-serif`
- `--font-mono: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace`
- 基准：html 14px / line-height 1.5

### 6.6 圆角

`--radius-xs:4px`、`--radius-sm:6px`、`--radius-md:8px`、`--radius-lg:12px`、`--radius-xl:16px`、`--radius-full:9999px`

### 6.7 阴影

| token | 值 |
|---|---|
| --shadow-xs | 0 1px 2px 0 rgba(15,23,42,0.05) |
| --shadow-sm | 0 1px 3px 0 rgba(15,23,42,0.08), 0 1px 2px -1px rgba(15,23,42,0.08) |
| --shadow-md | 0 4px 6px -1px rgba(15,23,42,0.08), 0 2px 4px -2px rgba(15,23,42,0.06) |
| --shadow-lg | 0 10px 15px -3px rgba(15,23,42,0.08), 0 4px 6px -4px rgba(15,23,42,0.04) |
| --shadow-xl | 0 20px 25px -5px rgba(15,23,42,0.1), 0 8px 10px -6px rgba(15,23,42,0.06) |
| --shadow-inner | inset 0 2px 4px 0 rgba(15,23,42,0.04) |

组件内硬编码阴影：主按钮 `0 1px 2px rgba(0,0,0,0.05)`（hover `0 2px 4px rgba(0,0,0,0.08)`）；品牌方块 `0 1px 3px rgba(0,0,0,0.1)`；激活步骤瓦片 `0 1px 4px rgba(217,133,0,0.25)`；ConfirmModal `0 12px 36px rgba(15,23,42,0.16)`；底部 sticky 条 `0 -6px 16px rgba(15,23,42,0.05)`；输入聚焦光晕 `0 0 0 3px rgba(255,174,0,0.2)`。

### 6.8 过渡与动画

| 名称 | 值 |
|---|---|
| --transition-fast | 150ms cubic-bezier(0.16, 1, 0.3, 1) |
| --transition-base | 200ms cubic-bezier(0.16, 1, 0.3, 1) |
| --transition-smooth | 300ms cubic-bezier(0.16, 1, 0.3, 1) |
| fadeIn（遮罩） | 150ms ease-out（opacity 0→1） |
| scaleUp（模态内容） | 200ms cubic-bezier(0.16,1,0.3,1)：opacity 0、scale(0.97)、translateY(4px) → 正常 |
| ConfirmModal 内容 | scaleUp 180ms（同曲线） |
| 页面切换 | scaleUp 220ms（同曲线） |
| animate-spin | spin 1s linear infinite |
| animate-pulse | opacity 1↔0.6，2s cubic-bezier(0.4,0,0.6,1) infinite |
| 进度条填充 | transform 250ms cubic-bezier(0.16,1,0.3,1) |
| 树/列表行 hover | background-color 100ms |

### 6.9 DESIGN.md 与实现的差异（以实现为准）

- DESIGN.md 称主色 `#FFAE00`；实现中所有 `var(--amber-500)` 实际为 `#f59e0b`（见文首陷阱）
- DESIGN.md 的间距标度（4/8/14/20/28）与排版级别为参考值；组件实际尺寸以第 2~4 章逐项数值为准
- 表格**没有斑马纹**（仅行 hover 底色），DESIGN.md 中"斑马纹"描述与实现不符

---

## 7. 交互细节与边界

### 7.1 全部确认弹窗清单（useConfirm，仅 2 处）

| # | 触发点 | title | message | description | tip | confirmText / cancelText / tone |
|---|---|---|---|---|---|---|
| 1 | 顶栏"重置"按钮 | `确认重置全部数据？` | `确定要清空当前的扫描结果、整理模板配置并重新开始吗？` | — | `若当前有正在后台执行的文件整理任务，重置将断开界面追踪。` | `确认重置` / `取消` / warning |
| 2a | 窗口关闭（有任务） | `确认退出应用？` | `确定要退出 Tag2Folders 吗？` | `当前有正在进行或未完成的文件整理任务，退出将中断处理。` | `建议等待任务整理完成后再退出应用。` | `确认退出` / `取消` / warning |
| 2b | 窗口关闭（无任务） | `确认退出应用？` | `确定要退出 Tag2Folders 吗？` | `退出后当前未保存的配置与扫描缓存将被清除。` | — | 同上 |

**不存在**其他确认弹窗：PreviewPage 的"开始执行整理"**没有**移动模式二次确认弹窗（移动模式的防护为 PreviewPage 静态警告条 + ProgressPage"准备开始"卡片说明）；ProgressPage 的"完成并开启新任务"重置也**不弹确认**（直接 `handleReset(false)`）。

### 7.2 键盘行为汇总

| 场景 | 键 | 行为 |
|---|---|---|
| ConfirmModal 打开 | Escape | onCancel（关闭并 resolve false） |
| ConfirmModal 打开 | Enter | 触发确认按钮（autoFocus 聚焦其上） |
| ConfirmModal 遮罩 | 鼠标按下空白处 | onCancel（loading 时忽略） |
| Modal（通用）遮罩 | 鼠标按下空白处 | onClose；**无 Escape 支持** |
| DirPicker 主输入框 | Enter | 调 onEnter（若传）；ScanPage 中冒泡至容器触发"开始扫描"（排除：焦点在按钮上、在 `.modal-overlay` 内、checkbox/radio） |
| DirPicker 模态路径输入 | Enter | navigate(editingPath) 跳转 |
| DirPicker 模态路径输入 | Escape | 关闭模态框 |
| 步骤导航项 | Enter / Space | 等同点击（仅已解锁步骤；tabIndex 0/-1） |

### 7.3 加载态

- 扫描按钮：loading=true → 禁用 + 前置旋转 RefreshIcon(14) + 文案 `正在扫描…`
- 预览按钮：loading=true → 禁用（`disabled = loading || noFiles || !template.trim()`）+ 旋转图标 + `生成预览中…`
- DirPicker 模态列表：navigate 期间显示旋转图标 + `正在加载目录内容...`；导航失败保持原列表
- IPC 均为本地 invoke，无网络超时概念；在途响应以 token/abort 标记丢弃（见 4.1.8 / 4.2.3）

### 7.4 禁用态

- 开始扫描：`!sourceDir.trim()`
- ScanPage 下一步：`files.length === 0`
- 生成预览：`loading || noFiles || !template.trim()`
- 开始执行整理：`organizableCount === 0`（全部文件为 unreadable/boundary_error/write_error 时）
- 选择此目录（DirPicker 模态）：`currentPath` 为空
- 上一级按钮：`currentPath === ''`
- 浏览按钮/输入框/清空：`disabled` prop
- 未解锁步骤项：不可点、tabIndex -1、opacity 0.5
- 通用禁用样式：opacity 0.55 + not-allowed + 无悬浮

### 7.5 空数据态

| 位置 | 文案/视觉 |
|---|---|
| 扫描成功但 0 文件 | sky 提示条：`未发现音频文件。请检查目录路径，或尝试开启「递归扫描子目录」后重新扫描。` |
| 筛选无结果 | SearchIcon 26 + `没有匹配筛选条件的文件` + `清空筛选` 按钮 |
| DirectoryTreeView 空树 | FolderIcon 28 + `暂无目录结构数据`（PreviewPage 传入的树在有 mappings 时非空） |
| DirPicker 模态空列表 | 无过滤词 `当前目录下无子文件夹`；有过滤词 `未找到匹配的子文件夹` |
| PreviewPage 无扫描数据 | amber 警告条 + `前往扫描` 按钮 |
| ProgressPage 无映射 | amber 警告条：`没有待处理的文件，请先完成扫描和预览步骤。` |
| 目标目录未设置（任务概览） | 显示 `（未设置）` |

### 7.6 错误提示形式

- **没有 toast 系统**；全部为内联 alert 条（rose=错误 / amber=警告 / sky=信息），带对应图标，见 4.1.2、4.2.2-4、4.3.3、4.3.6
- PreviewPage 错误条支持 `whiteSpace: pre-wrap`（多条错误换行展示：template_errors 用 `；` 连接；preflight_errors 用 `\n` 连接）
- ProgressPage 轮询异常静默重试；任务失败 errMsg = 事件 message（`Failed: ...` 格式）或 `执行出错`

### 7.7 任务状态恢复（刷新/切页重连）

- task_id 生命周期：startOrganize 成功 → App.setTaskId（写 localStorage）→ ProgressPage 轮询；任务终态后用户点"完成并开启新任务"→ handleReset(false) 清除
- 页面刷新：App 初始化时从 localStorage 读 taskId；三个页面常驻挂载（display 切换）保证步骤间状态不丢；ProgressPage 重新挂载时若 taskId 非空直接 startPolling 恢复进度
- 后端终态任务保留 300s（get_task_status 可查）；超时后返回 `Task not found: {id}`，前端轮询会持续静默失败（不会自动停止——现状行为，重写需保持或明确决策）
- 扫描页在任务运行期间修改源目录/递归会清空父级 scannedFiles，但不清 taskId（退出确认仍会提示"有任务"）

### 7.8 其他边界行为（保持一致）

- ScanPage 表格只渲染前 200 行；PreviewPage 映射表只渲染前 300 行（纯 UI 截断，父级数据完整）
- ScanPage"下一步"带筛选词时：提交的是**筛选后的子集**（按钮文案 `（N 个）` 同步显示）
- PreviewPage 表单（模板/目标目录/模式）任何变更都会立刻作废已生成的预览（"开始执行整理"消失），需重新生成
- 开始整理前剔除 unreadable / boundary_error / write_error 三类映射
- 复制/移动 toggle 默认 `copy`；重新扫描后模式、目标目录、模板结果全部重置
- 任务失败为"第一条错误即终止"语义（后端循环 return），非逐文件跳过
- 日志去重：与上一行完全相同则不追加

### 7.9 未定义 CSS 变量陷阱（渲染结果说明）

以下 `var(--x)` 引用的变量在 :root 中**不存在**。CSS 语义：属性在计算值阶段无效 → 继承值（对 color）或初始值。GPUI 重写请直接采用下述等效结果：

| 引用 | 等效渲染 |
|---|---|
| `color: var(--amber-950)`（PlaceholderChip 悬浮/激活文字） | 继承祖先文字色，链路上最近的着色祖先是 html 的 `--text-primary` = **#0f172a** |
| `color: var(--rose-800)`（错误条文字） | 同上 → **#0f172a**（视觉为近黑文字于浅玫红底） |
| `color: var(--sky-800)`（空扫描提示文字） | 同上 → **#0f172a** |

---

## 附：源文件索引

| 文件 | 作用 |
|---|---|
| frontend/src/App.tsx | 外壳、步骤状态机、确认弹窗、页面挂载策略 |
| frontend/src/main.tsx | React 入口（StrictMode） |
| frontend/src/index.css | 全部设计 token、按钮/徽章/卡片/表格/模态 CSS、动画 |
| frontend/src/components/CommonUI.tsx | 图标×32、StatusBadge、PlaceholderChip、StatCard、DirectoryTreeView、Button、Card、Tabs、Modal、ConfirmModal/Provider/useConfirm |
| frontend/src/components/DirPicker.tsx | 目录输入+浏览+降级目录树模态框 |
| frontend/src/pages/ScanPage.tsx | 步骤1 |
| frontend/src/pages/PreviewPage.tsx | 步骤2 |
| frontend/src/pages/ProgressPage.tsx | 步骤3（轮询/日志/进度） |
| frontend/src/api/client.ts | invoke 封装、toError、pickDirectory、isTauri、exitApp |
| frontend/src/api/types.ts | 全部 TS 类型 |
| src-tauri/src/commands.rs | 6 个 Tauri 命令 |
| src-tauri/src/task.rs | 任务注册表、progress:// 事件、快照 |
| src-tauri/src/core/mod.rs | AudioMetadata / OrganizeMode / MappingStatus / FileMappingItem |
| src-tauri/src/core/{scanner,metadata,template,path_security,path_util,organizer,preview}.rs | 业务核心（本文档仅引用其接口与错误文案） |
| src-tauri/tauri.conf.json | 窗口与打包配置 |

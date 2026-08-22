# 已知与源应用的差异(KNOWN_DIFFERENCES)

> 记录 GPUI 版与源 Tauri/React 版的**有意或不可避免**的 UI/行为差异。
> 功能性差异(文案、边界条件、错误处理)不在本文范围——那些一律保持一致。

## 1. 窗口关闭确认:已实现,非跳过

gpui 0.2.2 提供 `window.on_window_should_close(cx, f)`(返回 false 可阻止关闭),
因此 SPEC 1.5 的关闭确认弹窗**已等价实现**(`src/app.rs register_close_guard`),
不列入差异。两变体文案(有任务/无任务)已接入真实判定(`AppShell::has_running_task`:
task_id 非空且任务快照未到终态;与源 `Boolean(taskId)` 的差异见 §6)。

## 2. 布局/度量近似

| 项 | 源值 | GPUI 值 | 原因 |
|---|---|---|---|
| 顶栏水平 padding | `0 clamp(16px, 3vw, 32px)` | `0 24px` | gpui 无 vw/clamp 长度;取区间中值,窗口 900~1100 内偏差 ≤8px |
| 工作区 padding | `clamp(16px, 2.5vw, 32px)` | `24px` | 同上 |
| 左步骤栏宽度 | `clamp(210px, 22vw, 250px)` | `230px` | 同上(1100 窗口下源值 242,最小 900 下源值 210;230 为折中,偏差 ≤12px) |
| Modal maxHeight | `86vh` | `620px` | 无 vh;750 高窗口 86vh≈645,取保守值 |
| 扫描页表格滚动模型 | 整页滚动 + `th position:sticky top:0` 表头吸顶 | 表头固定行 + **表体容器内滚动**(max-height 480px) | gpui 无 position:sticky;容器滚动 + 固定表头是等价可达的形态,超长列表不再撑高整页 |
| 预览页映射表滚动模型 | 同上(sticky 表头) | 同扫描页方案(固定表头 + 表体容器内滚动 max-height 480px) | 同上 |
| 扫描页/预览页底部导航条 | `position: sticky; bottom: 0`(贴视口底) | 常规流元素(位于页面末尾) | 同上无 sticky;滚动到底部时视觉一致,滚动中途不悬浮 |
| 预览页统计卡网格 | `grid-template-columns: repeat(auto-fit, minmax(150px, 1fr))` | flex wrap + 每卡 `flex:1; min-width:150` | gpui 无 auto-fit 网格;1080 宽度下 6 卡同样单行等分,窄窗时逐行换行 |
| 进度页任务概览卡 gap | `14px 32px`(行距 14 / 列距 32 分离) | 统一 `32px` | gpui 0.2.2 无 gap_x/gap_y 分离设置;单行布局(常态)无差异,换行时行距偏大 |
| 进度页百分比大字 letterSpacing | `-0.02em` | 未设置 | gpui 0.2.2 无 letter_spacing 样式;32px 大字下视觉差异不可辨 |
| tabular-nums(等宽数字) | 概览计数/百分比/已处理计数等 `fontVariantNumeric: tabular-nums` | 未复刻 | gpui 无 font-variant API;数字跳动时宽度有轻微抖动 |

## 3. 动画降级

- **CSS transition 全部缺失**:gpui 的 hover/active 样式切换是瞬时的,
  源 150ms/100ms 过渡(按钮、瓦片、行 hover、输入框边框、占位符芯片)不可复刻;
- **scaleUp 退化为 opacity fadeIn**:gpui 0.2.2 的 div 无 transform 样式
  (`.scale()`/`translateY` 不存在,只有 Svg 支持 transformation),
  模态入场(scale 0.97 + translateY 4px + opacity)只保留 opacity 部分;
- **主按钮按下 `translateY(1px)`** 无法实现(同上),按下态仍有背景/文字/边框变化;
- 进度条填充的 `transform: scaleX(pct)` + 250ms 过渡 → 直接按百分比宽度呈现;
- 已实现保留的动画:loading 旋转图标(1s 线性)、进行中步骤脉冲圆点
  (opacity 1↔0.6,2s,三角波近似 cubic-bezier(0.4,0,0.6,1))。

## 4. 交互/视觉细节

| 项 | 差异 |
|---|---|
| `title` 悬浮提示 | 源大量使用 `title` 属性(重置按钮"清空所有数据并重新开始"、表格单元格全名、StatusBadge label、占位符芯片 `"{label}: {desc} (点击插入)"` 等);gpui 无原生 tooltip,当前未实现 |
| 输入框聚焦光晕 | 源 `box-shadow: 0 0 0 3px rgba(255,174,0,0.2)`;gpui-component Input 只换聚焦边框色(已设 amber-500 与源一致),3px 光晕未复刻 |
| 模态遮罩模糊 | 源 `backdrop-filter: blur(4px)`;gpui 遮罩为纯半透明色,无背景模糊 |
| 表格 hover 滚动条 | 源自定义 6px 滚动条(slate-300 thumb);gpui 用系统/组件库默认滚动条 |
| 全局焦点环 | 源 `:focus-visible` amber 500 2px outline;gpui 焦点样式随组件库,未全局复刻 |
| Enter 触发确认 | 已实现(ConfirmModal 卡片 on_key_down);与源的 autoFocus+原生按钮触发等价 |
| 未解锁步骤 tabIndex | 源 tabIndex 0/-1 键盘可达;gpui 无 Tab 序精细控制(组件库统一 Tab 遍历),点击规则一致 |
| 递归扫描复选框 | 源为原生 `<input type="checkbox">`(浏览器/系统绘制) | gpui-component `Checkbox`(自绘方框+对勾,换肤后主色 amber-500);行为等价(点整行切换、切换即触发作废效应) |
| 占位符芯片光标插入 | 源读 `selectionStart` 在光标处插入再恢复焦点/光标;GPUI 版:输入框**聚焦中**用 `InputState::insert()` 在内部光标处插入(插入后光标落在插入文本末尾,等价)、**未聚焦**追加到末尾,随后聚焦输入框(与源降级分支一致)。差异:输入框聚焦与否的判定改为跟踪 InputEvent::Focus/Blur(等价 `document.activeElement === el`);gpui 点击芯片不会抢走输入框焦点(浏览器会先 blur),因此"聚焦中插光标处"的分支比源**更容易命中**;非空选区时源在选区起点插入且保留选区文本,GPUI 在选区终点插入且保留选区文本(仅多字符选区下有细微位置差异) |
| 占位符芯片 hover | 源芯片自带 hovered 态(图标/文字随悬浮换色);GPUI 版用 `on_hover` 把 hover 状态上提到页面重渲染实现,**颜色/边框逐项一致**;仅 150ms 过渡缺失(见 §3) |
| 目录树节点顺序 | 源 JS 对象按 Python dict 插入序遍历;GPUI 版 `serde_json` 默认 BTreeMap,**同层目录/文件按字典序**排列 |
| 目录树全部展开/折叠 | 源靠 `key={expandAll}` 变更强制重挂载整棵树重置开合;GPUI 版清空用户开合记录 + 重算默认开合(depth<2),行为等价 |
| 日志滚动锚定实现 | 源 `onScroll` 事件持续维护 `atBottomRef`(scrollHeight-scrollTop-clientHeight < 40),新日志到达时查 ref;GPUI 版在新日志**追加时**直接读 `ScrollHandle` 实时位置(`max_offset.height + offset.y < 40px`)判定,判定后用 gpui 一次性 `scroll_to_bottom()` 标记跟随 | 语义等价(距底 40px 阈值一致、上翻不打扰),实现路径不同;实时读取比事件维护的 ref 更不会漏掉滚动状态 |
| 日志行内联排版 | 源 LogLine 两个 `<span>`(inline)前缀+正文同行流式排列 | GPUI 版 flex 行内两个子 div | 等宽字体下视觉一致;长正文在自身子元素内换行 |

## 5. 其他

- 源 `localStorage` 持久化(`tag2folders_task_id`)→ GPUI 版已实现为 `dirs` 数据目录
  `tag2folders/state.json`(仅存 task_id,读写失败静默忽略);启动时读取并在任务
  仍存活时恢复轮询追踪(PORT_NOTES §6 遗留项已闭环);
- 源 `exit_app` 兜底销毁窗口 → GPUI 版统一 `cx.quit()`;
- 字体栈:源为多级回退链,GPUI 版 UI 文本直接 "PingFang SC"、等宽 "Menlo"
  (macOS;已在运行时验证两者可解析,中文无豆腐块)。

## 6. 进度页任务生命周期决策(有意的行为修正)

| 项 | 源行为 | GPUI 行为 | 说明 |
|---|---|---|---|
| 退出确认"有任务"判定 | `Boolean(taskId)`:任务已 done/error 但未点"完成并开启新任务"时,关窗确认仍报"有任务" | task_id 非空**且**快照未到终态(done/error);尚无快照视为进行中 | 终态任务退出不会中断任何处理,报"有任务"是源的误报;按 D5 任务说明修正 |
| 启动恢复时任务已过期(终态超 300s 被淘汰) | 刷新后凭 localStorage taskId 无限静默轮询(永不停止,SPEC 7.7 注明"现状行为") | 启动时一次性探测 `get_task_status`,不存在则静默清空 taskId(含持久化文件)并停留步骤 1 | 避免无意义的永久轮询;按 D5 任务说明决策 |
| 运行中轮询遇"Task not found" | 继续静默轮询 | 同源:继续静默轮询(不停轮询) | 保持 SPEC 4.3.8 语义;该场景仅在注册表 32 容量挤占等极端情况下出现 |

# 已知与源应用的差异(KNOWN_DIFFERENCES)

> 记录 GPUI 版与源 Tauri/React 版的**有意或不可避免**的 UI/行为差异。
> 功能性差异(文案、边界条件、错误处理)不在本文范围——那些一律保持一致。

## 1. 窗口关闭确认:已实现,非跳过

gpui 0.2.2 提供 `window.on_window_should_close(cx, f)`(返回 false 可阻止关闭),
因此 SPEC 1.5 的关闭确认弹窗**已等价实现**(`src/app.rs register_close_guard`),
不列入差异。两变体文案(有任务/无任务)已就绪,`has_running_task` 待进度页
agent 接入真实 taskId。

## 2. 布局/度量近似

| 项 | 源值 | GPUI 值 | 原因 |
|---|---|---|---|
| 顶栏水平 padding | `0 clamp(16px, 3vw, 32px)` | `0 24px` | gpui 无 vw/clamp 长度;取区间中值,窗口 900~1100 内偏差 ≤8px |
| 工作区 padding | `clamp(16px, 2.5vw, 32px)` | `24px` | 同上 |
| 左步骤栏宽度 | `clamp(210px, 22vw, 250px)` | `230px` | 同上(1100 窗口下源值 242,最小 900 下源值 210;230 为折中,偏差 ≤12px) |
| Modal maxHeight | `86vh` | `620px` | 无 vh;750 高窗口 86vh≈645,取保守值 |

## 3. 动画降级

- **CSS transition 全部缺失**:gpui 的 hover/active 样式切换是瞬时的,
  源 150ms/100ms 过渡(按钮、瓦片、行 hover、输入框边框)不可复刻;
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
| `title` 悬浮提示 | 源大量使用 `title` 属性(重置按钮"清空所有数据并重新开始"、表格单元格全名、StatusBadge label 等);gpui 无原生 tooltip,当前未实现 |
| 输入框聚焦光晕 | 源 `box-shadow: 0 0 0 3px rgba(255,174,0,0.2)`;gpui-component Input 只换聚焦边框色(已设 amber-500 与源一致),3px 光晕未复刻 |
| 模态遮罩模糊 | 源 `backdrop-filter: blur(4px)`;gpui 遮罩为纯半透明色,无背景模糊 |
| 表格 hover 滚动条 | 源自定义 6px 滚动条(slate-300 thumb);gpui 用系统/组件库默认滚动条 |
| 全局焦点环 | 源 `:focus-visible` amber 500 2px outline;gpui 焦点样式随组件库,未全局复刻 |
| Enter 触发确认 | 已实现(ConfirmModal 卡片 on_key_down);与源的 autoFocus+原生按钮触发等价 |
| 未解锁步骤 tabIndex | 源 tabIndex 0/-1 键盘可达;gpui 无 Tab 序精细控制(组件库统一 Tab 遍历),点击规则一致 |

## 5. 其他

- 源 `localStorage` 持久化(`tag2folders_task_id`)→ GPUI 版计划用 `dirs` 数据目录
  文件,由进度页 agent 实现(PORT_NOTES §6 遗留项);
- 源 `exit_app` 兜底销毁窗口 → GPUI 版统一 `cx.quit()`;
- 字体栈:源为多级回退链,GPUI 版 UI 文本直接 "PingFang SC"、等宽 "Menlo"
  (macOS;已在运行时验证两者可解析,中文无豆腐块)。

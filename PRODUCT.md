# Product

<!-- impeccable:product-schema 1 -->

## Platform

adaptive
（原生桌面 macOS / Windows，GPUI 渲染。非 web/移动端，skill 的 ios/android 原生指引不适用）

## Users

个人音乐爱好者。本地下载/收藏的音乐文件散落各处（文件名混乱、目录无结构），需要在个人电脑上单机完成批量归档；最在意的是"不弄丢、不弄坏源文件"。

## Product Purpose

Tag2Folders 读取音频元数据标签（艺术家/专辑/标题/音轨/年份/流派），按用户模板自动规划目标路径并批量整理。成功 = 用户放心地把整理这件事交给它：预览完全只读、执行前全量预检、失败第一条即停、复制/移动模式明确可控。

## Positioning

纯本地、纯 Rust、单文件可分发的桌面工具：无需安装运行时、不上传任何数据、无遥测。差异化机制是"只读预览 + 全量预检 + 冲突自动消解"的安全整理管线——执行前用户已看到每一份文件的最终去向。

## Operating Context

- 三步向导工作流：扫描 → 模板预览 → 执行整理（1100×750 单窗口）。
- 整理任务在后台线程执行，进度与实时日志经 1s 轮询呈现；task_id 持久化于数据目录（重启可重连 300s 内的终态任务）。
- 分发形态：macOS DMG / Windows MSI（ad-hoc 签名，自用与内部分发；对外分发需 Developer ID 签名公证）。

## Capabilities and Constraints

- 支持格式：mp3 / flac / ogg / m4a / wav / aac / wma / ape / opus（lofty 解析）。
- 命名模板 7 占位符：`{artist}{album}{title}{track}{year}{genre}{ext}`；默认模板 `{album}/{track}. {title}.{ext}`，**默认模式为移动（Move）**——有意的产品决策。
- 安全管线：路径越界校验、Windows 保留名/非法字符清洗、冲突 `_1` 后缀消解、批内碰撞/文件-目录冲突/权限预检（任一失败整批拒绝）。
- 约束：面向用户的错误文案由测试锁定（改文案须同步用例）；amber 主色为历史生效值（具体以 DESIGN.md 为准）；MIT 协议。

## Brand Commitments

- 名称：Tag2Folders（窗口标题 / DMG / MSI 一致）。
- 应用图标：圆形插画风格（`assets/app-icon.png`，各平台打包图标同源）。
- 主色：琥珀 amber（生效值与用法以 DESIGN.md 为唯一权威）。

## Evidence on Hand

- 应用图标源图：`assets/app-icon.png`（1254×1254，透明底）。
- 真实窗口截图取证能力：`T2F_SHOT_*` 环境变量（见 docs/SPEC.md §11）。
- 无真实用户证言/数据；对外文案不得虚构。

## Product Principles

1. 安全第一：任何写入之前，用户已完整看到将要发生什么（预览 + 预检 + 明确模式）。
2. 本地优先：全部计算与存储在本机完成，零网络依赖。
3. 向导式简单：三步完成主流程，高级细节（模板占位符/筛选/目录树）按需浮现。
4. 文案即契约：面向用户的错误/确认文案是产品行为的一部分，锁定且有测试。

## Accessibility & Inclusion

未确立专门标准（原生桌面控件默认可达性）；交互件保持键盘可操作（Enter/Escape 语义见 docs/SPEC.md §7.3）。

## 项目规范

1. 你可以通过 README.md 快速了解项目。涉及项目级别变更，请及时维护 README.md 及其引用到的相关文档。
2. **设计系统规范（DESIGN.md）：**
   - `DESIGN.md` 是项目视觉设计系统（The Amber Workbench）的唯一权威规范，与 `src/ui/theme.rs` 代码严格对齐。
   - **严禁手写 Hex 色值**：所有前端/UI 颜色、圆角、阴影必须使用 `src/ui/theme.rs` 中定义的设计 Token。
   - 涉及样式、组件规格、视觉 Token 或设计规则变更时，必须同步更新 `DESIGN.md` 以及机器可读伴生文件 `.impeccable/design.json`。
   - 严格遵循单一暖色原则（琥珀 Amber 为唯一暖色焦点，slate 灰阶承载骨架，语义色 emerald/rose/sky 仅用于状态表达）。
3. **GPUI 官方文档与组件库规范（gpui-component）：**
   - **优先查阅官方文档**：遇到 GPUI 视图模型、实体生命周期（`Entity<T>`）、订阅监听（`subscribe`/`subscribe_in`）、异步上下文（`spawn_in`/`update_in`）、事件与焦点管理等不确定之处，严禁猜测 API，必须优先查阅官方文档与源码。
   - **纯文本/Markdown 导航源 (llms.txt)**：
     - `gpui-component` LLM 纯文本导航源：[`https://longbridge.github.io/gpui-component/llms.txt`](https://longbridge.github.io/gpui-component/llms.txt)（全量文档源：[`https://longbridge.github.io/gpui-component/llms-full.txt`](https://longbridge.github.io/gpui-component/llms-full.txt) / 中文主页：[`https://longbridge.github.io/gpui-component/zh-CN/docs.md`](https://longbridge.github.io/gpui-component/zh-CN/docs.md)）。
     - GPUI 官方源码与示例：[`https://github.com/zed-industries/zed/tree/main/crates/gpui`](https://github.com/zed-industries/zed/tree/main/crates/gpui) 与 API 手册 [`https://docs.rs/gpui`](https://docs.rs/gpui)。
   - **组件复用加速开发**：涉及高交互控件（如 `Input`、`Checkbox`、`Button`、`Modal/Dialog`、`Dropdown`、`Table`、`VirtualList` 等），优先引用或基于 `gpui-component`（配合 `theme::apply_to_gpui_component` 换肤）构建，避免低效造轮子。
   - **版本约束**：锁定使用 `Cargo.toml` 声明的 crates.io 注册表版本，**严禁混入 Git 依赖的 gpui**（会与 `gpui-component` 产生双版本类型冲突）。

## Git规范

1. 任何修改任务前，请先同步远端最新代码。如发生冲突，rebase 并详细对比双方代码进行语义合并。
2. **代码提交规范：** `<类型>([<范围>]): <中文改动说明>`。英文前缀（如 `feat`、`fix`、`refactor`、`docs`、`style`、`chore`、`perf`、`test`）表示改动大类，范围可选，中文部分写明改动内容和原因，避免过于简略。示例：`feat(api): 新增用户查询接口，支持按邮箱模糊搜索`。
3. 请保留 Git 提交的相对原子性，长任务或分配 subagents 多个子任务时，请按阶段分布提交。务必不要代码都攒到最后再提交。
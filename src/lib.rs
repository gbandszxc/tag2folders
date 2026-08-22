//! Tag2Folders GPUI 版后端库。
//!
//! 与源项目（Tauri 2）的 src-tauri/src/lib.rs 对应，但不再有 tauri Builder：
//! - `core`    业务核心（自源项目 1:1 平移，零改动）
//! - `task`    后台整理任务（去 tauri 事件发射，仅保留快照注册表）
//! - `service` 服务层（原 Tauri 命令层 commands.rs 的纯函数化改写）
//!
//! UI（gpui）只出现在二进制目标 src/main.rs 中，本库保持纯逻辑、可独立测试
//! （`cargo test --lib` 不依赖 gpui 编译产物之外的任何 UI 代码）。

pub mod core;
pub mod service;
pub mod task;

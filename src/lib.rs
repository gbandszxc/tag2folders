//! Tag2Folders 后端库（纯逻辑，无 gpui 依赖）。
//!
//! - `core`    业务核心：metadata / scanner / template / preview / organizer /
//!   path_util / path_security
//! - `task`    后台整理任务（快照注册表）
//! - `service` 服务层（UI 直接调用的纯函数接口）
//!
//! UI（gpui）只出现在二进制目标 src/main.rs 中，本库可独立测试
//! （`cargo test --lib` 不依赖任何 UI 代码）。

pub mod core;
pub mod service;
pub mod task;

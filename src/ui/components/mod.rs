//! 自绘基础组件库(Button/Badge/Card/AlertBar/ProgressBar/StepNav/Modal/ConfirmModal)。
//! 高交互控件(Input/Checkbox 等)不在此处——用 gpui-component 并经
//! [`crate::ui::theme::apply_to_gpui_component`] 换肤。

// 说明:部分组件/导出在当前外壳阶段尚未被引用(三个页面内容由后续 agent 接入),
// 保留全部公开 API,不因 dead_code 告警删减。
#![allow(unused_imports)]

pub mod alert_bar;
pub mod badge;
pub mod button;
pub mod card;
pub mod modal;
pub mod progress_bar;
pub mod step_nav;

pub use alert_bar::{AlertBar, AlertVariant};
pub use badge::{
    BadgeVariant, StatusBadge, StatusBadgeSize, badge, badge_text, mapping_status_str,
    status_config,
};
pub use button::{Button, ButtonSize, ButtonVariant};
pub use card::{Card, CardPadding, card};
pub use modal::{ConfirmModal, ConfirmOptions, ConfirmTone, Modal};
pub use progress_bar::ProgressBar;
pub use step_nav::{StepDef, StepNav, STEPS, step_nav_aside};

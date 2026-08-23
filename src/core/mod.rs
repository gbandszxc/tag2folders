//! 核心业务模块。
//!
//! - `scanner`       目录扫描
//! - `metadata`      音频元数据提取（lofty）
//! - `template`      目标路径模板渲染
//! - `path_security` 路径安全校验
//! - `path_util`     路径工具（规范化/父目录/边界与权限探测）
//! - `organizer`     文件整理（计划 + 执行）
//! - `preview`       预览（映射生成 + 预检）

pub mod metadata;
pub mod organizer;
pub mod path_security;
pub mod path_util;
pub mod preview;
pub mod scanner;
pub mod template;

use serde::{Deserialize, Serialize};

/// 单个音频文件的元数据（snake_case 序列化，UI 层按此消费，勿改字段名）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioMetadata {
    pub path: String,
    pub ext: String,
    pub artist: String,
    pub album: String,
    pub title: String,
    pub track: String,
    pub year: String,
    pub genre: String,
    pub readable: bool,
    #[serde(default)]
    pub error: String,
}

/// 整理模式：移动或复制。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrganizeMode {
    #[serde(rename = "move")]
    Move,
    #[serde(rename = "copy")]
    Copy,
}

/// 预览映射项的执行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingStatus {
    Ok,
    Conflict,
    BatchConflict,
    MissingMetadata,
    Unreadable,
    BoundaryError,
    WriteError,
}

/// 单个文件的 源 → 目标 映射（预览输出 / 整理输入，两端共享）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMappingItem {
    /// 源文件绝对路径
    pub source: String,
    /// 渲染出的原始目标路径（冲突消解前）
    pub target: String,
    /// 计划的最终路径（冲突消解后）
    pub final_target: String,
    /// 相对 target_dir 的显示用相对路径
    pub relative_target: String,
    pub status: MappingStatus,
    /// 预览时目标位置已存在同名文件
    #[serde(default)]
    pub conflict: bool,
    /// 批内目标路径碰撞
    #[serde(default)]
    pub batch_conflict: bool,
}

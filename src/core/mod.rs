//! 核心业务模块：从 Python 后端 1:1 移植。
//!
//! 模块划分与 `backend/core` 保持一致，便于对照溯源：
//! - `scanner`      目录扫描
//! - `metadata`     音频元数据提取（lofty 替代 mutagen）
//! - `template`     目标路径模板渲染
//! - `path_security` 路径安全校验
//! - `path_util`    Python pathlib/os.path 语义的路径工具
//! - `organizer`    文件整理（计划 + 执行）
//! - `preview`      预览（映射生成 + 预检，源自 backend/api/routes/preview.py 与 organize.py）

pub mod metadata;
pub mod organizer;
pub mod path_security;
pub mod path_util;
pub mod preview;
pub mod scanner;
pub mod template;

use serde::{Deserialize, Serialize};

/// 单个音频文件的元数据（对应 Python `AudioMetadata` / 前端 `AudioFileInfo`）。
/// 字段名与序列化形式必须与现有前端 `types.ts` 保持一致（snake_case）。
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

/// 预览映射项的执行状态（序列化值与 Python 端完全一致）。
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

//! 后台整理任务：注册表 + 进度快照。
//!
//! 对应源项目 src-tauri/src/task.rs（自 backend/core/task_manager.py 移植）。
//! GPUI 版差异（桌面化适配）：
//! - 去掉 Tauri 事件 `progress://{task_id}` 发射——源前端本就完全依赖
//!   `get_task_status` 每 1000ms 轮询（见 docs/SOURCE_SPEC.md 5.3），
//!   GPUI 版沿用轮询，快照注册表语义不变；
//! - 后台执行线程沿用 `std::thread::spawn`（源项目即如此，未用 tauri::async_runtime）；
//! - 终态任务 5 分钟后惰性淘汰，防止长期驻留内存。
//!
//! 状态机语义、快照结构、终态保留 300 秒、容量上限 32 与源项目完全一致。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::core::{path_util, organizer, FileMappingItem, OrganizeMode};

/// 终态任务在注册表中的存活时长（与 Python 端 TTL 一致）
const TERMINAL_TASK_TTL: Duration = Duration::from_secs(300);
/// 注册表容量上限：超出时淘汰最旧的终态任务
const MAX_TASKS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Done,
    Error,
}

/// 进度事件（字段与前端 ProgressEvent 接口逐一对齐）
#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub task_id: String,
    pub status: TaskStatus,
    pub current: usize,
    pub total: usize,
    pub current_file: String,
    pub message: String,
}

struct TaskState {
    snapshot: ProgressEvent,
    terminal_at: Option<Instant>,
}

fn registry() -> &'static Mutex<HashMap<String, TaskState>> {
    static REG: OnceLock<Mutex<HashMap<String, TaskState>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 创建任务并注册初始快照，返回 task_id。
pub fn create_task(total: usize) -> String {
    let task_id = uuid::Uuid::new_v4().to_string();
    let event = ProgressEvent {
        task_id: task_id.clone(),
        status: TaskStatus::Pending,
        current: 0,
        total,
        current_file: String::new(),
        message: String::new(),
    };
    if let Ok(mut reg) = registry().lock() {
        // 容量控制：先淘汰过期终态任务，仍满则淘汰最旧终态任务
        evict_expired(&mut reg);
        if reg.len() >= MAX_TASKS {
            if let Some(oldest) = reg
                .iter()
                .filter(|(_, s)| s.terminal_at.is_some())
                .min_by_key(|(_, s)| s.terminal_at)
                .map(|(k, _)| k.clone())
            {
                reg.remove(&oldest);
            }
        }
        reg.insert(
            task_id.clone(),
            TaskState {
                snapshot: event,
                terminal_at: None,
            },
        );
    }
    task_id
}

/// 读取任务快照。终态且超过 TTL 的任务视同不存在（惰性淘汰）。
pub fn get_snapshot(task_id: &str) -> Option<ProgressEvent> {
    let mut reg = registry().lock().ok()?;
    evict_expired(&mut reg);
    reg.get(task_id).map(|s| s.snapshot.clone())
}

fn evict_expired(reg: &mut HashMap<String, TaskState>) {
    let now = Instant::now();
    reg.retain(|_, s| !matches!(s.terminal_at, Some(t) if now.duration_since(t) > TERMINAL_TASK_TTL));
}

/// 更新注册表快照。
/// 源项目同时向 `progress://{task_id}` Tauri 事件通道广播；GPUI 版前端
/// 纯靠 `get_task_status` 轮询消费快照，事件发射已移除（见模块注释）。
fn publish(event: ProgressEvent, terminal_at: Option<Instant>) {
    if let Ok(mut reg) = registry().lock() {
        reg.insert(
            event.task_id.clone(),
            TaskState {
                snapshot: event,
                terminal_at,
            },
        );
    }
}

/// 后台执行器：逐条执行预计划的移动/复制（移植自 _run_organize）。
pub fn run_organize(task_id: String, mappings: Vec<FileMappingItem>, mode: OrganizeMode) {
    let total = mappings.len();

    for (idx, mapping) in mappings.iter().enumerate() {
        let i = idx + 1;
        publish(
            ProgressEvent {
                task_id: task_id.clone(),
                status: TaskStatus::Running,
                current: i - 1,
                total,
                current_file: mapping.source.clone(),
                message: String::new(),
            },
            None,
        );

        // normpath 消化 `..`/`.` 段（字符串级，不碰文件系统），
        // 保留用户大小写意图（避免 resolve 把大小写重命名变原地 no-op）
        let normalized = path_util::normpath_str(&mapping.final_target);
        let result = organizer::organize_file(&mapping.source, &normalized, mode);

        if !result.success {
            publish(
                ProgressEvent {
                    task_id: task_id.clone(),
                    status: TaskStatus::Error,
                    current: i - 1,
                    total,
                    current_file: mapping.source.clone(),
                    message: format!("Failed: {}: {}", mapping.source, result.error_message),
                },
                Some(Instant::now()),
            );
            return;
        }

        publish(
            ProgressEvent {
                task_id: task_id.clone(),
                status: TaskStatus::Running,
                current: i,
                total,
                current_file: mapping.source.clone(),
                message: format!("Processed {i}/{total}"),
            },
            None,
        );
    }

    publish(
        ProgressEvent {
            task_id,
            status: TaskStatus::Done,
            current: total,
            total,
            current_file: String::new(),
            message: format!("Completed {total} file(s)."),
        },
        Some(Instant::now()),
    );
}

/// 辅助：供命令层校验 final_target 规范化后的可执行性（预检内部已覆盖）。
#[allow(dead_code)]
fn assert_normalized(p: &str) -> String {
    let norm = path_util::normpath_str(p);
    debug_assert_eq!(norm, Path::new(&norm).to_string_lossy().into_owned());
    norm
}

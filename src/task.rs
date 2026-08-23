//! 后台整理任务：注册表 + 进度快照。
//!
//! - UI 通过 `get_task_status` 轮询快照消费进度（默认 1s 一次）；
//! - 后台执行线程为 `std::thread::spawn`；
//! - 终态任务 5 分钟（300s）后惰性淘汰，防止长期驻留内存；
//! - 注册表容量上限 32，满时淘汰最旧终态任务。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::core::{path_util, organizer, FileMappingItem, OrganizeMode};

/// 终态任务在注册表中的存活时长
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

/// 进度事件快照。
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

/// 更新注册表快照（UI 轮询 get_task_status 消费）。
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

/// 后台执行器：逐条执行预计划的移动/复制。
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

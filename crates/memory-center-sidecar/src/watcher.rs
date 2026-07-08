//! # 压缩事件检测器（v2.36 新增）
//!
//! 监控 OpenCode `session.time_compacting` 字段变化，检测压缩完成事件。
//!
//! ## 检测原理
//!
//! OpenCode 压缩流程：
//! 1. 压缩开始：`time_compacting` 从 NULL → 时间戳
//! 2. 压缩完成：`time_compacting` 从时间戳 → NULL
//!
//! sidecar 维护 `session_id -> Option<time_compacting>` 状态表，
//! 每次轮询对比变化，检测到"有值 → NULL"时返回该 session ID。
//!
//! ## backfill 模式
//!
//! 启动时若 `--backfill` 为 true，查询所有曾经压缩过的 session
//! （`session_message` 表中 `type='compaction'` 的记录），
//! 全量归档历史压缩会话。

use crate::opencode_db::{OpenCodeDb, SessionInfo};
use std::collections::{HashMap, HashSet};

/// 压缩事件检测器
pub struct CompactionWatcher {
    /// session_id -> 上次已知的 time_compacting 值
    ///
    /// None 表示该 session 未在压缩中（或尚未观测到）
    states: HashMap<String, Option<i64>>,
    /// 已归档过的 session（避免重复归档）
    archived: HashSet<String>,
}

/// 单次轮询检测结果
#[derive(Debug)]
pub struct PollResult {
    /// 新完成压缩的 session ID 列表（需要归档）
    pub newly_compacted: Vec<SessionInfo>,
    /// 正在压缩中的 session 数量
    pub compacting_count: usize,
    /// 总 session 数量
    pub total_sessions: usize,
}

impl CompactionWatcher {
    /// 创建新的检测器
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            archived: HashSet::new(),
        }
    }

    /// 执行一次轮询
    ///
    /// 查询所有 session 的 time_compacting 状态，对比变化，
    /// 返回新完成压缩的 session 列表。
    pub fn poll(&mut self, db: &OpenCodeDb) -> Result<PollResult, crate::opencode_db::DbError> {
        let sessions = db.query_all_compaction_states()?;
        let total = sessions.len();

        let mut newly_compacted = Vec::new();
        let mut compacting_count = 0;

        for session in &sessions {
            let prev = self.states.get(&session.id).copied();

            // 当前状态
            let current = session.time_compacting;

            // 统计正在压缩中的数量
            if current.is_some() {
                compacting_count += 1;
            }

            // 检测"有值 → NULL"变化（压缩完成）
            if let Some(Some(prev_ts)) = prev {
                if current.is_none() && !self.archived.contains(&session.id) {
                    tracing::info!(
                        session_id = %session.id,
                        session_title = %session.title,
                        compaction_started_at = prev_ts,
                        "检测到压缩完成事件"
                    );
                    newly_compacted.push(session.clone());
                    self.archived.insert(session.id.clone());
                }
            }

            // 更新状态
            self.states.insert(session.id.clone(), current);
        }

        Ok(PollResult {
            newly_compacted,
            compacting_count,
            total_sessions: total,
        })
    }

    /// backfill 模式：获取所有曾经压缩过的 session
    ///
    /// 查询 `session_message` 表中 `type='compaction'` 的 session_id，
    /// 排除已归档的，返回需要 backfill 归档的 session 列表。
    pub fn backfill_sessions(
        &mut self,
        db: &OpenCodeDb,
    ) -> Result<Vec<SessionInfo>, crate::opencode_db::DbError> {
        let ever_compacted = db.query_ever_compacted_sessions()?;
        let all_sessions = db.query_all_compaction_states()?;

        let mut result = Vec::new();
        for session in all_sessions {
            if ever_compacted.contains(&session.id) && !self.archived.contains(&session.id) {
                self.archived.insert(session.id.clone());
                result.push(session);
            }
        }

        tracing::info!(
            backfill_count = result.len(),
            total_ever_compacted = ever_compacted.len(),
            "backfill 扫描完成"
        );

        Ok(result)
    }

    /// 标记 session 已归档（手动添加到 archived 集合）
    pub fn mark_archived(&mut self, session_id: &str) {
        self.archived.insert(session_id.to_string());
    }
}

impl Default for CompactionWatcher {
    fn default() -> Self {
        Self::new()
    }
}

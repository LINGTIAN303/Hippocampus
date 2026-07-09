//! # 压缩事件记录（v2.46 新增，从 sidecar/opencode_db.rs 迁入）
//!
//! 通用压缩事件记录，不绑定具体 Agent。
//!
//! ## 设计
//!
//! `CompactionRecord` 原本定义在 `opencode_db.rs`，现提升到 adapter crate，
//! 让 `AgentAdapter::query_compactions()` 返回通用类型，
//! watcher 不再依赖 OpenCode 专属类型。
//!
//! ## 字段语义
//!
//! 不同 Agent 对 `seq` 的语义不同：
//! - OpenCode V2（CLI/TUI）：session_message.seq（整数序列号）
//! - OpenCode V1（桌面端）：time_created（毫秒时间戳，代替 seq）
//! - 未来 ClaudeCode：可能是 JSONL 行号或其他标识
//!
//! sidecar 只要求 seq 可排序、可比较范围，不关心具体语义。

/// 压缩事件记录（通用，不绑定具体 Agent）
#[derive(Debug, Clone)]
pub struct CompactionRecord {
    /// 消息唯一标识（用于去重，不同 Agent 格式不同，如 "msg_xxx"）
    pub message_id: String,
    /// 所属 session ID
    pub session_id: String,
    /// 序列号（用于确定增量归档范围，不同 Agent 语义不同）
    pub seq: i64,
    /// 创建时间戳（毫秒）
    pub time_created: i64,
    /// 压缩原因："auto" 或 "manual"
    pub reason: String,
    /// LLM 生成的压缩摘要（可为空）
    pub summary: String,
    /// 保留的最近上下文（可为空）
    pub recent: String,
}

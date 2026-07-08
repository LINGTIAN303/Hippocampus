//! # OpenCode SQLite 读取模块（v2.36 新增）
//!
//! 从 OpenCode 的会话 SQLite 数据库读取压缩事件和会话消息。
//!
//! ## OpenCode SQLite Schema（来自 sst/opencode 源码）
//!
//! ### session 表
//! - `id` (text PK): Session ID
//! - `title` (text): 会话标题
//! - `time_compacting` (integer|null): 压缩开始时间戳，NULL 表示未在压缩中
//! - `time_created` / `time_updated`: 时间戳
//!
//! ### session_message 表（V2 消息系统）
//! - `id` (text PK): 消息 ID
//! - `session_id` (text FK): 所属 session
//! - `type` (text): 消息类型（user/assistant/system/synthetic/shell/compaction）
//! - `seq` (integer): 序列号
//! - `data` (text JSON): 消息内容（不含 id 和 type）
//! - `time_created` (integer): 创建时间
//!
//! ### message + part 表（V1 消息系统，回退用）
//! - `message.id` / `message.session_id` / `message.data` (JSON)
//! - `part.id` / `part.message_id` / `part.session_id` / `part.data` (JSON)
//!
//! ## 压缩检测策略
//!
//! 监控 `session.time_compacting` 字段变化：
//! - NULL → 有值：压缩开始
//! - 有值 → NULL：压缩完成，触发归档

use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::collections::HashSet;

/// OpenCode SQLite 读取器
pub struct OpenCodeDb {
    conn: Connection,
}

/// Session 基本信息
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub time_compacting: Option<i64>,
}

/// 压缩状态变化检测结果
#[derive(Debug)]
pub struct CompactionChange {
    pub session_id: String,
    pub session_title: String,
    /// 压缩开始时间戳（用于日志）
    pub compaction_started_at: i64,
}

impl OpenCodeDb {
    /// 以只读模式打开 OpenCode SQLite
    ///
    /// 只读模式避免干扰 OpenCode 的写入，WAL 模式支持并发读。
    pub fn open(db_path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Ok(Self { conn })
    }

    /// 查询所有 session 的压缩状态
    ///
    /// 返回 (session_id, title, time_compacting) 列表。
    pub fn query_all_compaction_states(&self) -> Result<Vec<SessionInfo>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, time_compacting FROM session",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SessionInfo {
                id: row.get::<_, String>(0)?,
                title: row.get::<_, String>(1)?,
                time_compacting: row.get::<_, Option<i64>>(2)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// 读取 session 的完整消息并序列化为 full_context 字符串
    ///
    /// 优先从 V2 `session_message` 表读取，回退到 V1 `message`+`part` 表。
    /// 输出格式：`User: ...\n\nAssistant: ...\n\nUser: ...\n...`
    /// （MemoryCenter context_parser 支持 `User:`/`Assistant:` 分隔符格式）
    pub fn read_session_context(
        &self,
        session_id: &str,
        max_turns: usize,
    ) -> Result<String, DbError> {
        // 优先 V2 session_message 表
        let v2_context = self.read_v2_messages(session_id, max_turns)?;
        if !v2_context.is_empty() {
            return Ok(v2_context);
        }

        // 回退 V1 message + part 表
        let v1_context = self.read_v1_messages(session_id, max_turns)?;
        Ok(v1_context)
    }

    /// 从 V2 `session_message` 表读取消息
    ///
    /// 消息类型：user / assistant / system / synthetic / shell / compaction
    /// 按 seq 排序，跳过 compaction 类型（压缩产物不归档）。
    fn read_v2_messages(
        &self,
        session_id: &str,
        max_turns: usize,
    ) -> Result<String, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT type, data FROM session_message
             WHERE session_id = ?1
             ORDER BY seq ASC",
        )?;

        let rows = stmt.query_map([session_id], |row| {
            Ok((
                row.get::<_, String>(0)?, // type
                row.get::<_, String>(1)?, // data (JSON)
            ))
        })?;

        let mut parts = Vec::new();
        let mut turn_count = 0usize;

        for row in rows {
            let (msg_type, data_json) = row?;
            let data: serde_json::Value = serde_json::from_str(&data_json).unwrap_or_default();

            let serialized = serialize_v2_message(&msg_type, &data);
            if serialized.is_empty() {
                continue;
            }

            // 跳过压缩产物（不归档压缩摘要本身）
            if msg_type == "compaction" {
                continue;
            }

            parts.push(serialized);
            turn_count += 1;

            if turn_count >= max_turns {
                parts.push("[... truncated by sidecar max_turns ...]".to_string());
                break;
            }
        }

        Ok(parts.join("\n\n"))
    }

    /// 从 V1 `message` + `part` 表读取消息
    ///
    /// V1 结构：message 表存消息元数据，part 表存消息内容片段。
    fn read_v1_messages(
        &self,
        session_id: &str,
        max_turns: usize,
    ) -> Result<String, DbError> {
        // 查询 message 表
        let mut stmt = self.conn.prepare(
            "SELECT id, data FROM message
             WHERE session_id = ?1
             ORDER BY time_created ASC, id ASC",
        )?;

        let message_ids: Vec<(String, String)> = stmt
            .query_map([session_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        if message_ids.is_empty() {
            return Ok(String::new());
        }

        let mut parts = Vec::new();
        let mut turn_count = 0usize;

        for (msg_id, msg_data_json) in message_ids {
            let msg_data: serde_json::Value =
                serde_json::from_str(&msg_data_json).unwrap_or_default();

            // 查询该消息的 parts
            let part_texts = self.read_v1_parts(&msg_id)?;
            if part_texts.is_empty() {
                continue;
            }

            // V1 message 的 role 通常在 data.role 中
            let role = msg_data
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let role_label = match role {
                "user" => "User",
                "assistant" => "Assistant",
                "system" => "System",
                _ => "User", // 默认按用户处理
            };

            for text in part_texts {
                if !text.is_empty() {
                    parts.push(format!("{}: {}", role_label, text));
                    turn_count += 1;
                    if turn_count >= max_turns {
                        parts.push("[... truncated by sidecar max_turns ...]".to_string());
                        return Ok(parts.join("\n\n"));
                    }
                }
            }
        }

        Ok(parts.join("\n\n"))
    }

    /// 读取 V1 part 表中某消息的所有片段
    fn read_v1_parts(&self, message_id: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT data FROM part
             WHERE message_id = ?1
             ORDER BY id ASC",
        )?;

        let rows = stmt.query_map([message_id], |row| {
            row.get::<_, String>(0)
        })?;

        let mut parts = Vec::new();
        for row in rows {
            let data_json = row?;
            let data: serde_json::Value = serde_json::from_str(&data_json).unwrap_or_default();
            if let Some(text) = data.get("text").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    parts.push(text.to_string());
                }
            }
        }
        Ok(parts)
    }

    /// 获取已归档过的 session 集合（通过查询 time_compacting 已清空的历史记录）
    ///
    /// 用于 backfill 模式：启动时找出所有曾经压缩过的 session。
    pub fn query_ever_compacted_sessions(&self) -> Result<HashSet<String>, DbError> {
        // time_archived 不为空，或 time_compacting 曾经不为空（无法直接查询历史）
        // 简化：查询所有 session_message 中 type='compaction' 的 session_id
        let mut stmt = match self.conn.prepare(
            "SELECT DISTINCT session_id FROM session_message WHERE type = 'compaction'",
        ) {
            Ok(s) => s,
            Err(_) => {
                // session_message 表可能不存在（老版本 OpenCode）
                return Ok(HashSet::new());
            }
        };

        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut result = HashSet::new();
        for row in rows {
            if let Ok(sid) = row {
                result.insert(sid);
            }
        }
        Ok(result)
    }
}

/// 序列化 V2 消息为 `Role: text` 格式
///
/// 对应 OpenCode compaction.ts 中的 `serialize()` 函数（简化版）。
fn serialize_v2_message(msg_type: &str, data: &serde_json::Value) -> String {
    match msg_type {
        "user" => {
            let text = data.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if text.is_empty() {
                return String::new();
            }
            format!("User: {}", text)
        }
        "assistant" => {
            // assistant 消息的 content 是数组，每个 part 有 type
            if let Some(content) = data.get("content").and_then(|v| v.as_array()) {
                let mut parts = Vec::new();
                for part in content {
                    let part_type = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    match part_type {
                        "text" => {
                            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                                if !text.is_empty() {
                                    parts.push(text.to_string());
                                }
                            }
                        }
                        "reasoning" => {
                            // 跳过推理过程（不归档 thinking）
                        }
                        "tool" => {
                            // 工具调用：提取名称和结果
                            let name = part.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                            if let Some(state) = part.get("state") {
                                let status = state.get("status").and_then(|v| v.as_str()).unwrap_or("");
                                if status == "completed" {
                                    if let Some(content_arr) = state.get("content").and_then(|v| v.as_array()) {
                                        let result_text: Vec<String> = content_arr.iter()
                                            .filter_map(|c| c.get("text").and_then(|v| v.as_str()).map(String::from))
                                            .collect();
                                        if !result_text.is_empty() {
                                            let truncated = truncate_str(&result_text.join("\n"), 2000);
                                            parts.push(format!("[Tool: {}] {}", name, truncated));
                                        }
                                    }
                                } else if status == "error" {
                                    if let Some(err) = state.get("error").and_then(|v| v.get("message")).and_then(|v| v.as_str()) {
                                        parts.push(format!("[Tool: {} ERROR] {}", name, err));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if parts.is_empty() {
                    return String::new();
                }
                format!("Assistant: {}", parts.join("\n"))
            } else {
                String::new()
            }
        }
        "system" => {
            let text = data.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if text.is_empty() {
                return String::new();
            }
            format!("System: {}", text)
        }
        "synthetic" | "shell" => {
            // 跳过 synthetic 和 shell 消息（不归档）
            String::new()
        }
        "compaction" => {
            // 压缩产物不归档（跳过）
            String::new()
        }
        _ => String::new(),
    }
}

/// 截断字符串到指定字符数
fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}\n[truncated]", truncated)
    }
}

/// 数据库错误
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("SQLite 错误: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
}

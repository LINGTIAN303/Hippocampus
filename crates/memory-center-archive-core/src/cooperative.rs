//! # Cooperative 协作模式（v2.53 P8 Phase 1）
//!
//! 让 MemoryCenter 从被动归档者升级为主动协作管理者，在 Agent 压缩上下文前
//! 提供保留建议，减少关键信息丢失。
//!
//! ## 核心组件
//!
//! - [`CooperativeHandler`] trait：Agent ↔ MemoryCenter 协作接口
//! - [`CooperativeSession`] + [`CooperativeState`]：6 状态有限状态机
//! - [`PreCompressHintRequest`] / [`RetentionSuggestion`] / [`PostCompressAckRequest`]：请求/响应结构
//! - [`CooperativeError`]：错误类型（含降级语义）
//!
//! ## 状态机
//!
//! ```text
//! Idle → Notified → Analyzing → Suggesting → Awaiting → Compressing → Idle
//! ```
//!
//! 任一状态超时/错误后降级为 Independent（回到 Idle），不阻塞 Agent。
//!
//! ## 设计文档
//!
//! 详见 `docs/cooperative-design.md` 第 7-8 章。

use crate::ArchiveResult;
use crate::MessageTurn;

// ============================================================================
// 错误类型（设计文档第 8.3 节）
// ============================================================================

/// Cooperative 协作错误
///
/// 任一错误均不阻塞 Agent 的压缩操作 ——
/// Agent 收到错误后应降级为 Independent 模式独立压缩 + archive 归档。
#[derive(Debug, thiserror::Error)]
pub enum CooperativeError {
    #[error("会话不存在: {0}")]
    SessionNotFound(String),

    #[error("状态非法: 当前 {current}, 期望 {expected}")]
    InvalidState {
        current: String,
        expected: String,
    },

    #[error("语义检索失败: {0}")]
    SearchFailed(String),

    #[error("归档失败: {0}")]
    ArchiveFailed(String),

    #[error("超时: {0}")]
    Timeout(String),

    #[error("降级: {reason}, 已切换为 Independent 模式")]
    Degraded { reason: String },
}

// ============================================================================
// 状态机（设计文档第 7 章）
// ============================================================================

/// Cooperative 协作状态
///
/// 6 状态有限状态机，设计文档第 7.2 节。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CooperativeState {
    /// 空闲，等待 Agent 请求
    Idle,
    /// 已收到压缩通知（pre_compress_hint 到达）
    Notified,
    /// 正在语义检索 + 分析
    Analyzing,
    /// 已生成建议，准备返回
    Suggesting,
    /// 等待 Agent 执行压缩（建议已返回）
    Awaiting,
    /// 归档被压缩内容（post_compress_ack 到达）
    Compressing,
}

impl CooperativeState {
    /// 转为字符串（用于错误信息 + 日志）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Notified => "notified",
            Self::Analyzing => "analyzing",
            Self::Suggesting => "suggesting",
            Self::Awaiting => "awaiting",
            Self::Compressing => "compressing",
        }
    }

    /// 状态超时阈值（秒），Idle 无超时
    ///
    /// 设计文档第 7.2 节：
    /// - Notified: 5s
    /// - Analyzing: 10s
    /// - Suggesting: 1s
    /// - Awaiting: 120s
    /// - Compressing: 30s
    /// - Idle: 无超时
    pub fn timeout_secs(&self) -> Option<u64> {
        match self {
            Self::Idle => None,
            Self::Notified => Some(5),
            Self::Analyzing => Some(10),
            Self::Suggesting => Some(1),
            Self::Awaiting => Some(120),
            Self::Compressing => Some(30),
        }
    }

    /// 验证状态转换是否合法
    ///
    /// 合法转换（设计文档第 7.1 节）：
    /// - Idle → Notified
    /// - Notified → Analyzing
    /// - Analyzing → Suggesting
    /// - Suggesting → Awaiting
    /// - Awaiting → Compressing
    /// - Compressing → Idle
    ///
    /// 特殊：任何状态 → Idle 都合法（降级路径，包含 Compressing → Idle）
    pub fn can_transition_to(&self, target: CooperativeState) -> bool {
        match (self, target) {
            // 降级路径：任何状态 → Idle（包含 Compressing → Idle 正常归位）
            (_, CooperativeState::Idle) => true,
            // 正常流程（非 Idle 目标）
            (CooperativeState::Idle, CooperativeState::Notified) => true,
            (CooperativeState::Notified, CooperativeState::Analyzing) => true,
            (CooperativeState::Analyzing, CooperativeState::Suggesting) => true,
            (CooperativeState::Suggesting, CooperativeState::Awaiting) => true,
            (CooperativeState::Awaiting, CooperativeState::Compressing) => true,
            // 其他转换非法
            _ => false,
        }
    }
}

impl std::fmt::Display for CooperativeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ============================================================================
// CooperativeSession：协作会话（状态机实例）
// ============================================================================

/// Cooperative 协作会话
///
/// 维护单个 Agent 与 MemoryCenter 协作的完整状态。
/// 通过 `session_id` 隔离不同 Agent 会话。
///
/// 生命周期：从 Agent 首次请求协作到会话结束（降级或归档完成）。
#[derive(Debug, Clone)]
pub struct CooperativeSession {
    /// 会话 ID（与 Agent 的 session_id 对齐）
    pub session_id: String,
    /// 当前状态
    pub state: CooperativeState,
    /// 进入当前状态的时间（用于超时检查）
    pub entered_at: chrono::DateTime<chrono::Utc>,
    /// 当前建议 ID（Notified → Compressing 期间有效）
    pub suggestion_id: Option<String>,
    /// 降级原因（若已降级，记录原因供日志/监控查询）
    pub degraded_reason: Option<String>,
}

impl CooperativeSession {
    /// 创建新的协作会话（初始状态 Idle）
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            state: CooperativeState::Idle,
            entered_at: chrono::Utc::now(),
            suggestion_id: None,
            degraded_reason: None,
        }
    }

    /// 状态转换（验证合法性）
    ///
    /// 非法转换返回 [`CooperativeError::InvalidState`]。
    /// 转换成功后更新 `entered_at` 为当前时间。
    pub fn transition_to(&mut self, target: CooperativeState) -> Result<(), CooperativeError> {
        if !self.state.can_transition_to(target) {
            return Err(CooperativeError::InvalidState {
                current: self.state.to_string(),
                expected: target.to_string(),
            });
        }
        self.state = target;
        self.entered_at = chrono::Utc::now();
        // 进入 Idle 时清理 suggestion_id（会话归位/降级）
        if target == CooperativeState::Idle {
            self.suggestion_id = None;
        }
        Ok(())
    }

    /// 检查是否超时
    ///
    /// 返回 `Some(超时秒数)` 表示已超时，`None` 表示未超时。
    pub fn check_timeout(&self, now: chrono::DateTime<chrono::Utc>) -> Option<u64> {
        let timeout_secs = self.state.timeout_secs()?;
        let elapsed = (now - self.entered_at).num_seconds();
        if elapsed > timeout_secs as i64 {
            Some(timeout_secs)
        } else {
            None
        }
    }

    /// 降级为 Independent 模式
    ///
    /// 重置状态为 Idle，记录降级原因。
    /// 返回 [`CooperativeError::Degraded`] 供调用方传播或记录。
    pub fn degrade(&mut self, reason: impl Into<String>) -> CooperativeError {
        let reason = reason.into();
        tracing::warn!(
            session = %self.session_id,
            from_state = %self.state,
            reason = %reason,
            "Cooperative 会话降级为 Independent 模式"
        );
        self.state = CooperativeState::Idle;
        self.entered_at = chrono::Utc::now();
        self.suggestion_id = None;
        self.degraded_reason = Some(reason.clone());
        CooperativeError::Degraded { reason }
    }

    /// 是否已降级
    pub fn is_degraded(&self) -> bool {
        self.degraded_reason.is_some()
    }

    /// 生成新的 suggestion_id
    ///
    /// 在 Suggesting 阶段调用，生成唯一建议 ID。
    /// 格式：`sugg-YYYYMMDD-<8位uuid>`
    pub fn generate_suggestion_id(&mut self) -> String {
        let id = format!(
            "sugg-{}-{}",
            chrono::Utc::now().format("%Y%m%d"),
            &uuid::Uuid::new_v4().to_string()[..8]
        );
        self.suggestion_id = Some(id.clone());
        id
    }
}

// ============================================================================
// 请求 / 响应结构（设计文档第 8.2 节）
// ============================================================================

// ---- 交互 1：pre_compress_hint 请求 ----

/// 压缩前通知请求
///
/// Agent 在执行压缩前调用，MemoryCenter 返回 [`RetentionSuggestion`]。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PreCompressHintRequest {
    pub session_id: String,
    pub current_tokens: usize,
    pub token_threshold: usize,
    pub compression_scheme: String,
    pub context_snapshot: ContextSnapshot,
    /// 待压缩轮次预览（可选，空表示 Agent 未提供）
    #[serde(default)]
    pub turns_to_compress: Vec<TurnPreview>,
}

/// 上下文快照
///
/// Agent 提供给 MemoryCenter 的当前上下文摘要，作为语义检索的 query。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct ContextSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_turns_summary: Option<String>,
    #[serde(default)]
    pub key_files: Vec<String>,
    #[serde(default)]
    pub tool_calls_summary: Vec<String>,
}

/// 轮次预览
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct TurnPreview {
    pub turn_id: String,
    pub text_preview: String,
    pub token_count: usize,
}

// ---- 交互 2：RetentionSuggestion 响应 ----

/// 保留建议
///
/// MemoryCenter 反向提供给 Agent 的保留建议（三段式）：
/// 1. `retain_turns`：建议保留的轮次
/// 2. `prune_hints`：可安全修剪的内容
/// 3. `inject_memories`：建议注入上下文的已归档记忆
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetentionSuggestion {
    pub session_id: String,
    pub suggestion_id: String,
    pub retain_turns: Vec<RetainItem>,
    pub prune_hints: Vec<PruneHint>,
    pub inject_memories: Vec<InjectItem>,
}

/// 保留项
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetainItem {
    pub turn_id: String,
    pub priority: Priority,
    pub reason: String,
    /// 关联的已归档记忆 hook_id 列表
    #[serde(default)]
    pub related_memories: Vec<String>,
}

/// 优先级
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    High,
    Medium,
    Low,
}

/// 修剪建议
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PruneHint {
    /// 修剪目标（turn_id 或 "tool_results" 等类别）
    pub target: String,
    pub reason: String,
}

/// 注入项
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InjectItem {
    pub hook_id: String,
    pub reason: String,
    pub inject_strategy: InjectStrategy,
}

/// 注入策略
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InjectStrategy {
    /// 注入摘要
    Summary,
    /// 注入完整内容
    Full,
    /// 注入关键词
    Keywords,
}

// ---- 交互 3：post_compress_ack 请求 ----

/// 压缩后确认请求
///
/// Agent 在执行压缩后调用，归档被压缩内容并记录建议采纳率。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PostCompressAckRequest {
    pub session_id: String,
    pub suggestion_id: String,
    pub suggestion_adopted: SuggestionAdoption,
    /// 被压缩的轮次（复用现有 archive 链路归档）
    #[serde(default)]
    pub archived_turns: Vec<MessageTurn>,
}

/// 建议采纳记录
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct SuggestionAdoption {
    /// 已保留的 turn_id 列表
    #[serde(default)]
    pub retained: Vec<String>,
    /// 已修剪的 turn_id 列表
    #[serde(default)]
    pub pruned: Vec<String>,
    /// 已注入的 hook_id 列表
    #[serde(default)]
    pub injected: Vec<String>,
}

// ============================================================================
// CooperativeHandler trait（设计文档第 8.1 节）
// ============================================================================

/// Cooperative 协作处理器
///
/// 由 MemoryCenter 实现，Agent 通过 MCP/HTTP 调用。
/// Independent 模式下此 trait 不被调用。
///
/// ## 方法
///
/// - [`pre_compress_hint`](CooperativeHandler::pre_compress_hint)：压缩前通知 + 获取保留建议
/// - [`post_compress_ack`](CooperativeHandler::post_compress_ack)：压缩后确认 + 归档
/// - [`get_session_state`](CooperativeHandler::get_session_state)：查询协作状态（调试/监控）
///
/// ## 容错原则
///
/// 任何方法的失败都不应阻塞 Agent 的压缩操作。
/// Agent 收到错误后应降级为 Independent 模式独立压缩 + archive 归档。
#[async_trait::async_trait]
pub trait CooperativeHandler: Send + Sync {
    /// 压缩前通知 + 获取保留建议
    ///
    /// Agent 在执行压缩前调用，MemoryCenter 返回保留建议。
    /// 超时或失败时，Agent 应降级为 Independent 模式独立压缩。
    async fn pre_compress_hint(
        &self,
        request: PreCompressHintRequest,
    ) -> Result<RetentionSuggestion, CooperativeError>;

    /// 压缩后确认 + 归档
    ///
    /// Agent 在执行压缩后调用，归档被压缩内容并记录建议采纳率。
    /// 复用现有 archive 链路。
    async fn post_compress_ack(
        &self,
        request: PostCompressAckRequest,
    ) -> Result<ArchiveResult, CooperativeError>;

    /// 查询协作状态
    ///
    /// 返回当前 CooperativeSession 的状态（用于调试/监控）。
    async fn get_session_state(
        &self,
        session_id: &str,
    ) -> Result<CooperativeSession, CooperativeError>;
}

// ============================================================================
// CooperativeService：CooperativeHandler 默认实现
// ============================================================================

use std::collections::HashMap;
use std::sync::Mutex;

/// Cooperative 协作服务
///
/// `CooperativeHandler` trait 的默认实现，持有 `ArchiveEngine` + `RetentionBuilder`。
/// 由 MCP/HTTP 层直接调用，不通过 `ArchiveEngine` 中转。
///
/// ## 会话管理
///
/// - 通过 `Mutex<HashMap<String, CooperativeSession>>` 维护会话状态
/// - session_id 隔离不同 Agent 会话
/// - 状态非法时自动 degrade 重新开始（不阻塞 Agent）
///
/// ## 容错原则
///
/// - `pre_compress_hint` 失败 → Agent 降级为 Independent 模式
/// - `post_compress_ack` 归档失败 → 返回错误，Agent 可重试或降级
/// - 状态不一致时自动降级，确保不阻塞 Agent 的压缩操作
pub struct CooperativeService {
    /// 归档引擎（用于 post_compress_ack 归档）
    archive_engine: crate::ArchiveEngine,
    /// 保留建议构建器（用于 pre_compress_hint 语义检索）
    retention_builder: crate::retention::RetentionBuilder,
    /// 会话状态表（session_id → CooperativeSession）
    sessions: Mutex<HashMap<String, CooperativeSession>>,
}

impl CooperativeService {
    /// 创建新的 Cooperative 协作服务
    pub fn new(
        archive_engine: crate::ArchiveEngine,
        session_search: std::sync::Arc<memory_center_search::SessionSearchRouter>,
    ) -> Self {
        let retention_builder = crate::retention::RetentionBuilder::new(session_search);
        Self {
            archive_engine,
            retention_builder,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// 获取或创建会话（克隆副本，避免持锁跨 await）
    fn get_or_create_session(&self, session_id: &str) -> CooperativeSession {
        let mut sessions = self.sessions.lock().expect("sessions mutex poisoned");
        sessions
            .entry(session_id.to_string())
            .or_insert_with(|| CooperativeSession::new(session_id))
            .clone()
    }

    /// 更新会话状态
    fn update_session(&self, session_id: &str, session: CooperativeSession) {
        let mut sessions = self.sessions.lock().expect("sessions mutex poisoned");
        sessions.insert(session_id.to_string(), session);
    }
}

#[async_trait::async_trait]
impl CooperativeHandler for CooperativeService {
    async fn pre_compress_hint(
        &self,
        request: PreCompressHintRequest,
    ) -> Result<RetentionSuggestion, CooperativeError> {
        let mut session = self.get_or_create_session(&request.session_id);

        // 状态非法时自动降级重新开始（不阻塞 Agent）
        if session.state != CooperativeState::Idle {
            let reason = format!(
                "pre_compress_hint 时状态非 Idle（当前 {}），自动降级重新开始",
                session.state
            );
            tracing::warn!(
                session = %request.session_id,
                reason = %reason,
                "状态不一致，降级处理"
            );
            session.degrade(reason);
        }

        // 状态转换：Idle → Notified → Analyzing
        session.transition_to(CooperativeState::Notified)?;
        session.transition_to(CooperativeState::Analyzing)?;

        // 语义检索 + 生成建议（RetentionBuilder 内部处理 Embedder 降级）
        let suggestion = self
            .retention_builder
            .generate_suggestion(&mut session, &request)
            .await?;

        // 状态转换：Analyzing → Suggesting → Awaiting
        session.transition_to(CooperativeState::Suggesting)?;
        session.transition_to(CooperativeState::Awaiting)?;

        self.update_session(&request.session_id, session);

        Ok(suggestion)
    }

    async fn post_compress_ack(
        &self,
        request: PostCompressAckRequest,
    ) -> Result<crate::ArchiveResult, CooperativeError> {
        let mut session = self.get_or_create_session(&request.session_id);

        // 状态转换：Awaiting → Compressing
        // 若状态非法，降级后仍继续归档（不阻塞 Agent 的压缩操作）
        if session.state != CooperativeState::Awaiting {
            let reason = format!(
                "post_compress_ack 时状态非 Awaiting（当前 {}），降级后继续归档",
                session.state
            );
            tracing::warn!(
                session = %request.session_id,
                reason = %reason,
                "状态不一致，降级处理但仍归档"
            );
            session.degrade(reason);
        } else {
            session.transition_to(CooperativeState::Compressing)?;
        }

        // 归档被压缩内容（复用现有 archive 链路）
        let archive_result = if request.archived_turns.is_empty() {
            tracing::info!(
                session = %request.session_id,
                suggestion_id = %request.suggestion_id,
                "post_compress_ack 无归档内容，仅确认建议采纳"
            );
            return Err(CooperativeError::ArchiveFailed(
                "archived_turns 为空，无内容归档".to_string(),
            ));
        } else {
            self.archive_engine
                .archive(
                    &request.session_id,
                    request.archived_turns,
                    None,
                    None,
                )
                .await
                .map_err(|e| CooperativeError::ArchiveFailed(e.to_string()))?
        };

        // 状态转换：Compressing → Idle（降级路径已是 Idle，跳过转换）
        if session.state == CooperativeState::Compressing {
            session.transition_to(CooperativeState::Idle)?;
        }

        self.update_session(&request.session_id, session);

        Ok(archive_result)
    }

    async fn get_session_state(
        &self,
        session_id: &str,
    ) -> Result<CooperativeSession, CooperativeError> {
        Ok(self.get_or_create_session(session_id))
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 状态机正常流程测试 ----

    #[test]
    fn test_state_machine_happy_path() {
        // 完整正常流程：
        // Idle → Notified → Analyzing → Suggesting → Awaiting → Compressing → Idle
        let mut session = CooperativeSession::new("test-session");
        assert_eq!(session.state, CooperativeState::Idle);

        session.transition_to(CooperativeState::Notified).unwrap();
        session.transition_to(CooperativeState::Analyzing).unwrap();
        session.transition_to(CooperativeState::Suggesting).unwrap();
        session.transition_to(CooperativeState::Awaiting).unwrap();
        session.transition_to(CooperativeState::Compressing).unwrap();
        session.transition_to(CooperativeState::Idle).unwrap();
    }

    #[test]
    fn test_state_machine_invalid_transition() {
        let mut session = CooperativeSession::new("test-session");
        // Idle → Analyzing 非法（应先到 Notified）
        let err = session.transition_to(CooperativeState::Analyzing).unwrap_err();
        assert!(
            matches!(
                err,
                CooperativeError::InvalidState {
                    ref current,
                    ref expected
                } if current == "idle" && expected == "analyzing"
            ),
            "期望 InvalidState 错误，实际: {err:?}"
        );
        // 状态保持不变
        assert_eq!(session.state, CooperativeState::Idle);
    }

    #[test]
    fn test_state_machine_degrade_from_any_state() {
        // 任何状态都可以降级到 Idle
        for state in [
            CooperativeState::Notified,
            CooperativeState::Analyzing,
            CooperativeState::Suggesting,
            CooperativeState::Awaiting,
            CooperativeState::Compressing,
        ] {
            let mut session = CooperativeSession::new("test-session");
            session.state = state;
            session.transition_to(CooperativeState::Idle).unwrap();
            assert_eq!(session.state, CooperativeState::Idle);
        }
    }

    #[test]
    fn test_can_transition_to_all_valid_paths() {
        // 验证所有合法转换
        assert!(CooperativeState::Idle.can_transition_to(CooperativeState::Notified));
        assert!(CooperativeState::Notified.can_transition_to(CooperativeState::Analyzing));
        assert!(CooperativeState::Analyzing.can_transition_to(CooperativeState::Suggesting));
        assert!(CooperativeState::Suggesting.can_transition_to(CooperativeState::Awaiting));
        assert!(CooperativeState::Awaiting.can_transition_to(CooperativeState::Compressing));
        assert!(CooperativeState::Compressing.can_transition_to(CooperativeState::Idle));
    }

    #[test]
    fn test_can_transition_to_invalid_paths() {
        // 验证典型非法转换
        assert!(!CooperativeState::Idle.can_transition_to(CooperativeState::Analyzing));
        assert!(!CooperativeState::Idle.can_transition_to(CooperativeState::Awaiting));
        assert!(!CooperativeState::Idle.can_transition_to(CooperativeState::Compressing));
        assert!(!CooperativeState::Notified.can_transition_to(CooperativeState::Awaiting));
        assert!(!CooperativeState::Notified.can_transition_to(CooperativeState::Compressing));
        assert!(!CooperativeState::Analyzing.can_transition_to(CooperativeState::Awaiting));
        // 自转换也非法（除 Idle→Idle 降级路径外，但 Idle→Idle 无意义）
        assert!(!CooperativeState::Notified.can_transition_to(CooperativeState::Notified));
    }

    // ---- 超时测试 ----

    #[test]
    fn test_state_timeout_secs() {
        assert_eq!(CooperativeState::Idle.timeout_secs(), None);
        assert_eq!(CooperativeState::Notified.timeout_secs(), Some(5));
        assert_eq!(CooperativeState::Analyzing.timeout_secs(), Some(10));
        assert_eq!(CooperativeState::Suggesting.timeout_secs(), Some(1));
        assert_eq!(CooperativeState::Awaiting.timeout_secs(), Some(120));
        assert_eq!(CooperativeState::Compressing.timeout_secs(), Some(30));
    }

    #[test]
    fn test_check_timeout_not_yet() {
        let mut session = CooperativeSession::new("test-session");
        session.transition_to(CooperativeState::Notified).unwrap();
        // 刚进入，未超时
        let now = chrono::Utc::now();
        assert_eq!(session.check_timeout(now), None);
    }

    #[test]
    fn test_check_timeout_expired() {
        let mut session = CooperativeSession::new("test-session");
        // 模拟 6 秒前进入 Notified（超时 5s）
        session.state = CooperativeState::Notified;
        session.entered_at = chrono::Utc::now() - chrono::Duration::seconds(6);
        let now = chrono::Utc::now();
        assert_eq!(session.check_timeout(now), Some(5));
    }

    #[test]
    fn test_check_timeout_idle_never_expires() {
        let mut session = CooperativeSession::new("test-session");
        session.entered_at = chrono::Utc::now() - chrono::Duration::hours(1);
        let now = chrono::Utc::now();
        assert_eq!(session.check_timeout(now), None);
    }

    // ---- 降级测试 ----

    #[test]
    fn test_degrade_records_reason() {
        let mut session = CooperativeSession::new("test-session");
        session.transition_to(CooperativeState::Notified).unwrap();
        session.transition_to(CooperativeState::Analyzing).unwrap();
        let err = session.degrade("Embedder API 不可用");
        assert!(
            matches!(
                err,
                CooperativeError::Degraded { ref reason } if reason == "Embedder API 不可用"
            ),
            "期望 Degraded 错误，实际: {err:?}"
        );
        assert_eq!(session.state, CooperativeState::Idle);
        assert!(session.is_degraded());
        assert_eq!(session.degraded_reason.as_deref(), Some("Embedder API 不可用"));
    }

    #[test]
    fn test_degrade_clears_suggestion_id() {
        let mut session = CooperativeSession::new("test-session");
        session.transition_to(CooperativeState::Notified).unwrap();
        session.generate_suggestion_id();
        assert!(session.suggestion_id.is_some());
        session.degrade("测试降级");
        assert!(session.suggestion_id.is_none());
    }

    // ---- suggestion_id 生成测试 ----

    #[test]
    fn test_generate_suggestion_id_format() {
        let mut session = CooperativeSession::new("test-session");
        let id = session.generate_suggestion_id();
        assert!(
            id.starts_with("sugg-"),
            "suggestion_id 应以 'sugg-' 开头，实际: {id}"
        );
        assert!(session.suggestion_id.is_some());
        assert_eq!(session.suggestion_id.as_deref(), Some(id.as_str()));
    }

    #[test]
    fn test_transition_to_idle_clears_suggestion_id() {
        let mut session = CooperativeSession::new("test-session");
        session.transition_to(CooperativeState::Notified).unwrap();
        session.generate_suggestion_id();
        assert!(session.suggestion_id.is_some());
        // 正常流程走完到 Idle 也会清理
        session.transition_to(CooperativeState::Analyzing).unwrap();
        session.transition_to(CooperativeState::Suggesting).unwrap();
        session.transition_to(CooperativeState::Awaiting).unwrap();
        session.transition_to(CooperativeState::Compressing).unwrap();
        session.transition_to(CooperativeState::Idle).unwrap();
        assert!(session.suggestion_id.is_none());
    }

    // ---- 序列化测试 ----

    #[test]
    fn test_priority_serde_lowercase() {
        let json = serde_json::to_string(&Priority::High).unwrap();
        assert_eq!(json, "\"high\"");
        let p: Priority = serde_json::from_str("\"medium\"").unwrap();
        assert_eq!(p, Priority::Medium);
        let p: Priority = serde_json::from_str("\"low\"").unwrap();
        assert_eq!(p, Priority::Low);
    }

    #[test]
    fn test_cooperative_state_serde_snake_case() {
        let json = serde_json::to_string(&CooperativeState::Awaiting).unwrap();
        assert_eq!(json, "\"awaiting\"");
        let s: CooperativeState = serde_json::from_str("\"compressing\"").unwrap();
        assert_eq!(s, CooperativeState::Compressing);
    }

    #[test]
    fn test_inject_strategy_serde_snake_case() {
        let json = serde_json::to_string(&InjectStrategy::Full).unwrap();
        assert_eq!(json, "\"full\"");
        let s: InjectStrategy = serde_json::from_str("\"keywords\"").unwrap();
        assert_eq!(s, InjectStrategy::Keywords);
        let s: InjectStrategy = serde_json::from_str("\"summary\"").unwrap();
        assert_eq!(s, InjectStrategy::Summary);
    }

    #[test]
    fn test_pre_compress_hint_request_round_trip() {
        let req = PreCompressHintRequest {
            session_id: "s1".to_string(),
            current_tokens: 100,
            token_threshold: 200,
            compression_scheme: "ClaudeCodeCompact".to_string(),
            context_snapshot: ContextSnapshot {
                current_task: Some("写设计文档".to_string()),
                recent_turns_summary: None,
                key_files: vec!["a.rs".to_string()],
                tool_calls_summary: vec![],
            },
            turns_to_compress: vec![TurnPreview {
                turn_id: "t1".to_string(),
                text_preview: "预览".to_string(),
                token_count: 50,
            }],
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: PreCompressHintRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_id, "s1");
        assert_eq!(back.current_tokens, 100);
        assert_eq!(back.token_threshold, 200);
        assert_eq!(back.turns_to_compress.len(), 1);
        assert_eq!(back.context_snapshot.current_task.as_deref(), Some("写设计文档"));
    }

    #[test]
    fn test_pre_compress_hint_request_default_empty_turns() {
        // turns_to_compress 应支持默认空
        let json = r#"{"session_id":"s","current_tokens":0,"token_threshold":0,"compression_scheme":"x","context_snapshot":{}}"#;
        let req: PreCompressHintRequest = serde_json::from_str(json).unwrap();
        assert!(req.turns_to_compress.is_empty());
    }

    #[test]
    fn test_retention_suggestion_serialize() {
        let sugg = RetentionSuggestion {
            session_id: "s1".to_string(),
            suggestion_id: "sugg-001".to_string(),
            retain_turns: vec![RetainItem {
                turn_id: "t1".to_string(),
                priority: Priority::High,
                reason: "关键决策".to_string(),
                related_memories: vec!["hook-1".to_string()],
            }],
            prune_hints: vec![PruneHint {
                target: "tool_results".to_string(),
                reason: "可安全修剪".to_string(),
            }],
            inject_memories: vec![InjectItem {
                hook_id: "h1".to_string(),
                reason: "相关历史记忆".to_string(),
                inject_strategy: InjectStrategy::Summary,
            }],
        };
        let json = serde_json::to_string(&sugg).unwrap();
        assert!(json.contains("\"priority\":\"high\""));
        assert!(json.contains("\"inject_strategy\":\"summary\""));
        assert!(json.contains("\"suggestion_id\":\"sugg-001\""));
    }

    #[test]
    fn test_suggestion_adoption_default_empty() {
        let adoption = SuggestionAdoption::default();
        assert!(adoption.retained.is_empty());
        assert!(adoption.pruned.is_empty());
        assert!(adoption.injected.is_empty());
    }

    #[test]
    fn test_context_snapshot_default_empty() {
        let snap = ContextSnapshot::default();
        assert!(snap.current_task.is_none());
        assert!(snap.recent_turns_summary.is_none());
        assert!(snap.key_files.is_empty());
        assert!(snap.tool_calls_summary.is_empty());
    }

    // ---- 错误显示测试 ----

    #[test]
    fn test_cooperative_error_display() {
        let err = CooperativeError::SessionNotFound("s1".to_string());
        assert!(err.to_string().contains("会话不存在"));

        let err = CooperativeError::InvalidState {
            current: "idle".to_string(),
            expected: "notified".to_string(),
        };
        assert!(err.to_string().contains("状态非法"));
        assert!(err.to_string().contains("idle"));
        assert!(err.to_string().contains("notified"));

        let err = CooperativeError::Degraded {
            reason: "超时".to_string(),
        };
        assert!(err.to_string().contains("降级"));
        assert!(err.to_string().contains("Independent"));
    }

    #[test]
    fn test_cooperative_state_display() {
        assert_eq!(CooperativeState::Idle.to_string(), "idle");
        assert_eq!(CooperativeState::Notified.to_string(), "notified");
        assert_eq!(CooperativeState::Analyzing.to_string(), "analyzing");
        assert_eq!(CooperativeState::Suggesting.to_string(), "suggesting");
        assert_eq!(CooperativeState::Awaiting.to_string(), "awaiting");
        assert_eq!(CooperativeState::Compressing.to_string(), "compressing");
    }
}

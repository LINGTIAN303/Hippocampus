//! # 保留建议构建器（v2.53 P8 Phase 2）
//!
//! 实现 Cooperative 模式的语义检索 + 保留建议生成逻辑。
//!
//! ## 核心流程（设计文档第 9 章）
//!
//! ```text
//! ContextSnapshot → build_search_query → SessionSearchRouter 检索
//!     → SearchHit 列表 → 生成 RetentionSuggestion
//!     (retain_turns + prune_hints + inject_memories)
//! ```
//!
//! ## 降级策略（设计文档第 9.5 节）
//!
//! - Embedder 可用 → BM25 + 向量检索融合排序（Hybrid）
//! - Embedder 不可用 → 仅 BM25 关键词检索（KeywordOnly）
//! - 两者都不可用 → 返回空建议（Agent 独立压缩）
//!
//! `SessionSearchRouter::search_with_rebuild` 内部已处理降级，此处只需处理检索失败。

use std::sync::Arc;

use memory_center_core::semantic::SearchHit;
use memory_center_search::SessionSearchRouter;

use crate::cooperative::{
    CooperativeError, CooperativeSession, ContextSnapshot, InjectItem, InjectStrategy,
    PreCompressHintRequest, Priority, PruneHint, RetainItem, RetentionSuggestion, TurnPreview,
};

// ============================================================================
// query 构建（设计文档第 9.2 节）
// ============================================================================

/// 从上下文快照构建检索 query
///
/// 拼接 `current_task` + `recent_turns_summary` + `key_files`，
/// 作为 BM25 + 向量检索的 query。
pub fn build_search_query(snapshot: &ContextSnapshot) -> String {
    let mut parts = Vec::new();

    if let Some(task) = &snapshot.current_task {
        if !task.is_empty() {
            parts.push(task.clone());
        }
    }
    if let Some(summary) = &snapshot.recent_turns_summary {
        if !summary.is_empty() {
            parts.push(summary.clone());
        }
    }
    for file in &snapshot.key_files {
        if !file.is_empty() {
            parts.push(file.clone());
        }
    }

    parts.join(" ")
}

// ============================================================================
// RetentionBuilder：保留建议构建器
// ============================================================================

/// 保留建议构建器（设计文档第 9 章）
///
/// 持有 `SessionSearchRouter` 引用，执行语义检索并生成 `RetentionSuggestion`。
///
/// ## 配置项（设计文档第 14.4 节）
///
/// - `search_top_k`：语义检索返回的最大条数（默认 5）
/// - `max_retain_items`：保留建议最大项数（默认 10）
/// - `score_threshold_full`：注入完整内容的分数阈值（默认 0.7）
/// - `score_threshold_summary`：注入摘要的分数阈值（默认 0.4）
pub struct RetentionBuilder {
    session_search: Arc<SessionSearchRouter>,
    /// 语义检索返回的最大条数
    search_top_k: usize,
    /// 保留建议最大项数
    max_retain_items: usize,
    /// 注入完整内容的分数阈值
    score_threshold_full: f32,
    /// 注入摘要的分数阈值
    score_threshold_summary: f32,
    /// 高优先级的 token_count 阈值
    high_priority_tokens: usize,
    /// 中优先级的 token_count 阈值
    medium_priority_tokens: usize,
}

impl RetentionBuilder {
    /// 创建新的保留建议构建器
    pub fn new(session_search: Arc<SessionSearchRouter>) -> Self {
        Self {
            session_search,
            search_top_k: 5,
            max_retain_items: 10,
            score_threshold_full: 0.7,
            score_threshold_summary: 0.4,
            high_priority_tokens: 1000,
            medium_priority_tokens: 500,
        }
    }

    /// 设置语义检索 top_k
    pub fn with_search_top_k(mut self, k: usize) -> Self {
        self.search_top_k = k;
        self
    }

    /// 设置保留建议最大项数
    pub fn with_max_retain_items(mut self, max: usize) -> Self {
        self.max_retain_items = max;
        self
    }

    /// 是否配置了向量检索能力（用于判断降级状态）
    pub fn has_embedder(&self) -> bool {
        self.session_search.has_embedder()
    }

    /// 生成保留建议（设计文档第 9.1 节检索流程）
    ///
    /// # 流程
    ///
    /// 1. 从 `context_snapshot` 构建 query
    /// 2. 调用 `SessionSearchRouter::search_with_rebuild` 语义检索
    /// 3. 根据 `turns_to_compress` + 检索结果生成三段式建议
    ///
    /// # 降级
    ///
    /// - query 为空 → 跳过检索，返回空建议
    /// - 检索失败 → 返回 `CooperativeError::SearchFailed`
    /// - Embedder 不可用 → `search_with_rebuild` 内部降级为 BM25
    pub async fn generate_suggestion(
        &self,
        session: &mut CooperativeSession,
        request: &PreCompressHintRequest,
    ) -> Result<RetentionSuggestion, CooperativeError> {
        // 1. 构建 query
        let query = build_search_query(&request.context_snapshot);

        // 2. 语义检索（query 为空时跳过）
        let hits: Vec<SearchHit> = if query.trim().is_empty() {
            tracing::debug!(
                session = %request.session_id,
                "query 为空，跳过语义检索"
            );
            Vec::new()
        } else {
            tracing::debug!(
                session = %request.session_id,
                query = %query,
                top_k = self.search_top_k,
                has_embedder = self.has_embedder(),
                "执行语义检索"
            );
            self.session_search
                .search_with_rebuild(
                    &request.session_id,
                    None,
                    &query,
                    self.search_top_k,
                )
                .await
                .map_err(|e| {
                    tracing::warn!(
                        session = %request.session_id,
                        error = %e,
                        "语义检索失败，降级为空建议"
                    );
                    CooperativeError::SearchFailed(e.to_string())
                })?
        };

        // 3. 生成 suggestion_id（在 Suggesting 阶段）
        let suggestion_id = session.generate_suggestion_id();

        // 4. 构建三段式建议
        let retain_turns = self.build_retain_items(&request.turns_to_compress, &hits);
        let prune_hints = Self::build_prune_hints();
        let inject_memories = self.build_inject_items(&hits);

        tracing::info!(
            session = %request.session_id,
            suggestion_id = %suggestion_id,
            query_len = query.len(),
            hits_count = hits.len(),
            retain_count = retain_turns.len(),
            inject_count = inject_memories.len(),
            has_embedder = self.has_embedder(),
            "保留建议生成完成"
        );

        Ok(RetentionSuggestion {
            session_id: request.session_id.clone(),
            suggestion_id,
            retain_turns,
            prune_hints,
            inject_memories,
        })
    }

    // ========================================================================
    // 三段式建议生成
    // ========================================================================

    /// 构建 retain_turns（设计文档第 9.3 节优先级判定）
    ///
    /// 优先级判定规则：
    /// - High：token_count > 1000（可能包含关键代码/架构决策）
    /// - Medium：token_count > 500（包含相关技术参考）
    /// - Low：token_count > 100（探索性内容）
    /// - token_count <= 100：不建议保留（可安全修剪）
    fn build_retain_items(
        &self,
        turns: &[TurnPreview],
        hits: &[SearchHit],
    ) -> Vec<RetainItem> {
        // 收集相关记忆的 hook_id（用于 related_memories 字段）
        let related_hook_ids: Vec<String> =
            hits.iter().map(|h| h.hook_id.clone()).collect();

        turns
            .iter()
            .filter(|t| t.token_count > 100)
            .take(self.max_retain_items)
            .map(|t| {
                let priority = self.determine_priority(t, hits);
                let reason = self.build_retain_reason(t, priority);
                RetainItem {
                    turn_id: t.turn_id.clone(),
                    priority,
                    reason,
                    related_memories: related_hook_ids.clone(),
                }
            })
            .collect()
    }

    /// 判定轮次优先级（设计文档第 9.3 节）
    fn determine_priority(&self, turn: &TurnPreview, _hits: &[SearchHit]) -> Priority {
        if turn.token_count > self.high_priority_tokens {
            Priority::High
        } else if turn.token_count > self.medium_priority_tokens {
            Priority::Medium
        } else {
            Priority::Low
        }
    }

    /// 构建保留原因
    fn build_retain_reason(&self, turn: &TurnPreview, priority: Priority) -> String {
        match priority {
            Priority::High => {
                format!(
                    "token_count={}，包含大量内容，建议保留以避免信息丢失",
                    turn.token_count
                )
            }
            Priority::Medium => {
                format!(
                    "token_count={}，包含相关技术参考，后续可能引用",
                    turn.token_count
                )
            }
            Priority::Low => {
                format!(
                    "token_count={}，轻量内容，可选择性保留",
                    turn.token_count
                )
            }
        }
    }

    /// 构建 prune_hints（设计文档：工具原始输出可安全修剪）
    fn build_prune_hints() -> Vec<PruneHint> {
        vec![PruneHint {
            target: "tool_results".to_string(),
            reason: "工具原始输出可安全修剪，摘要在 context_snapshot 中已保留".to_string(),
        }]
    }

    /// 构建 inject_memories（设计文档第 9.4 节注入策略）
    ///
    /// 注入策略判定：
    /// - score > 0.7 → Full（注入完整内容，关键架构决策）
    /// - score > 0.4 → Summary（注入摘要，已完成任务总结）
    /// - score <= 0.4 → Keywords（注入关键词，技术参考）
    /// - 最多注入 3 条
    fn build_inject_items(&self, hits: &[SearchHit]) -> Vec<InjectItem> {
        hits.iter()
            .take(3)
            .map(|h| {
                let strategy = if h.score > self.score_threshold_full {
                    InjectStrategy::Full
                } else if h.score > self.score_threshold_summary {
                    InjectStrategy::Summary
                } else {
                    InjectStrategy::Keywords
                };
                InjectItem {
                    hook_id: h.hook_id.clone(),
                    reason: format!(
                        "相关历史记忆，score={:.2}，source={:?}",
                        h.score, h.source
                    ),
                    inject_strategy: strategy,
                }
            })
            .collect()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- build_search_query 测试 ----

    #[test]
    fn test_build_search_query_full() {
        let snap = ContextSnapshot {
            current_task: Some("写设计文档".to_string()),
            recent_turns_summary: Some("正在编写 P8".to_string()),
            key_files: vec!["a.rs".to_string(), "b.md".to_string()],
            tool_calls_summary: vec![],
        };
        let query = build_search_query(&snap);
        assert!(query.contains("写设计文档"));
        assert!(query.contains("正在编写 P8"));
        assert!(query.contains("a.rs"));
        assert!(query.contains("b.md"));
    }

    #[test]
    fn test_build_search_query_empty() {
        let snap = ContextSnapshot::default();
        let query = build_search_query(&snap);
        assert!(query.is_empty());
    }

    #[test]
    fn test_build_search_query_partial() {
        let snap = ContextSnapshot {
            current_task: Some("任务".to_string()),
            recent_turns_summary: None,
            key_files: vec![],
            tool_calls_summary: vec![],
        };
        let query = build_search_query(&snap);
        assert_eq!(query, "任务");
    }

    #[test]
    fn test_build_search_query_skips_empty_parts() {
        let snap = ContextSnapshot {
            current_task: Some("".to_string()),
            recent_turns_summary: Some("有效摘要".to_string()),
            key_files: vec!["".to_string(), "file.rs".to_string()],
            tool_calls_summary: vec![],
        };
        let query = build_search_query(&snap);
        // 空字符串部分应被跳过
        assert_eq!(query, "有效摘要 file.rs");
    }

    // ---- RetentionBuilder 配置测试 ----

    #[test]
    fn test_retention_builder_defaults() {
        let router = Arc::new(SessionSearchRouter::new(None, 0));
        let builder = RetentionBuilder::new(router);
        assert_eq!(builder.search_top_k, 5);
        assert_eq!(builder.max_retain_items, 10);
        assert_eq!(builder.score_threshold_full, 0.7);
        assert_eq!(builder.score_threshold_summary, 0.4);
    }

    #[test]
    fn test_retention_builder_with_config() {
        let router = Arc::new(SessionSearchRouter::new(None, 0));
        let builder = RetentionBuilder::new(router)
            .with_search_top_k(10)
            .with_max_retain_items(20);
        assert_eq!(builder.search_top_k, 10);
        assert_eq!(builder.max_retain_items, 20);
    }

    #[test]
    fn test_has_embedder_false_when_no_embedder() {
        let router = Arc::new(SessionSearchRouter::new(None, 0));
        let builder = RetentionBuilder::new(router);
        assert!(!builder.has_embedder());
    }

    // ---- build_retain_items 测试 ----

    #[test]
    fn test_determine_priority_high() {
        let router = Arc::new(SessionSearchRouter::new(None, 0));
        let builder = RetentionBuilder::new(router);
        let turn = TurnPreview {
            turn_id: "t1".to_string(),
            text_preview: "大量内容".to_string(),
            token_count: 1500,
        };
        assert_eq!(builder.determine_priority(&turn, &[]), Priority::High);
    }

    #[test]
    fn test_determine_priority_medium() {
        let router = Arc::new(SessionSearchRouter::new(None, 0));
        let builder = RetentionBuilder::new(router);
        let turn = TurnPreview {
            turn_id: "t1".to_string(),
            text_preview: "中等内容".to_string(),
            token_count: 700,
        };
        assert_eq!(builder.determine_priority(&turn, &[]), Priority::Medium);
    }

    #[test]
    fn test_determine_priority_low() {
        let router = Arc::new(SessionSearchRouter::new(None, 0));
        let builder = RetentionBuilder::new(router);
        let turn = TurnPreview {
            turn_id: "t1".to_string(),
            text_preview: "少量内容".to_string(),
            token_count: 200,
        };
        assert_eq!(builder.determine_priority(&turn, &[]), Priority::Low);
    }

    // ---- build_prune_hints 测试 ----

    #[test]
    fn test_build_prune_hints() {
        let hints = RetentionBuilder::build_prune_hints();
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].target, "tool_results");
        assert!(hints[0].reason.contains("工具原始输出"));
    }

    // ---- build_inject_items 测试 ----

    #[test]
    fn test_build_inject_items_empty() {
        let router = Arc::new(SessionSearchRouter::new(None, 0));
        let builder = RetentionBuilder::new(router);
        let items = builder.build_inject_items(&[]);
        assert!(items.is_empty());
    }
}

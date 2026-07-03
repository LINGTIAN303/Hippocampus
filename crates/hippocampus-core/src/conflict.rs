//! # 记忆冲突检测（v2.6 批次 8）
//!
//! 在记忆迭代更新（`update_memory`）时检测新旧事实之间的冲突，
//! 让 Agent 能识别「用户立场反转」「事实矛盾」等情况，而非盲目追加。
//!
//! ## 设计参考
//!
//! - **BeliefShift 基准**：衡量 Agent 识别跨会话矛盾立场的能力
//! - **Kumiho / 信念修正（Belief Revision）**：形式化语义，修正过去判断而不丢失历史
//!
//! ## 架构（可插拔 trait，类比 [`crate::score::Scorer`]）
//!
//! ```text
//! update 请求 → ConflictDetector.detect(update, &existing_memory) → ConflictReport
//!                                                                   ↓
//! MemoryUpdateRecord.conflicts ← Vec<ConflictRecord> ← 持久化到记忆文件
//! ```
//!
//! - [`HeuristicDetector`](crate::heuristic::HeuristicDetector)：默认纯算法实现（无 LLM 依赖）
//! - [`NoopDetector`]：空实现，不做任何检测
//!
//! ## 冲突维度（三维度）
//!
//! 1. **自我矛盾（SelfContradict）**：同一批 update 内 added 与 deprecated 包含相同/相似事实
//! 2. **直接矛盾（DirectContradict）**：added_facts 与现有 key_facts 语义相反（反义词匹配）
//! 3. **立场反转（StanceReversal）**：added_facts 与历史 updates 的 added_facts 直接冲突

use crate::model::{MemoryFile, MemoryUpdate};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ============================================================================
// 数据结构
// ============================================================================

/// 冲突类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// 自我矛盾：同一批 update 内 added 与 deprecated 包含相同/相似事实
    SelfContradict,
    /// 直接矛盾：added_facts 与现有 key_facts 语义相反（反义词匹配）
    DirectContradict,
    /// 立场反转：added_facts 与历史 updates 的 added_facts 直接冲突
    StanceReversal,
}

/// 冲突严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// 信息性（如无效 deprecate，留待未来扩展）
    Info,
    /// 警告（可能矛盾，如立场反转）
    Warning,
    /// 严重（明确矛盾，如自我矛盾、直接反义）
    Critical,
}

/// 单条冲突记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictRecord {
    /// 冲突类型
    pub kind: ConflictKind,
    /// 严重级别
    pub severity: Severity,
    /// 中文描述（人类可读）
    pub description: String,
    /// 冲突的已有事实（DirectContradict / StanceReversal 时有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_fact: Option<String>,
    /// 新事实（触发冲突的 update 中的事实）
    pub new_fact: String,
}

/// 冲突检测报告
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConflictReport {
    /// 检测到的所有冲突记录
    pub conflicts: Vec<ConflictRecord>,
}

impl ConflictReport {
    /// 创建空报告
    pub fn empty() -> Self {
        Self::default()
    }

    /// 是否无冲突
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty()
    }

    /// 冲突总数
    pub fn count(&self) -> usize {
        self.conflicts.len()
    }

    /// 是否存在 Critical 级别冲突
    pub fn has_critical(&self) -> bool {
        self.conflicts
            .iter()
            .any(|c| c.severity == Severity::Critical)
    }

    /// 按严重级别筛选
    pub fn by_severity(&self, severity: Severity) -> Vec<&ConflictRecord> {
        self.conflicts
            .iter()
            .filter(|c| c.severity == severity)
            .collect()
    }

    /// 追加一条冲突记录
    pub fn push(&mut self, record: ConflictRecord) {
        self.conflicts.push(record);
    }
}

// ============================================================================
// ConflictDetector trait
// ============================================================================

/// 记忆冲突检测器 trait（可插拔）
///
/// 实现方提供具体的冲突检测算法：
/// - [`HeuristicDetector`](crate::heuristic::HeuristicDetector)：启发式纯算法（默认）
/// - [`NoopDetector`]：空实现（不检测）
///
/// ## 调用时机
///
/// 在 `Storage::update_memory` **之前**同步调用：
///
/// ```text,ignore
/// let memory = storage.read_memory(&memory_id).await?;
/// let report = detector.detect(&update, &memory).await;
/// storage.update_memory_with_conflicts(&memory_id, update, report.conflicts).await?;
/// ```
///
/// ## 设计原则
///
/// - **仅记录不阻止**：即使检测到 Critical 冲突，也不阻止更新（保留历史，交由上层 LLM 决策）
/// - **无副作用**：detect 方法不修改输入数据
/// - **可插拔**：通过 trait 注入，Storage 层不感知具体实现
#[async_trait]
pub trait ConflictDetector: Send + Sync {
    /// 检测 `update` 与 `existing_memory` 之间的冲突
    ///
    /// ## 参数
    /// - `update`：待应用的更新（added/revised/deprecated facts）
    /// - `existing_memory`：现有的记忆文件（包含 turns + 历史 updates）
    ///
    /// ## 返回
    /// 冲突检测报告（即使无冲突也返回空报告，不返回错误）
    async fn detect(
        &self,
        update: &MemoryUpdate,
        existing_memory: &MemoryFile,
    ) -> ConflictReport;
}

// ============================================================================
// NoopDetector（默认空实现）
// ============================================================================

/// 空实现（不做任何冲突检测）
///
/// 用于未配置检测器时的默认行为，或测试中需要跳过检测的场景。
#[derive(Debug, Default, Clone)]
pub struct NoopDetector;

impl NoopDetector {
    /// 创建空检测器
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ConflictDetector for NoopDetector {
    async fn detect(
        &self,
        _update: &MemoryUpdate,
        _existing_memory: &MemoryFile,
    ) -> ConflictReport {
        ConflictReport::empty()
    }
}

// ============================================================================
// HybridDetector（v2.11 串联检测器）
// ============================================================================

/// 混合冲突检测器（v2.11）
///
/// 串联两个检测器：先跑启发式（快速、无网络依赖），
/// 再跑 LLM（语义级补充），合并两份报告。
///
/// ## 设计
///
/// - **降级策略**：LLM 失败时返回空报告（与 `HttpLlmDetector` 行为一致），
///   启发式结果仍然保留
/// - **去重**：直接合并，不去重（避免误合并相似但不同的冲突记录）
/// - **使用场景**：同时配置了启发式 + LLM 检测器时使用
///
/// ## 示例
///
/// ```rust,ignore
/// use hippocampus_core::conflict::{ConflictDetector, HybridDetector};
/// use hippocampus_core::heuristic::HeuristicDetector;
/// // use hippocampus_server::HttpLlmDetector;
///
/// let heuristic = std::sync::Arc::new(HeuristicDetector::new());
/// let llm = std::sync::Arc::new(HttpLlmDetector::new(config));
/// let hybrid = HybridDetector::new(heuristic, llm);
/// let report = hybrid.detect(&update, &memory).await;
/// ```
#[derive(Clone)]
pub struct HybridDetector {
    /// 启发式检测器（通常为 `HeuristicDetector`）
    heuristic: Arc<dyn ConflictDetector>,
    /// LLM 检测器（通常为 `HttpLlmDetector`）
    llm: Arc<dyn ConflictDetector>,
}

impl HybridDetector {
    /// 创建混合检测器
    ///
    /// ## 参数
    ///
    /// - `heuristic`：启发式检测器（先执行，无网络依赖）
    /// - `llm`：LLM 检测器（后执行，失败时返回空报告不阻塞）
    pub fn new(
        heuristic: Arc<dyn ConflictDetector>,
        llm: Arc<dyn ConflictDetector>,
    ) -> Self {
        Self { heuristic, llm }
    }

    /// 启发式检测器引用（用于测试与诊断）
    pub fn heuristic(&self) -> &Arc<dyn ConflictDetector> {
        &self.heuristic
    }

    /// LLM 检测器引用（用于测试与诊断）
    pub fn llm(&self) -> &Arc<dyn ConflictDetector> {
        &self.llm
    }
}

#[async_trait]
impl ConflictDetector for HybridDetector {
    async fn detect(
        &self,
        update: &MemoryUpdate,
        existing_memory: &MemoryFile,
    ) -> ConflictReport {
        // 1. 先跑启发式（快速、无网络依赖）
        let mut report = self.heuristic.detect(update, existing_memory).await;

        // 2. 再跑 LLM（语义级补充，失败时返回空报告不阻塞）
        let llm_report = self.llm.detect(update, existing_memory).await;

        // 3. 合并报告（不去重，保留所有检测到的冲突）
        //    LLM 报告为空时（降级或无冲突）不影响启发式结果
        for conflict in llm_report.conflicts {
            report.push(conflict);
        }

        report
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ArchivePeriod, MessageContent, MessageTurn};
    use chrono::Utc;
    use uuid::Uuid;

    /// 构造测试用 MemoryFile
    fn make_test_memory() -> MemoryFile {
        let turn = MessageTurn {
            id: Uuid::new_v4(),
            user_message: MessageContent {
                text: Some("用户消息".to_string()),
                attachments: vec![],
                tool_calls: vec![],
                thinking: None,
            },
            llm_message: MessageContent {
                text: Some("助手回复".to_string()),
                attachments: vec![],
                tool_calls: vec![],
                thinking: None,
            },
            tags: vec![],
            timestamp: Utc::now(),
            token_count: 100,
        };

        MemoryFile {
            id: Uuid::new_v4(),
            schema_version: 1,
            archived_at: Utc::now(),
            session_id: "test-sess".to_string(),
            project_id: None,
            turns: vec![turn],
            tags: vec![],
            total_tokens: 100,
            truncated: false,
            period: ArchivePeriod::Daily,
            access_count: 0,
            importance: 0,
            updates: vec![],
        }
    }

    #[test]
    fn test_conflict_report_empty() {
        let report = ConflictReport::empty();
        assert!(report.is_clean());
        assert_eq!(report.count(), 0);
        assert!(!report.has_critical());
    }

    #[test]
    fn test_conflict_report_push_and_query() {
        let mut report = ConflictReport::empty();
        report.push(ConflictRecord {
            kind: ConflictKind::SelfContradict,
            severity: Severity::Critical,
            description: "测试冲突".to_string(),
            existing_fact: None,
            new_fact: "fact A".to_string(),
        });
        report.push(ConflictRecord {
            kind: ConflictKind::StanceReversal,
            severity: Severity::Warning,
            description: "立场反转".to_string(),
            existing_fact: Some("旧立场".to_string()),
            new_fact: "新立场".to_string(),
        });

        assert!(!report.is_clean());
        assert_eq!(report.count(), 2);
        assert!(report.has_critical());
        assert_eq!(report.by_severity(Severity::Critical).len(), 1);
        assert_eq!(report.by_severity(Severity::Warning).len(), 1);
        assert_eq!(report.by_severity(Severity::Info).len(), 0);
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
        assert!(Severity::Critical > Severity::Info);
    }

    #[tokio::test]
    async fn test_noop_detector_returns_empty() {
        let detector = NoopDetector::new();
        let memory = make_test_memory();
        let update = MemoryUpdate::new().add_fact("新事实");
        let report = detector.detect(&update, &memory).await;
        assert!(report.is_clean());
    }

    #[test]
    fn test_conflict_record_serialization() {
        let record = ConflictRecord {
            kind: ConflictKind::DirectContradict,
            severity: Severity::Critical,
            description: "用户先说喜欢，后说不喜欢".to_string(),
            existing_fact: Some("用户喜欢咖啡".to_string()),
            new_fact: "用户不喜欢咖啡".to_string(),
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("direct_contradict"));
        assert!(json.contains("critical"));
        assert!(json.contains("用户喜欢咖啡"));

        // 反序列化往返
        let restored: ConflictRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.kind, ConflictKind::DirectContradict);
        assert_eq!(restored.severity, Severity::Critical);
        assert_eq!(restored.new_fact, "用户不喜欢咖啡");
    }

    #[test]
    fn test_conflict_report_serialization_skip_none() {
        let record = ConflictRecord {
            kind: ConflictKind::SelfContradict,
            severity: Severity::Critical,
            description: "自我矛盾".to_string(),
            existing_fact: None,
            new_fact: "fact".to_string(),
        };
        let json = serde_json::to_string(&record).unwrap();
        // existing_fact 为 None 时应被跳过
        assert!(!json.contains("existing_fact"));
    }

    // ========================================================================
    // v2.11：HybridDetector 测试
    // ========================================================================

    /// Mock 检测器：返回预设的 ConflictReport
    ///
    /// 用于模拟 LLM 检测器（成功/失败降级/返回特定冲突），
    /// 避免在单元测试中发起真实 HTTP 请求。
    struct MockDetector {
        report: ConflictReport,
    }

    impl MockDetector {
        fn new(report: ConflictReport) -> Self {
            Self { report }
        }

        fn empty() -> Self {
            Self::new(ConflictReport::empty())
        }

        fn single_critical() -> Self {
            let mut report = ConflictReport::empty();
            report.push(ConflictRecord {
                kind: ConflictKind::DirectContradict,
                severity: Severity::Critical,
                description: "LLM 检测到语义矛盾".to_string(),
                existing_fact: Some("旧事实".to_string()),
                new_fact: "新事实".to_string(),
            });
            Self::new(report)
        }

        fn single_warning() -> Self {
            let mut report = ConflictReport::empty();
            report.push(ConflictRecord {
                kind: ConflictKind::StanceReversal,
                severity: Severity::Warning,
                description: "LLM 检测到立场反转".to_string(),
                existing_fact: Some("旧立场".to_string()),
                new_fact: "新立场".to_string(),
            });
            Self::new(report)
        }
    }

    #[async_trait]
    impl ConflictDetector for MockDetector {
        async fn detect(
            &self,
            _update: &MemoryUpdate,
            _existing_memory: &MemoryFile,
        ) -> ConflictReport {
            // 克隆预设报告返回
            self.report.clone()
        }
    }

    /// 构造一个 Heuristic 检测到 1 条 Critical 冲突的 update + memory 组合
    ///
    /// 场景：历史已添加"用户喜欢咖啡"，本次 update 添加"用户不喜欢咖啡"
    fn make_heuristic_contradiction_case() -> (MemoryUpdate, MemoryFile) {
        let mut memory = make_test_memory();
        // 历史已添加"用户喜欢咖啡"
        memory.updates.push(crate::model::MemoryUpdateRecord {
            updated_at: chrono::Utc::now(),
            update: MemoryUpdate::new().add_fact("用户喜欢咖啡"),
            conflicts: vec![],
        });
        // 本次 update 添加"用户不喜欢咖啡"（与历史直接矛盾）
        let update = MemoryUpdate::new().add_fact("用户不喜欢咖啡");
        (update, memory)
    }

    #[tokio::test]
    async fn test_hybrid_detector_merges_both_reports() {
        // heuristic 返回 1 条 Critical + LLM 返回 1 条 Warning → 合并后 2 条
        let heuristic: Arc<dyn ConflictDetector> =
            Arc::new(crate::heuristic::HeuristicDetector::new());
        let llm: Arc<dyn ConflictDetector> =
            Arc::new(MockDetector::single_warning());
        let hybrid = HybridDetector::new(heuristic, llm);

        let (update, memory) = make_heuristic_contradiction_case();
        let report = hybrid.detect(&update, &memory).await;

        // heuristic 检测到 1 条 DirectContradict（Critical）
        // LLM 检测到 1 条 StanceReversal（Warning）
        assert_eq!(
            report.count(),
            2,
            "合并后应有 2 条冲突，实际: {}",
            report.count()
        );
        assert!(
            report.has_critical(),
            "应存在 Critical 级别冲突（来自 heuristic）"
        );
    }

    #[tokio::test]
    async fn test_hybrid_detector_llm_empty_keeps_heuristic() {
        // 模拟 LLM 失败降级（返回空报告）→ 启发式结果仍保留
        let heuristic: Arc<dyn ConflictDetector> =
            Arc::new(crate::heuristic::HeuristicDetector::new());
        let llm: Arc<dyn ConflictDetector> = Arc::new(MockDetector::empty());
        let hybrid = HybridDetector::new(heuristic, llm);

        let (update, memory) = make_heuristic_contradiction_case();
        let report = hybrid.detect(&update, &memory).await;

        // LLM 降级为空，只剩 heuristic 的 1 条 DirectContradict
        assert_eq!(
            report.count(),
            1,
            "LLM 降级为空时应保留 heuristic 的 1 条冲突"
        );
        assert!(report.has_critical());
        // 唯一一条应是 DirectContradict（来自 heuristic）
        assert_eq!(
            report.conflicts[0].kind,
            ConflictKind::DirectContradict
        );
    }

    #[tokio::test]
    async fn test_hybrid_detector_both_empty() {
        // 两者都返回空 → 空报告
        let heuristic: Arc<dyn ConflictDetector> =
            Arc::new(crate::heuristic::HeuristicDetector::new());
        let llm: Arc<dyn ConflictDetector> = Arc::new(MockDetector::empty());
        let hybrid = HybridDetector::new(heuristic, llm);

        // 无冲突的 update（添加一个无关事实）
        let memory = make_test_memory();
        let update = MemoryUpdate::new().add_fact("用户住在上海");
        let report = hybrid.detect(&update, &memory).await;

        assert!(report.is_clean(), "无冲突场景应返回空报告");
        assert!(!report.has_critical());
    }

    #[tokio::test]
    async fn test_hybrid_detector_both_noop() {
        // 两个 NoopDetector 串联 → 永远空报告
        let heuristic: Arc<dyn ConflictDetector> = Arc::new(NoopDetector::new());
        let llm: Arc<dyn ConflictDetector> = Arc::new(NoopDetector::new());
        let hybrid = HybridDetector::new(heuristic, llm);

        let (update, memory) = make_heuristic_contradiction_case();
        let report = hybrid.detect(&update, &memory).await;

        assert!(report.is_clean());
    }

    #[tokio::test]
    async fn test_hybrid_detector_accessor_methods() {
        // 验证 heuristic() / llm() 访问器
        let heuristic: Arc<dyn ConflictDetector> =
            Arc::new(crate::heuristic::HeuristicDetector::new());
        let llm: Arc<dyn ConflictDetector> = Arc::new(MockDetector::single_critical());
        let hybrid = HybridDetector::new(heuristic, llm);

        // 通过访问器获取引用并调用 detect
        let memory = make_test_memory();
        let update = MemoryUpdate::new().add_fact("测试");
        let h_report = hybrid.heuristic().detect(&update, &memory).await;
        let l_report = hybrid.llm().detect(&update, &memory).await;

        // heuristic 对此场景应无冲突，Mock single_critical 应有 1 条
        assert!(h_report.is_clean());
        assert_eq!(l_report.count(), 1);
    }

    #[tokio::test]
    async fn test_hybrid_detector_preserves_severity_ordering() {
        // heuristic 检测到 Warning + LLM 检测到 Critical → 合并后 has_critical=true
        // 使用 NoopDetector 作为 heuristic（不产生冲突），LLM 提供 Critical
        let heuristic: Arc<dyn ConflictDetector> = Arc::new(NoopDetector::new());
        let llm: Arc<dyn ConflictDetector> = Arc::new(MockDetector::single_critical());
        let hybrid = HybridDetector::new(heuristic, llm);

        let memory = make_test_memory();
        let update = MemoryUpdate::new().add_fact("测试");
        let report = hybrid.detect(&update, &memory).await;

        assert_eq!(report.count(), 1);
        assert!(report.has_critical());
        assert_eq!(report.by_severity(Severity::Critical).len(), 1);
    }
}

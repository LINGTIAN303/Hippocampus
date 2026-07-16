//! # MemoryCenter 归档核心引擎（v2.50 新增）
//!
//! 抽取 server `pre_compress` + `archive` handler 的核心归档逻辑，
//! 供 `memory-center-server` 和 `memory-center-sidecar` 共享，消除重复。
//!
//! ## 核心价值
//!
//! - **sidecar 直写存储**：sidecar 不再依赖 HTTP server 中转，直接调用 `ArchiveEngine` 写 LocalStorage
//! - **消除归档逻辑重复**：server 和 sidecar 共用同一套归档链路
//! - **组件复用**：LLM 组件（SummaryGenerator/ScenarioDetector/SessionSearchRouter）初始化逻辑共享
//!
//! ## 架构
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │               memory-center-archive-core                │
//! │                                                         │
//! │   ArchiveEngine                                         │
//! │   ├── pre_compress()  压缩前一次性完整归档              │
//! │   ├── archive()       日常归档                          │
//! │   └── health_check()  存储目录可写检查                  │
//! │                                                         │
//! │   组件构建（from_env）                                  │
//! │   ├── build_summary_generator()  LLM 摘要生成器         │
//! │   ├── build_scenario_detector()  场景识别器             │
//! │   └── build_session_search()     搜索索引路由器         │
//! └─────────────────────────────────────────────────────────┘
//!           ▲                              ▲
//!           │                              │
//!     ┌─────┴──────┐              ┌───────┴───────┐
//!     │   server   │              │   sidecar     │
//!     │ (HTTP API) │              │ (直写存储)    │
//!     └────────────┘              └───────────────┘
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use memory_center_core::archive::Archiver;
use memory_center_core::model::{
    apply_turn_defaults, ArchiveConfig, IndexHook, MessageContent, TaskStateSnapshot,
};

// P8 Cooperative：re-export MessageTurn 供 cooperative 模块使用
pub use memory_center_core::model::MessageTurn;

// P8 Cooperative 协作模式（v2.53 P8）
pub mod cooperative;
pub use cooperative::{
    CooperativeError, CooperativeHandler, CooperativeService, CooperativeSession,
    CooperativeState, ContextSnapshot, InjectItem, InjectStrategy, PostCompressAckRequest,
    PreCompressHintRequest, PruneHint, Priority, RetainItem, RetentionSuggestion,
    SuggestionAdoption, TurnPreview,
};

// P8 保留建议构建器（v2.53 P8 Phase 2）
pub mod retention;
pub use retention::{build_search_query, RetentionBuilder};
use memory_center_core::retrieve::SummaryView;
use memory_center_core::storage::{LocalStorage, Storage};
use memory_center_search::SessionSearchRouter;

// ============================================================================
// 请求 / 响应结构（与 server handlers.rs 对齐，供 sidecar 复用）
// ============================================================================

/// 预设请求（与 server `PresetRequest` 对齐）
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PresetRequest {
    pub agent: Option<String>,
    pub scenario: Option<String>,
    pub model: Option<String>,
    pub archive_threshold: Option<usize>,
    pub summary_template: Option<String>,
}

/// 任务状态快照请求（与 server `TaskStateSnapshotRequest` 对齐）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TaskStateSnapshotRequest {
    pub current_task: String,
    #[serde(default)]
    pub completed_steps: Vec<String>,
    #[serde(default)]
    pub in_progress_step: Option<String>,
    pub next_step: String,
}

/// pre_compress 结果（与 server 响应 JSON 对齐）
#[derive(Debug, Clone, serde::Serialize)]
pub struct PreCompressResult {
    pub hook_id: String,
    pub raw_context_path: String,
    pub parse_success: bool,
    pub parsed_turns_count: usize,
    pub archived_tokens: usize,
    pub estimated_total_tokens: usize,
    pub threshold: usize,
    pub threshold_ratio_percent: u64,
    pub suggestion: String,
    pub archived_at: String,
}

/// archive 结果（SummaryView + 搜索索引用的 turns_text）
#[derive(Debug, Clone)]
pub struct ArchiveResult {
    pub summary: SummaryView,
    /// 归档的 turns 文本（用于触发搜索索引）
    pub turns_text: String,
    /// 归档后的 IndexHook（用于触发搜索索引）
    pub hook: IndexHook,
}

// ============================================================================
// 错误类型
// ============================================================================

/// 归档错误
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("参数错误: {0}")]
    BadRequest(String),
    #[error("存储错误: {0}")]
    Storage(String),
    #[error("归档失败: {0}")]
    Archive(String),
    #[error("预设构建失败: {0}")]
    Preset(String),
}

// ============================================================================
// Token 估算器（闭包注入，避免依赖 models crate）
// ============================================================================

/// Token 估算器：将文本映射为 token 数的闭包
///
/// 通过 [`ArchiveEngine::with_token_estimator`] 注入，用于替换 `chars/3` 简化估算。
/// 未注入时降级为 `chars/3`（向后兼容）。
///
/// 调用方通常从 `ModelVariant::count_tokens` 构建闭包：
///
/// ```ignore
/// use memory_center_models::ModelVariant;
/// use std::sync::Arc;
///
/// let model = ModelVariant::gpt_5_2();
/// let estimator: TokenEstimator = Arc::new(move |text: &str| model.count_tokens(text));
/// let engine = ArchiveEngine::new(storage_root).with_token_estimator(estimator);
/// ```
pub type TokenEstimator = Arc<dyn Fn(&str) -> usize + Send + Sync>;

/// 用 estimator 估算 MessageContent 的 token 数
///
/// 遍历 text / thinking / tool_calls(arguments+result+error) / file_changes(patch)，
/// 对每个文本片段调用 estimator 累加。
fn estimate_content_tokens(content: &MessageContent, estimator: &TokenEstimator) -> usize {
    let mut tokens: usize = 0;

    if let Some(text) = &content.text {
        tokens += estimator(text);
    }
    if let Some(thinking) = &content.thinking {
        tokens += estimator(thinking);
    }
    for tc in &content.tool_calls {
        tokens += estimator(&tc.arguments);
        tokens += estimator(&tc.result);
        if let Some(err) = &tc.error {
            tokens += estimator(err);
        }
    }
    for fc in &content.file_changes {
        if let Some(patch) = &fc.patch {
            tokens += estimator(patch);
        }
    }

    tokens
}

/// 用 estimator 估算 MessageTurn 的 token 数（user_message + llm_message）
fn estimate_turn_tokens(turn: &MessageTurn, estimator: &TokenEstimator) -> usize {
    let user = estimate_content_tokens(&turn.user_message, estimator);
    let llm = estimate_content_tokens(&turn.llm_message, estimator);
    (user + llm).max(1) // 最小 1，避免全 0
}

// ============================================================================
// ArchiveEngine：归档核心引擎
// ============================================================================

/// 归档核心引擎（v2.50 新增）
///
/// 封装 server `pre_compress` + `archive` 的核心逻辑，供 server 和 sidecar 共享。
///
/// ## 组件注入
///
/// - `summary_generator`：LLM 摘要生成器（未注入时降级为启发式）
/// - `scenario_detector`：场景识别器（未注入时用 preset 原行为）
/// - `session_search`：搜索索引路由器（未注入时跳过索引）
///
/// ## 使用示例
///
/// ```no_run
/// # use memory_center_archive_core::ArchiveEngine;
/// # use std::path::PathBuf;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let engine = ArchiveEngine::new(PathBuf::from("./data"));
/// // .with_summary_generator(gen) / .with_session_search(router) 按需注入
///
/// // 压缩前归档
/// let result = engine.pre_compress(
///     "session-001",
///     vec![],
///     Some(50000),
///     Some("myproject"),
///     None,
///     None,
///     None,
/// ).await?;
/// # Ok(())
/// # }
/// ```
// v2.52：派生 Clone 以便 MCP 端 MemoryCenterMcp（派生 Clone）能持有此引擎
#[derive(Clone)]
pub struct ArchiveEngine {
    /// 存储根目录
    storage_root: PathBuf,
    /// 可选的 LLM 摘要生成器
    summary_generator: Option<Arc<dyn memory_center_core::generate::SummaryGenerator>>,
    /// 可选的场景识别器
    scenario_detector:
        Option<Arc<memory_center_presets::HybridScenarioDetector>>,
    /// 可选的搜索索引路由器
    session_search: Option<Arc<SessionSearchRouter>>,
    /// 可选的 Token 估算器（v2.52 阶段 4 新增，未注入时降级为 chars/3）
    token_estimator: Option<TokenEstimator>,
    /// 可选的 Cooperative 处理器（v2.53 P8 新增，设计文档第 8.4 节）
    ///
    /// 未注入时（Independent 模式），pre_compress 走现有逻辑。
    /// 注入后（Cooperative 模式），可用于在 pre_compress 前调用 handler 获取保留建议。
    /// 当前 Phase 2 仅存储字段，pre_compress 行为未改变（向后兼容）。
    cooperative_handler: Option<Arc<dyn cooperative::CooperativeHandler>>,
}

impl ArchiveEngine {
    /// 创建新的归档引擎
    pub fn new(storage_root: PathBuf) -> Self {
        Self {
            storage_root,
            summary_generator: None,
            scenario_detector: None,
            session_search: None,
            token_estimator: None,
            cooperative_handler: None,
        }
    }

    /// 注入 Cooperative 处理器（v2.53 P8 新增，设计文档第 8.4 节）
    ///
    /// 注入后 ArchiveEngine 标记为 Cooperative 模式。
    /// CooperativeService 持有的 ArchiveEngine 不注入 handler（保持 Independent 行为用于归档）。
    pub fn with_cooperative_handler(
        mut self,
        handler: Arc<dyn cooperative::CooperativeHandler>,
    ) -> Self {
        self.cooperative_handler = Some(handler);
        self
    }

    /// 是否已注入 Cooperative 处理器
    pub fn has_cooperative_handler(&self) -> bool {
        self.cooperative_handler.is_some()
    }

    /// 获取 Cooperative 处理器引用
    pub fn cooperative_handler(&self) -> Option<&Arc<dyn cooperative::CooperativeHandler>> {
        self.cooperative_handler.as_ref()
    }

    /// 注入 LLM 摘要生成器
    pub fn with_summary_generator(
        mut self,
        gen: Arc<dyn memory_center_core::generate::SummaryGenerator>,
    ) -> Self {
        self.summary_generator = Some(gen);
        self
    }

    /// 注入场景识别器
    pub fn with_scenario_detector(
        mut self,
        det: Arc<memory_center_presets::HybridScenarioDetector>,
    ) -> Self {
        self.scenario_detector = Some(det);
        self
    }

    /// 注入搜索索引路由器
    pub fn with_session_search(mut self, router: Arc<SessionSearchRouter>) -> Self {
        self.session_search = Some(router);
        self
    }

    /// 注入 Token 估算器（v2.52 阶段 4 新增）
    ///
    /// 注入后，`pre_compress` / `archive` 中的 token 估算将从 `chars/3` 升级为
    /// estimator 精确计数（仅对 Agent 未传 token_count 的轮次生效，Agent 显式传入的不覆盖）。
    ///
    /// 调用方通常从 `ModelVariant::count_tokens` 构建闭包注入：
    ///
    /// ```ignore
    /// use memory_center_models::ModelVariant;
    /// use std::sync::Arc;
    ///
    /// let model = ModelVariant::gpt_5_2();
    /// let estimator: TokenEstimator = Arc::new(move |text: &str| model.count_tokens(text));
    /// let engine = ArchiveEngine::new(storage_root).with_token_estimator(estimator);
    /// ```
    pub fn with_token_estimator(mut self, estimator: TokenEstimator) -> Self {
        self.token_estimator = Some(estimator);
        self
    }

    /// 获取存储根目录
    pub fn storage_root(&self) -> &std::path::Path {
        &self.storage_root
    }

    /// 获取摘要生成器引用
    pub fn summary_generator(&self) -> Option<&Arc<dyn memory_center_core::generate::SummaryGenerator>> {
        self.summary_generator.as_ref()
    }

    /// 获取搜索路由器引用
    pub fn session_search(&self) -> Option<&Arc<SessionSearchRouter>> {
        self.session_search.as_ref()
    }

    /// 健康检查：存储目录可写
    pub fn health_check(&self) -> Result<bool, ArchiveError> {
        if !self.storage_root.exists() {
            std::fs::create_dir_all(&self.storage_root).map_err(|e| {
                ArchiveError::Storage(format!("创建存储目录失败: {e}"))
            })?;
        }
        // 测试可写：尝试创建 .healthcheck 临时文件
        let test_file = self.storage_root.join(".archive_engine_healthcheck");
        std::fs::write(&test_file, b"ok").map_err(|e| {
            ArchiveError::Storage(format!("存储目录不可写: {e}"))
        })?;
        let _ = std::fs::remove_file(&test_file);
        Ok(true)
    }

    /// 创建 Storage 实例（每次调用创建，无内存缓存）
    fn create_storage(&self) -> Arc<dyn Storage> {
        Arc::new(LocalStorage::new(self.storage_root.clone()))
    }

    // ========================================================================
    // pre_compress：压缩前一次性完整归档
    // ========================================================================

    /// 压缩前一次性完整归档（抽取自 server `pre_compress` handler）
    ///
    /// 双轨处理：
    /// 1. raw_context 永远先存（失败才阻塞返回错误）
    /// 2. 尝试解析 turns：成功复用 Archiver 归档；失败仅存 raw_context
    ///
    /// # 参数
    ///
    /// - `session_id`: 会话 ID
    /// - `turns`: 结构化轮次列表（保留 tool_calls/thinking）
    /// - `estimated_tokens`: 客户端估算的 token 数（None 时服务端按内容长度 / 3 估算）
    /// - `project_id`: 项目 ID（可选，影响存储路径）
    /// - `preset`: 预设配置（可选）
    /// - `task_state_snapshot`: 任务状态快照（可选，持久化供下次 prompt 校准）
    /// - `raw_context_override`: 覆盖 raw_context 内容（可选）。
    ///   传 `Some(content)` 时用 `content` 作为 raw_context 内容（MCP 端传 full_context）；
    ///   传 `None` 时用 turns 的 JSON 序列化（server/sidecar 默认行为）。
    pub async fn pre_compress(
        &self,
        session_id: &str,
        turns: Vec<MessageTurn>,
        estimated_tokens: Option<usize>,
        project_id: Option<&str>,
        preset: Option<&PresetRequest>,
        task_state_snapshot: Option<&TaskStateSnapshotRequest>,
        raw_context_override: Option<&str>,
    ) -> Result<PreCompressResult, ArchiveError> {
        // v2.52：移除空 turns 早返回，让空 turns 场景（full_context 解析失败）
        // 正确走"仅存 raw_context"路径，避免后续空 turns 处理分支成为死代码。

        // 1. 生成 hook_id（提前生成，用于 raw_context 文件命名）
        let hook_id = uuid::Uuid::new_v4().to_string();

        // 2. 确定 raw_context 内容
        // - raw_context_override 有值：用调用方传入的内容（MCP 传 full_context）
        // - 否则：用 turns 的 JSON 序列化（server/sidecar 默认行为）
        let raw_context_content = if let Some(override_content) = raw_context_override {
            override_content.to_string()
        } else {
            serde_json::to_string_pretty(&turns)
                .unwrap_or_else(|_| "<turns 序列化失败>".to_string())
        };

        // 3. 写 raw_context（spec 第七章：永远先存，失败才阻塞返回错误）
        let storage = self.create_storage();
        let raw_context_path = storage
            .write_raw_context(session_id, &hook_id, &raw_context_content)
            .await
            .map_err(|e| {
                ArchiveError::Storage(format!(
                    "写 raw_context 失败: {e}\n\n\
                     raw_context 是 pre_compress 的核心兜底，失败则阻塞返回。\
                     后续解析/归档步骤不会执行。"
                ))
            })?;

        // 4. 估算 token（v2.52 阶段 4：有 estimator 时用精确计数，否则启发式兜底）
        // v2.54 P17：兜底从 raw_context_content.len() / 3（字节级，中文低估 78%）
        // 改为 estimate_tokens_heuristic（字符级 + CJK 比例动态公式）
        let estimated_total_tokens = estimated_tokens.unwrap_or_else(|| {
            if let Some(estimator) = &self.token_estimator {
                estimator(&raw_context_content)
            } else {
                memory_center_core::model::estimate_tokens_heuristic(&raw_context_content)
            }
        });

        // 5. 路径 A：turns 直接用（结构化，保留 tool_calls/thinking）
        let parsed_turns = turns;
        let parse_source = "structured";

        // 6. 归档 turns
        let (archived_tokens, parsed_turns_count, parse_success) = if parsed_turns.is_empty() {
            tracing::info!(
                session = %session_id,
                hook_id = %hook_id,
                parse_source,
                "解析得到空 turns，仅存 raw_context"
            );
            (estimated_total_tokens, 0, false)
        } else {
            let turns_count = parsed_turns.len();
            match self
                .archive_parsed_turns_in_pre_compress(
                    session_id,
                    project_id,
                    parsed_turns,
                    preset,
                    task_state_snapshot,
                    &hook_id,
                    &raw_context_path,
                )
                .await
            {
                Ok(tokens) => (tokens, turns_count, true),
                Err(e) => {
                    tracing::warn!(
                        session = %session_id,
                        hook_id = %hook_id,
                        error = %e,
                        "Archiver 归档失败，降级为仅 raw_context（parse_success=false）"
                    );
                    (estimated_total_tokens, 0, false)
                }
            }
        };

        // 7. 计算 threshold / ratio / suggestion
        let threshold = get_archive_threshold(preset);
        let ratio = if threshold > 0 {
            (archived_tokens as f64 / threshold as f64 * 100.0).round() as u64
        } else {
            0
        };
        let suggestion = if parse_success {
            format!(
                "压缩前归档完成，共 {} 轮，原始 ~{} tokens（阈值 {}，当前 {}%）。可安全压缩。",
                parsed_turns_count, estimated_total_tokens, threshold, ratio
            )
        } else {
            format!(
                "压缩前归档完成（仅 raw_context，解析失败），原始 ~{} tokens（阈值 {}，当前 {}%）。可安全压缩。",
                estimated_total_tokens, threshold, ratio
            )
        };

        tracing::info!(
            session = %session_id,
            hook_id = %hook_id,
            parse_success,
            parsed_turns_count,
            archived_tokens,
            threshold,
            ratio_percent = ratio,
            "pre_compress 完成"
        );

        Ok(PreCompressResult {
            hook_id,
            raw_context_path,
            parse_success,
            parsed_turns_count,
            archived_tokens,
            estimated_total_tokens,
            threshold,
            threshold_ratio_percent: ratio,
            suggestion,
            archived_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// pre_compress 内部辅助：解析成功后复用 Archiver 归档 turns
    ///
    /// 提取自 server `archive_parsed_turns_in_pre_compress`。
    /// 场景识别 + 构建 Archiver + 应用 preset + 注入 summary_generator
    /// + 写 task_state_snapshot + 触发搜索索引。
    async fn archive_parsed_turns_in_pre_compress(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        turns: Vec<MessageTurn>,
        preset: Option<&PresetRequest>,
        task_state_snapshot: Option<&TaskStateSnapshotRequest>,
        hook_id: &str,
        raw_context_path: &str,
    ) -> Result<usize, String> {
        // 1. 场景识别（仅首次 archive 时识别，后续读 session_meta 跳过）
        let effective_scenario_name: Option<String> = if let Some(detector) = &self.scenario_detector
        {
            let family = preset
                .and_then(|p| p.agent.as_deref())
                .and_then(memory_center_agents::AgentFamily::from_str)
                .unwrap_or_else(|| {
                    memory_center_agents::AgentFamily::Custom("unknown".to_string())
                });

            let user_explicit = preset.and_then(|p| p.scenario.as_deref());

            let storage_for_detect = self.create_storage();
            let scenario = memory_center_presets::resolve_effective_scenario(
                storage_for_detect.as_ref(),
                session_id,
                user_explicit,
                &family,
                detector.as_ref(),
                &turns,
            )
            .await;
            Some(memory_center_presets::scenario_to_str(&scenario))
        } else {
            preset.and_then(|p| p.scenario.clone())
        };

        // 2. 构建 preset（archive_threshold + summary_template）
        let (archive_threshold, summary_template) = if let Some(preset_req) = preset {
            let combined = build_combined_from_request(preset_req)
                .map_err(|e| format!("预设构建失败: {e}"))?;
            (
                Some(combined.archive_threshold()),
                Some(combined.summary_template().to_string()),
            )
        } else if let Some(scenario_name) = effective_scenario_name {
            let combined = memory_center_presets::build_from_strings(
                None,
                Some(&scenario_name),
                None,
                None,
                None,
            )
            .map_err(|e| format!("识别场景构建预设失败: {e}"))?;
            (
                Some(combined.archive_threshold()),
                Some(combined.summary_template().to_string()),
            )
        } else {
            (None, None)
        };

        // 3. 构建 Archiver
        let storage = self.create_storage();
        let config = if let Some(threshold) = archive_threshold {
            // v2.54 P19：改用 ArchiveConfig::from_threshold，统一硬上限系数来源
            ArchiveConfig::from_threshold(threshold)
        } else {
            ArchiveConfig::default()
        };
        let storage_for_snapshot = storage.clone();
        let mut archiver = Archiver::new(
            config,
            storage,
            session_id,
            project_id.map(|s| s.to_string()),
        );

        // 4. 注入 summary_generator
        if let Some(gen) = &self.summary_generator {
            archiver = archiver.with_summary_generator(gen.clone());
        }

        // 5. 注入 summary_template
        if let Some(tpl) = summary_template {
            archiver = archiver.with_summary_template_override(tpl);
        }

        // 6. 注入覆盖（hook_id 一致性 + archive_reason + raw_context_path）
        archiver = archiver
            .with_override_hook_id(hook_id)
            .with_archive_reason("pre_compress")
            .with_raw_context_path(raw_context_path);

        // 7. 提取 turns 文本用于索引（在 move 消费前 borrow）
        let turns_text = memory_center_search::extract_turns_text(&turns);

        // 8. 对每个 turn 应用默认值补全（推断 tags + 估算 token_count）
        for mut turn in turns {
            let was_zero = turn.token_count == 0;
            apply_turn_defaults(&mut turn);
            // v2.52 阶段 4：若有 estimator，用精确 tokenizer 覆盖 chars/3 估算
            // （仅对 Agent 未传 token_count 的轮次，Agent 显式传入的不覆盖）
            if was_zero {
                if let Some(estimator) = &self.token_estimator {
                    turn.token_count = estimate_turn_tokens(&turn, estimator);
                }
            }
            archiver.push_turn(turn);
        }

        let (_, hook) = archiver
            .archive()
            .await
            .map_err(|e| format!("归档失败: {e}"))?;

        // 9. 归档后触发搜索索引（按 session 隔离）
        if let Some(router) = &self.session_search {
            router.index_hook(session_id, &hook, &turns_text).await;
        }

        // 10. 写 task_state_snapshot（若有，失败不影响归档结果）
        if let Some(snap) = task_state_snapshot {
            let snapshot = TaskStateSnapshot {
                current_task: snap.current_task.clone(),
                completed_steps: snap.completed_steps.clone(),
                in_progress_step: snap.in_progress_step.clone(),
                next_step: snap.next_step.clone(),
                snapshot_at: chrono::Utc::now(),
            };
            if let Err(e) = storage_for_snapshot
                .write_session_state(session_id, &snapshot)
                .await
            {
                tracing::warn!(
                    session = %session_id,
                    error = %e,
                    "task_state_snapshot 持久化失败（不影响归档结果）"
                );
            }
        }

        Ok(hook.token_count)
    }

    // ========================================================================
    // archive：日常归档
    // ========================================================================

    /// 日常归档（抽取自 server `archive` handler）
    ///
    /// 归档一批轮次为记忆文件，生成索引钩子。
    pub async fn archive(
        &self,
        session_id: &str,
        turns: Vec<MessageTurn>,
        project_id: Option<&str>,
        preset: Option<&PresetRequest>,
    ) -> Result<ArchiveResult, ArchiveError> {
        if turns.is_empty() {
            return Err(ArchiveError::BadRequest(
                "turns 不能为空".to_string(),
            ));
        }

        // 1. 场景识别
        let effective_scenario_name: Option<String> = if let Some(detector) = &self.scenario_detector
        {
            let family = preset
                .and_then(|p| p.agent.as_deref())
                .and_then(memory_center_agents::AgentFamily::from_str)
                .unwrap_or_else(|| {
                    memory_center_agents::AgentFamily::Custom("unknown".to_string())
                });

            let user_explicit = preset.and_then(|p| p.scenario.as_deref());

            let storage_for_detect = self.create_storage();
            let scenario = memory_center_presets::resolve_effective_scenario(
                storage_for_detect.as_ref(),
                session_id,
                user_explicit,
                &family,
                detector.as_ref(),
                &turns,
            )
            .await;
            Some(memory_center_presets::scenario_to_str(&scenario))
        } else {
            preset.and_then(|p| p.scenario.clone())
        };

        // 2. 构建 preset
        let (archive_threshold, summary_template) = if let Some(preset_req) = preset {
            let combined = build_combined_from_request(preset_req)
                .map_err(|e| ArchiveError::Preset(e))?;
            (
                Some(combined.archive_threshold()),
                Some(combined.summary_template().to_string()),
            )
        } else if let Some(scenario_name) = effective_scenario_name {
            let combined = memory_center_presets::build_from_strings(
                None,
                Some(&scenario_name),
                None,
                None,
                None,
            )
            .map_err(|e| ArchiveError::Preset(format!("识别场景构建预设失败: {e}")))?;
            (
                Some(combined.archive_threshold()),
                Some(combined.summary_template().to_string()),
            )
        } else {
            (None, None)
        };

        // 3. 构建 Archiver
        let storage = self.create_storage();
        let config = if let Some(threshold) = archive_threshold {
            // v2.54 P19：改用 ArchiveConfig::from_threshold，统一硬上限系数来源
            ArchiveConfig::from_threshold(threshold)
        } else {
            ArchiveConfig::default()
        };
        // v2.54 P23：在 config 被 move 前提取阈值用于日志（补全可观测性）
        let logged_threshold = config.token_threshold;
        let logged_hard_limit = config.force_truncate_limit;
        let mut archiver = Archiver::new(
            config,
            storage,
            session_id,
            project_id.map(|s| s.to_string()),
        );

        // 4. 注入 summary_generator
        if let Some(gen) = &self.summary_generator {
            archiver = archiver.with_summary_generator(gen.clone());
        }

        // 5. 注入 summary_template
        if let Some(tpl) = summary_template {
            archiver = archiver.with_summary_template_override(tpl);
        }

        // 6. 提取 turns 文本用于索引
        let turns_text = memory_center_search::extract_turns_text(&turns);

        // 7. apply_turn_defaults + push（v2.52 阶段 4：estimator 覆盖 chars/3）
        for mut turn in turns {
            let was_zero = turn.token_count == 0;
            apply_turn_defaults(&mut turn);
            if was_zero {
                if let Some(estimator) = &self.token_estimator {
                    turn.token_count = estimate_turn_tokens(&turn, estimator);
                }
            }
            archiver.push_turn(turn);
        }

        let (_, hook) = archiver.archive().await.map_err(|e| {
            ArchiveError::Archive(format!("归档失败: {e}"))
        })?;
        let summary = SummaryView::from(&hook);

        // 8. 触发搜索索引
        if let Some(router) = &self.session_search {
            router.index_hook(session_id, &hook, &turns_text).await;
        }

        tracing::info!(
            session = %session_id,
            hook_id = %summary.hook_id,
            tokens = summary.token_count,
            threshold = logged_threshold,
            hard_limit = logged_hard_limit,
            has_preset = archive_threshold.is_some(),
            "归档成功"
        );

        Ok(ArchiveResult {
            summary,
            turns_text,
            hook,
        })
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 获取当前 archive 阈值
///
/// 优先级：
/// 1. preset.archive_threshold（用户显式覆盖，最高优先级）
/// 2. preset 构建的 CombinedProfile.archive_threshold()
/// 3. 默认 FALLBACK_ARCHIVE_THRESHOLD（v2.54 P15：从 120000 统一为 400000）
pub fn get_archive_threshold(preset: Option<&PresetRequest>) -> usize {
    if let Some(preset_req) = preset {
        if let Some(t) = preset_req.archive_threshold {
            return t;
        }
        if let Ok(combined) = build_combined_from_request(preset_req) {
            return combined.archive_threshold();
        }
    }
    memory_center_core::model::FALLBACK_ARCHIVE_THRESHOLD
}

/// 从 PresetRequest 构建 CombinedProfile
///
/// 抽取自 server `presets::build_combined_from_request`，
/// 供 archive-core 内部复用（不依赖 server 模块）。
fn build_combined_from_request(
    req: &PresetRequest,
) -> Result<memory_center_presets::CombinedProfile, String> {
    memory_center_presets::build_from_strings(
        req.agent.as_deref(),
        req.scenario.as_deref(),
        req.model.as_deref(),
        req.archive_threshold,
        req.summary_template.as_deref(),
    )
    .map_err(|e| e.to_string())
}

// ============================================================================
// 组件构建函数（从环境变量构造，供 sidecar 复用）
// ============================================================================

/// 从环境变量构造 LLM 摘要生成器
///
/// 未配置 `MEMORY_CENTER_GENERATOR_API_URL` 时返回 None（降级为启发式）。
pub fn build_summary_generator(
) -> Option<Arc<dyn memory_center_core::generate::SummaryGenerator>> {
    use memory_center_core::generate::LlmGeneratorConfig;
    use memory_center_llm::HttpSummaryGenerator;

    let config = match LlmGeneratorConfig::from_env() {
        Some(config) => config,
        None => {
            tracing::info!(
                "摘要生成器：未配置 LLM API（MEMORY_CENTER_GENERATOR_API_URL），使用启发式 Summary::from_title"
            );
            return None;
        }
    };

    tracing::info!(
        api_url = %config.api_url,
        model = %config.model,
        max_tokens = config.max_tokens,
        "摘要生成器：LLM API 已配置，启用 HttpSummaryGenerator"
    );

    Some(Arc::new(HttpSummaryGenerator::new(config)))
}

/// 从环境变量构造场景识别器
pub fn build_scenario_detector() -> Arc<memory_center_presets::HybridScenarioDetector> {
    use memory_center_llm::LlmDetectorConfig;
    use memory_center_presets::scenario_detect::HttpScenarioDetector;

    let llm_config = match LlmDetectorConfig::from_env() {
        Some(config) => {
            tracing::info!(
                api_url = %config.api_url,
                model = %config.model,
                "场景识别器：LLM API 已配置，启用关键词 + LLM 兜底"
            );
            Some(Arc::new(HttpScenarioDetector::new(config)))
        }
        None => {
            tracing::info!(
                "场景识别器：未配置 LLM API，仅用关键词规则识别（7 场景 × 15 关键词）"
            );
            None
        }
    };

    Arc::new(memory_center_presets::HybridScenarioDetector::new(llm_config))
}

/// 从环境变量构造 SessionSearchRouter
///
/// 未配置 `MEMORY_CENTER_EMBEDDER_API_URL` 时降级为仅关键词检索。
pub fn build_session_search(
    storage_root: &std::path::Path,
) -> Option<Arc<SessionSearchRouter>> {
    use memory_center_core::semantic::Embedder;
    use memory_center_llm::{EmbedderConfig, HttpEmbedder};

    let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(storage_root.to_path_buf()));

    let embedder_config = match EmbedderConfig::from_env() {
        Some(config) => config,
        None => {
            tracing::info!(
                "语义检索：未配置 Embedder API，降级为仅关键词检索（KeywordOnlyRetriever + storage 懒重建）"
            );
            let router = SessionSearchRouter::new(None, 0).with_storage(storage);
            return Some(Arc::new(router));
        }
    };

    let dim = embedder_config.dim;
    tracing::info!(
        api_url = %embedder_config.api_url,
        model = %embedder_config.model,
        dim,
        "语义检索：Embedder 已配置，启用 session 级混合检索"
    );

    let embedder: Arc<dyn Embedder> = Arc::new(HttpEmbedder::new(embedder_config));
    let router = SessionSearchRouter::new(Some(embedder), dim).with_storage(storage);
    Some(Arc::new(router))
}

/// 从环境变量构造 Token 估算器（v2.54 P16 从 mcp/bootstrap.rs 下沉至此）
///
/// 用于注入 `ArchiveEngine::with_token_estimator`，替换 archive-core 的 `chars/3` 简化估算。
///
/// ## 环境变量
///
/// | 变量 | 说明 | 默认值 |
/// |------|------|--------|
/// | `MEMORY_CENTER_TOKENIZER_MODEL` | 模型名（见 `ModelRegistry::find` 支持的型号） | `deepseek-v4-flash` |
///
/// ## 默认选择 deepseek-v4-flash 的原因
///
/// - DeepSeekApprox tokenizer（cl100k_base + 系数 1.1）对中英文混合场景较准
/// - DeepSeek 模型在中文场景 token 估算更贴近实际
/// - 用户可通过环境变量切换为其他模型（如 `gpt-5.2` 用 O200kBase）
///
/// ## 降级行为
///
/// - 指定模型名不存在 → 回退到 `deepseek-v4-flash`
/// - tiktoken 初始化失败 → `TokenizerKind::build` 内部降级为 CharTokenizer
///
/// ## 返回
///
/// `Arc<dyn Fn(&str) -> usize + Send + Sync>`：可直接传入 `with_token_estimator`
pub fn build_token_estimator_from_env() -> TokenEstimator {
    use memory_center_models::{build_token_estimator, ModelRegistry, ModelVariant};

    let model_name = std::env::var("MEMORY_CENTER_TOKENIZER_MODEL")
        .unwrap_or_else(|_| "deepseek-v4-flash".to_string());

    let model = ModelRegistry::find(&model_name).unwrap_or_else(|| {
        if model_name != "deepseek-v4-flash" {
            tracing::warn!(
                specified = %model_name,
                fallback = "deepseek-v4-flash",
                "指定的 tokenizer 模型不存在，回退到默认模型"
            );
        }
        ModelVariant::deepseek_v4_flash()
    });

    tracing::info!(
        model = %model.name,
        tokenizer = %model.tokenizer.type_name(),
        "Token 估算器：已构造（替换 chars/3 简化估算）"
    );

    let tokenizer = model.build_tokenizer();
    build_token_estimator(tokenizer)
}

/// 从环境变量构造完整 ArchiveEngine（便捷函数）
///
/// 自动注入 SummaryGenerator + ScenarioDetector + SessionSearchRouter + TokenEstimator。
/// 未配置 LLM API 时各组件降级。
///
/// v2.54 P16：追加 `with_token_estimator` 注入，让 sidecar 通过 `build_engine_from_env`
/// 也能获得精确的 token 估算（此前仅 MCP/Server 手动注入，sidecar 漏注入导致用 chars/3）。
pub fn build_engine_from_env(storage_root: PathBuf) -> ArchiveEngine {
    let summary_generator = build_summary_generator();
    let scenario_detector = build_scenario_detector();
    let session_search = build_session_search(&storage_root);
    let token_estimator = build_token_estimator_from_env();

    let mut engine = ArchiveEngine::new(storage_root)
        .with_token_estimator(token_estimator);
    if let Some(gen) = summary_generator {
        engine = engine.with_summary_generator(gen);
    }
    engine = engine.with_scenario_detector(scenario_detector);
    if let Some(router) = session_search {
        engine = engine.with_session_search(router);
    }
    engine
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use memory_center_core::model::{FileChange, Tag, ToolInvocation};
    use uuid::Uuid;

    #[test]
    fn test_engine_new() {
        let engine = ArchiveEngine::new(PathBuf::from("/tmp/test"));
        assert_eq!(engine.storage_root(), std::path::Path::new("/tmp/test"));
        assert!(engine.summary_generator().is_none());
        assert!(engine.session_search().is_none());
    }

    #[test]
    fn test_get_archive_threshold_default() {
        // v2.54 P15：兜底值从 120000 统一为 FALLBACK_ARCHIVE_THRESHOLD（400000）
        let threshold = get_archive_threshold(None);
        assert_eq!(threshold, memory_center_core::model::FALLBACK_ARCHIVE_THRESHOLD);
    }

    #[test]
    fn test_p15_fallback_threshold_consistency() {
        // v2.54 P15：验证三处兜底值一致
        // 1. get_archive_threshold(None) 应返回 FALLBACK_ARCHIVE_THRESHOLD
        let from_get = get_archive_threshold(None);
        let const_val = memory_center_core::model::FALLBACK_ARCHIVE_THRESHOLD;
        assert_eq!(from_get, const_val, "get_archive_threshold 兜底应等于 FALLBACK_ARCHIVE_THRESHOLD");

        // 2. ArchiveConfig::default() 的 token_threshold 应等于 FALLBACK_ARCHIVE_THRESHOLD
        let config_default = memory_center_core::model::ArchiveConfig::default();
        assert_eq!(config_default.token_threshold, const_val, "ArchiveConfig::default token_threshold 应等于 FALLBACK_ARCHIVE_THRESHOLD");

        // 3. force_truncate_limit 应为 threshold × HARD_LIMIT_RATIO（v2.54 P19 统一系数来源）
        let expected_hard_limit = (const_val as f32 * memory_center_core::model::HARD_LIMIT_RATIO) as usize;
        assert_eq!(config_default.force_truncate_limit, expected_hard_limit, "force_truncate_limit 应为 threshold × HARD_LIMIT_RATIO");

        // 4. presets 层 DEFAULT_ARCHIVE_THRESHOLD 应与 FALLBACK_ARCHIVE_THRESHOLD 一致
        let presets_default = memory_center_presets::DEFAULT_ARCHIVE_THRESHOLD;
        assert_eq!(presets_default, const_val, "presets DEFAULT_ARCHIVE_THRESHOLD 应等于 FALLBACK_ARCHIVE_THRESHOLD");
    }

    #[test]
    fn test_p19_hard_limit_ratio_consistency() {
        // v2.54 P19：验证 ArchiveConfig::from_threshold 与 ArchiveStrategy::hard_limit 使用相同系数
        use memory_center_models::variant::{ArchiveStrategy, ModelVariant};

        // 测试多种 threshold 值
        let test_cases = [100_000, 200_000, 400_000, 500_000, 800_000, 1_000_000];

        for threshold in test_cases {
            // ArchiveConfig::from_threshold 计算的 force_truncate_limit
            let config = memory_center_core::model::ArchiveConfig::from_threshold(threshold);
            let config_hard_limit = config.force_truncate_limit;

            // 期望值：threshold × HARD_LIMIT_RATIO
            let expected = (threshold as f32 * memory_center_core::model::HARD_LIMIT_RATIO) as usize;
            assert_eq!(
                config_hard_limit, expected,
                "ArchiveConfig::from_threshold({}).force_truncate_limit 应为 threshold × HARD_LIMIT_RATIO = {}",
                threshold, expected
            );

            // ArchiveStrategy::hard_limit() 应与 ArchiveConfig 使用相同系数
            for strategy in [
                ArchiveStrategy::LargeWindow { threshold },
                ArchiveStrategy::Standard { threshold },
                ArchiveStrategy::SmallWindow { threshold },
            ] {
                let strategy_hard_limit = strategy.hard_limit();
                assert_eq!(
                    strategy_hard_limit, expected,
                    "ArchiveStrategy.hard_limit() (threshold={}) 应为 {} × HARD_LIMIT_RATIO = {}，实际 {}",
                    threshold, threshold, expected, strategy_hard_limit
                );
            }
        }

        // 验证内置 ModelVariant 的 hard_limit 与 from_threshold 一致
        for model in [
            ModelVariant::claude_opus_4_6(), // LargeWindow, threshold=400K
            ModelVariant::claude_opus_4_8(), // Standard, threshold=80K
            ModelVariant::local_default(),   // SmallWindow, threshold=4K
        ] {
            let threshold = model.archive_strategy.threshold();
            let config = memory_center_core::model::ArchiveConfig::from_threshold(threshold);
            assert_eq!(
                model.archive_strategy.hard_limit(),
                config.force_truncate_limit,
                "ModelVariant::{} 的 archive_strategy.hard_limit() 应与 ArchiveConfig::from_threshold({}).force_truncate_limit 一节",
                model.name, threshold
            );
        }
    }

    #[test]
    fn test_get_archive_threshold_user_override() {
        let preset = PresetRequest {
            archive_threshold: Some(50000),
            ..Default::default()
        };
        let threshold = get_archive_threshold(Some(&preset));
        assert_eq!(threshold, 50000);
    }

    #[test]
    fn test_health_check_creates_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let engine = ArchiveEngine::new(tmp.path().to_path_buf());
        assert!(engine.health_check().unwrap());
    }

    // ========================================================================
    // v2.52 阶段 4：TokenEstimator 测试
    // ========================================================================

    /// 构造测试用 MessageTurn（参考 cache.rs make_turn）
    fn make_turn(text: &str, token_count: usize) -> MessageTurn {
        MessageTurn {
            id: Uuid::new_v4(),
            user_message: MessageContent {
                text: Some(text.into()),
                attachments: Vec::new(),
                tool_calls: Vec::new(),
                thinking: None,
                file_changes: Vec::new(),
            },
            llm_message: MessageContent {
                text: Some("LLM 回复".into()),
                attachments: Vec::new(),
                tool_calls: Vec::new(),
                thinking: None,
                file_changes: Vec::new(),
            },
            tags: vec![Tag::Text],
            timestamp: Utc::now(),
            token_count,
            stop_reason: None,
            cost: None,
        }
    }

    #[test]
    fn test_engine_without_token_estimator() {
        // 无 estimator 时 token_estimator 为 None
        let engine = ArchiveEngine::new(PathBuf::from("/tmp/test"));
        assert!(engine.token_estimator.is_none());
    }

    #[test]
    fn test_engine_with_token_estimator() {
        // 注入 estimator 后 token_estimator 为 Some
        let estimator: TokenEstimator = Arc::new(|_text: &str| 42);
        let engine = ArchiveEngine::new(PathBuf::from("/tmp/test"))
            .with_token_estimator(estimator);
        assert!(engine.token_estimator.is_some());
    }

    #[test]
    fn test_estimate_content_tokens_text_only() {
        // estimator: 每个字符 1 token
        let estimator: TokenEstimator = Arc::new(|text: &str| text.chars().count());
        let content = MessageContent {
            text: Some("Hello".to_string()), // 5 chars
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            thinking: None,
            file_changes: Vec::new(),
        };
        assert_eq!(estimate_content_tokens(&content, &estimator), 5);
    }

    #[test]
    fn test_estimate_content_tokens_with_tool_calls_and_thinking() {
        let estimator: TokenEstimator = Arc::new(|text: &str| text.chars().count());
        let content = MessageContent {
            text: Some("Hi".to_string()), // 2
            attachments: Vec::new(),
            tool_calls: vec![ToolInvocation {
                name: "Read".to_string(),
                arguments: "{\"path\":\"a\"}".to_string(), // 12 chars（{"path":"a"}）
                result: "content".to_string(),              // 7 chars
                duration_ms: None,
                status: None,
                error: None,
                call_id: None,
            }],
            thinking: Some("thinking...".to_string()), // 11 chars
            file_changes: Vec::new(),
        };
        // 2 + 12 + 7 + 11 = 32（不算 name 字段，与 estimate_tokens 略有差异）
        assert_eq!(estimate_content_tokens(&content, &estimator), 32);
    }

    #[test]
    fn test_estimate_content_tokens_with_file_changes() {
        let estimator: TokenEstimator = Arc::new(|text: &str| text.chars().count());
        let content = MessageContent {
            text: Some("ab".to_string()), // 2
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            thinking: None,
            file_changes: vec![FileChange {
                file_path: "/tmp/test.rs".to_string(),
                status: Some("modified".to_string()),
                additions: Some(10),
                deletions: Some(2),
                patch: Some("@@ diff @@".to_string()), // 10 chars
                hash: None,
            }],
        };
        // 2 + 10 = 12
        assert_eq!(estimate_content_tokens(&content, &estimator), 12);
    }

    #[test]
    fn test_estimate_turn_tokens() {
        // estimator: 每个字符 1 token
        let estimator: TokenEstimator = Arc::new(|text: &str| text.chars().count());
        let turn = make_turn("User", 0);
        // user="User"(4 chars) + llm="LLM 回复"(6 chars: L,L,M,space,回,复)
        // 4 + 6 = 10
        assert_eq!(estimate_turn_tokens(&turn, &estimator), 10);
    }

    #[test]
    fn test_estimate_turn_tokens_empty_text() {
        // 空 text 时返回最小 1
        let estimator: TokenEstimator = Arc::new(|_text: &str| 0);
        let mut turn = make_turn("", 0);
        turn.user_message.text = None;
        turn.llm_message.text = None;
        // 0 + 0 → .max(1) = 1
        assert_eq!(estimate_turn_tokens(&turn, &estimator), 1);
    }

    #[test]
    fn test_estimator_replaces_chars_div_3() {
        // 验证 estimator 与 chars/3 的差异（中英文混合场景）
        // "Hello 世界" = 8 chars, chars/3 ≈ 2, 但精确 tokenizer 可能更准
        let text = "Hello 世界";
        let chars_div_3 = text.len() / 3; // 字节数 / 3 = 12/3 = 4（字节级）
        let char_count = text.chars().count(); // 8 chars（字符级）
        assert_ne!(chars_div_3, char_count, "chars/3 与 chars().count() 应有差异");

        // 模拟 estimator 用 chars().count()
        let estimator: TokenEstimator = Arc::new(|t: &str| t.chars().count());
        let content = MessageContent {
            text: Some(text.to_string()),
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            thinking: None,
            file_changes: Vec::new(),
        };
        assert_eq!(estimate_content_tokens(&content, &estimator), char_count);
    }
}

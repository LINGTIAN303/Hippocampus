//! # Hippocampus HTTP 服务入口
//!
//! 启动 Axum HTTP 服务，将 Core 的能力暴露为 REST API。
//!
//! ## 语义检索配置（v2.5 批次 7，v2.13 默认值更新）
//!
//! 通过环境变量配置 Embedder API 后，`/search` 端点和归档后自动索引生效：
//!
//! | 环境变量 | 说明 | 默认值 |
//! |---------|------|--------|
//! | `HIPPOCAMPUS_EMBEDDER_API_URL` | Embedding API 地址（OpenAI 兼容 `/v1/embeddings`） | 空（降级为仅关键词） |
//! | `HIPPOCAMPUS_EMBEDDER_API_KEY` | API Key | 空 |
//! | `HIPPOCAMPUS_EMBEDDER_MODEL` | 模型名 | `text-embedding-3-large` |
//! | `HIPPOCAMPUS_EMBEDDER_DIM` | 向量维度 | `3072` |
//! | `HIPPOCAMPUS_EMBEDDER_TIMEOUT` | 超时秒数 | `30` |
//!
//! 未配置 `API_URL` 时，自动降级为 `KeywordOnlyRetriever`（仅 BM25 关键词检索）。
//!
//! ## 冲突检测配置（v2.10，v2.13 默认值更新，v2.14 升级 HybridDetector）
//!
//! | 环境变量 | 说明 | 默认值 |
//! |---------|------|--------|
//! | `HIPPOCAMPUS_DETECTOR_API_URL` | LLM API 地址（OpenAI 兼容 `/v1/chat/completions`） | 空（降级为 HeuristicDetector） |
//! | `HIPPOCAMPUS_DETECTOR_API_KEY` | API Key | 空 |
//! | `HIPPOCAMPUS_DETECTOR_MODEL` | 模型名 | `gpt-5.5-instant` |
//! | `HIPPOCAMPUS_DETECTOR_TIMEOUT` | 超时秒数 | `30` |
//! | `HIPPOCAMPUS_DETECTOR_MAX_TOKENS` | LLM 最大输出 token | `500` |
//!
//! - 未配置 `API_URL`：使用 `HeuristicDetector`（启发式纯算法，三维度检测）
//! - 配置完整：使用 `HybridDetector`（串联 Heuristic + LLM，合并两份报告，v2.14 语义去重默认阈值 0.7）

use hippocampus_server::{create_router, AppState, Config};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

/// 从环境变量读取 Embedder 配置并构造 SessionSearchRouter
///
/// v2.8：替代 v2.5 的全局单例 build_search_components
/// v2.13：使用 `EmbedderConfig::from_env()` 简化环境变量读取
///
/// - 配置完整：每 session 独立 HybridRetriever（关键词 + 向量 + RRF 融合）
/// - 未配置或失败：每 session 独立 KeywordOnlyRetriever（仅关键词，降级模式）
fn build_session_search() -> Option<Arc<hippocampus_server::SessionSearchRouter>> {
    use hippocampus_core::semantic::Embedder;
    use hippocampus_server::{EmbedderConfig, HttpEmbedder, SessionSearchRouter};

    // v2.13：使用 EmbedderConfig::from_env() 统一环境变量读取
    let embedder_config = match EmbedderConfig::from_env() {
        Some(config) => config,
        None => {
            // 降级模式：仅关键词检索（每 session 独立）
            tracing::info!("未配置 Embedder API，降级为仅关键词检索（KeywordOnlyRetriever，session 级隔离）");
            return Some(Arc::new(SessionSearchRouter::new(None, 0)));
        }
    };

    let dim = embedder_config.dim;
    tracing::info!(
        api_url = %embedder_config.api_url,
        model = %embedder_config.model,
        dim = embedder_config.dim,
        "Embedder 已配置，启用 session 级混合检索（HybridRetriever）"
    );

    let embedder: Arc<dyn Embedder> = Arc::new(HttpEmbedder::new(embedder_config));
    Some(Arc::new(SessionSearchRouter::new(Some(embedder), dim)))
}

/// 从环境变量构造冲突检测器（v2.10，v2.13 简化，v2.14 升级 HybridDetector）
///
/// - 配置了 `HIPPOCAMPUS_DETECTOR_API_URL` + `API_KEY`：
///   返回 `HybridDetector`（串联 Heuristic + LLM，合并两份报告，v2.14 语义去重默认阈值 0.7）
/// - 未配置：返回 `HeuristicDetector`（启发式纯算法，无 LLM 依赖）
///
/// ## v2.14 升级说明
///
/// v2.11 引入 `HybridDetector` 时 mcp 已升级，server 遗漏。v2.14 修正：
/// server 与 mcp 对齐，统一使用 `HybridDetector`，享受启发式 + LLM 互补 +
/// v2.12 精确去重 + v2.14 语义去重（字符 Jaccard 相似度 >= 0.7 视为重复）。
fn build_conflict_detector() -> std::sync::Arc<dyn hippocampus_core::conflict::ConflictDetector> {
    use hippocampus_core::conflict::{ConflictDetector, HybridDetector};
    use hippocampus_core::heuristic::HeuristicDetector;
    use hippocampus_server::{HttpLlmDetector, LlmDetectorConfig};

    // v2.13：使用 LlmDetectorConfig::from_env() 统一环境变量读取
    let config = match LlmDetectorConfig::from_env() {
        Some(config) => config,
        None => {
            tracing::info!(
                "冲突检测器：未配置 LLM API，使用 HeuristicDetector（启发式纯算法，三维度检测）"
            );
            return std::sync::Arc::new(HeuristicDetector::new());
        }
    };

    // v2.14：串联 Heuristic + LLM，合并两份报告（与 mcp/main.rs 对齐）
    // - HybridDetector::new() 默认 dedup_threshold = 0.7（中文短句经验值）
    // - 启发式全部保留，LLM 报告中语义重复的不重复加入
    // - LLM 失败时返回空报告，启发式结果仍保留（降级策略）
    let heuristic: std::sync::Arc<dyn ConflictDetector> = std::sync::Arc::new(HeuristicDetector::new());
    let llm: std::sync::Arc<dyn ConflictDetector> = std::sync::Arc::new(HttpLlmDetector::new(config.clone()));
    let hybrid = HybridDetector::new(heuristic, llm);

    tracing::info!(
        api_url = %config.api_url,
        model = %config.model,
        max_tokens = config.max_tokens,
        dedup_threshold = hybrid.dedup_threshold(),
        "冲突检测器：LLM API 已配置，使用 HybridDetector（串联 Heuristic + LLM，失败时降级保留启发式结果）"
    );

    std::sync::Arc::new(hybrid)
}

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hippocampus_server=info,tower_http=info".into()),
        )
        .init();

    let config = Config::default();

    // 确保存储目录存在
    std::fs::create_dir_all(&config.storage_root).expect("创建存储目录失败");

    // v2.8：构造 Session 级索引隔离路由器（替代 v2.5 全局单例）
    let session_search = build_session_search();

    // v2.10：构造冲突检测器（环境变量驱动：LLM 优先，降级为 HeuristicDetector）
    let conflict_detector = Some(build_conflict_detector());

    let state = AppState {
        storage_root: config.storage_root.clone(),
        retriever: None,            // v2.8 起由 session_search 替代
        search_indexer: None,       // v2.8 起由 session_search 替代
        session_search,
        conflict_detector,
    };

    let app = create_router(state).layer(TraceLayer::new_for_http());

    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("Hippocampus HTTP 服务启动于 http://{}", addr);
    tracing::info!("存储根目录: {:?}", config.storage_root);
    tracing::info!("API 端点:");
    tracing::info!("  POST   /api/v1/sessions/{{sid}}/archive");
    tracing::info!("  GET    /api/v1/sessions/{{sid}}/memories/{{hook_id}}");
    tracing::info!("  GET    /api/v1/sessions/{{sid}}/summaries");
    tracing::info!("  GET    /api/v1/sessions/{{sid}}/prompt");
    tracing::info!("  POST   /api/v1/sessions/{{sid}}/compaction");
    tracing::info!("  POST   /api/v1/sessions/{{sid}}/search");
    tracing::info!("  GET    /api/v1/sessions/{{sid}}/memories/{{hook_id}}/conflicts");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

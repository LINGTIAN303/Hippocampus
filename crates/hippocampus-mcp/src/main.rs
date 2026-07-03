//! # Hippocampus MCP Server (stdio)
//!
//! stdio 传输模式的 MCP server 入口。
//! 被 Claude Code / Cursor / Trae 等 MCP 客户端作为子进程拉起。
//!
//! ## 环境变量
//!
//! - `HIPPOCAMPUS_ROOT`：存储根目录（默认 `./data`）
//! - `RUST_LOG`：日志级别（默认 `info`）
//!
//! ## 冲突检测器配置（v2.11，v2.13 默认值更新）
//!
//! | 环境变量 | 说明 | 默认值 |
//! |---------|------|--------|
//! | `HIPPOCAMPUS_DETECTOR_API_URL` | LLM API 地址（OpenAI 兼容 `/v1/chat/completions`） | 空（降级为 HeuristicDetector） |
//! | `HIPPOCAMPUS_DETECTOR_API_KEY` | API Key | 空 |
//! | `HIPPOCAMPUS_DETECTOR_MODEL` | 模型名 | `gpt-5.5-instant` |
//! | `HIPPOCAMPUS_DETECTOR_TIMEOUT` | 超时秒数 | `30` |
//! | `HIPPOCAMPUS_DETECTOR_MAX_TOKENS` | LLM 最大输出 token | `500` |
//!
//! 未配置 `API_URL` 时：注入 `HeuristicDetector`（启发式纯算法，三维度检测）。
//! 配置完整时：注入 `HybridDetector`（串联 Heuristic + LLM，合并两份报告）。

use std::path::PathBuf;
use std::sync::Arc;

use hippocampus_core::conflict::{ConflictDetector, HybridDetector};
use hippocampus_core::heuristic::HeuristicDetector;
use hippocampus_mcp::HippocampusMcp;
use hippocampus_llm::{HttpLlmDetector, LlmDetectorConfig};
use rmcp::ServiceExt;
use rmcp::transport::stdio;

/// 从环境变量构造冲突检测器（v2.11，v2.13 简化）
///
/// - 配置了 `HIPPOCAMPUS_DETECTOR_API_URL` + `API_KEY`：
///   返回 `HybridDetector`（串联 Heuristic + LLM，合并两份报告）
/// - 未配置：返回 `HeuristicDetector`（启发式纯算法，无 LLM 依赖）
fn build_conflict_detector() -> Arc<dyn ConflictDetector> {
    // v2.13：使用 LlmDetectorConfig::from_env() 统一环境变量读取
    let config = match LlmDetectorConfig::from_env() {
        Some(config) => config,
        None => {
            tracing::info!(
                "冲突检测器：未配置 LLM API，使用 HeuristicDetector（启发式纯算法，三维度检测）"
            );
            return Arc::new(HeuristicDetector::new());
        }
    };

    tracing::info!(
        api_url = %config.api_url,
        model = %config.model,
        max_tokens = config.max_tokens,
        "冲突检测器：LLM API 已配置，使用 HybridDetector（串联 Heuristic + LLM，失败时降级保留启发式结果）"
    );

    // v2.11：串联 Heuristic + LLM，合并两份报告
    let heuristic: Arc<dyn ConflictDetector> = Arc::new(HeuristicDetector::new());
    let llm: Arc<dyn ConflictDetector> = Arc::new(HttpLlmDetector::new(config));
    Arc::new(HybridDetector::new(heuristic, llm))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hippocampus_mcp=info".into()),
        )
        .init();

    // 读取存储根目录配置
    let storage_root = PathBuf::from(
        std::env::var("HIPPOCAMPUS_ROOT").unwrap_or_else(|_| "./data".to_string()),
    );

    // v2.11：构造冲突检测器并注入
    let conflict_detector = build_conflict_detector();

    tracing::info!(
        root = %storage_root.display(),
        "启动 Hippocampus MCP server (stdio 传输)"
    );

    // 启动 stdio MCP server
    let service = HippocampusMcp::with_conflict_detector(
        storage_root,
        Some(conflict_detector),
    )
    .serve(stdio())
    .await?;

    service.waiting().await?;

    Ok(())
}

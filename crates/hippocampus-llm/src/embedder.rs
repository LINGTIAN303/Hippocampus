//! # HTTP Embedder 实现（v2.5 批次 7）
//!
//! 基于 [`hippocampus_core::semantic::Embedder`] trait 的 HTTP 实现，
//! 通过调用外部 Embedding API（OpenAI 兼容格式）将文本转换为向量。
//!
//! ## 架构定位
//!
//! - **core**：定义 `Embedder` trait（纯逻辑）
//! - **llm**（本 crate）：实现 `HttpEmbedder`（HTTP IO，依赖 reqwest）
//!
//! ## 调用流程
//!
//! 1. 构造请求体（model + input）
//! 2. POST 到 `/v1/embeddings` 端点
//! 3. 解析响应：`data[0].embedding`
//! 4. 返回 `Vec<f32>`
//!
//! ## OpenAI 兼容 API
//!
//! ```http
//! POST /v1/embeddings
//! Authorization: Bearer <api_key>
//! Content-Type: application/json
//!
//! {
//!   "model": "text-embedding-3-small",
//!   "input": "要向量化的文本"
//! }
//! ```
//!
//! 响应：
//! ```json
//! {
//!   "data": [
//!     { "embedding": [0.1, -0.2, ...], "index": 0 }
//!   ]
//! }
//! ```

use hippocampus_core::semantic::Embedder;
use std::time::Duration;

/// Embedder 配置
///
/// 定义调用外部 Embedding API 的参数。
#[derive(Debug, Clone)]
pub struct EmbedderConfig {
    /// API 端点 URL（如 https://api.openai.com/v1/embeddings）
    pub api_url: String,
    /// API Key（Bearer token）
    pub api_key: String,
    /// 模型名称（如 text-embedding-3-large / text-embedding-3-small）
    pub model: String,
    /// 向量维度（如 3072 for text-embedding-3-large）
    pub dim: usize,
    /// 请求超时（秒），默认 30
    pub timeout_secs: u64,
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            api_url: String::new(),
            api_key: String::new(),
            // v2.13：默认值更新为 text-embedding-3-large（2026 年 RAG 架构首推，性能优于 3-small）
            model: "text-embedding-3-large".into(),
            dim: 3072,
            timeout_secs: 30,
        }
    }
}

impl EmbedderConfig {
    /// 环境变量前缀
    pub const ENV_PREFIX: &'static str = "HIPPOCAMPUS_EMBEDDER";

    /// 从环境变量构造配置（v2.13 新增）
    ///
    /// 读取以下环境变量（前缀 `HIPPOCAMPUS_EMBEDDER_`）：
    ///
    /// | 环境变量 | 字段 | 默认值 |
    /// |---------|------|--------|
    /// | `_API_URL` | `api_url` | 必填（缺失返回 None） |
    /// | `_API_KEY` | `api_key` | 必填（缺失返回 None） |
    /// | `_MODEL` | `model` | `text-embedding-3-large` |
    /// | `_DIM` | `dim` | `3072` |
    /// | `_TIMEOUT` | `timeout_secs` | `30` |
    ///
    /// ## 返回
    ///
    /// - `Some(config)`：`api_url` 和 `api_key` 均非空
    /// - `None`：`api_url` 或 `api_key` 为空（调用方应降级为 KeywordOnlyRetriever）
    pub fn from_env() -> Option<Self> {
        let api_url = std::env::var(format!("{}_API_URL", Self::ENV_PREFIX)).ok()?;
        let api_key = std::env::var(format!("{}_API_KEY", Self::ENV_PREFIX)).ok()?;
        if api_url.is_empty() || api_key.is_empty() {
            return None;
        }

        let config = Self {
            api_url,
            api_key,
            model: std::env::var(format!("{}_MODEL", Self::ENV_PREFIX))
                .unwrap_or_else(|_| Self::default().model),
            dim: std::env::var(format!("{}_DIM", Self::ENV_PREFIX))
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3072),
            timeout_secs: std::env::var(format!("{}_TIMEOUT", Self::ENV_PREFIX))
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
        };
        Some(config)
    }
}

/// HTTP Embedder
///
/// 通过调用外部 Embedding API（OpenAI 兼容格式）将文本转换为向量。
///
/// ## 错误处理
///
/// - 网络错误 / API 错误：返回 `Error::Storage`（HybridRetriever 会降级为仅关键词检索）
/// - 超时：按配置 `timeout_secs` 处理
pub struct HttpEmbedder {
    /// 配置
    config: EmbedderConfig,
    /// HTTP 客户端
    client: reqwest::Client,
}

impl HttpEmbedder {
    /// 创建新的 HTTP Embedder
    pub fn new(config: EmbedderConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { config, client }
    }
}

#[async_trait::async_trait]
impl Embedder for HttpEmbedder {
    fn dim(&self) -> usize {
        self.config.dim
    }

    async fn embed(&self, text: &str) -> hippocampus_core::Result<Vec<f32>> {
        // 未配置 API URL 时返回错误（触发降级）
        if self.config.api_url.is_empty() {
            return Err(hippocampus_core::Error::Storage(
                "Embedder 未配置 api_url".into(),
            ));
        }

        let request_body = serde_json::json!({
            "model": self.config.model,
            "input": text,
        });

        let resp = self
            .client
            .post(&self.config.api_url)
            .bearer_auth(&self.config.api_key)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                hippocampus_core::Error::Storage(format!("Embedding API 请求失败: {}", e))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(status = %status, body = %body, "Embedding API 返回错误状态");
            return Err(hippocampus_core::Error::Storage(format!(
                "Embedding API 错误: {} {}",
                status, body
            )));
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| hippocampus_core::Error::Storage(format!("Embedding API 响应解析失败: {}", e)))?;

        // OpenAI 兼容格式：data[0].embedding
        let embedding = resp_json
            .get("data")
            .and_then(|d| d.get(0))
            .and_then(|d| d.get("embedding"))
            .and_then(|e| e.as_array())
            .ok_or_else(|| {
                hippocampus_core::Error::Storage("Embedding API 响应格式错误：缺少 data[0].embedding".into())
            })?;

        // 转换为 Vec<f32>
        let vector: Vec<f32> = embedding
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect();

        // 维度校验
        if vector.len() != self.config.dim {
            tracing::warn!(
                expected = self.config.dim,
                got = vector.len(),
                "Embedding 维度与配置不符"
            );
            // 不返回错误，让 VectorIndex 的维度校验处理
        }

        Ok(vector)
    }

    async fn embed_batch(&self, texts: &[&str]) -> hippocampus_core::Result<Vec<Vec<f32>>> {
        // 未配置时降级为逐条调用（触发降级）
        if self.config.api_url.is_empty() {
            return Err(hippocampus_core::Error::Storage(
                "Embedder 未配置 api_url".into(),
            ));
        }

        // OpenAI 兼容 API 支持批量：input 为字符串数组
        let request_body = serde_json::json!({
            "model": self.config.model,
            "input": texts,
        });

        let resp = self
            .client
            .post(&self.config.api_url)
            .bearer_auth(&self.config.api_key)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                hippocampus_core::Error::Storage(format!("Embedding API 批量请求失败: {}", e))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(hippocampus_core::Error::Storage(format!(
                "Embedding API 错误: {} {}",
                status, body
            )));
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| hippocampus_core::Error::Storage(format!("Embedding API 响应解析失败: {}", e)))?;

        // 解析 data 数组（按 index 排序）
        let data = resp_json
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| {
                hippocampus_core::Error::Storage("Embedding API 响应格式错误：缺少 data 数组".into())
            })?;

        let mut results: Vec<(usize, Vec<f32>)> = Vec::with_capacity(data.len());
        for item in data {
            let index = item
                .get("index")
                .and_then(|i| i.as_u64())
                .unwrap_or(0) as usize;
            let embedding = item
                .get("embedding")
                .and_then(|e| e.as_array())
                .ok_or_else(|| {
                    hippocampus_core::Error::Storage("Embedding API 响应格式错误：缺少 embedding".into())
                })?;
            let vector: Vec<f32> = embedding
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect();
            results.push((index, vector));
        }

        // 按 index 排序，确保顺序与输入一致
        results.sort_by_key(|(i, _)| *i);

        Ok(results.into_iter().map(|(_, v)| v).collect())
    }

    fn is_normalized(&self) -> bool {
        // OpenAI text-embedding-3 系列返回的向量已归一化
        // 其他模型可能未归一化，这里默认 true（多数现代 embedding 模型都归一化）
        true
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedder_config_default() {
        let config = EmbedderConfig::default();
        // v2.13：默认值更新为 text-embedding-3-large + 3072 维
        assert_eq!(config.model, "text-embedding-3-large");
        assert_eq!(config.dim, 3072);
        assert_eq!(config.timeout_secs, 30);
        assert!(config.api_url.is_empty());
    }

    #[tokio::test]
    async fn test_embed_without_api_url_returns_error() {
        let config = EmbedderConfig::default(); // api_url 为空
        let embedder = HttpEmbedder::new(config);
        let result = embedder.embed("测试文本").await;
        assert!(result.is_err(), "未配置 api_url 应返回错误");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("未配置 api_url"));
    }

    #[tokio::test]
    async fn test_embed_batch_without_api_url_returns_error() {
        let config = EmbedderConfig::default();
        let embedder = HttpEmbedder::new(config);
        let result = embedder.embed_batch(&["文本1", "文本2"]).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_embedder_dim() {
        let config = EmbedderConfig {
            dim: 768,
            ..Default::default()
        };
        let embedder = HttpEmbedder::new(config);
        assert_eq!(embedder.dim(), 768);
    }

    #[test]
    fn test_embedder_is_normalized() {
        let embedder = HttpEmbedder::new(EmbedderConfig::default());
        assert!(embedder.is_normalized());
    }

    #[test]
    fn test_embedder_config_custom() {
        let config = EmbedderConfig {
            api_url: "https://api.example.com/v1/embeddings".into(),
            api_key: "sk-test-key".into(),
            model: "text-embedding-ada-002".into(),
            dim: 1536,
            timeout_secs: 60,
        };
        let embedder = HttpEmbedder::new(config);
        assert_eq!(embedder.dim(), 1536);
    }
}

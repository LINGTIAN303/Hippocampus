//! HTTP 客户端封装 - 调用 MemoryCenter REST API

use reqwest::Client;
use serde::{Deserialize, Serialize};

/// MemoryCenter HTTP 客户端
pub struct McClient {
    base_url: String,
    api_key: Option<String>,
    http: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryItem {
    pub hook_id: String,
    pub memory_id: String,
    pub summary_title: String,
    #[serde(default)]
    pub abstract_text: Option<String>,
    #[serde(default)]
    pub key_facts: Vec<String>,
    #[serde(default)]
    pub key_entities: Vec<String>,
    pub tags: Vec<String>,
    pub archived_at: String,
    pub period: String,
    #[serde(default)]
    pub token_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummariesResponse {
    pub summaries: Vec<SummaryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFile {
    pub hook_id: String,
    pub created_at: String,
    pub turns: Vec<TurnData>,
    pub summary: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnData {
    pub id: String,
    pub timestamp: Option<String>,
    pub user_message: MessageContent,
    pub llm_message: MessageContent,
    pub tags: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageContent {
    pub text: String,
    #[serde(default)]
    pub tool_calls: Vec<serde_json::Value>,
    #[serde(default)]
    pub thinking: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub top_k: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub hook_id: String,
    pub score: f64,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchHit>,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptResponse {
    pub prompt: String,
}

impl McClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .no_proxy()
                .build()
                .expect("HTTP 客户端创建失败"),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn add_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(key) = &self.api_key {
            req.header("Authorization", format!("Bearer {}", key))
        } else {
            req
        }
    }

    /// 获取 session 的所有摘要
    pub async fn get_summaries(&self, session_id: &str) -> Result<Vec<SummaryItem>, String> {
        let url = self.url(&format!("/api/v1/sessions/{}/summaries", session_id));
        let resp = self
            .add_auth(self.http.get(&url))
            .send()
            .await
            .map_err(|e| format!("请求失败: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }

        // 先尝试数组格式，再尝试 { summaries: [...] } 格式
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if let Ok(arr) = serde_json::from_str::<Vec<SummaryItem>>(&text) {
            return Ok(arr);
        }
        if let Ok(obj) = serde_json::from_str::<SummariesResponse>(&text) {
            return Ok(obj.summaries);
        }
        Ok(vec![])
    }

    /// 获取单个记忆文件
    pub async fn get_memory(&self, session_id: &str, hook_id: &str) -> Result<MemoryFile, String> {
        let url = self.url(&format!(
            "/api/v1/sessions/{}/memories/{}",
            session_id, hook_id
        ));
        let resp = self
            .add_auth(self.http.get(&url))
            .send()
            .await
            .map_err(|e| format!("请求失败: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }

        resp.json::<MemoryFile>()
            .await
            .map_err(|e| format!("解析失败: {e}"))
    }

    /// 语义检索
    pub async fn search(
        &self,
        session_id: &str,
        query: &str,
        top_k: usize,
    ) -> Result<SearchResponse, String> {
        let url = self.url(&format!("/api/v1/sessions/{}/search", session_id));
        let body = SearchRequest {
            query: query.to_string(),
            top_k: Some(top_k),
        };
        let resp = self
            .add_auth(self.http.post(&url).json(&body))
            .send()
            .await
            .map_err(|e| format!("请求失败: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP {status}: {text}"));
        }

        resp.json::<SearchResponse>()
            .await
            .map_err(|e| format!("解析失败: {e}"))
    }

    /// 获取 prompt
    pub async fn get_prompt(&self, session_id: &str) -> Result<String, String> {
        let url = self.url(&format!("/api/v1/sessions/{}/prompt", session_id));
        let resp = self
            .add_auth(self.http.get(&url))
            .send()
            .await
            .map_err(|e| format!("请求失败: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }

        let text = resp.text().await.map_err(|e| e.to_string())?;
        // 尝试解析 JSON { "prompt": "..." } 格式
        if let Ok(obj) = serde_json::from_str::<PromptResponse>(&text) {
            return Ok(obj.prompt);
        }
        Ok(text)
    }

    /// 健康检查
    pub async fn health_check(&self) -> Result<bool, String> {
        let url = self.url("/api/v1/sessions/health-check/summaries");
        let resp = self
            .add_auth(self.http.get(&url))
            .send()
            .await
            .map_err(|e| format!("连接失败: {e}"))?;
        Ok(resp.status().is_success() || resp.status().as_u16() == 404)
    }
}

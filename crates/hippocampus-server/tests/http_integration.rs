//! # HTTP API 全链路集成测试
//!
//! 启动真实 Axum HTTP 服务（随机端口），用 reqwest 客户端验证 5 个端点的全链路：
//! - archive → summaries → retrieve → prompt 全闭环
//! - 错误处理（空 turns / 不存在 hook_id / 无效 period）
//! - 周期任务全流程（weekly_merge / monthly_evict）
//! - 会话隔离 / 项目隔离

use hippocampus_server::{create_router, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::net::TcpListener;

// ============================================================================
// 测试辅助
// ============================================================================

/// 测试用 HTTP 服务句柄
///
/// 持有临时目录（防止存储被清理）和服务端任务句柄
struct TestServer {
    /// 基础 URL（如 http://127.0.0.1:54321）
    base_url: String,
    /// 临时存储目录（drop 时自动清理）
    _tmpdir: TempDir,
}

impl TestServer {
    /// 启动一个新的测试服务（随机端口 + 独立临时目录）
    async fn start() -> Self {
        let tmpdir = TempDir::new().expect("创建临时目录失败");
        let storage_root: PathBuf = tmpdir.path().to_path_buf();

        let state = AppState { storage_root };
        let app = create_router(state);

        // 绑定到随机端口，避免测试间冲突
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("绑定端口失败");
        let addr = listener.local_addr().expect("获取地址失败");
        let base_url = format!("http://{}", addr);

        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("服务异常退出");
        });

        Self {
            base_url,
            _tmpdir: tmpdir,
        }
    }

    /// 拼接完整 URL
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

/// 构造一个最小合法 MessageTurn JSON
fn make_turn_json(user_text: &str, llm_text: &str, tokens: usize) -> Value {
    json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "user_message": {
            "text": user_text,
            "attachments": [],
            "tool_calls": [],
            "thinking": null
        },
        "llm_message": {
            "text": llm_text,
            "attachments": [],
            "tool_calls": [],
            "thinking": null
        },
        "tags": [{"kind": "Text"}],
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "token_count": tokens
    })
}

/// 构造一批 turns（n 个）
fn make_turns_json(n: usize, base_tokens: usize) -> Vec<Value> {
    (0..n)
        .map(|i| {
            make_turn_json(
                &format!("用户消息 #{}", i),
                &format!("助手回复 #{}", i),
                base_tokens + i,
            )
        })
        .collect()
}

// ============================================================================
// 基础端点测试
// ============================================================================

#[tokio::test]
async fn test_archive_success() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    let body = json!({
        "turns": make_turns_json(3, 100),
        "project_id": null
    });

    let resp = client
        .post(server.url("/api/v1/sessions/sess-1/archive"))
        .json(&body)
        .send()
        .await
        .expect("请求失败");

    assert_eq!(resp.status(), 200);

    let summary: Value = resp.json().await.expect("解析响应失败");
    assert!(!summary["hook_id"].as_str().unwrap().is_empty());
    assert!(!summary["memory_file_id"].as_str().unwrap().is_empty());
    assert_eq!(summary["period"].as_str().unwrap(), "daily");
    assert_eq!(summary["token_count"].as_u64().unwrap(), 303); // 100+101+102
}

#[tokio::test]
async fn test_archive_empty_turns_returns_400() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    let body = json!({ "turns": [], "project_id": null });

    let resp = client
        .post(server.url("/api/v1/sessions/sess-1/archive"))
        .json(&body)
        .send()
        .await
        .expect("请求失败");

    assert_eq!(resp.status(), 400);
    let err: Value = resp.json().await.expect("解析错误响应失败");
    assert_eq!(err["error"]["code"].as_str().unwrap(), "BAD_REQUEST");
    assert!(err["error"]["message"]
        .as_str()
        .unwrap()
        .contains("turns 不能为空"));
}

#[tokio::test]
async fn test_summaries_empty_session() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(server.url("/api/v1/sessions/never-exist/summaries"))
        .send()
        .await
        .expect("请求失败");

    assert_eq!(resp.status(), 200);
    let arr: Vec<Value> = resp.json().await.expect("解析响应失败");
    assert!(arr.is_empty());
}

#[tokio::test]
async fn test_summaries_after_archive() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    // 归档 2 次
    for _ in 0..2 {
        let body = json!({
            "turns": make_turns_json(2, 100),
            "project_id": null
        });
        client
            .post(server.url("/api/v1/sessions/sess-a/archive"))
            .json(&body)
            .send()
            .await
            .expect("请求失败");
    }

    let resp = client
        .get(server.url("/api/v1/sessions/sess-a/summaries"))
        .send()
        .await
        .expect("请求失败");

    assert_eq!(resp.status(), 200);
    let arr: Vec<Value> = resp.json().await.expect("解析响应失败");
    assert_eq!(arr.len(), 2);
    // 所有摘要都应是 daily 周期
    for s in &arr {
        assert_eq!(s["period"].as_str().unwrap(), "daily");
    }
}

// ============================================================================
// 检索测试
// ============================================================================

#[tokio::test]
async fn test_retrieve_memory_full_chain() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    // 1. 归档
    let body = json!({
        "turns": make_turns_json(3, 50),
        "project_id": null
    });
    let resp = client
        .post(server.url("/api/v1/sessions/sess-r/archive"))
        .json(&body)
        .send()
        .await
        .expect("请求失败");
    let summary: Value = resp.json().await.expect("解析响应失败");
    let hook_id = summary["hook_id"].as_str().unwrap();

    // 2. 通过 hook_id 检索完整记忆
    let url = format!("/api/v1/sessions/sess-r/memories/{}", hook_id);
    let resp = client
        .get(server.url(&url))
        .send()
        .await
        .expect("请求失败");

    assert_eq!(resp.status(), 200);
    let memory: Value = resp.json().await.expect("解析响应失败");
    assert_eq!(memory["turns"].as_array().unwrap().len(), 3);
    assert_eq!(memory["session_id"].as_str().unwrap(), "sess-r");
    assert_eq!(memory["total_tokens"].as_u64().unwrap(), 153); // 50+51+52
}

#[tokio::test]
async fn test_retrieve_nonexistent_hook_returns_404() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4().to_string();
    let url = format!("/api/v1/sessions/sess-x/memories/{}", fake_id);
    let resp = client
        .get(server.url(&url))
        .send()
        .await
        .expect("请求失败");

    assert_eq!(resp.status(), 404);
    let err: Value = resp.json().await.expect("解析错误响应失败");
    assert_eq!(err["error"]["code"].as_str().unwrap(), "NOT_FOUND");
    assert!(err["error"]["message"]
        .as_str()
        .unwrap()
        .contains("未找到"));
}

// ============================================================================
// Prompt 渲染测试
// ============================================================================

#[tokio::test]
async fn test_render_prompt_empty() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(server.url("/api/v1/sessions/empty-sess/prompt"))
        .send()
        .await
        .expect("请求失败");

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("解析响应失败");
    assert_eq!(body["prompt"].as_str().unwrap(), "");
}

#[tokio::test]
async fn test_render_prompt_with_memory() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    // 归档一次
    let body = json!({
        "turns": make_turns_json(2, 100),
        "project_id": null
    });
    client
        .post(server.url("/api/v1/sessions/sess-p/archive"))
        .json(&body)
        .send()
        .await
        .expect("请求失败");

    // 渲染 prompt
    let resp = client
        .get(server.url("/api/v1/sessions/sess-p/prompt"))
        .send()
        .await
        .expect("请求失败");

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("解析响应失败");
    let prompt = body["prompt"].as_str().unwrap();
    assert!(!prompt.is_empty());
    assert!(prompt.contains("可用记忆索引"));
    assert!(prompt.contains("近期记忆"));
}

// ============================================================================
// 周期任务测试
// ============================================================================

#[tokio::test]
async fn test_compaction_invalid_period_returns_400() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    let body = json!({ "period": "yearly", "project_id": null });
    let resp = client
        .post(server.url("/api/v1/sessions/sess-c/compaction"))
        .json(&body)
        .send()
        .await
        .expect("请求失败");

    assert_eq!(resp.status(), 400);
    let err: Value = resp.json().await.expect("解析错误响应失败");
    assert_eq!(err["error"]["code"].as_str().unwrap(), "BAD_REQUEST");
    assert!(err["error"]["message"]
        .as_str()
        .unwrap()
        .contains("无效的 period"));
}

#[tokio::test]
async fn test_compaction_weekly_without_daily_returns_500() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    // 无任何归档，直接 weekly_merge
    let body = json!({ "period": "weekly", "project_id": null });
    let resp = client
        .post(server.url("/api/v1/sessions/sess-w/compaction"))
        .json(&body)
        .send()
        .await
        .expect("请求失败");

    assert_eq!(resp.status(), 500);
    let err: Value = resp.json().await.expect("解析错误响应失败");
    assert_eq!(err["error"]["code"].as_str().unwrap(), "INTERNAL_ERROR");
}

#[tokio::test]
async fn test_compaction_full_workflow() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    // 1. 归档多次（产生 daily 记忆）
    for _ in 0..3 {
        let body = json!({
            "turns": make_turns_json(2, 100),
            "project_id": null
        });
        let resp = client
            .post(server.url("/api/v1/sessions/sess-fw/archive"))
            .json(&body)
            .send()
            .await
            .expect("请求失败");
        assert_eq!(resp.status(), 200);
    }

    // 2. 周级合并
    let body = json!({ "period": "weekly", "project_id": null });
    let resp = client
        .post(server.url("/api/v1/sessions/sess-fw/compaction"))
        .json(&body)
        .send()
        .await
        .expect("请求失败");
    assert_eq!(resp.status(), 200);
    let weekly_result: Value = resp.json().await.expect("解析响应失败");
    assert_eq!(weekly_result["period"].as_str().unwrap(), "weekly");
    assert!(weekly_result["total_turns"].as_u64().unwrap() != 0);
    assert!(weekly_result["hooks_count"].as_u64().unwrap() != 0);

    // 3. 再归档几次产生多个 weekly（用于月级淘汰）
    // 注意：monthly_evict 需要至少 1 个 weekly 文件，这里已有 1 个
    let body = json!({ "period": "monthly", "project_id": null });
    let resp = client
        .post(server.url("/api/v1/sessions/sess-fw/compaction"))
        .json(&body)
        .send()
        .await
        .expect("请求失败");
    assert_eq!(resp.status(), 200);
    let monthly_result: Value = resp.json().await.expect("解析响应失败");
    assert_eq!(monthly_result["period"].as_str().unwrap(), "monthly");
    assert!(monthly_result["total_turns"].as_u64().unwrap() != 0);
}

// ============================================================================
// 隔离性测试
// ============================================================================

#[tokio::test]
async fn test_session_isolation() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    // 会话 A 归档
    let body = json!({ "turns": make_turns_json(2, 100), "project_id": null });
    client
        .post(server.url("/api/v1/sessions/sess-iso-a/archive"))
        .json(&body)
        .send()
        .await
        .expect("请求失败");

    // 会话 B 查 summaries 应为空
    let resp = client
        .get(server.url("/api/v1/sessions/sess-iso-b/summaries"))
        .send()
        .await
        .expect("请求失败");
    let arr: Vec<Value> = resp.json().await.expect("解析响应失败");
    assert!(arr.is_empty(), "会话 B 不应看到会话 A 的记忆");

    // 会话 A 查 summaries 应有 1 个
    let resp = client
        .get(server.url("/api/v1/sessions/sess-iso-a/summaries"))
        .send()
        .await
        .expect("请求失败");
    let arr: Vec<Value> = resp.json().await.expect("解析响应失败");
    assert_eq!(arr.len(), 1);
}

#[tokio::test]
async fn test_project_id_isolation() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    // project-A 归档
    let body = json!({
        "turns": make_turns_json(2, 100),
        "project_id": "proj-a"
    });
    client
        .post(server.url("/api/v1/sessions/sess-proj/archive"))
        .json(&body)
        .send()
        .await
        .expect("请求失败");

    // project-B 查 summaries 应为空
    let resp = client
        .get(server.url("/api/v1/sessions/sess-proj/summaries?project_id=proj-b"))
        .send()
        .await
        .expect("请求失败");
    let arr: Vec<Value> = resp.json().await.expect("解析响应失败");
    assert!(arr.is_empty(), "project-B 不应看到 project-A 的记忆");

    // project-A 查 summaries 应有 1 个
    let resp = client
        .get(server.url("/api/v1/sessions/sess-proj/summaries?project_id=proj-a"))
        .send()
        .await
        .expect("请求失败");
    let arr: Vec<Value> = resp.json().await.expect("解析响应失败");
    assert_eq!(arr.len(), 1);
}

// ============================================================================
// 完整工作流测试
// ============================================================================

#[tokio::test]
async fn test_full_agent_workflow() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();
    let sid = "agent-full";

    // 1. 模拟 Agent 一轮对话：归档
    let body = json!({
        "turns": make_turns_json(5, 200),
        "project_id": "demo-project"
    });
    let resp = client
        .post(server.url(&format!("/api/v1/sessions/{}/archive", sid)))
        .json(&body)
        .send()
        .await
        .expect("归档失败");
    assert_eq!(resp.status(), 200);
    let summary: Value = resp.json().await.expect("解析摘要失败");
    let hook_id = summary["hook_id"].as_str().unwrap().to_string();

    // 2. 获取摘要列表（注入 system prompt 用）
    let resp = client
        .get(server.url(&format!("/api/v1/sessions/{}/summaries?project_id=demo-project", sid)))
        .send()
        .await
        .expect("获取摘要失败");
    assert_eq!(resp.status(), 200);
    let summaries: Vec<Value> = resp.json().await.expect("解析摘要失败");
    assert_eq!(summaries.len(), 1);

    // 3. 渲染 system prompt
    let resp = client
        .get(server.url(&format!("/api/v1/sessions/{}/prompt?project_id=demo-project", sid)))
        .send()
        .await
        .expect("渲染 prompt 失败");
    assert_eq!(resp.status(), 200);
    let prompt_body: Value = resp.json().await.expect("解析 prompt 失败");
    assert!(prompt_body["prompt"].as_str().unwrap().contains("可用记忆索引"));

    // 4. LLM 通过 tool 主动检索详细记忆
    let resp = client
        .get(server.url(&format!(
            "/api/v1/sessions/{}/memories/{}?project_id=demo-project",
            sid, hook_id
        )))
        .send()
        .await
        .expect("检索记忆失败");
    assert_eq!(resp.status(), 200);
    let memory: Value = resp.json().await.expect("解析记忆失败");
    assert_eq!(memory["turns"].as_array().unwrap().len(), 5);
    assert_eq!(memory["session_id"].as_str().unwrap(), sid);
}

//! # 通用数据类型（v2.46 新增，从 sidecar/archive.rs 迁入）
//!
//! 这些类型原本定义在 `memory-center-sidecar/src/archive.rs`，
//! 现提升到 adapter crate，让 trait 方法可以返回这些类型，
//! 同时让 sidecar 和其他未来 crate 共享同一套类型定义。
//!
//! ## 与服务端的兼容性
//!
//! 所有类型只派生 `Serialize`（sidecar 只产出数据，不需要反序列化），
//! 与服务器 `MessageTurn` / `MessageContent` / `ToolInvocation` / `FileChange`
//! JSON 格式兼容。服务器反序列化时用 `#[serde(default)]` 补全缺失字段。

use serde::Serialize;

/// sidecar 本地的轮次结构
///
/// 与服务器 `MessageTurn` JSON 格式兼容，但只包含 sidecar 能产出的字段。
/// 服务器反序列化时用 `#[serde(default)]` 补全 id/timestamp/tags/token_count。
///
/// ## 字段演进
///
/// - v2.43: 初始版本（user_message + llm_message）
/// - v2.44: 加 token_count（单轮实际 token 消耗）
/// - v2.45: 加 stop_reason / cost
#[derive(Serialize, Clone, Debug)]
pub struct SidecarTurn {
    pub user_message: SidecarContent,
    pub llm_message: SidecarContent,
    /// 单轮实际 token 消耗
    ///
    /// 来源：opencode part 表 step-finish 的 `input + output + reasoning`。
    /// None 表示未提取到（旧版 opencode 或解析失败），服务器会按内容估算。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<usize>,
    /// LLM 停止原因
    ///
    /// 通用值：`"stop"` / `"length"` / `"tool_use"` / `"max_tokens"`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// 单轮成本（单位：美元）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
}

/// sidecar 本地的消息内容结构
///
/// 与服务器 `MessageContent` JSON 格式兼容。
/// `attachments` 字段 sidecar 暂不产生，序列化时省略（服务器默认空 Vec）。
#[derive(Serialize, Clone, Debug)]
pub struct SidecarContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<SidecarToolCall>,
    /// 文件变更记录
    ///
    /// 来源：opencode patch part + user 消息的 summary.diffs。
    /// 不同 Agent adapter 按能力填充。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_changes: Vec<SidecarFileChange>,
}

/// sidecar 本地的工具调用结构
///
/// 与服务器 `ToolInvocation` JSON 格式兼容。
#[derive(Serialize, Clone, Debug)]
pub struct SidecarToolCall {
    pub name: String,
    pub arguments: String,
    pub result: String,
    /// 工具执行状态
    ///
    /// 通用值：`"completed"` / `"error"` / `"running"` / `"pending"`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// 错误信息（仅 status="error" 时有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 工具调用唯一标识
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    /// 调用耗时（毫秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// sidecar 本地的文件变更记录
///
/// 与服务器 `FileChange` JSON 格式兼容。
/// 来源：opencode patch part（hash + files）+ user 消息的 summary.diffs。
#[derive(Serialize, Clone, Debug)]
pub struct SidecarFileChange {
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additions: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deletions: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

impl SidecarContent {
    /// 创建仅含文本的内容
    pub fn text_only(text: String) -> Self {
        Self {
            text: if text.is_empty() { None } else { Some(text) },
            thinking: None,
            tool_calls: Vec::new(),
            file_changes: Vec::new(),
        }
    }
}

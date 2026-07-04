//! # Agent 家族枚举（family / variant 分离设计）
//!
//! family 稳定（本模块维护），variant 高频迭代（字符串保存）。
//!
//! ## 11 个主流 Agent family
//!
//! 4 主流（有完整预设）：ClaudeCode / Cursor / Trae / Codex
//! 7 待补（generic 预设）：Zcode / OpenCode / Qoder / WorkBuddy / CatPaw / OpenClaw / Marvis
//! 1 兜底：Custom(String)

use serde::{Deserialize, Serialize};

/// Agent 代理工具家族（稳定枚举）
///
/// 11 个主流 Agent + Custom 兜底。variant（型号）由 [`crate::AgentProfile`]
/// 单独保存为字符串，避免 family 枚举频繁变动。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum AgentFamily {
    /// Anthropic Claude Code CLI（有 /compact 命令）
    ClaudeCode,
    /// Cursor IDE（chat 压缩机制）
    Cursor,
    /// ByteDance Trae IDE（conversation 压缩）
    Trae,
    /// OpenAI Codex CLI（rolling 压缩，无摘要）
    Codex,
    /// Zcode
    Zcode,
    /// OpenCode
    OpenCode,
    /// Qoder
    Qoder,
    /// WorkBuddy
    WorkBuddy,
    /// CatPaw
    CatPaw,
    /// OpenClaw
    OpenClaw,
    /// Marvis
    Marvis,
    /// 用户自定义兜底（支持未来扩展）
    Custom(String),
}

impl AgentFamily {
    /// 返回所有内置 family（11 个，不含 Custom）
    pub fn all_builtin() -> Vec<Self> {
        vec![
            Self::ClaudeCode,
            Self::Cursor,
            Self::Trae,
            Self::Codex,
            Self::Zcode,
            Self::OpenCode,
            Self::Qoder,
            Self::WorkBuddy,
            Self::CatPaw,
            Self::OpenClaw,
            Self::Marvis,
        ]
    }

    /// 是否为 4 主流之一（有完整预设）
    pub fn is_mainstream(&self) -> bool {
        matches!(
            self,
            Self::ClaudeCode | Self::Cursor | Self::Trae | Self::Codex
        )
    }

    /// 是否为内置 family（非 Custom）
    pub fn is_builtin(&self) -> bool {
        !matches!(self, Self::Custom(_))
    }

    /// 中文显示名（用于 UI 展示 / 日志）
    pub fn display_name(&self) -> &str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Cursor => "Cursor",
            Self::Trae => "Trae",
            Self::Codex => "Codex",
            Self::Zcode => "Zcode",
            Self::OpenCode => "OpenCode",
            Self::Qoder => "Qoder",
            Self::WorkBuddy => "WorkBuddy",
            Self::CatPaw => "CatPaw",
            Self::OpenClaw => "OpenClaw",
            Self::Marvis => "Marvis",
            Self::Custom(s) => s,
        }
    }

    /// 从字符串解析（与 [`display_name`](Self::display_name) 互逆）
    ///
    /// 大小写敏感，Custom 不参与解析（调用方自行构造 `Custom(String)`）
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Claude Code" => Some(Self::ClaudeCode),
            "Cursor" => Some(Self::Cursor),
            "Trae" => Some(Self::Trae),
            "Codex" => Some(Self::Codex),
            "Zcode" => Some(Self::Zcode),
            "OpenCode" => Some(Self::OpenCode),
            "Qoder" => Some(Self::Qoder),
            "WorkBuddy" => Some(Self::WorkBuddy),
            "CatPaw" => Some(Self::CatPaw),
            "OpenClaw" => Some(Self::OpenClaw),
            "Marvis" => Some(Self::Marvis),
            _ => None,
        }
    }

    /// 默认 session ID 前缀（用于按 Agent 隔离记忆）
    ///
    /// 4 主流有专用前缀，其他返回 family 小写名
    pub fn default_session_prefix(&self) -> &str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Cursor => "cursor",
            Self::Trae => "trae",
            Self::Codex => "codex",
            Self::Zcode => "zcode",
            Self::OpenCode => "opencode",
            Self::Qoder => "qoder",
            Self::WorkBuddy => "workbuddy",
            Self::CatPaw => "catpaw",
            Self::OpenClaw => "openclaw",
            Self::Marvis => "marvis",
            Self::Custom(_) => "custom",
        }
    }
}

impl Default for AgentFamily {
    /// 默认为 Custom("unknown")，强制调用方显式指定
    fn default() -> Self {
        Self::Custom("unknown".to_string())
    }
}

impl std::fmt::Display for AgentFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_builtin_count() {
        assert_eq!(AgentFamily::all_builtin().len(), 11);
    }

    #[test]
    fn test_is_mainstream() {
        assert!(AgentFamily::ClaudeCode.is_mainstream());
        assert!(AgentFamily::Cursor.is_mainstream());
        assert!(AgentFamily::Trae.is_mainstream());
        assert!(AgentFamily::Codex.is_mainstream());
        assert!(!AgentFamily::Zcode.is_mainstream());
        assert!(!AgentFamily::Custom("x".into()).is_mainstream());
    }

    #[test]
    fn test_is_builtin() {
        assert!(AgentFamily::ClaudeCode.is_builtin());
        assert!(AgentFamily::Marvis.is_builtin());
        assert!(!AgentFamily::Custom("x".into()).is_builtin());
    }

    #[test]
    fn test_display_name() {
        assert_eq!(AgentFamily::ClaudeCode.display_name(), "Claude Code");
        assert_eq!(AgentFamily::Cursor.display_name(), "Cursor");
        assert_eq!(AgentFamily::Custom("MyAgent".into()).display_name(), "MyAgent");
    }

    #[test]
    fn test_from_str_roundtrip() {
        for family in AgentFamily::all_builtin() {
            let name = family.display_name();
            let parsed = AgentFamily::from_str(name);
            assert_eq!(parsed, Some(family.clone()), "{} 往返失败", name);
        }
    }

    #[test]
    fn test_from_str_unknown_returns_none() {
        assert!(AgentFamily::from_str("UnknownAgent").is_none());
        assert!(AgentFamily::from_str("").is_none());
    }

    #[test]
    fn test_default_session_prefix() {
        assert_eq!(AgentFamily::ClaudeCode.default_session_prefix(), "claude-code");
        assert_eq!(AgentFamily::Cursor.default_session_prefix(), "cursor");
        assert_eq!(AgentFamily::Trae.default_session_prefix(), "trae");
        assert_eq!(AgentFamily::Codex.default_session_prefix(), "codex");
        assert_eq!(AgentFamily::Zcode.default_session_prefix(), "zcode");
        assert_eq!(
            AgentFamily::Custom("x".into()).default_session_prefix(),
            "custom"
        );
    }

    #[test]
    fn test_default_is_custom_unknown() {
        let f = AgentFamily::default();
        assert!(matches!(f, AgentFamily::Custom(s) if s == "unknown"));
    }

    #[test]
    fn test_serialize_deserialize() {
        let f = AgentFamily::ClaudeCode;
        let json = serde_json::to_string(&f).unwrap();
        let back: AgentFamily = serde_json::from_str(&json).unwrap();
        assert_eq!(f, back);

        let custom = AgentFamily::Custom("MyAgent".into());
        let json = serde_json::to_string(&custom).unwrap();
        let back: AgentFamily = serde_json::from_str(&json).unwrap();
        assert_eq!(custom, back);
    }

    #[test]
    fn test_display_trait() {
        assert_eq!(format!("{}", AgentFamily::ClaudeCode), "Claude Code");
        assert_eq!(format!("{}", AgentFamily::Custom("Foo".into())), "Foo");
    }

    #[test]
    fn test_hash_set_usage() {
        // 验证 AgentFamily 可放入 HashSet（派生了 Hash）
        use std::collections::HashSet;
        let mut set: HashSet<AgentFamily> = HashSet::new();
        set.insert(AgentFamily::ClaudeCode);
        set.insert(AgentFamily::Cursor);
        set.insert(AgentFamily::ClaudeCode); // 重复，不会增加
        assert_eq!(set.len(), 2);
    }
}

//! # Agent Profile（代理工具配置 + Builder 模式）
//!
//! AgentProfile 描述当前使用 MemoryCenter 的 Agent 代理工具特性：
//! - family + variant（family / variant 分离）
//! - capabilities（工具调用 / 原生压缩）
//! - 归档策略（是否归档到 MemoryCenter / session 前缀）

use crate::agent_family::AgentFamily;
use serde::{Deserialize, Serialize};

/// Agent 代理工具配置
///
/// ## 4 主流预设
///
/// | family | supports_tool_calls | has_native_compression | session_prefix |
/// |---|---|---|---|
/// | ClaudeCode | true | true（/compact 10:1） | claude-code |
/// | Cursor | true | true（chat 5:1） | cursor |
/// | Trae | true | true（conversation 5:1） | trae |
/// | Codex | true | true（rolling 3:1 无摘要） | codex |
///
/// 其他 7 + Custom 使用 [`AgentProfile::generic`]（全能力开启 + 调用方自定义）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Agent 家族（稳定枚举）
    pub family: AgentFamily,
    /// 型号 / 版本（高频迭代字段，如 "2.0"、"1.45"）
    ///
    /// 由调用方按需注入，MemoryCenter 不维护版本库
    #[serde(default)]
    pub variant: Option<String>,
    /// 是否支持工具调用（影响 ToolCall 标签策略）
    pub supports_tool_calls: bool,
    /// 是否有原生压缩机制（影响 window 策略选择，由 presets 层映射）
    pub has_native_compression: bool,
    /// 是否归档到 MemoryCenter
    ///
    /// 部分用户可能希望 Agent 用自己的记忆机制而不归档，默认 true
    pub archive_to_MemoryCenter: bool,
    /// session ID 前缀（用于按 Agent 隔离记忆）
    ///
    /// 默认从 family 推导，可由调用方覆盖
    pub session_prefix: String,
}

impl AgentProfile {
    /// Claude Code 预设
    ///
    /// - family=ClaudeCode, supports_tool_calls=true
    /// - has_native_compression=true（/compact 10:1 压缩 + 摘要）
    /// - session_prefix="claude-code"
    pub fn claude_code() -> Self {
        Self {
            family: AgentFamily::ClaudeCode,
            variant: None,
            supports_tool_calls: true,
            has_native_compression: true,
            archive_to_MemoryCenter: true,
            session_prefix: AgentFamily::ClaudeCode
                .default_session_prefix()
                .to_string(),
        }
    }

    /// Cursor 预设
    ///
    /// - family=Cursor, supports_tool_calls=true
    /// - has_native_compression=true（chat 5:1 压缩 + 摘要）
    /// - session_prefix="cursor"
    pub fn cursor() -> Self {
        Self {
            family: AgentFamily::Cursor,
            variant: None,
            supports_tool_calls: true,
            has_native_compression: true,
            archive_to_MemoryCenter: true,
            session_prefix: AgentFamily::Cursor.default_session_prefix().to_string(),
        }
    }

    /// Trae 预设
    ///
    /// - family=Trae, supports_tool_calls=true
    /// - has_native_compression=true（conversation 5:1 压缩 + 摘要）
    /// - session_prefix="trae"
    pub fn trae() -> Self {
        Self {
            family: AgentFamily::Trae,
            variant: None,
            supports_tool_calls: true,
            has_native_compression: true,
            archive_to_MemoryCenter: true,
            session_prefix: AgentFamily::Trae.default_session_prefix().to_string(),
        }
    }

    /// Codex 预设
    ///
    /// - family=Codex, supports_tool_calls=true
    /// - has_native_compression=true（rolling 3:1 压缩，无摘要）
    /// - session_prefix="codex"
    pub fn codex() -> Self {
        Self {
            family: AgentFamily::Codex,
            variant: None,
            supports_tool_calls: true,
            has_native_compression: true,
            archive_to_MemoryCenter: true,
            session_prefix: AgentFamily::Codex.default_session_prefix().to_string(),
        }
    }

    /// 通用预设（其他 7 + Custom 使用）
    ///
    /// - 全能力开启（保守策略）
    /// - session_prefix 从 family 推导
    pub fn generic(family: AgentFamily) -> Self {
        let session_prefix = family.default_session_prefix().to_string();
        Self {
            family,
            variant: None,
            supports_tool_calls: true,
            has_native_compression: false, // 未知 Agent 默认无原生压缩
            archive_to_MemoryCenter: true,
            session_prefix,
        }
    }

    /// 从 family 推导预设（4 主流返回专用预设，其他返回 generic）
    pub fn from_family(family: AgentFamily) -> Self {
        match &family {
            AgentFamily::ClaudeCode => Self::claude_code(),
            AgentFamily::Cursor => Self::cursor(),
            AgentFamily::Trae => Self::trae(),
            AgentFamily::Codex => Self::codex(),
            other => Self::generic(other.clone()),
        }
    }

    // ========================================================================
    // Builder 方法
    // ========================================================================

    /// 设置 variant（型号 / 版本字符串）
    pub fn with_variant(mut self, variant: impl Into<String>) -> Self {
        self.variant = Some(variant.into());
        self
    }

    /// 禁用归档（Agent 使用自己的记忆机制）
    pub fn with_archive_disabled(mut self) -> Self {
        self.archive_to_MemoryCenter = false;
        self
    }

    /// 覆盖 session 前缀
    pub fn with_session_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.session_prefix = prefix.into();
        self
    }

    /// 显式设置 supports_tool_calls
    pub fn with_tool_calls_support(mut self, supports: bool) -> Self {
        self.supports_tool_calls = supports;
        self
    }

    /// 显式设置 has_native_compression
    pub fn with_native_compression(mut self, has: bool) -> Self {
        self.has_native_compression = has;
        self
    }

    /// 校验配置合法性
    ///
    /// - session_prefix 不能为空
    /// - archive_to_MemoryCenter=false 时无需校验其他字段（Agent 自管记忆）
    pub fn validate(&self) -> Result<(), String> {
        if self.session_prefix.trim().is_empty() {
            return Err("session_prefix 不能为空".to_string());
        }
        Ok(())
    }
}

impl Default for AgentProfile {
    /// 默认为 generic(Custom("unknown"))
    fn default() -> Self {
        Self::generic(AgentFamily::default())
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_code_preset() {
        let p = AgentProfile::claude_code();
        assert_eq!(p.family, AgentFamily::ClaudeCode);
        assert!(p.supports_tool_calls);
        assert!(p.has_native_compression);
        assert!(p.archive_to_MemoryCenter);
        assert_eq!(p.session_prefix, "claude-code");
        assert!(p.variant.is_none());
        assert!(p.validate().is_ok());
    }

    #[test]
    fn test_cursor_preset() {
        let p = AgentProfile::cursor();
        assert_eq!(p.family, AgentFamily::Cursor);
        assert!(p.supports_tool_calls);
        assert!(p.has_native_compression);
        assert_eq!(p.session_prefix, "cursor");
    }

    #[test]
    fn test_trae_preset() {
        let p = AgentProfile::trae();
        assert_eq!(p.family, AgentFamily::Trae);
        assert!(p.has_native_compression);
        assert_eq!(p.session_prefix, "trae");
    }

    #[test]
    fn test_codex_preset() {
        let p = AgentProfile::codex();
        assert_eq!(p.family, AgentFamily::Codex);
        assert!(p.has_native_compression);
        assert_eq!(p.session_prefix, "codex");
    }

    #[test]
    fn test_generic_preset() {
        let p = AgentProfile::generic(AgentFamily::Zcode);
        assert_eq!(p.family, AgentFamily::Zcode);
        assert!(p.supports_tool_calls);
        assert!(!p.has_native_compression); // generic 默认无原生压缩
        assert_eq!(p.session_prefix, "zcode");
    }

    #[test]
    fn test_generic_custom() {
        let p = AgentProfile::generic(AgentFamily::Custom("MyAgent".into()));
        assert_eq!(p.family, AgentFamily::Custom("MyAgent".into()));
        assert_eq!(p.session_prefix, "custom");
    }

    #[test]
    fn test_from_family_mainstream() {
        let p = AgentProfile::from_family(AgentFamily::ClaudeCode);
        assert_eq!(p.family, AgentFamily::ClaudeCode);
        assert!(p.has_native_compression);

        let p = AgentProfile::from_family(AgentFamily::Cursor);
        assert_eq!(p.family, AgentFamily::Cursor);
        assert!(p.has_native_compression);
    }

    #[test]
    fn test_from_family_generic() {
        let p = AgentProfile::from_family(AgentFamily::Qoder);
        assert_eq!(p.family, AgentFamily::Qoder);
        assert!(!p.has_native_compression); // generic 预设

        let p = AgentProfile::from_family(AgentFamily::Custom("X".into()));
        assert!(matches!(p.family, AgentFamily::Custom(_)));
    }

    #[test]
    fn test_builder_with_variant() {
        let p = AgentProfile::claude_code().with_variant("2.0");
        assert_eq!(p.variant.as_deref(), Some("2.0"));
    }

    #[test]
    fn test_builder_with_archive_disabled() {
        let p = AgentProfile::cursor().with_archive_disabled();
        assert!(!p.archive_to_MemoryCenter);
    }

    #[test]
    fn test_builder_with_session_prefix() {
        let p = AgentProfile::claude_code().with_session_prefix("my-claude");
        assert_eq!(p.session_prefix, "my-claude");
    }

    #[test]
    fn test_builder_with_tool_calls_support() {
        let p = AgentProfile::generic(AgentFamily::Marvis).with_tool_calls_support(false);
        assert!(!p.supports_tool_calls);
    }

    #[test]
    fn test_builder_with_native_compression() {
        let p = AgentProfile::generic(AgentFamily::OpenCode).with_native_compression(true);
        assert!(p.has_native_compression);
    }

    #[test]
    fn test_builder_chain() {
        let p = AgentProfile::claude_code()
            .with_variant("2.0")
            .with_session_prefix("custom-prefix")
            .with_archive_disabled();
        assert_eq!(p.variant.as_deref(), Some("2.0"));
        assert_eq!(p.session_prefix, "custom-prefix");
        assert!(!p.archive_to_MemoryCenter);
    }

    #[test]
    fn test_validate_empty_session_prefix() {
        let mut p = AgentProfile::claude_code();
        p.session_prefix = "".to_string();
        assert!(p.validate().is_err());
    }

    #[test]
    fn test_validate_whitespace_session_prefix() {
        let mut p = AgentProfile::cursor();
        p.session_prefix = "   ".to_string();
        assert!(p.validate().is_err());
    }

    #[test]
    fn test_validate_valid_profile() {
        let p = AgentProfile::claude_code();
        assert!(p.validate().is_ok());
    }

    #[test]
    fn test_default_profile() {
        let p = AgentProfile::default();
        assert!(matches!(p.family, AgentFamily::Custom(_)));
        assert!(p.validate().is_ok()); // session_prefix="custom"
    }

    #[test]
    fn test_serialize_deserialize() {
        let p = AgentProfile::claude_code().with_variant("2.0");
        let json = serde_json::to_string(&p).unwrap();
        let back: AgentProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.family, AgentFamily::ClaudeCode);
        assert_eq!(back.variant.as_deref(), Some("2.0"));
        assert!(back.has_native_compression);
    }

    #[test]
    fn test_all_builtin_families_have_profile() {
        // 验证所有 11 个内置 family 都能生成 profile 且校验通过
        for family in AgentFamily::all_builtin() {
            let p = AgentProfile::from_family(family.clone());
            assert_eq!(p.family, family);
            assert!(p.validate().is_ok(), "{} profile 校验失败", family.display_name());
        }
    }
}

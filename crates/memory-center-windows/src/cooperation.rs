//! # 协作模式
//!
//! 描述 Agent 工具与 MemoryCenter 的协作方式。
//!
//! v2.53 P8 起 Cooperative 已实现（archive-core 的 CooperativeService + MCP/HTTP 端点）。
//!
//! ## 模式说明
//!
//! ### Independent（独立模式）
//!
//! Agent 工具独立管理自己的上下文，MemoryCenter 被动接收归档：
//! - Agent 工具触发压缩时，调用 MemoryCenter 归档被丢弃的内容
//! - MemoryCenter 不主动干预 Agent 工具的上下文管理
//! - 归档时机由 Agent 工具决定
//!
//! ### Cooperative（协作模式，v2.53 P8 实现）
//!
//! Agent 工具与 MemoryCenter 协同管理上下文：
//! - 主动通知 MemoryCenter 压缩事件（pre_compress_hint）
//! - MemoryCenter 可建议保留哪些记忆（基于检索相关性）
//! - 双向通信，MemoryCenter 可触发 Agent 工具的压缩
//!
//! 详见 `docs/cooperative-design.md`。

use serde::{Deserialize, Serialize};

/// 协作模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CooperationMode {
    /// 独立模式
    ///
    /// Agent 工具独立管理上下文，MemoryCenter 被动接收归档
    Independent,

    /// 协作模式（v2.53 P8 实现）
    ///
    /// Agent 工具与 MemoryCenter 协同管理上下文
    Cooperative,
}

impl Default for CooperationMode {
    fn default() -> Self {
        Self::Independent
    }
}

impl CooperationMode {
    /// 是否为支持的模式
    ///
    /// v2.53 P8 起 Cooperative 已实现，两种模式均支持。
    pub fn is_supported(&self) -> bool {
        true
    }

    /// 中文显示名
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Independent => "独立模式",
            Self::Cooperative => "协作模式",
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_independent() {
        assert_eq!(CooperationMode::default(), CooperationMode::Independent);
    }

    #[test]
    fn test_independent_is_supported() {
        assert!(CooperationMode::Independent.is_supported());
    }

    #[test]
    fn test_cooperative_is_supported() {
        // v2.53 P8：Cooperative 已实现，is_supported() 返回 true
        assert!(CooperationMode::Cooperative.is_supported());
    }

    #[test]
    fn test_display_name() {
        assert_eq!(CooperationMode::Independent.display_name(), "独立模式");
        assert_eq!(
            CooperationMode::Cooperative.display_name(),
            "协作模式"
        );
    }

    #[test]
    fn test_serialize_deserialize() {
        let m = CooperationMode::Independent;
        let json = serde_json::to_string(&m).unwrap();
        let de: CooperationMode = serde_json::from_str(&json).unwrap();
        assert_eq!(m, de);
    }
}

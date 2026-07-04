//! # MemoryLink 策略（技能输出与记忆的链接方式）
//!
//! MVP 仅实现 2 种：
//! - **AttachedToTurn**（默认）：技能输出附加到当前轮次，随轮次归档
//! - **SkipArchive**：技能输出不归档（如定时任务触发、临时计算）
//!
//! v2 可扩展：StandaloneMemory（独立记忆）/ LinkedToProject（项目级记忆）等。

use serde::{Deserialize, Serialize};

/// 技能输出与记忆的链接方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryLink {
    /// 附加到当前轮次（默认）
    ///
    /// 技能输出作为 hippocampus-core 中 `MessageTurn` 的 `tool_calls`
    /// 字段的一部分，随轮次归档。
    AttachedToTurn,

    /// 不归档
    ///
    /// 技能输出仅在当前会话窗口中使用，不写入记忆文件。
    /// 适用于：定时任务触发、临时计算、调试输出等无需长期保存的内容。
    SkipArchive,
}

impl MemoryLink {
    /// 是否归档到记忆
    pub fn archives(&self) -> bool {
        matches!(self, Self::AttachedToTurn)
    }

    /// 中文显示名
    pub fn display_name(&self) -> &str {
        match self {
            Self::AttachedToTurn => "附加到轮次",
            Self::SkipArchive => "不归档",
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "AttachedToTurn" | "attached_to_turn" => Some(Self::AttachedToTurn),
            "SkipArchive" | "skip_archive" => Some(Self::SkipArchive),
            _ => None,
        }
    }

    /// 返回字符串标识（与 [`from_str`](Self::from_str) 互逆）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AttachedToTurn => "AttachedToTurn",
            Self::SkipArchive => "SkipArchive",
        }
    }
}

impl Default for MemoryLink {
    /// 默认附加到轮次（保守策略，保留全部信息）
    fn default() -> Self {
        Self::AttachedToTurn
    }
}

impl std::fmt::Display for MemoryLink {
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
    fn test_default_is_attached() {
        assert_eq!(MemoryLink::default(), MemoryLink::AttachedToTurn);
    }

    #[test]
    fn test_archives() {
        assert!(MemoryLink::AttachedToTurn.archives());
        assert!(!MemoryLink::SkipArchive.archives());
    }

    #[test]
    fn test_display_name() {
        assert_eq!(MemoryLink::AttachedToTurn.display_name(), "附加到轮次");
        assert_eq!(MemoryLink::SkipArchive.display_name(), "不归档");
    }

    #[test]
    fn test_as_str() {
        assert_eq!(MemoryLink::AttachedToTurn.as_str(), "AttachedToTurn");
        assert_eq!(MemoryLink::SkipArchive.as_str(), "SkipArchive");
    }

    #[test]
    fn test_from_str_roundtrip() {
        let variants = [MemoryLink::AttachedToTurn, MemoryLink::SkipArchive];
        for v in variants {
            let s = v.as_str();
            let back = MemoryLink::from_str(s);
            assert_eq!(back, Some(v));
        }
    }

    #[test]
    fn test_from_str_snake_case() {
        assert_eq!(
            MemoryLink::from_str("attached_to_turn"),
            Some(MemoryLink::AttachedToTurn)
        );
        assert_eq!(
            MemoryLink::from_str("skip_archive"),
            Some(MemoryLink::SkipArchive)
        );
    }

    #[test]
    fn test_from_str_unknown() {
        assert!(MemoryLink::from_str("unknown").is_none());
        assert!(MemoryLink::from_str("").is_none());
    }

    #[test]
    fn test_display_trait() {
        assert_eq!(format!("{}", MemoryLink::AttachedToTurn), "附加到轮次");
        assert_eq!(format!("{}", MemoryLink::SkipArchive), "不归档");
    }

    #[test]
    fn test_serialize_deserialize() {
        for v in [MemoryLink::AttachedToTurn, MemoryLink::SkipArchive] {
            let json = serde_json::to_string(&v).unwrap();
            let back: MemoryLink = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back);
        }
    }

    #[test]
    fn test_hash_set_usage() {
        use std::collections::HashSet;
        let mut set: HashSet<MemoryLink> = HashSet::new();
        set.insert(MemoryLink::AttachedToTurn);
        set.insert(MemoryLink::SkipArchive);
        set.insert(MemoryLink::AttachedToTurn);
        assert_eq!(set.len(), 2);
    }
}

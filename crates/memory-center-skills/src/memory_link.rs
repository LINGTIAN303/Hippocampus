//! # MemoryLink 策略（技能输出与记忆的链接方式）
//!
//! v2.52 阶段 4（P7 Phase 1）扩展为 4 种：
//! - **AttachedToTurn**（默认）：技能输出附加到当前轮次，随轮次归档
//! - **SkipArchive**：技能输出不归档（如定时任务触发、临时计算）
//! - **StandaloneMemory**：独立记忆，不绑定轮次，存到 session 内 standalone/ 目录
//! - **LinkedToProject**：项目级记忆，跨 session 共享，存到 projects/{project_id}/linked/
//!
//! ## 存储路径（Phase 2 实现）
//!
//! | MemoryLink | 存储路径 | 检索范围 |
//! |---|---|---|
//! | AttachedToTurn | `sessions/{session_id}/{period}/...` | 按 session + period |
//! | SkipArchive | 不存储 | 不检索 |
//! | StandaloneMemory | `sessions/{session_id}/standalone/...` | 按 session |
//! | LinkedToProject | `projects/{project_id}/linked/...` | 按 project（跨 session） |
//!
//! ## destructive 技能约束
//!
//! Write/Edit/Bash 等破坏性技能**强制 AttachedToTurn**，不允许设为其他变体
//! （破坏性操作需可追溯到具体轮次，StandaloneMemory/LinkedToProject 不绑定轮次，无法追溯）。

use serde::{Deserialize, Serialize};

/// 技能输出与记忆的链接方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryLink {
    /// 附加到当前轮次（默认）
    ///
    /// 技能输出作为 MemoryCenter-core 中 `MessageTurn` 的 `tool_calls`
    /// 字段的一部分，随轮次归档。
    AttachedToTurn,

    /// 不归档
    ///
    /// 技能输出仅在当前会话窗口中使用，不写入记忆文件。
    /// 适用于：定时任务触发、临时计算、调试输出等无需长期保存的内容。
    SkipArchive,

    /// 独立记忆（v2.52 P7 Phase 1 新增）
    ///
    /// 技能输出作为独立记忆文件存储，不绑定到具体轮次。
    /// 存储路径：`sessions/{session_id}/standalone/...`
    /// 检索范围：仅当前 session（不污染其他 session）
    ///
    /// 适用场景：当前会话的临时知识（中途计算结果、调试输出、会话级约定），
    /// 需要独立检索但不绑定到轮次。
    StandaloneMemory,

    /// 项目级记忆（v2.52 P7 Phase 1 新增）
    ///
    /// 技能输出作为项目级记忆存储，跨 session 共享。
    /// 存储路径：`projects/{project_id}/linked/...`
    /// 检索范围：同项目的所有 session
    ///
    /// 适用场景：项目级约定、通用规则、跨会话共享的知识。
    /// 与 AttachedToTurn 的区别：不绑定到具体轮次，所有 session 都可检索。
    LinkedToProject,
}

impl MemoryLink {
    /// 是否归档到记忆
    ///
    /// 返回 `true` 表示会写入记忆文件（只是存储位置不同）：
    /// - `AttachedToTurn`：随轮次归档
    /// - `StandaloneMemory`：独立存储到 session standalone/ 目录
    /// - `LinkedToProject`：存储到 project linked/ 目录
    ///
    /// 返回 `false` 表示不归档：
    /// - `SkipArchive`：不写入记忆文件
    pub fn archives(&self) -> bool {
        !matches!(self, Self::SkipArchive)
    }

    /// 是否绑定到轮次
    ///
    /// 返回 `true` 表示记忆绑定到具体轮次（可追溯）：
    /// - `AttachedToTurn`：绑定到当前轮次
    ///
    /// 返回 `false` 表示不绑定轮次（不可追溯）：
    /// - `StandaloneMemory` / `LinkedToProject`：独立/项目级，不绑定轮次
    /// - `SkipArchive`：不归档
    pub fn is_attached_to_turn(&self) -> bool {
        matches!(self, Self::AttachedToTurn)
    }

    /// 中文显示名
    pub fn display_name(&self) -> &str {
        match self {
            Self::AttachedToTurn => "附加到轮次",
            Self::SkipArchive => "不归档",
            Self::StandaloneMemory => "独立记忆",
            Self::LinkedToProject => "项目级记忆",
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "AttachedToTurn" | "attached_to_turn" => Some(Self::AttachedToTurn),
            "SkipArchive" | "skip_archive" => Some(Self::SkipArchive),
            "StandaloneMemory" | "standalone_memory" => Some(Self::StandaloneMemory),
            "LinkedToProject" | "linked_to_project" => Some(Self::LinkedToProject),
            _ => None,
        }
    }

    /// 返回字符串标识（与 [`from_str`](Self::from_str) 互逆）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AttachedToTurn => "AttachedToTurn",
            Self::SkipArchive => "SkipArchive",
            Self::StandaloneMemory => "StandaloneMemory",
            Self::LinkedToProject => "LinkedToProject",
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
        // 3 个变体归档（存储位置不同）
        assert!(MemoryLink::AttachedToTurn.archives());
        assert!(MemoryLink::StandaloneMemory.archives());
        assert!(MemoryLink::LinkedToProject.archives());
        // 1 个变体不归档
        assert!(!MemoryLink::SkipArchive.archives());
    }

    #[test]
    fn test_is_attached_to_turn() {
        // 仅 AttachedToTurn 返回 true（可追溯）
        assert!(MemoryLink::AttachedToTurn.is_attached_to_turn());
        // 其他变体不绑定轮次
        assert!(!MemoryLink::SkipArchive.is_attached_to_turn());
        assert!(!MemoryLink::StandaloneMemory.is_attached_to_turn());
        assert!(!MemoryLink::LinkedToProject.is_attached_to_turn());
    }

    #[test]
    fn test_display_name() {
        assert_eq!(MemoryLink::AttachedToTurn.display_name(), "附加到轮次");
        assert_eq!(MemoryLink::SkipArchive.display_name(), "不归档");
        assert_eq!(MemoryLink::StandaloneMemory.display_name(), "独立记忆");
        assert_eq!(MemoryLink::LinkedToProject.display_name(), "项目级记忆");
    }

    #[test]
    fn test_as_str() {
        assert_eq!(MemoryLink::AttachedToTurn.as_str(), "AttachedToTurn");
        assert_eq!(MemoryLink::SkipArchive.as_str(), "SkipArchive");
        assert_eq!(MemoryLink::StandaloneMemory.as_str(), "StandaloneMemory");
        assert_eq!(MemoryLink::LinkedToProject.as_str(), "LinkedToProject");
    }

    #[test]
    fn test_from_str_roundtrip() {
        let variants = [
            MemoryLink::AttachedToTurn,
            MemoryLink::SkipArchive,
            MemoryLink::StandaloneMemory,
            MemoryLink::LinkedToProject,
        ];
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
        assert_eq!(
            MemoryLink::from_str("standalone_memory"),
            Some(MemoryLink::StandaloneMemory)
        );
        assert_eq!(
            MemoryLink::from_str("linked_to_project"),
            Some(MemoryLink::LinkedToProject)
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
        assert_eq!(format!("{}", MemoryLink::StandaloneMemory), "独立记忆");
        assert_eq!(format!("{}", MemoryLink::LinkedToProject), "项目级记忆");
    }

    #[test]
    fn test_serialize_deserialize() {
        let variants = [
            MemoryLink::AttachedToTurn,
            MemoryLink::SkipArchive,
            MemoryLink::StandaloneMemory,
            MemoryLink::LinkedToProject,
        ];
        for v in variants {
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
        set.insert(MemoryLink::StandaloneMemory);
        set.insert(MemoryLink::LinkedToProject);
        set.insert(MemoryLink::AttachedToTurn); // 重复
        assert_eq!(set.len(), 4);
    }
}

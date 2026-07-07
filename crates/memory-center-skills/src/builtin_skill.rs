//! # 内置技能枚举（15 + Custom 兜底）
//!
//! 识别 Agent 代理工具的具体技能，用于：
//! - 标签策略（破坏性操作标记）
//! - 归档策略（MemoryLink 选择）
//! - 制品追踪（产生文件的技能需特殊处理）

use serde::{Deserialize, Serialize};

/// 技能类别（用于聚合统计 / 文档展示）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillCategory {
    /// 文件操作（Read/Write/Edit/Glob/Grep/LS）
    FileOps,
    /// 执行（Bash/Task）
    Execution,
    /// 网络（WebSearch/WebFetch）
    Web,
    /// 搜索（SearchCodebase）
    Search,
    /// 交互（AskUserQuestion）
    Interaction,
    /// 规划（TodoWrite/Schedule）
    Planning,
    /// 元操作（Skill）
    Meta,
    /// 自定义
    Custom,
}

/// 内置技能枚举（15 + Custom 兜底）
///
/// 对应主流 Agent 代理工具（Claude Code / Cursor / Trae / Codex 等）的常用技能。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum BuiltinSkill {
    /// 读取文件
    Read,
    /// 写入文件
    Write,
    /// 编辑文件（精确替换）
    Edit,
    /// 文件模式匹配
    Glob,
    /// 内容正则搜索
    Grep,
    /// 列出目录
    LS,
    /// 执行 shell 命令
    Bash,
    /// 启动子 agent
    Task,
    /// 网页搜索
    WebSearch,
    /// 抓取网页内容
    WebFetch,
    /// 语义代码搜索
    SearchCodebase,
    /// 询问用户问题
    AskUserQuestion,
    /// 任务列表管理
    TodoWrite,
    /// 定时任务
    Schedule,
    /// 执行技能
    Skill,
    /// 用户自定义兜底
    Custom(String),
}

impl BuiltinSkill {
    /// 返回所有内置技能（15 个，不含 Custom）
    pub fn all_builtin() -> Vec<Self> {
        vec![
            Self::Read,
            Self::Write,
            Self::Edit,
            Self::Glob,
            Self::Grep,
            Self::LS,
            Self::Bash,
            Self::Task,
            Self::WebSearch,
            Self::WebFetch,
            Self::SearchCodebase,
            Self::AskUserQuestion,
            Self::TodoWrite,
            Self::Schedule,
            Self::Skill,
        ]
    }

    /// 是否为内置技能（非 Custom）
    pub fn is_builtin(&self) -> bool {
        !matches!(self, Self::Custom(_))
    }

    /// 中文显示名
    pub fn display_name(&self) -> &str {
        match self {
            Self::Read => "读取文件",
            Self::Write => "写入文件",
            Self::Edit => "编辑文件",
            Self::Glob => "文件匹配",
            Self::Grep => "内容搜索",
            Self::LS => "列出目录",
            Self::Bash => "执行命令",
            Self::Task => "子 Agent",
            Self::WebSearch => "网页搜索",
            Self::WebFetch => "抓取网页",
            Self::SearchCodebase => "语义搜索",
            Self::AskUserQuestion => "询问用户",
            Self::TodoWrite => "任务列表",
            Self::Schedule => "定时任务",
            Self::Skill => "执行技能",
            Self::Custom(s) => s,
        }
    }

    /// 技能类别
    pub fn category(&self) -> SkillCategory {
        match self {
            Self::Read | Self::Write | Self::Edit | Self::Glob | Self::Grep | Self::LS => {
                SkillCategory::FileOps
            }
            Self::Bash | Self::Task => SkillCategory::Execution,
            Self::WebSearch | Self::WebFetch => SkillCategory::Web,
            Self::SearchCodebase => SkillCategory::Search,
            Self::AskUserQuestion => SkillCategory::Interaction,
            Self::TodoWrite | Self::Schedule => SkillCategory::Planning,
            Self::Skill => SkillCategory::Meta,
            Self::Custom(_) => SkillCategory::Custom,
        }
    }

    /// 是否产生制品（文件 / 持久化输出）
    ///
    /// Write/Edit 产生文件变更，其他技能输出为临时内容
    pub fn produces_artifact(&self) -> bool {
        matches!(self, Self::Write | Self::Edit)
    }

    /// 是否破坏性操作
    ///
    /// Write/Edit 修改文件，Bash 可能执行破坏性命令
    pub fn is_destructive(&self) -> bool {
        matches!(self, Self::Write | Self::Edit | Self::Bash)
    }

    /// 从字符串解析（大小写敏感）
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Read" => Some(Self::Read),
            "Write" => Some(Self::Write),
            "Edit" => Some(Self::Edit),
            "Glob" => Some(Self::Glob),
            "Grep" => Some(Self::Grep),
            "LS" => Some(Self::LS),
            "Bash" => Some(Self::Bash),
            "Task" => Some(Self::Task),
            "WebSearch" => Some(Self::WebSearch),
            "WebFetch" => Some(Self::WebFetch),
            "SearchCodebase" => Some(Self::SearchCodebase),
            "AskUserQuestion" => Some(Self::AskUserQuestion),
            "TodoWrite" => Some(Self::TodoWrite),
            "Schedule" => Some(Self::Schedule),
            "Skill" => Some(Self::Skill),
            _ => None,
        }
    }
}

impl Default for BuiltinSkill {
    fn default() -> Self {
        Self::Custom("unknown".to_string())
    }
}

impl std::fmt::Display for BuiltinSkill {
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
        assert_eq!(BuiltinSkill::all_builtin().len(), 15);
    }

    #[test]
    fn test_is_builtin() {
        assert!(BuiltinSkill::Read.is_builtin());
        assert!(BuiltinSkill::Skill.is_builtin());
        assert!(!BuiltinSkill::Custom("X".into()).is_builtin());
    }

    #[test]
    fn test_display_name() {
        assert_eq!(BuiltinSkill::Read.display_name(), "读取文件");
        assert_eq!(BuiltinSkill::Bash.display_name(), "执行命令");
        assert_eq!(
            BuiltinSkill::Custom("MySkill".into()).display_name(),
            "MySkill"
        );
    }

    #[test]
    fn test_category() {
        assert_eq!(BuiltinSkill::Read.category(), SkillCategory::FileOps);
        assert_eq!(BuiltinSkill::Write.category(), SkillCategory::FileOps);
        assert_eq!(BuiltinSkill::Bash.category(), SkillCategory::Execution);
        assert_eq!(BuiltinSkill::Task.category(), SkillCategory::Execution);
        assert_eq!(BuiltinSkill::WebSearch.category(), SkillCategory::Web);
        assert_eq!(BuiltinSkill::SearchCodebase.category(), SkillCategory::Search);
        assert_eq!(BuiltinSkill::AskUserQuestion.category(), SkillCategory::Interaction);
        assert_eq!(BuiltinSkill::TodoWrite.category(), SkillCategory::Planning);
        assert_eq!(BuiltinSkill::Schedule.category(), SkillCategory::Planning);
        assert_eq!(BuiltinSkill::Skill.category(), SkillCategory::Meta);
        assert_eq!(BuiltinSkill::Custom("X".into()).category(), SkillCategory::Custom);
    }

    #[test]
    fn test_produces_artifact() {
        assert!(BuiltinSkill::Write.produces_artifact());
        assert!(BuiltinSkill::Edit.produces_artifact());
        assert!(!BuiltinSkill::Read.produces_artifact());
        assert!(!BuiltinSkill::Bash.produces_artifact());
    }

    #[test]
    fn test_is_destructive() {
        assert!(BuiltinSkill::Write.is_destructive());
        assert!(BuiltinSkill::Edit.is_destructive());
        assert!(BuiltinSkill::Bash.is_destructive());
        assert!(!BuiltinSkill::Read.is_destructive());
        assert!(!BuiltinSkill::Glob.is_destructive());
    }

    #[test]
    fn test_from_str_roundtrip() {
        for skill in BuiltinSkill::all_builtin() {
            let name = format!("{:?}", skill);
            let parsed = BuiltinSkill::from_str(&name);
            assert_eq!(parsed, Some(skill.clone()), "{} 往返失败", name);
        }
    }

    #[test]
    fn test_from_str_unknown() {
        assert!(BuiltinSkill::from_str("UnknownSkill").is_none());
        assert!(BuiltinSkill::from_str("").is_none());
    }

    #[test]
    fn test_default_is_custom_unknown() {
        let s = BuiltinSkill::default();
        assert!(matches!(s, BuiltinSkill::Custom(ref inner) if inner == "unknown"));
    }

    #[test]
    fn test_display_trait() {
        assert_eq!(format!("{}", BuiltinSkill::Read), "读取文件");
        assert_eq!(format!("{}", BuiltinSkill::Custom("Foo".into())), "Foo");
    }

    #[test]
    fn test_serialize_deserialize() {
        let s = BuiltinSkill::Bash;
        let json = serde_json::to_string(&s).unwrap();
        let back: BuiltinSkill = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);

        let custom = BuiltinSkill::Custom("MySkill".into());
        let json = serde_json::to_string(&custom).unwrap();
        let back: BuiltinSkill = serde_json::from_str(&json).unwrap();
        assert_eq!(custom, back);
    }

    #[test]
    fn test_hash_set_usage() {
        use std::collections::HashSet;
        let mut set: HashSet<BuiltinSkill> = HashSet::new();
        set.insert(BuiltinSkill::Read);
        set.insert(BuiltinSkill::Write);
        set.insert(BuiltinSkill::Read);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_category_serialize() {
        let cat = SkillCategory::FileOps;
        let json = serde_json::to_string(&cat).unwrap();
        let back: SkillCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(cat, back);
    }
}

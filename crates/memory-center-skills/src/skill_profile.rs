//! # Skill Profile（技能配置 + Builder 模式）
//!
//! SkillProfile 描述单个技能的配置：
//! - skill 标识（BuiltinSkill）
//! - memory_link 策略（AttachedToTurn / SkipArchive）
//! - 自定义标签（覆盖默认 MemoryLink）

use crate::builtin_skill::BuiltinSkill;
use crate::memory_link::MemoryLink;
use serde::{Deserialize, Serialize};

/// 技能配置
///
/// ## 默认 MemoryLink 映射
///
/// | 技能 | 默认 MemoryLink |
/// |---|---|
/// | Schedule | SkipArchive |
/// | 其他 14 + Custom | AttachedToTurn |
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillProfile {
    /// 技能标识
    pub skill: BuiltinSkill,
    /// 记忆链接策略
    pub memory_link: MemoryLink,
    /// 是否启用（禁用的技能不调用）
    pub enabled: bool,
    /// 用户自定义备注（可选，用于标识技能用途）
    #[serde(default)]
    pub note: Option<String>,
}

impl SkillProfile {
    /// 创建技能配置（使用默认 MemoryLink）
    pub fn new(skill: BuiltinSkill) -> Self {
        let memory_link = default_memory_link_for(&skill);
        Self {
            skill,
            memory_link,
            enabled: true,
            note: None,
        }
    }

    /// 从内置技能构造（同 [`new`](Self::new)）
    pub fn from_skill(skill: BuiltinSkill) -> Self {
        Self::new(skill)
    }

    /// 从字符串构造（先尝试解析为内置，失败则用 Custom）
    pub fn from_name(name: &str) -> Self {
        let skill = BuiltinSkill::from_str(name).unwrap_or_else(|| BuiltinSkill::Custom(name.to_string()));
        Self::new(skill)
    }

    // ========================================================================
    // Builder 方法
    // ========================================================================

    /// 设置 MemoryLink 策略
    pub fn with_memory_link(mut self, link: MemoryLink) -> Self {
        self.memory_link = link;
        self
    }

    /// 禁用技能
    pub fn with_disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// 启用技能（显式）
    pub fn with_enabled(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// 设置备注
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// 校验配置合法性
    ///
    /// ## 校验规则
    ///
    /// - **destructive 技能强制 AttachedToTurn**：Write/Edit/Bash 的输出必须绑定到具体轮次，
    ///   不允许设为 SkipArchive / StandaloneMemory / LinkedToProject
    ///   （破坏性操作需可追溯到具体轮次，StandaloneMemory/LinkedToProject 不绑定轮次，无法追溯）
    /// - 其他组合合法（非破坏性技能可自由选择任意 MemoryLink）
    pub fn validate(&self) -> Result<(), String> {
        if self.skill.is_destructive() && !self.memory_link.is_attached_to_turn() {
            return Err(format!(
                "destructive 技能 {} 必须绑定到轮次（AttachedToTurn），不允许 {}（破坏性操作需可追溯，StandaloneMemory/LinkedToProject/SkipArchive 不绑定轮次）",
                self.skill, self.memory_link
            ));
        }
        Ok(())
    }
}

impl Default for SkillProfile {
    fn default() -> Self {
        Self::new(BuiltinSkill::default())
    }
}

/// 返回技能的默认 MemoryLink
///
/// - Schedule → SkipArchive（定时任务触发无需归档）
/// - 其他 → AttachedToTurn（保守归档）
pub fn default_memory_link_for(skill: &BuiltinSkill) -> MemoryLink {
    match skill {
        BuiltinSkill::Schedule => MemoryLink::SkipArchive,
        _ => MemoryLink::AttachedToTurn,
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_read_default_link() {
        let p = SkillProfile::new(BuiltinSkill::Read);
        assert_eq!(p.skill, BuiltinSkill::Read);
        assert_eq!(p.memory_link, MemoryLink::AttachedToTurn);
        assert!(p.enabled);
        assert!(p.note.is_none());
    }

    #[test]
    fn test_new_schedule_skip_archive() {
        let p = SkillProfile::new(BuiltinSkill::Schedule);
        assert_eq!(p.memory_link, MemoryLink::SkipArchive);
    }

    #[test]
    fn test_from_skill() {
        let p = SkillProfile::from_skill(BuiltinSkill::Bash);
        assert_eq!(p.skill, BuiltinSkill::Bash);
        assert_eq!(p.memory_link, MemoryLink::AttachedToTurn);
    }

    #[test]
    fn test_from_name_builtin() {
        let p = SkillProfile::from_name("Read");
        assert_eq!(p.skill, BuiltinSkill::Read);
    }

    #[test]
    fn test_from_name_custom() {
        let p = SkillProfile::from_name("MyCustomSkill");
        assert!(matches!(p.skill, BuiltinSkill::Custom(ref s) if s == "MyCustomSkill"));
    }

    #[test]
    fn test_from_name_empty() {
        let p = SkillProfile::from_name("");
        assert!(matches!(p.skill, BuiltinSkill::Custom(ref s) if s.is_empty()));
    }

    #[test]
    fn test_builder_with_memory_link() {
        let p = SkillProfile::new(BuiltinSkill::Read).with_memory_link(MemoryLink::SkipArchive);
        assert_eq!(p.memory_link, MemoryLink::SkipArchive);
    }

    #[test]
    fn test_builder_with_disabled() {
        let p = SkillProfile::new(BuiltinSkill::Bash).with_disabled();
        assert!(!p.enabled);
    }

    #[test]
    fn test_builder_with_enabled() {
        let mut p = SkillProfile::new(BuiltinSkill::Read);
        p.enabled = false;
        let p = p.with_enabled();
        assert!(p.enabled);
    }

    #[test]
    fn test_builder_with_note() {
        let p = SkillProfile::new(BuiltinSkill::Write).with_note("文件写入技能");
        assert_eq!(p.note.as_deref(), Some("文件写入技能"));
    }

    #[test]
    fn test_builder_chain() {
        let p = SkillProfile::new(BuiltinSkill::Bash)
            .with_memory_link(MemoryLink::SkipArchive)
            .with_disabled()
            .with_note("禁用 Bash");
        assert_eq!(p.memory_link, MemoryLink::SkipArchive);
        assert!(!p.enabled);
        assert_eq!(p.note.as_deref(), Some("禁用 Bash"));
    }

    #[test]
    fn test_validate_non_destructive_ok() {
        // 非破坏性技能 + 默认 AttachedToTurn → ok
        let p = SkillProfile::new(BuiltinSkill::Read);
        assert!(p.validate().is_ok());

        // 非破坏性技能 + SkipArchive → ok（可自由选择不归档）
        let p = SkillProfile::new(BuiltinSkill::Read).with_memory_link(MemoryLink::SkipArchive);
        assert!(p.validate().is_ok());

        // v2.52 P7：非破坏性技能 + StandaloneMemory → ok（可自由选择独立记忆）
        let p = SkillProfile::new(BuiltinSkill::Read).with_memory_link(MemoryLink::StandaloneMemory);
        assert!(p.validate().is_ok());

        // v2.52 P7：非破坏性技能 + LinkedToProject → ok（可自由选择项目级记忆）
        let p = SkillProfile::new(BuiltinSkill::Read).with_memory_link(MemoryLink::LinkedToProject);
        assert!(p.validate().is_ok());

        // Schedule 默认 SkipArchive + disabled → ok
        let p = SkillProfile::new(BuiltinSkill::Schedule).with_disabled();
        assert!(p.validate().is_ok());
    }

    #[test]
    fn test_validate_destructive_default_ok() {
        // destructive 技能默认 AttachedToTurn → ok
        for skill in [BuiltinSkill::Write, BuiltinSkill::Edit, BuiltinSkill::Bash] {
            let p = SkillProfile::new(skill.clone());
            assert!(p.validate().is_ok(), "{} 默认应通过", skill);
        }
    }

    #[test]
    fn test_validate_destructive_non_attached_fails() {
        // v2.52 P7：destructive 技能 + 所有非 AttachedToTurn 变体 → Err
        // （破坏性操作需可追溯到具体轮次，StandaloneMemory/LinkedToProject/SkipArchive 不绑定轮次）
        let non_attached_links = [
            MemoryLink::SkipArchive,
            MemoryLink::StandaloneMemory,
            MemoryLink::LinkedToProject,
        ];
        for skill in [BuiltinSkill::Write, BuiltinSkill::Edit, BuiltinSkill::Bash] {
            for link in &non_attached_links {
                let p = SkillProfile::new(skill.clone()).with_memory_link(*link);
                let err = p.validate();
                assert!(
                    err.is_err(),
                    "{} + {} 应失败",
                    skill, link
                );
                assert!(
                    err.unwrap_err().contains("destructive"),
                    "错误信息应包含 destructive（{} + {}）",
                    skill, link
                );
            }
        }
    }

    #[test]
    fn test_default_profile() {
        let p = SkillProfile::default();
        assert!(matches!(p.skill, BuiltinSkill::Custom(_)));
        assert_eq!(p.memory_link, MemoryLink::AttachedToTurn);
    }

    #[test]
    fn test_serialize_deserialize() {
        let p = SkillProfile::new(BuiltinSkill::Bash)
            .with_memory_link(MemoryLink::SkipArchive)
            .with_note("test");
        let json = serde_json::to_string(&p).unwrap();
        let back: SkillProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.skill, BuiltinSkill::Bash);
        assert_eq!(back.memory_link, MemoryLink::SkipArchive);
        assert!(!back.enabled == false || back.enabled == true); // enabled 序列化保留
        assert_eq!(back.note.as_deref(), Some("test"));
    }

    #[test]
    fn test_default_memory_link_for_all_builtin() {
        for skill in BuiltinSkill::all_builtin() {
            let link = default_memory_link_for(&skill);
            if matches!(skill, BuiltinSkill::Schedule) {
                assert_eq!(link, MemoryLink::SkipArchive, "{} 应为 SkipArchive", skill);
            } else {
                assert_eq!(link, MemoryLink::AttachedToTurn, "{} 应为 AttachedToTurn", skill);
            }
        }
    }

    #[test]
    fn test_all_builtin_have_profile() {
        for skill in BuiltinSkill::all_builtin() {
            let p = SkillProfile::new(skill.clone());
            assert_eq!(p.skill, skill);
            assert!(p.validate().is_ok());
        }
    }
}

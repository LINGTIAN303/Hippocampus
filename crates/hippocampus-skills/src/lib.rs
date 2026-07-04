//! # Hippocampus Agent 技能特配库
//!
//! 5 种特配 crate 之一（平行拓扑，仅依赖 hippocampus-core）。
//!
//! ## 定位
//!
//! 识别 Agent 代理工具内置的具体技能（Read / Write / Edit / Bash 等），
//! 为 presets 组合层提供技能维度的默认配置：
//! - 技能输出如何链接到记忆（AttachedToTurn / SkipArchive）
//! - 技能是否产生制品（影响归档策略）
//! - 技能是否破坏性（影响标签策略）
//!
//! ## 15 个内置技能
//!
//! | 技能 | 类别 | 产生制品 | 破坏性 | 默认 MemoryLink |
//! |---|---|---|---|---|
//! | Read | 文件操作 | 否 | 否 | AttachedToTurn |
//! | Write | 文件操作 | 是 | 是 | AttachedToTurn |
//! | Edit | 文件操作 | 是 | 是 | AttachedToTurn |
//! | Glob | 文件操作 | 否 | 否 | AttachedToTurn |
//! | Grep | 文件操作 | 否 | 否 | AttachedToTurn |
//! | LS | 文件操作 | 否 | 否 | AttachedToTurn |
//! | Bash | 执行 | 否 | 是 | AttachedToTurn |
//! | Task | 执行 | 否 | 否 | AttachedToTurn |
//! | WebSearch | 网络 | 否 | 否 | AttachedToTurn |
//! | WebFetch | 网络 | 否 | 否 | AttachedToTurn |
//! | SearchCodebase | 搜索 | 否 | 否 | AttachedToTurn |
//! | AskUserQuestion | 交互 | 否 | 否 | AttachedToTurn |
//! | TodoWrite | 规划 | 否 | 否 | AttachedToTurn |
//! | Schedule | 规划 | 否 | 否 | SkipArchive |
//! | Skill | 元操作 | 否 | 否 | AttachedToTurn |
//! | Custom(String) | 自定义 | 否 | 否 | AttachedToTurn |
//!
//! ## MemoryLink 策略
//!
//! - **AttachedToTurn**（默认）：技能输出附加到当前轮次，随轮次归档
//! - **SkipArchive**：技能输出不归档（如定时任务触发、临时计算）
//!
//! MVP 仅实现以上 2 种，未来可扩展 StandaloneMemory（独立记忆）等。

pub mod builtin_skill;
pub mod memory_link;
pub mod skill_profile;

pub use builtin_skill::BuiltinSkill;
pub use memory_link::MemoryLink;
pub use skill_profile::SkillProfile;

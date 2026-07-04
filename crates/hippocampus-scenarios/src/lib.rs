//! # Hippocampus 工作场景特配库
//!
//! 识别 Agent 工作的场景，为不同场景提供针对化记忆工作流程：
//! - 摘要 focus：不同场景关注不同信息维度
//! - 评分权重：不同场景对 4 维评分的权重不同
//! - 标签优先级：不同场景优先保留的标签类型不同
//! - 检索策略：不同场景适合不同的检索方式
//! - 归档阈值：不同场景的对话长度特征不同
//!
//! ## 7 个内置场景
//!
//! | 场景 | 描述 | 摘要重点 |
//! |------|------|----------|
//! | Coding | 编码场景 | 代码片段/技术决策/bug 修复/架构变更 |
//! | Writing | 写作场景 | 观点/论据/素材/结构 |
//! | Research | 科研场景 | 假设/方法/数据/结论/引用 |
//! | Daily | 日常场景 | 事件/地点/人物/情感 |
//! | Finance | 金融场景 | 交易/金额/时间/风险/收益 |
//! | Design | 设计场景 | 设计决策/用户反馈/迭代版本 |
//! | OfficeWork | 工作场景 | 会议决议/待办/文档变更 |
//!
//! ## 摘要 prompt 优先级链
//!
//! 用户 custom_summary_template > SummaryFocus 预设模板 > 默认硬编码模板
//!
//! ## 架构定位
//!
//! 本 crate 是 5 个特配 crate 之一，与 hippocampus-models/windows/agents/skills 平行，
//! 不依赖其他特配 crate，联动由 hippocampus-presets 组合层处理。

pub mod profile;
pub mod priority_tags;
pub mod retrieval_strategy;
pub mod scenario;
pub mod score_weights;
pub mod summary_focus;

pub use profile::ScenarioProfile;
pub use priority_tags::{priority_tags_for, tag_priority_score};
pub use retrieval_strategy::RetrievalStrategy;
pub use scenario::Scenario;
pub use score_weights::ScoreWeights;
pub use summary_focus::{SummaryFocus, summary_template_for};

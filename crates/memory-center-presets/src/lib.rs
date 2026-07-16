//! # MemoryCenter 预设组合层
//!
//! 5 个特配 crate 的组合层，提供 Builder + 叠加引擎 + 联动机制。
//!
//! ## 架构定位
//!
//! ```text
//!                ┌──────────────────────┐
//!                │ MemoryCenter-presets  │ ← 本 crate（组合层）
//!                └──────────┬───────────┘
//!                           │
//!    ┌──────────┬───────────┼───────────┬──────────┐
//!    ▼          ▼           ▼           ▼          ▼
//! ┌────────┐┌──────────┐┌─────────┐┌──────────┐┌────────┐
//! │ models ││scenarios ││windows  ││ agents   ││skills  │ ← 5 个特配 crate（平行）
//! └────┬───┘└────┬─────┘└────┬────┘└────┬─────┘└───┬────┘
//!      │         │           │          │          │
//!      ▼         ▼           ▼          ▼          ▼
//!    ┌──────────────────────────────────────────────────┐
//!    │                  MemoryCenter-core                │ ← 核心依赖
//!    └──────────────────────────────────────────────────┘
//! ```
//!
//! ## 核心职责
//!
//! 1. **Builder**：链式收集 5 个可选 Profile + 用户覆盖参数
//! 2. **联动机制**：Agent → Window 自动推导（Claude Code → ClaudeCodeCompact 等）
//! 3. **叠加引擎**：解析字段优先级，生成最终生效值
//!
//! ## 优先级链
//!
//! 不同字段的优先级链不同（v2.54 P20 修正：与 `builder.rs` 实际实现对齐）。
//!
//! ### 归档阈值（archive_threshold）优先级
//!
//! 实际实现 4 层（[builder.rs](src/builder.rs) 的 `or_else` 链）：
//!
//! ```text
//! 用户显式参数 > ScenarioProfile.archive_threshold > ModelVariant.archive_strategy.threshold() > 默认 400K
//! ```
//!
//! v2.54 P18 引入 Scenario/Model 协商：当解析阈值 > `model.context_window × 0.8` 时
//! 降级到 model 窗口的 80%（用户显式阈值不受影响）。
//!
//! ### 摘要模板（summary_template）优先级
//!
//! ```text
//! 用户 custom > ScenarioProfile.custom_summary_template > SummaryFocus 预设 > 默认硬编码
//! ```
//!
//! ### 其他字段的参与方
//!
//! > **历史说明**：早期文档承诺 7 层优先级
//! > （用户 > Scenario > Model > Window > Skill > Agent > 默认），
//! > 但 `archive_threshold` 实际只实现 4 层。Window/Skill/Agent 三层不参与阈值解析，
//! > 仅参与其他字段：
//!
//! - **Window**：`archive_to_memory_center` 联合判断（Agent/Window 任一禁用则不归档）+ Agent→Window 联动推导
//! - **Skill**：当前未参与任何阈值解析（预留扩展，未来可能影响 summary focus）
//! - **Agent**：`session_prefix` / `archive_to_memory_center` / `usage_protocol` 生成
//!
//! ## 联动规则
//!
//! 当 Agent 已设置但 Window 未设置时，自动推导 Window：
//!
//! | Agent | 推导 Window |
//! |---|---|
//! | ClaudeCode | WindowProfile::claude_code()（ClaudeCodeCompact, 180K） |
//! | Cursor | WindowProfile::cursor()（CursorChat, 150K） |
//! | Trae | WindowProfile::trae()（TraeConversation, 120K） |
//! | Codex | WindowProfile::codex()（CodexRolling, 100K） |
//! | 其他 | WindowProfile::default()（GenericSliding, 100K） |
//!
//! ## 使用示例
//!
//! ```rust
//! use memory_center_presets::PresetBuilder;
//! use memory_center_agents::AgentProfile;
//! use memory_center_scenarios::{Scenario, ScenarioProfile};
//!
//! let combined = PresetBuilder::new()
//!     .with_agent(AgentProfile::claude_code())
//!     .with_scenario(ScenarioProfile::from_scenario(Scenario::Coding))
//!     .build()
//!     .unwrap();
//!
//! // combined.archive_threshold() 返回解析后的归档阈值
//! // combined.summary_template() 返回解析后的摘要模板
//! ```

pub mod builder;
pub mod combined;
pub mod detect;
pub mod linkage;
pub mod resolver;
pub mod scenario_detect;

pub use builder::{build_from_strings, scenario_from_str, scenario_to_str, PresetBuilder};
pub use combined::{CombinedProfile, DEFAULT_ARCHIVE_THRESHOLD, TriggerRule, UsageProtocol};
pub use detect::{detect_agent_client, default_scenario_for_agent, resolve_scenario_name, DetectedAgent, DetectionSource};
pub use linkage::derive_window_from_agent;
pub use resolver::{resolve_archive_threshold, ResolutionTrace, ThresholdSource};
pub use scenario_detect::{
    DetectionResult, HybridScenarioDetector, HttpScenarioDetector, KeywordScenarioDetector,
    resolve_effective_scenario,
};

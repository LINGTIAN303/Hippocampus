//! # PriorityResolver（优先级裁决器，v2.54 P21 新增）
//!
//! 抽取 archive_threshold 的裁决逻辑，独立可测。
//!
//! ## 优先级链（v2.54 P20 修正：与 builder.rs 实际实现对齐）
//!
//! ```text
//! 用户显式参数 > ScenarioProfile.archive_threshold > ModelVariant.archive_strategy.threshold() > 默认 400K
//! ```
//!
//! v2.54 P18 引入 Scenario/Model 协商：当解析阈值 > `model.context_window × 0.8` 时
//! 降级到 model 窗口的 80%（用户显式阈值不受影响）。
//!
//! ## 设计目标
//!
//! - **可测试**：裁决逻辑从 `PresetBuilder::build` 内联的 `or_else` 链抽取为独立函数
//! - **可观测**：`ResolutionTrace` 记录裁决过程，用于日志和单测断言
//! - **可扩展**：未来可在此增加加权/协商策略，不破坏 PresetBuilder

use memory_center_models::ModelVariant;
use memory_center_scenarios::ScenarioProfile;

use crate::combined::DEFAULT_ARCHIVE_THRESHOLD;

/// 阈值来源（哪一层胜出）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdSource {
    /// 用户显式参数（最高优先级）
    UserExplicit,
    /// ScenarioProfile.archive_threshold
    Scenario,
    /// ModelVariant.archive_strategy.threshold()
    Model,
    /// 默认兜底值（DEFAULT_ARCHIVE_THRESHOLD = 400K）
    Default,
}

/// 裁决轨迹（轻量结构体，v2.54 P21）
///
/// 记录 archive_threshold 的裁决过程，用于日志输出和单测断言。
///
/// # 字段说明
///
/// - `source`：哪一层胜出（UserExplicit / Scenario / Model / Default）
/// - `threshold`：最终阈值（可能经过 P18 协商降级）
/// - `negotiated`：是否触发 P18 协商降级
/// - `model_ceiling`：协商上限（`model.context_window × 0.8`），None 表示无协商或无 model
#[derive(Debug, Clone, Copy)]
pub struct ResolutionTrace {
    /// 哪一层胜出
    pub source: ThresholdSource,
    /// 最终阈值（可能经过协商降级）
    pub threshold: usize,
    /// 是否触发 P18 协商降级
    pub negotiated: bool,
    /// 协商上限（model.context_window × 0.8），None 表示无协商或无 model
    pub model_ceiling: Option<usize>,
}

impl ResolutionTrace {
    /// 未协商的轨迹
    fn no_negotiation(source: ThresholdSource, threshold: usize) -> Self {
        Self {
            source,
            threshold,
            negotiated: false,
            model_ceiling: None,
        }
    }

    /// 触发协商降级的轨迹
    fn negotiated_down(source: ThresholdSource, threshold: usize, model_ceiling: usize) -> Self {
        Self {
            source,
            threshold,
            negotiated: true,
            model_ceiling: Some(model_ceiling),
        }
    }
}

/// 解析归档阈值（v2.54 P21 抽取自 builder.rs）
///
/// ## 优先级链
///
/// ```text
/// 用户显式参数 > ScenarioProfile.archive_threshold > ModelVariant.archive_strategy.threshold() > 默认 400K
/// ```
///
/// ## P18 协商机制
///
/// 当阈值非用户显式设定时，若解析出的阈值 > `model.context_window × 0.8`，
/// 降级到 model 窗口的 80%，避免「scenario 阈值 > model 窗口」导致的「永不触发」死区。
///
/// **用户显式阈值不受协商影响**（仍为最高优先级，用户对窗口有明确认知时尊重其选择）。
///
/// # 参数
///
/// - `user_threshold`: 用户显式阈值（None 表示未设定）
/// - `scenario`: 场景配置（可选，提供 archive_threshold）
/// - `model`: 模型型号（可选，提供 context_window 和 archive_strategy）
///
/// # 返回
///
/// `(最终阈值, ResolutionTrace)`，trace 用于日志输出和单测断言
pub fn resolve_archive_threshold(
    user_threshold: Option<usize>,
    scenario: Option<&ScenarioProfile>,
    model: Option<&ModelVariant>,
) -> (usize, ResolutionTrace) {
    // 1. 4 层优先级链：用户 > scenario > model > 默认
    let (raw_threshold, source) = match user_threshold {
        Some(t) => (t, ThresholdSource::UserExplicit),
        None => match scenario.as_ref().map(|s| s.archive_threshold) {
            Some(t) => (t, ThresholdSource::Scenario),
            None => match model.as_ref().map(|m| m.archive_strategy.threshold()) {
                Some(t) => (t, ThresholdSource::Model),
                None => (DEFAULT_ARCHIVE_THRESHOLD, ThresholdSource::Default),
            },
        },
    };

    // 2. P18 协商：用户显式阈值不受影响
    if user_threshold.is_some() {
        return (raw_threshold, ResolutionTrace::no_negotiation(source, raw_threshold));
    }

    let Some(model) = model else {
        // 无 model，跳过协商
        return (raw_threshold, ResolutionTrace::no_negotiation(source, raw_threshold));
    };

    let model_ceiling = (model.context_window as f64 * 0.8) as usize;
    if raw_threshold > model_ceiling {
        // 触发协商降级
        tracing::warn!(
            resolved_threshold = raw_threshold,
            model_name = %model.name,
            model_context_window = model.context_window,
            model_ceiling,
            negotiated_threshold = model_ceiling,
            threshold_source = ?source,
            "v2.54 P18：解析阈值超过模型上下文窗口的 80%，已协商降级（用户显式阈值不受此影响）"
        );
        (
            model_ceiling,
            ResolutionTrace::negotiated_down(source, model_ceiling, model_ceiling),
        )
    } else {
        (
            raw_threshold,
            ResolutionTrace::no_negotiation(source, raw_threshold),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_center_models::{ModelFamily, ModelVariant};
    use memory_center_scenarios::{Scenario, ScenarioProfile};

    // ========================================================================
    // 优先级链测试（4 层）
    // ========================================================================

    #[test]
    fn test_user_explicit_wins_all() {
        // 用户显式 500K + scenario(500K) + model(8K 窗口) → 用户胜出，不协商
        let scenario = ScenarioProfile::from_scenario(Scenario::Coding);
        let model = ModelVariant::local_default();
        let (threshold, trace) =
            resolve_archive_threshold(Some(500_000), Some(&scenario), Some(&model));
        assert_eq!(threshold, 500_000);
        assert_eq!(trace.source, ThresholdSource::UserExplicit);
        assert!(!trace.negotiated);
        assert_eq!(trace.model_ceiling, None);
    }

    #[test]
    fn test_scenario_wins_when_no_user() {
        // 无用户 + scenario(500K) + model(1M 窗口, 80% = 800K) → scenario 胜出，不协商
        let scenario = ScenarioProfile::from_scenario(Scenario::Coding);
        let model = ModelVariant::claude_opus_4_6();
        let (threshold, trace) = resolve_archive_threshold(None, Some(&scenario), Some(&model));
        assert_eq!(threshold, 500_000);
        assert_eq!(trace.source, ThresholdSource::Scenario);
        assert!(!trace.negotiated);
    }

    #[test]
    fn test_model_wins_when_no_user_no_scenario() {
        // 无用户 + 无 scenario + model(Claude Opus 4.6, threshold=400K)
        let model = ModelVariant::claude_opus_4_6();
        let (threshold, trace) = resolve_archive_threshold(None, None, Some(&model));
        assert_eq!(threshold, 400_000);
        assert_eq!(trace.source, ThresholdSource::Model);
        assert!(!trace.negotiated);
    }

    #[test]
    fn test_default_when_all_none() {
        // 全部 None → 默认 400K
        let (threshold, trace) = resolve_archive_threshold(None, None, None);
        assert_eq!(threshold, DEFAULT_ARCHIVE_THRESHOLD);
        assert_eq!(trace.source, ThresholdSource::Default);
        assert!(!trace.negotiated);
    }

    // ========================================================================
    // P18 协商机制测试
    // ========================================================================

    #[test]
    fn test_p18_negotiation_scenario_exceeds_model_window() {
        // Coding(500K) + local_default(8K 窗口, 80% = 6.4K) → 协商到 6.4K
        let scenario = ScenarioProfile::from_scenario(Scenario::Coding);
        let model = ModelVariant::local_default();
        let (threshold, trace) = resolve_archive_threshold(None, Some(&scenario), Some(&model));
        assert_eq!(threshold, 6_400);
        assert_eq!(trace.source, ThresholdSource::Scenario);
        assert!(trace.negotiated);
        assert_eq!(trace.model_ceiling, Some(6_400));
    }

    #[test]
    fn test_p18_negotiation_scenario_within_model_window() {
        // Coding(500K) + claude_opus_4_6(1M 窗口, 80% = 800K) → 不协商
        let scenario = ScenarioProfile::from_scenario(Scenario::Coding);
        let model = ModelVariant::claude_opus_4_6();
        let (threshold, trace) = resolve_archive_threshold(None, Some(&scenario), Some(&model));
        assert_eq!(threshold, 500_000);
        assert_eq!(trace.source, ThresholdSource::Scenario);
        assert!(!trace.negotiated);
    }

    #[test]
    fn test_p18_negotiation_user_override_skips_negotiation() {
        // 用户显式 500K + local_default(8K 窗口) → 不协商（用户显式阈值不受影响）
        let scenario = ScenarioProfile::from_scenario(Scenario::Coding);
        let model = ModelVariant::local_default();
        let (threshold, trace) =
            resolve_archive_threshold(Some(500_000), Some(&scenario), Some(&model));
        assert_eq!(threshold, 500_000);
        assert_eq!(trace.source, ThresholdSource::UserExplicit);
        assert!(!trace.negotiated);
    }

    #[test]
    fn test_p18_negotiation_no_model_skips_negotiation() {
        // 无用户 + scenario(500K) + 无 model → 不协商
        let scenario = ScenarioProfile::from_scenario(Scenario::Coding);
        let (threshold, trace) = resolve_archive_threshold(None, Some(&scenario), None);
        assert_eq!(threshold, 500_000);
        assert_eq!(trace.source, ThresholdSource::Scenario);
        assert!(!trace.negotiated);
    }

    // ========================================================================
    // 边界场景测试
    // ========================================================================

    #[test]
    fn test_threshold_exactly_at_model_ceiling() {
        // 阈值正好等于 model_ceiling（不触发协商，因为 > 是严格大于）
        // scenario.archive_threshold = 6400, model.context_window = 8000（80% = 6400）
        let scenario = ScenarioProfile::from_scenario(Scenario::Coding)
            .with_archive_threshold(6_400);
        let model = ModelVariant::local_default();
        let (threshold, trace) = resolve_archive_threshold(None, Some(&scenario), Some(&model));
        assert_eq!(threshold, 6_400);
        assert!(!trace.negotiated, "阈值等于 model_ceiling 时不触发协商（严格大于）");
    }

    #[test]
    fn test_custom_model_triggers_negotiation() {
        // custom model：context_window=100K → LargeWindow threshold=20K（100K/5）
        // 20K > 100K × 0.8 = 80K？否，20K < 80K，不协商
        // 改用 Standard：context_window=100K → Standard threshold=25K（100K/4），仍 < 80K
        // 要触发协商，需让 archive_strategy.threshold > context_window × 0.8
        // 用 with_archive_threshold 设置 scenario 阈值为 90K + custom model(100K 窗口)
        let scenario = ScenarioProfile::from_scenario(Scenario::Coding)
            .with_archive_threshold(90_000);
        let model = ModelVariant::custom(
            "test-model",
            ModelFamily::Custom,
            100_000,
        );
        let (threshold, trace) = resolve_archive_threshold(None, Some(&scenario), Some(&model));
        assert_eq!(threshold, 80_000, "应协商到 80K（100K × 0.8）");
        assert_eq!(trace.source, ThresholdSource::Scenario);
        assert!(trace.negotiated);
        assert_eq!(trace.model_ceiling, Some(80_000));
    }

    #[test]
    fn test_model_source_with_custom_small_window() {
        // custom model：context_window=50K → Standard threshold=12.5K（50K/4）
        // 12.5K < 50K × 0.8 = 40K → 不协商
        let model = ModelVariant::custom(
            "small-model",
            ModelFamily::Custom,
            50_000,
        );
        let (threshold, trace) = resolve_archive_threshold(None, None, Some(&model));
        assert_eq!(threshold, 12_500, "custom 50K → Standard 12.5K");
        assert_eq!(trace.source, ThresholdSource::Model);
        assert!(!trace.negotiated);
    }

    #[test]
    fn test_resolution_trace_debug_format() {
        // 验证 ResolutionTrace 可被 Debug 格式化（用于日志输出）
        let trace = ResolutionTrace::no_negotiation(ThresholdSource::Default, 400_000);
        let debug_str = format!("{:?}", trace);
        assert!(debug_str.contains("Default"));
        assert!(debug_str.contains("400000"));
    }
}

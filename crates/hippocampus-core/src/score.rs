//! # 评分模块
//!
//! 可插拔评分架构。
//!
//! ## 4 维加权评分（P3 实现 3 维，TF-IDF 留 v2）
//!
//! 月级评分淘汰时，对每个周级记忆文件按以下 4 维加权评分：
//!
//! 1. **时效性**：时间衰减，越新分越高（纯算法，已实现）
//! 2. **访问频率**：`access_count` 归一化（纯算法，已实现）
//! 3. **主题相关性**：TF-IDF 匹配（需要语义理解，v2 实现）
//! 4. **用户显式标记**：`importance` 字段归一化（纯算法，已实现）
//!
//! ## 架构
//!
//! - [`Scorer`] trait：评分器接口，可插拔
//! - [`DefaultScorer`]：默认启发式实现（时间衰减 + 访问频率 + importance）
//! - LLM 评分作为可选实现（v2 路线图）

use crate::model::MemoryFile;
use chrono::{DateTime, Utc};

/// 评分器 trait
///
/// 评分器对记忆文件打分，用于月级评分淘汰。
/// 默认实现使用启发式算法，LLM 评分可作为可选实现。
pub trait Scorer: Send + Sync {
    /// 对记忆文件评分，返回 0.0-100.0 的分数
    fn score(&self, file: &MemoryFile) -> f64;
}

/// 评分权重配置
///
/// 各维度权重应为 0.0-1.0，四项之和应等于 1.0（不强制校验，由调用方保证）。
#[derive(Debug, Clone)]
pub struct ScoreWeights {
    /// 时效性权重（0.0-1.0）
    pub timeliness: f64,
    /// 访问频率权重（0.0-1.0）
    pub access_frequency: f64,
    /// 主题相关性权重（0.0-1.0，P3 未实现，固定 0.0）
    pub topic_relevance: f64,
    /// 用户显式标记权重（0.0-1.0）
    pub user_marked: f64,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        // P3 默认权重：3 维均分（topic_relevance 留 v2，权重为 0）
        // 时效性 / 访问频率 / 用户标记 各占 1/3
        Self {
            timeliness: 1.0 / 3.0,
            access_frequency: 1.0 / 3.0,
            topic_relevance: 0.0, // v2 实现
            user_marked: 1.0 / 3.0,
        }
    }
}

/// 默认启发式评分器
///
/// P3 实现的 3 维启发式算法：
/// - **时效性**：时间衰减，半衰期 7 天（一周前的记忆得 50 分，两周前得 25 分）
/// - **访问频率**：`access_count` 归一化，10 次访问即满分
/// - **用户显式标记**：`importance` 字段直接归一化（0-100 → 0-100）
///
/// 主题相关性（TF-IDF）留 v2 实现。
pub struct DefaultScorer {
    /// 评分权重
    weights: ScoreWeights,
    /// 时间衰减的参考点（默认为当前时间）
    now: DateTime<Utc>,
    /// 时效性半衰期（天数），默认 7 天
    half_life_days: f64,
    /// 访问频率满分的阈值（次），默认 10 次
    access_full_score_threshold: f64,
}

impl DefaultScorer {
    /// 用默认配置创建（半衰期 7 天，访问满分阈值 10 次）
    pub fn new() -> Self {
        Self {
            weights: ScoreWeights::default(),
            now: Utc::now(),
            half_life_days: 7.0,
            access_full_score_threshold: 10.0,
        }
    }

    /// 用自定义权重创建
    pub fn with_weights(weights: ScoreWeights) -> Self {
        Self {
            weights,
            ..Self::new()
        }
    }

    /// 设置评分参考时间点（用于测试）
    pub fn with_now(mut self, now: DateTime<Utc>) -> Self {
        self.now = now;
        self
    }

    /// 设置时效性半衰期（天数）
    pub fn with_half_life_days(mut self, days: f64) -> Self {
        self.half_life_days = days;
        self
    }

    /// 设置访问频率满分的阈值
    pub fn with_access_threshold(mut self, threshold: f64) -> Self {
        self.access_full_score_threshold = threshold;
        self
    }

    /// 计算时效性分数（0-100）
    ///
    /// 半衰期衰减：`score = 100 * 0.5^(age_days / half_life_days)`
    /// - 当前时间归档：100 分
    /// - 半衰期（7 天）前归档：50 分
    /// - 两个半衰期（14 天）前归档：25 分
    fn timeliness_score(&self, file: &MemoryFile) -> f64 {
        let age_seconds = (self.now - file.archived_at).num_seconds().max(0) as f64;
        let age_days = age_seconds / 86400.0;
        let score = 100.0 * 0.5_f64.powf(age_days / self.half_life_days);
        score.clamp(0.0, 100.0)
    }

    /// 计算访问频率分数（0-100）
    ///
    /// 线性归一化：`score = (access_count / threshold) * 100`
    /// 达到阈值（默认 10 次）即满分。
    fn access_frequency_score(&self, file: &MemoryFile) -> f64 {
        let score = (file.access_count as f64 / self.access_full_score_threshold) * 100.0;
        score.clamp(0.0, 100.0)
    }

    /// 计算用户显式标记分数（0-100）
    ///
    /// `importance` 字段已经是 0-100，直接归一化。
    fn user_marked_score(&self, file: &MemoryFile) -> f64 {
        (file.importance as f64).clamp(0.0, 100.0)
    }
}

impl Default for DefaultScorer {
    fn default() -> Self {
        Self::new()
    }
}

impl Scorer for DefaultScorer {
    fn score(&self, file: &MemoryFile) -> f64 {
        let timeliness = self.timeliness_score(file);
        let access = self.access_frequency_score(file);
        let user = self.user_marked_score(file);
        // 主题相关性留 v2，固定 50 分占位（但因权重为 0 不影响结果）
        let topic = 50.0;

        let w = &self.weights;
        let total = timeliness * w.timeliness
            + access * w.access_frequency
            + topic * w.topic_relevance
            + user * w.user_marked;

        total.clamp(0.0, 100.0)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ArchivePeriod, MessageContent, MessageTurn, Tag};
    use chrono::Duration;
    use uuid::Uuid;

    /// 构造测试用 MemoryFile
    fn make_memory(
        archived_at: DateTime<Utc>,
        access_count: u64,
        importance: u8,
    ) -> MemoryFile {
        let turn = MessageTurn {
            id: Uuid::new_v4(),
            user_message: MessageContent {
                text: Some("测试内容".into()),
                attachments: Vec::new(),
                tool_calls: Vec::new(),
                thinking: None,
            },
            llm_message: MessageContent {
                text: Some("测试回复".into()),
                attachments: Vec::new(),
                tool_calls: Vec::new(),
                thinking: None,
            },
            tags: vec![Tag::Text],
            timestamp: archived_at,
            token_count: 100,
        };
        let mut file = MemoryFile::new("test-session", None, vec![turn], ArchivePeriod::Weekly);
        file.archived_at = archived_at;
        file.access_count = access_count;
        file.importance = importance;
        file
    }

    #[test]
    fn test_timeliness_score_fresh() {
        let now = Utc::now();
        let scorer = DefaultScorer::new().with_now(now);
        let file = make_memory(now, 0, 0);
        // 刚归档：100 分
        let score = scorer.timeliness_score(&file);
        assert!((score - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_timeliness_score_half_life() {
        let now = Utc::now();
        let scorer = DefaultScorer::new().with_now(now).with_half_life_days(7.0);
        // 7 天前归档：50 分
        let file = make_memory(now - Duration::days(7), 0, 0);
        let score = scorer.timeliness_score(&file);
        assert!((score - 50.0).abs() < 1.0);
    }

    #[test]
    fn test_timeliness_score_two_half_lives() {
        let now = Utc::now();
        let scorer = DefaultScorer::new().with_now(now).with_half_life_days(7.0);
        // 14 天前归档：25 分
        let file = make_memory(now - Duration::days(14), 0, 0);
        let score = scorer.timeliness_score(&file);
        assert!((score - 25.0).abs() < 1.0);
    }

    #[test]
    fn test_access_frequency_score_zero() {
        let scorer = DefaultScorer::new();
        let file = make_memory(Utc::now(), 0, 0);
        let score = scorer.access_frequency_score(&file);
        assert!((score - 0.0).abs() < 0.1);
    }

    #[test]
    fn test_access_frequency_score_full() {
        let scorer = DefaultScorer::new().with_access_threshold(10.0);
        let file = make_memory(Utc::now(), 10, 0);
        let score = scorer.access_frequency_score(&file);
        assert!((score - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_access_frequency_score_half() {
        let scorer = DefaultScorer::new().with_access_threshold(10.0);
        let file = make_memory(Utc::now(), 5, 0);
        let score = scorer.access_frequency_score(&file);
        assert!((score - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_user_marked_score() {
        let scorer = DefaultScorer::new();
        let file = make_memory(Utc::now(), 0, 80);
        let score = scorer.user_marked_score(&file);
        assert!((score - 80.0).abs() < 0.1);
    }

    #[test]
    fn test_total_score_with_default_weights() {
        // 默认权重：3 维均分（各 1/3）
        let now = Utc::now();
        let scorer = DefaultScorer::new().with_now(now);
        let file = make_memory(now, 10, 100); // 全满分
        let score = scorer.score(&file);
        assert!((score - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_total_score_zero() {
        let now = Utc::now();
        let scorer = DefaultScorer::new().with_now(now);
        // 14 天前归档 + 0 访问 + 0 importance
        let file = make_memory(now - Duration::days(14), 0, 0);
        let score = scorer.score(&file);
        // 时效性 25 * 1/3 + 访问 0 * 1/3 + 用户 0 * 1/3 = ~8.33
        assert!((score - 25.0 / 3.0).abs() < 1.0);
    }

    #[test]
    fn test_score_range_0_to_100() {
        let now = Utc::now();
        let scorer = DefaultScorer::new().with_now(now);
        // 各种极端情况
        let cases = vec![
            make_memory(now, 0, 0),           // 最低
            make_memory(now - Duration::days(30), 0, 0), // 很旧
            make_memory(now, 100, 100),       // 最高
            make_memory(now, 5, 50),          // 中等
        ];
        for file in cases {
            let score = scorer.score(&file);
            assert!((0.0..=100.0).contains(&score), "分数越界: {}", score);
        }
    }
}

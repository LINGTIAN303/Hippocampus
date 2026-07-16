//! # 模型型号（ModelVariant）
//!
//! 具体模型版本的参数描述，高频迭代（几个月一代）。
//! 内置 2026 年 7 月最新主流型号构造器（已核查官方文档），用户也可通过 [`ModelVariant::custom`] 自定义。
//!
//! ## 家族/型号分离设计
//!
//! - 家族（[`crate::family::ModelFamily`]）：稳定大类，enum，低频迭代
//! - 型号（[`ModelVariant`]）：具体版本，struct + 构造器，高频迭代
//!
//! 新型号发布时只需新增构造器方法，无需改家族 enum。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::family::ModelFamily;
use crate::tokenizer::{Tokenizer, TokenizerKind};

/// 工具调用格式
///
/// 不同模型家族支持的工具调用协议不同，影响 tool_calls 消息的序列化方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallFormat {
    /// OpenAI function calling 格式（JSON）
    /// GPT 系列、DeepSeek、Qwen、Llama、Grok 等
    OpenAI,

    /// Anthropic tool_use content block 格式
    /// Claude 系列
    Anthropic,

    /// Gemini function call 格式
    /// Gemini 系列
    Gemini,

    /// XML 标签格式（部分开源模型）
    Xml,

    /// 无工具调用能力
    None,
}

/// 归档策略
///
/// 根据模型上下文窗口大小，采用不同的归档阈值与策略。
///
/// # 阈值来源（v2.54 P22 文档化）
///
/// 阈值有两个来源，使用时需注意区分：
///
/// | 来源 | 适用 | 比率 | 说明 |
/// |---|---|---|---|
/// | **专家调校值** | 内置构造器（15 个型号） | 0.20-0.50 | 根据模型实际能力调优，充分利用窗口 |
/// | **custom 推导值** | [`ModelVariant::custom`] | 0.25（统一） | 对未知模型保守，无边界跳变 |
///
/// **设计原则**：
/// - 内置构造器：专家根据模型能力调优（如 Claude Opus 4.6 的 1M 窗口取 400K = 0.40，
///   因为 Claude 在长上下文下仍保持高质量；DeepSeek V4 的 1M 窗口取 200K = 0.20，
///   因为 MoE 模型在超长上下文下质量衰减更快）
/// - custom 推导：统一 0.25 比率，对未知模型安全，且 200K 边界无跳变
///
/// 详见 [`ModelVariant::custom`] 的文档注释。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "threshold")]
pub enum ArchiveStrategy {
    /// 长窗口模型（≥200K）：阈值高，单次归档多内容
    /// 如 Claude Opus 4.6（1M）、Gemini 3 Pro（1M+）
    LargeWindow { threshold: usize },

    /// 标准窗口（32K-128K）：标准归档
    /// 如 GPT-5.2（128K）、Qwen 3（128K）、Llama 4（128K）
    Standard { threshold: usize },

    /// 小窗口（≤16K）：频繁归档，摘要更精炼
    /// 如本地小模型、旧模型
    SmallWindow { threshold: usize },
}

impl ArchiveStrategy {
    /// 返回归档阈值
    pub fn threshold(&self) -> usize {
        match self {
            Self::LargeWindow { threshold } => *threshold,
            Self::Standard { threshold } => *threshold,
            Self::SmallWindow { threshold } => *threshold,
        }
    }

    /// 返回硬上限（`threshold × HARD_LIMIT_RATIO`，v2.54 P19 统一系数来源）
    ///
    /// 与 `ArchiveConfig::from_threshold` 使用相同的系数，
    /// 保证 `ArchiveStrategy` 与 `ArchiveConfig` 的硬上限计算一致。
    pub fn hard_limit(&self) -> usize {
        (self.threshold() as f32 * memory_center_core::model::HARD_LIMIT_RATIO) as usize
    }
}

/// 模型型号（具体版本参数）
///
/// 描述一个具体模型的所有参数，驱动 MemoryCenter 的针对化记忆工作流。
///
/// # 设计原则
///
/// - 内置构造器（如 [`ModelVariant::claude_opus_4_6`]）提供 2026 最新型号预设
/// - 用户可通过 [`ModelVariant::custom`] 自定义新型号，无需等发版
/// - 家族稳定，型号高频迭代——新型号只需新增构造器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVariant {
    /// 模型家族
    pub family: ModelFamily,

    /// 型号名称（如 "claude-opus-4.6" / "gpt-5.2" / "gemini-3-pro"）
    pub name: String,

    /// 上下文窗口大小（token 数）
    pub context_window: usize,

    /// Tokenizer 类型
    pub tokenizer: TokenizerKind,

    /// 是否支持思考链（reasoning / thinking）
    ///
    /// Claude 4.x / DeepSeek R1 / GPT-5（o1/o3 系列）支持
    /// 影响：Thinking 标签特殊处理、思考链独立归档
    pub supports_thinking: bool,

    /// 是否支持多模态（图片输入）
    ///
    /// Claude 4.x / GPT-5 / Gemini 3 / Qwen 3 支持
    /// 影响：Image 标签 + 附件归档策略
    pub supports_vision: bool,

    /// 是否支持音频输入
    ///
    /// Gemini 3 / Qwen 3 Audio 支持
    /// 影响：Voice 标签处理
    pub supports_audio: bool,

    /// 工具调用格式
    pub tool_call_format: ToolCallFormat,

    /// 归档策略（基于上下文窗口大小）
    pub archive_strategy: ArchiveStrategy,

    /// 摘要生成的最大 token 数
    pub summary_max_tokens: usize,

    /// 废弃标记（v2.54 P25 新增）
    ///
    /// - `None`：活跃型号，推荐使用
    /// - `Some(原因)`：已废弃，建议迁移到替代型号
    ///
    /// 示例：`Some("已停服，请迁移至 deepseek-v4-pro")`
    ///
    /// 影响：
    /// - `preset_list_models` 输出时标记 deprecated
    /// - 日志记录时会附带 deprecated 标记
    /// - 不影响 `find()` 查找（仍可正常使用，便于向后兼容）
    ///
    /// # 序列化说明
    ///
    /// - `Serialize`：正常输出（None → null，Some(s) → 字符串）
    /// - `Deserialize`：跳过（`skip_deserializing`），反序列化时默认为 None
    ///
    /// 原因：`&'static str` 无法从反序列化输入（非 'static 生命周期）构造，
    /// 该字段由服务端构造器权威设置，客户端无需传入。
    /// 若需通过 JSON 配置文件加载自定义 deprecated 标记，应改用 `Option<String>`
    /// 并调整构造器逻辑（当前无此需求）。
    #[serde(skip_deserializing)]
    pub deprecated: Option<&'static str>,
}

impl ModelVariant {
    /// 构建对应的 Tokenizer 实例
    pub fn build_tokenizer(&self) -> Arc<dyn Tokenizer> {
        self.tokenizer.build()
    }

    /// 快速计算文本的 token 数
    pub fn count_tokens(&self, text: &str) -> usize {
        self.build_tokenizer().count_tokens(text)
    }

    // ========================================================================
    // 2026 年 7 月最新主流型号内置构造器（已核查官方文档）
    // ========================================================================

    /// Anthropic Claude Opus 4.6（2026 年 2 月发布）
    ///
    /// - 上下文：100 万 token（Beta 版，正式版 200K）
    /// - 架构特点：思考链、多模态、超长上下文
    /// - tokenizer：cl100k 近似 × 1.05
    pub fn claude_opus_4_6() -> Self {
        Self {
            family: ModelFamily::Claude,
            name: "claude-opus-4.6".into(),
            context_window: 1_000_000,
            tokenizer: TokenizerKind::ClaudeApprox,
            supports_thinking: true,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::Anthropic,
            // 专家调校：1M 窗口取 400K（0.40），Claude 在长上下文下仍保持高质量
            // （custom 推导同窗口取 250K = 0.25，更保守）
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 400_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// Anthropic Claude Opus 4.8（2026 年 5 月发布）
    ///
    /// - 上下文：200K token（正式版规格，与 4.6 正式版一致）
    /// - 架构特点：思考链、多模态、Opus 4.7 的全面升级
    /// - 定位：Opus 级旗舰，API 普遍可用
    /// - 发布时间：Fable 5 之前约两周
    pub fn claude_opus_4_8() -> Self {
        Self {
            family: ModelFamily::Claude,
            name: "claude-opus-4.8".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::ClaudeApprox,
            supports_thinking: true,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::Anthropic,
            // 专家调校：200K 窗口取 80K（0.40），Claude 在长上下文下仍保持高质量
            // （custom 推导同窗口取 50K = 0.25，更保守）
            archive_strategy: ArchiveStrategy::Standard { threshold: 80_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// Anthropic Claude Sonnet 5（2026 年 6 月 30 日发布）
    ///
    /// - 上下文：200K token
    /// - 架构特点：思考链、多模态、Agent 属性强化（Anthropic 默认模型）
    /// - 定价：输入 $2/M tokens，输出 $10/M tokens
    /// - tokenizer：cl100k 近似 × 1.05
    pub fn claude_sonnet_5() -> Self {
        Self {
            family: ModelFamily::Claude,
            name: "claude-sonnet-5".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::ClaudeApprox,
            supports_thinking: true,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::Anthropic,
            archive_strategy: ArchiveStrategy::Standard { threshold: 80_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// Anthropic Claude Fable 5（2026 年 6 月 10 日发布，7 月 2 日全球恢复可用）
    ///
    /// - 上下文：200K token（与 Claude 5 代标准一致）
    /// - 架构特点：Mythos 级（位置在 Opus 之上）、思考链、多模态、防护版
    /// - 定位：面向公众的 Mythos 级模型（带安全防护网）
    /// - 与 Mythos 5 共享底层模型，Fable 5 为防护版本
    /// - 曾因出口管制暂停，2026-07-01 解除，7-02 全球恢复
    pub fn claude_fable_5() -> Self {
        Self {
            family: ModelFamily::Claude,
            name: "claude-fable-5".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::ClaudeApprox,
            supports_thinking: true,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::Anthropic,
            archive_strategy: ArchiveStrategy::Standard { threshold: 80_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// Anthropic Claude Mythos 5（2026 年 6 月 10 日发布，面向特定合作方）
    ///
    /// - 上下文：200K token（与 Fable 5 一致，共享底层模型）
    /// - 架构特点：Mythos 级（最高级）、思考链、多模态、无防护网
    /// - 定位：面向特定合作方的未防护版本，普通用户难访问
    /// - 与 Fable 5 共享底层模型，Mythos 5 为未防护版本
    /// - 2026-07-01 部分解禁
    /// - 注意：访问受限，普通场景建议使用 Fable 5
    pub fn claude_mythos_5() -> Self {
        Self {
            family: ModelFamily::Claude,
            name: "claude-mythos-5".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::ClaudeApprox,
            supports_thinking: true,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::Anthropic,
            archive_strategy: ArchiveStrategy::Standard { threshold: 80_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// OpenAI GPT-5.2（2026 年最新）
    ///
    /// - 上下文：128K token
    /// - 架构特点：function calling、JSON mode、六边形战士
    /// - tokenizer：o200k_base
    pub fn gpt_5_2() -> Self {
        Self {
            family: ModelFamily::Gpt,
            name: "gpt-5.2".into(),
            context_window: 128_000,
            tokenizer: TokenizerKind::O200kBase,
            supports_thinking: false,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::Standard { threshold: 60_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// OpenAI GPT-5-Codex（编程优化版）
    ///
    /// - 上下文：128K token
    /// - 架构特点：Codex 编程优化、沙箱执行
    pub fn gpt_5_codex() -> Self {
        Self {
            family: ModelFamily::Gpt,
            name: "gpt-5-codex".into(),
            context_window: 128_000,
            tokenizer: TokenizerKind::O200kBase,
            supports_thinking: false,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::Standard { threshold: 60_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// Google Gemini 3.1 Pro（2026 年 2 月 20 日发布）
    ///
    /// - 上下文：1M token
    /// - 架构特点：原生多模态、超长上下文、推理能力 2x（vs 3.0 Pro）
    /// - ARC-AGI-2 测试 77.1%
    /// - 定价：<200K token 输入 $2/M，输出价格分级
    /// - tokenizer：v2.53 P9 起改用 `spm_or_char()`（启用 feature 时为 SentencePiece）
    pub fn gemini_3_1_pro() -> Self {
        Self {
            family: ModelFamily::Gemini,
            name: "gemini-3.1-pro".into(),
            context_window: 1_000_000,
            tokenizer: TokenizerKind::spm_or_char(), // v2.53 P9：SentencePiece 或 CharTokenizer
            supports_thinking: true, // 3.1 Pro 强化推理
            supports_vision: true,
            supports_audio: true,
            tool_call_format: ToolCallFormat::Gemini,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 400_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// DeepSeek V4-Pro（2026 年 4 月 24 日发布预览版，7 月中旬正式版）
    ///
    /// - 上下文：1M token
    /// - 架构特点：MoE 1.6T 总参数 / 49B 激活、MIT 开源、思考链
    /// - 注意：V3/V3.2 于 2026-07-24 停服，需迁移至 V4
    pub fn deepseek_v4_pro() -> Self {
        Self {
            family: ModelFamily::DeepSeek,
            name: "deepseek-v4-pro".into(),
            context_window: 1_000_000,
            tokenizer: TokenizerKind::DeepSeekApprox,
            supports_thinking: true,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            // 专家调校：1M 窗口取 200K（0.20），MoE 模型在超长上下文下质量衰减更快
            // （custom 推导同窗口取 250K = 0.25，略激进但安全）
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 200_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// DeepSeek V4-Flash（2026 年 4 月 24 日发布预览版）
    ///
    /// - 上下文：1M token
    /// - 架构特点：MoE 284B 总参数 / 13B 激活、MIT 开源、轻量高效
    /// - 适用：成本敏感场景，价格约为 V4-Pro 的 1/4
    pub fn deepseek_v4_flash() -> Self {
        Self {
            family: ModelFamily::DeepSeek,
            name: "deepseek-v4-flash".into(),
            context_window: 1_000_000,
            tokenizer: TokenizerKind::DeepSeekApprox,
            supports_thinking: false,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 200_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// 阿里 Qwen3-Coder（2025 年 7 月 23 日开源）
    ///
    /// - 上下文：原生 256K token（YaRN 可扩展至 1M）
    /// - 架构特点：编程优化、358 种编程语言、Agentic Coding
    /// - tokenizer：v2.53 P9 起改用 `spm_or_char()`（启用 feature 时为 SentencePiece）
    pub fn qwen_3_coder() -> Self {
        Self {
            family: ModelFamily::Qwen,
            name: "qwen-3-coder".into(),
            context_window: 256_000,
            tokenizer: TokenizerKind::spm_or_char(), // v2.53 P9：SentencePiece 或 CharTokenizer
            supports_thinking: false,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::Standard { threshold: 100_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// Meta Llama 4 Scout（2025 年 4 月发布）
    ///
    /// - 上下文：保守取 1M token（理论支持 10M，API 实际部署多为 1M）
    /// - 架构特点：MoE 109B 总参数、多模态、轻量化
    /// - 定位：Llama 4 家族入门级 MoE 型号
    /// - tokenizer：v2.53 P9 起改用 `spm_or_char()`（启用 feature 时为 SentencePiece）
    ///
    /// **注意**：Meta 官方理论上下文为 10M token，但实际 API 部署多为 1M。
    /// 本构造器保守取 1M，如需 10M 上下文请通过 `ModelVariant::custom()` 覆盖。
    pub fn llama_4_scout() -> Self {
        Self {
            family: ModelFamily::Llama,
            name: "llama-4-scout".into(),
            context_window: 1_000_000, // 保守取 1M（理论 10M，API 实际部署多为 1M）
            tokenizer: TokenizerKind::spm_or_char(), // v2.53 P9：SentencePiece 或 CharTokenizer
            supports_thinking: false,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 200_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// Meta Llama 4 Maverick（2025 年 4 月发布）
    ///
    /// - 上下文：1M token
    /// - 架构特点：MoE 400B 总参数、多模态、旗舰级
    /// - 定位：Llama 4 家族旗舰型号
    /// - tokenizer：v2.53 P9 起改用 `spm_or_char()`（启用 feature 时为 SentencePiece）
    pub fn llama_4_maverick() -> Self {
        Self {
            family: ModelFamily::Llama,
            name: "llama-4-maverick".into(),
            context_window: 1_000_000,
            tokenizer: TokenizerKind::spm_or_char(), // v2.53 P9：SentencePiece 或 CharTokenizer
            supports_thinking: false,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 200_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// xAI Grok 4.1（2026 年最新）
    ///
    /// - 上下文：128K token
    /// - 架构特点：实时数据接入
    pub fn grok_4_1() -> Self {
        Self {
            family: ModelFamily::Grok,
            name: "grok-4.1".into(),
            context_window: 128_000,
            tokenizer: TokenizerKind::O200kBase,
            supports_thinking: false,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::Standard { threshold: 60_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    // ========================================================================
    // v2.54 P26：Trae 内置 12 个型号（统一限制 200K 上下文）
    //
    // Trae 作为 Agent 客户端有内置模型清单，统一限制为 200K 上下文。
    // 即使原生支持更大（如 DeepSeek V4 原生 1M），在 Trae 中也被限制为 200K。
    //
    // archive_strategy 统一用 LargeWindow { threshold: 50_000 }（200K/4，0.25），
    // 与 P22 custom 推导规则一致（200K → LargeWindow, window/4）。
    //
    // tokenizer 选择：
    // - Doubao/MiniMax/Kimi：未开源，暂用 CharacterBased 兜底（待 P24 实测替换）
    // - GLM：与 Claude 分词接近，用 ClaudeApprox 近似
    // - DeepSeek/Qwen：复用家族默认 tokenizer
    // ========================================================================

    /// 字节跳动豆包 Doubao-Seed-2.1-Pro（Trae 内置，v2.54 P26 新增；P27 能力标记更新）
    ///
    /// - 上下文：200K token（Trae 内置限制，原生 256K——2026-06-23 火山引擎 FORCE 大会发布，全系 256K）
    /// - 架构特点：旗舰深度推理版、多模态图文视频深度理解、工具调用、联网检索
    /// - supports_thinking: true（"旗舰深度推理版"）
    /// - supports_vision: true（"多模态图文视频深度理解"）
    /// - tokenizer：未开源，暂用 CharacterBased 兜底
    pub fn trae_doubao_seed_2_1_pro() -> Self {
        Self {
            family: ModelFamily::Doubao,
            name: "doubao-seed-2.1-pro".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::CharacterBased,
            supports_thinking: true,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// 字节跳动豆包 Doubao-Seed-2.1-Turbo（Trae 内置，v2.54 P26 新增；P27 能力标记更新）
    ///
    /// - 上下文：200K token（Trae 内置限制，原生 256K）
    /// - 架构特点：深度思考模型、成本优化、高速响应
    /// - supports_thinking: true（"深度思考模型"，与 Pro 同系列延续思考能力）
    /// - supports_vision: true（多模态能力延续 Pro）
    /// - tokenizer：未开源，暂用 CharacterBased 兜底
    pub fn trae_doubao_seed_2_1_turbo() -> Self {
        Self {
            family: ModelFamily::Doubao,
            name: "doubao-seed-2.1-turbo".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::CharacterBased,
            supports_thinking: true,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// 字节跳动豆包 Doubao-Seed-Code（Trae 内置，代码专用，v2.54 P26 新增；P27 注释更新）
    ///
    /// - 上下文：200K token（Trae 内置限制，原生 256K）
    /// - 架构特点：代码生成优化、工业代码生成、编程语言支持
    /// - supports_thinking: false（保守，代码模型专注代码生成，未明确宣传思考链）
    /// - supports_vision: false（保守，代码模型通常不含多模态）
    /// - tokenizer：未开源，暂用 CharacterBased 兜底
    pub fn trae_doubao_seed_code() -> Self {
        Self {
            family: ModelFamily::Doubao,
            name: "doubao-seed-code".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::CharacterBased,
            supports_thinking: false,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// MiniMax-M3（Trae 内置，v2.54 P26 新增；P27 能力标记更新）
    ///
    /// - 上下文：200K token（Trae 内置限制，原生 1M——2026-06-01 发布，最高 1M tokens 长上下文）
    /// - 架构特点：MoE 架构 + MSA（稀疏注意力）、原生多模态、推理能力、代码能力、agentic work、开放权重
    /// - supports_thinking: true（"推理能力"，MSA 架构支持长上下文推理）
    /// - supports_vision: true（"原生多模态"）
    /// - tokenizer：未开源，暂用 CharacterBased 兜底
    pub fn trae_minimax_m3() -> Self {
        Self {
            family: ModelFamily::MiniMax,
            name: "minimax-m3".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::CharacterBased,
            supports_thinking: true,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// 智谱 GLM-5.2（Trae 内置，v2.54 P26 新增；P27 能力标记更新）
    ///
    /// - 上下文：200K token（Trae 内置限制，原生 200K+——2026-06-15 全量开放，旗舰 MoE）
    /// - 架构特点：混合专家架构、中英双语优化、思考链、多模态
    /// - supports_thinking: true（GLM-4.7-Flash 起即混合思考模型，5.x 延续）
    /// - supports_vision: true（GLM-4.5V 已支持 4K 图像 + 10 分钟视频，5.2 旗舰版延续多模态）
    /// - tokenizer：与 Claude 分词接近，用 ClaudeApprox 近似（待 P24 实测替换为 SentencePiece，GLM 在 HF THUDM/ 已开源）
    pub fn trae_glm_5_2() -> Self {
        Self {
            family: ModelFamily::Glm,
            name: "glm-5.2".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::ClaudeApprox,
            supports_thinking: true,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// 智谱 GLM-5.1（Trae 内置，v2.54 P26 新增；P27 能力标记更新）
    ///
    /// - 上下文：200K token（Trae 内置限制，原生 200K+——2026-04-08 开源）
    /// - 架构特点：混合专家架构、中英双语优化、思考链、多模态
    /// - supports_thinking: true（开源版延续思考链）
    /// - supports_vision: true（5.x 旗舰版延续多模态能力）
    /// - tokenizer：与 Claude 分词接近，用 ClaudeApprox 近似（待 P24 实测替换为 SentencePiece）
    pub fn trae_glm_5_1() -> Self {
        Self {
            family: ModelFamily::Glm,
            name: "glm-5.1".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::ClaudeApprox,
            supports_thinking: true,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// 智谱 GLM-5（Trae 内置，v2.54 P26 新增；P27 注释更新）
    ///
    /// - 上下文：200K token（Trae 内置限制，原生 200K——2026-02-11 发布，旗舰 MoE）
    /// - 架构特点：混合专家架构、中英双语优化、思考链
    /// - supports_thinking: true（5 系列起始即支持思考链）
    /// - supports_vision: false（保守，初代 GLM-5 多模态能力未明确宣传）
    /// - tokenizer：与 Claude 分词接近，用 ClaudeApprox 近似（待 P24 实测替换为 SentencePiece）
    pub fn trae_glm_5() -> Self {
        Self {
            family: ModelFamily::Glm,
            name: "glm-5".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::ClaudeApprox,
            supports_thinking: true,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// DeepSeek-V4-Pro（Trae 内置版，v2.54 P26 新增；P27 注释更新）
    ///
    /// - 上下文：200K token（Trae 内置限制，原生 1M——DeepSeek V4 系列 1M token 上下文，FP4+FP8 混合精度）
    /// - 注意：与 [`ModelVariant::deepseek_v4_pro`]（原生 1M）不同，此为 Trae 限制版
    /// - supports_thinking: true（V3.1 起混合模型，可切换推理层，V4 延续）
    /// - tokenizer：复用 DeepSeekApprox（V4 沿用 V3 的 GPT-2 BPE 分词）
    pub fn trae_deepseek_v4_pro() -> Self {
        Self {
            family: ModelFamily::DeepSeek,
            name: "trae-deepseek-v4-pro".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::DeepSeekApprox,
            supports_thinking: true,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// DeepSeek-V4-Flash（Trae 内置版，v2.54 P26 新增；P27 能力标记更新）
    ///
    /// - 上下文：200K token（Trae 内置限制，原生 1M——284B 总参数 / 13B 激活参数 MoE）
    /// - 注意：与 [`ModelVariant::deepseek_v4_flash`]（原生 1M）不同，此为 Trae 限制版
    /// - supports_thinking: true（V3.1 起混合模型路线，V4-Flash 延续思考链开关）
    /// - tokenizer：复用 DeepSeekApprox
    pub fn trae_deepseek_v4_flash() -> Self {
        Self {
            family: ModelFamily::DeepSeek,
            name: "trae-deepseek-v4-flash".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::DeepSeekApprox,
            supports_thinking: true,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// 月之暗面 Kimi-K2.7-Code（Trae 内置，代码专用，v2.54 P26 新增；P27 注释更新）
    ///
    /// - 上下文：200K token（Trae 内置限制，原生 256K——2026-06-12 发布，1.1T 参数）
    /// - 架构特点：编程模型、长程任务优化、长上下文编程指令遵循能力提升
    /// - supports_thinking: false（保守，Code 版专注编程，未明确宣传思考链）
    /// - supports_vision: false（保守，Code 版通常不含多模态）
    /// - tokenizer：未开源，暂用 CharacterBased 兜底
    pub fn trae_kimi_k2_7_code() -> Self {
        Self {
            family: ModelFamily::Kimi,
            name: "kimi-k2.7-code".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::CharacterBased,
            supports_thinking: false,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// 月之暗面 Kimi-K2.6（Trae 内置，v2.54 P26 新增；P27 注释更新）
    ///
    /// - 上下文：200K token（Trae 内置限制，原生 200K+——2026-05-25 K2 系列下线后推荐版本）
    /// - 架构特点：长上下文优化、通用能力、agentic 能力
    /// - supports_thinking: false（保守，K2.6 通用版未明确宣传思考链，K2.5-Thinking 为独立分支）
    /// - supports_vision: false（保守，未明确多模态能力）
    /// - tokenizer：未开源，暂用 CharacterBased 兜底
    pub fn trae_kimi_k2_6() -> Self {
        Self {
            family: ModelFamily::Kimi,
            name: "kimi-k2.6".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::CharacterBased,
            supports_thinking: false,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// 阿里 Qwen3.7-Plus（Trae 内置版，v2.54 P26 新增；P27 能力标记更新）
    ///
    /// - 上下文：200K token（Trae 内置限制，原生 1M——2026-05-20 阿里云峰会发布，Qwen3.7 统一标配 1M 上下文）
    /// - 注意：与 [`ModelVariant::qwen_3_coder`]（原生 256K）不同，此为 Trae 限制版；原生 Qwen3.7-Plus 实际 1M
    /// - 架构特点：全域思考模式（All-domain thinking）、原生多模态、35 小时连续自治执行能力
    /// - supports_thinking: true（"全域思考模式"，Qwen3 系列原生支持思考链）
    /// - supports_vision: true（"原生多模态"，Qwen3.6-Plus 起即原生多模态）
    /// - tokenizer：复用家族默认 spm_or_char()（Qwen tokenizer 在 HF Qwen/ 已开源，tiktoken 系）
    pub fn trae_qwen_3_7_plus() -> Self {
        Self {
            family: ModelFamily::Qwen,
            name: "trae-qwen-3.7-plus".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::spm_or_char(),
            supports_thinking: true,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    // ========================================================================
    // v2.54 P28：OpenCode 内置 5 个型号（Zen 计划免费层，各有上下文限制）
    //
    // OpenCode 作为 Agent 客户端有内置模型清单，Zen 计划是免费层。
    // 各型号有独立的上下文限制（190K/200K/256K/1M），与 Trae 统一 200K 限制不同。
    // 付费 Go 计划应不做限制（用户原话），后续可补充 Go 计划型号。
    //
    // archive_strategy 按 P22 custom 推导规则（window/4）：
    // - 200K → LargeWindow 50K
    // - 190K → Standard 47.5K（<200K 走 Standard）
    // - 1M → LargeWindow 250K
    // - 256K → LargeWindow 64K
    //
    // tokenizer 选择：
    // - DeepSeek：复用 DeepSeekApprox（V4 沿用 GPT-2 BPE）
    // - Hunyuan/Mimo/Nimotron/North：未开源，暂用 CharacterBased 兜底
    // ========================================================================

    /// DeepSeek-V4-Flash（OpenCode Zen 版，v2.54 P28 新增）
    ///
    /// - 上下文：200K token（OpenCode Zen 计划免费层限制，原生 1M——284B 总参数 / 13B 激活 MoE）
    /// - 注意：与 [`ModelVariant::deepseek_v4_flash`]（原生 1M）和 [`ModelVariant::trae_deepseek_v4_flash`]（Trae 200K）不同
    /// - supports_thinking: true（V3.1 起混合模型路线，V4-Flash 延续思考链开关，与 Trae 版一致）
    /// - supports_vision: false（DeepSeek V4 系列未明确多模态，保守）
    /// - tokenizer：复用 DeepSeekApprox
    pub fn opencode_deepseek_v4_flash() -> Self {
        Self {
            family: ModelFamily::DeepSeek,
            name: "opencode-deepseek-v4-flash".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::DeepSeekApprox,
            supports_thinking: true,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// 腾讯混元 Hunyuan-Hy3（OpenCode Zen 版，v2.54 P28 新增）
    ///
    /// - 上下文：190K token（OpenCode Zen 计划免费层限制，原生 256K——295B MoE）
    /// - 架构特点：MoE 推理优化、多模态、思考链
    /// - supports_thinking: true（295B MoE 推理优化版，支持思考链）
    /// - supports_vision: true（混元多模态能力）
    /// - tokenizer：未开源，暂用 CharacterBased 兜底（待后续替换；Hunyuan-Lite 在 HF 已开源）
    /// - archive_strategy：190K 走 Standard（<200K 阈值），window/4 = 47.5K
    pub fn opencode_hy3() -> Self {
        Self {
            family: ModelFamily::Hunyuan,
            name: "opencode-hy3".into(),
            context_window: 190_000,
            tokenizer: TokenizerKind::CharacterBased,
            supports_thinking: true,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::Standard { threshold: 47_500 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// 小米 MiMo-V2.5（OpenCode Zen 版，v2.54 P28 新增）
    ///
    /// - 上下文：200K token（OpenCode Zen 计划免费层限制，原生 200K+——全模态 Agent 模型）
    /// - 架构特点：全模态 Agent、思考链、长程任务优化
    /// - supports_thinking: true（Agent 模型，支持思考链）
    /// - supports_vision: true（全模态能力）
    /// - tokenizer：未开源，暂用 CharacterBased 兜底
    pub fn opencode_mimo_v2_5() -> Self {
        Self {
            family: ModelFamily::Mimo,
            name: "opencode-mimo-v2.5".into(),
            context_window: 200_000,
            tokenizer: TokenizerKind::CharacterBased,
            supports_thinking: true,
            supports_vision: true,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 50_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// NVIDIA Nimotron-3-Ultra（OpenCode Zen 版，v2.54 P28 新增）
    ///
    /// - 上下文：1M token（OpenCode Zen 计划免费层即开放 1M，与多数平台限制不同）
    /// - 架构特点：MoE Hybrid Mamba-Transformer、Agentic Reasoning、超长上下文
    /// - supports_thinking: true（NVIDIA "Agentic Reasoning" 宣传）
    /// - supports_vision: false（保守，未明确多模态宣传）
    /// - tokenizer：未开源，暂用 CharacterBased 兜底
    /// - archive_strategy：1M 走 LargeWindow，window/4 = 250K
    pub fn opencode_nimotron_3_ultra() -> Self {
        Self {
            family: ModelFamily::Nimotron,
            name: "opencode-nimotron-3-ultra".into(),
            context_window: 1_000_000,
            tokenizer: TokenizerKind::CharacterBased,
            supports_thinking: true,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 250_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// Cohere North-Mini-Code（OpenCode Zen 版，v2.54 P28 新增）
    ///
    /// - 上下文：256K token（OpenCode Zen 计划免费层限制，原生 256K——30B MoE / 3B 活跃参数）
    /// - 架构特点：30B MoE 编码模型、Apache 2.0 开源、代码生成优化
    /// - supports_thinking: false（保守，代码模型未明确宣传思考链）
    /// - supports_vision: false（保守，代码模型未明确多模态）
    /// - tokenizer：未开源，暂用 CharacterBased 兜底
    /// - archive_strategy：256K 走 LargeWindow，window/4 = 64K
    pub fn opencode_north_mini_code() -> Self {
        Self {
            family: ModelFamily::North,
            name: "opencode-north-mini-code".into(),
            context_window: 256_000,
            tokenizer: TokenizerKind::CharacterBased,
            supports_thinking: false,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy: ArchiveStrategy::LargeWindow { threshold: 64_000 },
            summary_max_tokens: 1024,
            deprecated: None,
        }
    }

    /// 本地模型（通用预设）
    ///
    /// - 上下文：默认 8K（用户应通过 [`ModelVariant::custom`] 覆盖）
    /// - 架构特点：离线运行、隐私优先
    pub fn local_default() -> Self {
        Self {
            family: ModelFamily::Local,
            name: "local-default".into(),
            context_window: 8_000,
            tokenizer: TokenizerKind::CharacterBased,
            supports_thinking: false,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::None,
            archive_strategy: ArchiveStrategy::SmallWindow { threshold: 4_000 },
            summary_max_tokens: 512,
            deprecated: None,
        }
    }

    /// 自定义模型
    ///
    /// 用户通过此方法配置任意新型号，无需等 MemoryCenter 发版。
    ///
    /// # 参数
    /// - `name`：型号名称
    /// - `family`：模型家族（决定默认 tokenizer）
    /// - `context_window`：上下文窗口大小
    ///
    /// # archive_strategy 推导规则（v2.54 P22 修正）
    ///
    /// 统一比率为 `window / 4`（0.25），消除 200K 边界跳变：
    ///
    /// | 窗口大小 | ArchiveStrategy | 阈值 | 比率 |
    /// |---|---|---|---|
    /// | ≥200K | LargeWindow | window / 4 | 0.25 |
    /// | 32K-200K | Standard | window / 4 | 0.25 |
    /// | <32K | SmallWindow | window / 4 | 0.25 |
    ///
    /// **与内置构造器的关系**：
    /// - 内置构造器使用**专家调校值**（比率 0.20-0.50），充分利用各模型实际能力
    /// - custom 推导取**统一保守值**（0.25），对未知模型安全
    /// - 同窗口下 custom 阈值通常低于或等于内置保守端（如 1M 窗口：custom 250K vs DeepSeek/Llama 200K）
    ///
    /// **200K 边界平滑验证**：
    /// - 199K → Standard 49.75K
    /// - 200K → LargeWindow 50K
    /// - 跳变：+0.25K（平滑，无回退）
    pub fn custom(name: impl Into<String>, family: ModelFamily, context_window: usize) -> Self {
        let tokenizer = family.default_tokenizer();
        // v2.54 P22：统一比率为 window/4（0.25），消除 200K 边界跳变
        // （原规则 ≥200K 用 /5=0.20 导致 199K→49.75K、200K→40K 的 -9.75K 跳变）
        let archive_strategy = if context_window >= 200_000 {
            ArchiveStrategy::LargeWindow { threshold: context_window / 4 }
        } else if context_window >= 32_000 {
            ArchiveStrategy::Standard { threshold: context_window / 4 }
        } else {
            ArchiveStrategy::SmallWindow { threshold: context_window / 4 }
        };

        Self {
            family,
            name: name.into(),
            context_window,
            tokenizer,
            supports_thinking: false,
            supports_vision: false,
            supports_audio: false,
            tool_call_format: ToolCallFormat::OpenAI,
            archive_strategy,
            summary_max_tokens: 1024,
            // v2.54 P25：custom 用户自定义型号，deprecated 默认 None
            deprecated: None,
        }
    }
}

impl Default for ModelVariant {
    fn default() -> Self {
        // 默认用本地模型预设（最保守配置）
        Self::local_default()
    }
}

impl std::fmt::Display for ModelVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}, {}K ctx, thinking={}, vision={})",
            self.name,
            self.family,
            self.context_window / 1000,
            self.supports_thinking,
            self.supports_vision
        )
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_opus_4_6() {
        let v = ModelVariant::claude_opus_4_6();
        assert_eq!(v.family, ModelFamily::Claude);
        assert_eq!(v.name, "claude-opus-4.6");
        assert_eq!(v.context_window, 1_000_000);
        assert!(v.supports_thinking);
        assert!(v.supports_vision);
        assert_eq!(v.tool_call_format, ToolCallFormat::Anthropic);
        match v.archive_strategy {
            ArchiveStrategy::LargeWindow { threshold } => assert_eq!(threshold, 400_000),
            _ => panic!("应为 LargeWindow 策略"),
        }
    }

    #[test]
    fn test_claude_opus_4_8() {
        let v = ModelVariant::claude_opus_4_8();
        assert_eq!(v.family, ModelFamily::Claude);
        assert_eq!(v.name, "claude-opus-4.8");
        assert_eq!(v.context_window, 200_000);
        assert!(v.supports_thinking);
        assert!(v.supports_vision);
        assert_eq!(v.tool_call_format, ToolCallFormat::Anthropic);
        match v.archive_strategy {
            ArchiveStrategy::Standard { threshold } => assert_eq!(threshold, 80_000),
            _ => panic!("200K 窗口应为 Standard"),
        }
    }

    #[test]
    fn test_claude_fable_5() {
        let v = ModelVariant::claude_fable_5();
        assert_eq!(v.family, ModelFamily::Claude);
        assert_eq!(v.name, "claude-fable-5");
        assert_eq!(v.context_window, 200_000);
        assert!(v.supports_thinking, "Fable 5 应支持思考链");
        assert!(v.supports_vision);
        assert_eq!(v.tool_call_format, ToolCallFormat::Anthropic);
    }

    #[test]
    fn test_claude_mythos_5() {
        let v = ModelVariant::claude_mythos_5();
        assert_eq!(v.family, ModelFamily::Claude);
        assert_eq!(v.name, "claude-mythos-5");
        assert_eq!(v.context_window, 200_000);
        assert!(v.supports_thinking, "Mythos 5 应支持思考链");
        // Mythos 5 与 Fable 5 共享底层模型，参数应一致
        let fable = ModelVariant::claude_fable_5();
        assert_eq!(v.context_window, fable.context_window);
        assert_eq!(v.supports_thinking, fable.supports_thinking);
        assert_eq!(v.supports_vision, fable.supports_vision);
    }

    #[test]
    fn test_claude_sonnet_5() {
        let v = ModelVariant::claude_sonnet_5();
        assert_eq!(v.family, ModelFamily::Claude);
        assert_eq!(v.name, "claude-sonnet-5");
        assert_eq!(v.context_window, 200_000);
        assert!(v.supports_thinking, "Sonnet 5 应支持思考链");
        assert!(v.supports_vision);
        assert_eq!(v.tool_call_format, ToolCallFormat::Anthropic);
        match v.archive_strategy {
            ArchiveStrategy::Standard { threshold } => assert_eq!(threshold, 80_000),
            _ => panic!("应为 Standard 策略"),
        }
    }

    #[test]
    fn test_gpt_5_2() {
        let v = ModelVariant::gpt_5_2();
        assert_eq!(v.family, ModelFamily::Gpt);
        assert_eq!(v.context_window, 128_000);
        assert!(!v.supports_thinking);
        assert_eq!(v.tool_call_format, ToolCallFormat::OpenAI);
        match v.archive_strategy {
            ArchiveStrategy::Standard { threshold } => assert_eq!(threshold, 60_000),
            _ => panic!("应为 Standard 策略"),
        }
    }

    #[test]
    fn test_gemini_3_1_pro() {
        let v = ModelVariant::gemini_3_1_pro();
        assert_eq!(v.family, ModelFamily::Gemini);
        assert_eq!(v.name, "gemini-3.1-pro");
        assert_eq!(v.context_window, 1_000_000);
        assert!(v.supports_audio);
        assert!(v.supports_thinking, "3.1 Pro 应支持思考链");
        assert_eq!(v.tool_call_format, ToolCallFormat::Gemini);
    }

    #[test]
    fn test_deepseek_v4_pro_thinking() {
        let v = ModelVariant::deepseek_v4_pro();
        assert_eq!(v.name, "deepseek-v4-pro");
        assert_eq!(v.context_window, 1_000_000);
        assert!(v.supports_thinking, "V4-Pro 应支持思考链");
        assert_eq!(v.tool_call_format, ToolCallFormat::OpenAI);
        match v.archive_strategy {
            ArchiveStrategy::LargeWindow { threshold } => assert_eq!(threshold, 200_000),
            _ => panic!("1M 上下文应为 LargeWindow"),
        }
    }

    #[test]
    fn test_deepseek_v4_flash() {
        let v = ModelVariant::deepseek_v4_flash();
        assert_eq!(v.name, "deepseek-v4-flash");
        assert_eq!(v.context_window, 1_000_000);
        assert!(!v.supports_thinking, "V4-Flash 不支持思考链");
        match v.archive_strategy {
            ArchiveStrategy::LargeWindow { threshold } => assert_eq!(threshold, 200_000),
            _ => panic!("1M 上下文应为 LargeWindow"),
        }
    }

    #[test]
    fn test_qwen_3_coder() {
        let v = ModelVariant::qwen_3_coder();
        assert_eq!(v.family, ModelFamily::Qwen);
        assert_eq!(v.name, "qwen-3-coder");
        assert_eq!(v.context_window, 256_000);
        match v.archive_strategy {
            ArchiveStrategy::Standard { threshold } => assert_eq!(threshold, 100_000),
            _ => panic!("256K 上下文应为 Standard"),
        }
    }

    #[test]
    fn test_llama_4_scout() {
        let v = ModelVariant::llama_4_scout();
        assert_eq!(v.family, ModelFamily::Llama);
        assert_eq!(v.name, "llama-4-scout");
        assert_eq!(v.context_window, 1_000_000);
        assert!(v.supports_vision);
        match v.archive_strategy {
            ArchiveStrategy::LargeWindow { threshold } => assert_eq!(threshold, 200_000),
            _ => panic!("1M 上下文应为 LargeWindow"),
        }
    }

    #[test]
    fn test_llama_4_maverick() {
        let v = ModelVariant::llama_4_maverick();
        assert_eq!(v.name, "llama-4-maverick");
        assert_eq!(v.context_window, 1_000_000);
        assert!(v.supports_vision);
    }

    #[test]
    fn test_custom_model_large_window() {
        // v2.54 P22：统一比率 window/4，500K → 125K（原 /5 = 100K）
        let v = ModelVariant::custom("my-model", ModelFamily::Custom, 500_000);
        match v.archive_strategy {
            ArchiveStrategy::LargeWindow { threshold } => assert_eq!(threshold, 125_000),
            _ => panic!("500K 窗口应为 LargeWindow"),
        }
    }

    #[test]
    fn test_custom_model_standard_window() {
        let v = ModelVariant::custom("my-model", ModelFamily::Custom, 64_000);
        match v.archive_strategy {
            ArchiveStrategy::Standard { threshold } => assert_eq!(threshold, 16_000),
            _ => panic!("64K 窗口应为 Standard"),
        }
    }

    #[test]
    fn test_custom_model_small_window() {
        let v = ModelVariant::custom("my-model", ModelFamily::Custom, 8_000);
        match v.archive_strategy {
            ArchiveStrategy::SmallWindow { threshold } => assert_eq!(threshold, 2_000),
            _ => panic!("8K 窗口应为 SmallWindow"),
        }
    }

    /// v2.54 P22：验证 200K 边界无跳变（原 bug：199K→49.75K、200K→40K）
    #[test]
    fn test_custom_model_200k_boundary_no_jump() {
        // 199K → Standard 49_750
        let v_199 = ModelVariant::custom("m199", ModelFamily::Custom, 199_000);
        match v_199.archive_strategy {
            ArchiveStrategy::Standard { threshold } => assert_eq!(threshold, 49_750),
            _ => panic!("199K 窗口应为 Standard"),
        }
        // 200K → LargeWindow 50_000（原为 40_000，导致 -9_750 跳变）
        let v_200 = ModelVariant::custom("m200", ModelFamily::Custom, 200_000);
        match v_200.archive_strategy {
            ArchiveStrategy::LargeWindow { threshold } => assert_eq!(threshold, 50_000),
            _ => panic!("200K 窗口应为 LargeWindow"),
        }
        // 边界差：50_000 - 49_750 = 250（平滑，无回退）
        let t_199 = v_199.archive_strategy.threshold();
        let t_200 = v_200.archive_strategy.threshold();
        assert!(
            t_200 >= t_199,
            "200K 阈值 {} 不应低于 199K 阈值 {}（P22 修复点）",
            t_200,
            t_199
        );
    }

    /// v2.54 P22：验证 custom 推导与内置构造器保守端的关系
    #[test]
    fn test_custom_vs_builtin_conservative_alignment() {
        // 1M 窗口 custom 阈值 = 250K
        // 内置保守端：DeepSeek V4-Pro / Llama 4 = 200K
        // custom 比内置保守端略激进（250K > 200K），但仍远低于 Claude/Gemini 专家值 400K
        let custom_1m = ModelVariant::custom("custom-1m", ModelFamily::Custom, 1_000_000);
        assert_eq!(custom_1m.archive_strategy.threshold(), 250_000);

        // 200K 窗口 custom 阈值 = 50K
        // 内置保守端：Claude 4.8/5 = 80K（专家调校，充分利用 Claude 能力）
        // custom 比内置保守，符合"未知模型保守"设计
        let custom_200k = ModelVariant::custom("custom-200k", ModelFamily::Custom, 200_000);
        assert_eq!(custom_200k.archive_strategy.threshold(), 50_000);
    }

    #[test]
    fn test_archive_strategy_hard_limit() {
        let s = ArchiveStrategy::LargeWindow { threshold: 400_000 };
        assert_eq!(s.hard_limit(), 600_000);
    }

    #[test]
    fn test_count_tokens() {
        let v = ModelVariant::gpt_5_2();
        let count = v.count_tokens("Hello, world!");
        assert!(count > 0);
    }

    #[test]
    fn test_display() {
        let v = ModelVariant::claude_opus_4_6();
        let s = format!("{}", v);
        assert!(s.contains("claude-opus-4.6"));
        assert!(s.contains("1000K"));
    }

    #[test]
    fn test_all_builtin_variants() {
        // 确保所有内置构造器能正常创建（共 15 个型号）
        let _ = ModelVariant::claude_opus_4_6();
        let _ = ModelVariant::claude_opus_4_8();
        let _ = ModelVariant::claude_sonnet_5();
        let _ = ModelVariant::claude_fable_5();
        let _ = ModelVariant::claude_mythos_5();
        let _ = ModelVariant::gpt_5_2();
        let _ = ModelVariant::gpt_5_codex();
        let _ = ModelVariant::gemini_3_1_pro();
        let _ = ModelVariant::deepseek_v4_pro();
        let _ = ModelVariant::deepseek_v4_flash();
        let _ = ModelVariant::qwen_3_coder();
        let _ = ModelVariant::llama_4_scout();
        let _ = ModelVariant::llama_4_maverick();
        let _ = ModelVariant::grok_4_1();
        let _ = ModelVariant::local_default();
    }

    // ========================================================================
    // v2.54 P28：OpenCode Zen 计划型号构造器测试
    // ========================================================================

    #[test]
    fn test_p28_opencode_constructors() {
        // 确保所有 OpenCode Zen 构造器能正常创建（共 5 个型号）
        let _ = ModelVariant::opencode_deepseek_v4_flash();
        let _ = ModelVariant::opencode_hy3();
        let _ = ModelVariant::opencode_mimo_v2_5();
        let _ = ModelVariant::opencode_nimotron_3_ultra();
        let _ = ModelVariant::opencode_north_mini_code();
    }

    #[test]
    fn test_p28_opencode_deepseek_v4_flash_constructor() {
        let v = ModelVariant::opencode_deepseek_v4_flash();
        assert_eq!(v.family, ModelFamily::DeepSeek);
        assert_eq!(v.name, "opencode-deepseek-v4-flash");
        assert_eq!(v.context_window, 200_000);
        assert!(v.supports_thinking);
        assert!(!v.supports_vision);
        assert!(matches!(v.tokenizer, TokenizerKind::DeepSeekApprox));
    }

    #[test]
    fn test_p28_opencode_hy3_constructor() {
        let v = ModelVariant::opencode_hy3();
        assert_eq!(v.family, ModelFamily::Hunyuan);
        assert_eq!(v.name, "opencode-hy3");
        assert_eq!(v.context_window, 190_000);
        assert!(v.supports_thinking);
        assert!(v.supports_vision);
        assert!(matches!(v.tokenizer, TokenizerKind::CharacterBased));
        // 190K → Standard 47.5K（<200K 走 Standard，按 P22 推导）
        match v.archive_strategy {
            ArchiveStrategy::Standard { threshold } => assert_eq!(threshold, 47_500),
            _ => panic!("P28: 190K 应为 Standard"),
        }
    }

    #[test]
    fn test_p28_opencode_mimo_v2_5_constructor() {
        let v = ModelVariant::opencode_mimo_v2_5();
        assert_eq!(v.family, ModelFamily::Mimo);
        assert_eq!(v.name, "opencode-mimo-v2.5");
        assert_eq!(v.context_window, 200_000);
        assert!(v.supports_thinking);
        assert!(v.supports_vision);
        assert!(matches!(v.tokenizer, TokenizerKind::CharacterBased));
    }

    #[test]
    fn test_p28_opencode_nimotron_3_ultra_constructor() {
        let v = ModelVariant::opencode_nimotron_3_ultra();
        assert_eq!(v.family, ModelFamily::Nimotron);
        assert_eq!(v.name, "opencode-nimotron-3-ultra");
        assert_eq!(v.context_window, 1_000_000);
        assert!(v.supports_thinking);
        assert!(!v.supports_vision);
        assert!(matches!(v.tokenizer, TokenizerKind::CharacterBased));
        // 1M → LargeWindow 250K（按 P22 推导）
        match v.archive_strategy {
            ArchiveStrategy::LargeWindow { threshold } => assert_eq!(threshold, 250_000),
            _ => panic!("P28: 1M 应为 LargeWindow"),
        }
    }

    #[test]
    fn test_p28_opencode_north_mini_code_constructor() {
        let v = ModelVariant::opencode_north_mini_code();
        assert_eq!(v.family, ModelFamily::North);
        assert_eq!(v.name, "opencode-north-mini-code");
        assert_eq!(v.context_window, 256_000);
        assert!(!v.supports_thinking);
        assert!(!v.supports_vision);
        assert!(matches!(v.tokenizer, TokenizerKind::CharacterBased));
        // 256K → LargeWindow 64K（按 P22 推导）
        match v.archive_strategy {
            ArchiveStrategy::LargeWindow { threshold } => assert_eq!(threshold, 64_000),
            _ => panic!("P28: 256K 应为 LargeWindow"),
        }
    }

    #[test]
    fn test_p28_opencode_distinct_from_native_and_trae() {
        // 验证 OpenCode 版 DeepSeek-V4-Flash 与原生版、Trae 版不同
        let native = ModelVariant::deepseek_v4_flash();
        let trae = ModelVariant::trae_deepseek_v4_flash();
        let opencode = ModelVariant::opencode_deepseek_v4_flash();

        // 原生版 1M
        assert_eq!(native.context_window, 1_000_000);
        assert_eq!(native.name, "deepseek-v4-flash");

        // Trae 版 200K
        assert_eq!(trae.context_window, 200_000);
        assert_eq!(trae.name, "trae-deepseek-v4-flash");

        // OpenCode 版 200K（与 Trae 同限制，但属于不同客户端）
        assert_eq!(opencode.context_window, 200_000);
        assert_eq!(opencode.name, "opencode-deepseek-v4-flash");

        // 三者名称互不相同
        assert_ne!(native.name, trae.name);
        assert_ne!(native.name, opencode.name);
        assert_ne!(trae.name, opencode.name);
    }
}

//! # 模型家族 enum
//!
//! 稳定的模型大类，低频迭代（几年一变）。
//! 具体型号（如 Claude Opus 4.6 / GPT-5.2）由 [`crate::variant::ModelVariant`] 表达。

use serde::{Deserialize, Serialize};

/// LLM 模型家族（稳定大类）
///
/// 每个家族对应一类架构相似的模型系列，具体型号由 `ModelVariant` 描述。
/// 家族稳定，型号高频迭代——新型号只需新增 `ModelVariant` 构造器，无需改家族 enum。
///
/// # v2.54 P26 新增 4 个家族
///
/// 为支持 Trae 内置 12 个型号，新增 Doubao / MiniMax / Kimi / Glm 四个家族。
/// 这些家族在 Trae 中统一限制为 200K 上下文。
///
/// # v2.54 P28 新增 4 个家族（OpenCode Zen 计划）
///
/// 为支持 OpenCode 内置 5 个 Zen 计划型号，新增 Hunyuan / Mimo / Nimotron / North 四个家族。
/// Zen 计划是 OpenCode 的免费层，各模型有独立的上下文限制（190K/200K/256K/1M）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelFamily {
    /// Anthropic Claude 系列（Opus/Sonnet/Haiku）
    /// 架构特点：思考链、多模态、超长上下文（200K-2M）
    Claude,

    /// OpenAI GPT 系列（GPT-4/4o/5/5.2/Codex）
    /// 架构特点：function calling、JSON mode、Codex 编程优化
    Gpt,

    /// Google Gemini 系列（Gemini 2.5/3 Pro）
    /// 架构特点：原生多模态、超长上下文（1M+）、sentencepiece 分词
    Gemini,

    /// DeepSeek 系列（V3/V3.2/R1）
    /// 架构特点：R1 思考链、MoE 架构、近似 cl100k 分词
    DeepSeek,

    /// 阿里 Qwen 系列（Qwen 2.5/3）
    /// 架构特点：中文优化、多模态、BPE 分词
    Qwen,

    /// Meta Llama 系列（Llama 3.3/4）
    /// 架构特点：开源、sentencepiece 分词、128K 上下文
    Llama,

    /// xAI Grok 系列（Grok 3/4/4.1）
    /// 架构特点：实时数据接入、128K 上下文
    Grok,

    /// 字节跳动豆包系列（Doubao-Seed-2.1-Pro/Turbo/Code，v2.54 P26 新增）
    /// 架构特点：中文优化、Trae 内置限制 200K 上下文
    /// tokenizer：未开源，暂用 CharacterBased 兜底（待 P24 实测替换）
    Doubao,

    /// MiniMax 系列（MiniMax-M3，v2.54 P26 新增）
    /// 架构特点：MoE 架构、长上下文、Trae 内置限制 200K
    /// tokenizer：未开源，暂用 CharacterBased 兜底
    MiniMax,

    /// 月之暗面 Kimi 系列（K2.6/K2.7-Code，v2.54 P26 新增）
    /// 架构特点：长上下文优化、代码能力、Trae 内置限制 200K
    /// tokenizer：未开源，暂用 CharacterBased 兜底
    Kimi,

    /// 智谱 GLM 系列（GLM-5/5.1/5.2，v2.54 P26 新增）
    /// 架构特点：中英双语优化、思考链、Trae 内置限制 200K
    /// tokenizer：与 Claude 分词接近，暂用 ClaudeApprox 近似（待 P24 实测替换）
    Glm,

    /// 腾讯混元 Hunyuan 系列（Hy3，v2.54 P28 新增）
    /// 架构特点：295B MoE、推理优化、多模态、OpenCode Zen 计划免费层（190K 限制）
    /// tokenizer：未开源，暂用 CharacterBased 兜底
    Hunyuan,

    /// 小米 MiMo 系列（MiMo-V2.5，v2.54 P28 新增）
    /// 架构特点：全模态 Agent 模型、思考链、OpenCode Zen 计划免费层（200K 限制）
    /// tokenizer：未开源，暂用 CharacterBased 兜底
    Mimo,

    /// NVIDIA Nimotron 系列（Nimotron-3-Ultra，v2.54 P28 新增）
    /// 架构特点：MoE Hybrid Mamba-Transformer、Agentic Reasoning、超长上下文（1M）
    /// OpenCode Zen 计划免费层
    /// tokenizer：未开源，暂用 CharacterBased 兜底
    Nimotron,

    /// Cohere North 系列（North-Mini-Code，v2.54 P28 新增）
    /// 架构特点：30B MoE（3B 活跃）、Apache 2.0 开源编码模型、OpenCode Zen 计划免费层（256K 限制）
    /// tokenizer：未开源，暂用 CharacterBased 兜底
    North,

    /// 本地模型（Ollama/vLLM/llama.cpp 部署的开源模型）
    /// 架构特点：离线运行、隐私优先、tokenizer 取决于具体模型
    Local,

    /// 自定义模型（用户通过 ModelVariant::custom 配置）
    Custom,
}

impl ModelFamily {
    /// 返回家族的中文名称
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Claude => "Anthropic Claude",
            Self::Gpt => "OpenAI GPT",
            Self::Gemini => "Google Gemini",
            Self::DeepSeek => "DeepSeek",
            Self::Qwen => "阿里 Qwen",
            Self::Llama => "Meta Llama",
            Self::Grok => "xAI Grok",
            Self::Doubao => "字节跳动豆包",
            Self::MiniMax => "MiniMax",
            Self::Kimi => "月之暗面 Kimi",
            Self::Glm => "智谱 GLM",
            Self::Hunyuan => "腾讯混元 Hunyuan",
            Self::Mimo => "小米 MiMo",
            Self::Nimotron => "NVIDIA Nimotron",
            Self::North => "Cohere North",
            Self::Local => "本地模型",
            Self::Custom => "自定义模型",
        }
    }

    /// 返回家族的默认 tokenizer 类型（用户未指定时使用）
    ///
    /// # v2.53 P9 更新
    ///
    /// Gemini / Qwen / Llama 家族改用 [`TokenizerKind::spm_or_char`]：
    /// - 启用 `tokenizer-sentencepiece` feature → SentencePiece（需配置 `MEMORY_CENTER_SPM_MODEL_PATH`）
    /// - 未启用 feature → CharacterBased（字符级兜底，向后兼容）
    ///
    /// 其他家族保持原有 tiktoken 策略不变。
    ///
    /// # v2.54 P26 新增
    ///
    /// Doubao / MiniMax / Kimi 家族：tokenizer 未开源，暂用 CharacterBased 兜底（待 P24 实测替换）。
    /// Glm 家族：与 Claude 分词接近，暂用 ClaudeApprox 近似（待 P24 实测替换）。
    pub fn default_tokenizer(&self) -> crate::tokenizer::TokenizerKind {
        use crate::tokenizer::TokenizerKind;
        match self {
            Self::Claude => TokenizerKind::ClaudeApprox,    // Claude 官方未开源 tokenizer
            Self::Gpt => TokenizerKind::O200kBase,          // GPT-4o/5 系列
            Self::Gemini => TokenizerKind::spm_or_char(),   // v2.53 P9：SentencePiece（启用 feature 时）
            Self::DeepSeek => TokenizerKind::DeepSeekApprox,
            Self::Qwen => TokenizerKind::spm_or_char(),     // v2.53 P9：SentencePiece（启用 feature 时）
            Self::Llama => TokenizerKind::spm_or_char(),    // v2.53 P9：SentencePiece（启用 feature 时）
            Self::Grok => TokenizerKind::O200kBase,         // Grok 近似 GPT 分词
            // v2.54 P26：新家族 tokenizer 暂用兜底，待 P24 实测后替换
            Self::Doubao => TokenizerKind::CharacterBased,  // 豆包 tokenizer 未开源
            Self::MiniMax => TokenizerKind::CharacterBased, // MiniMax tokenizer 未开源
            Self::Kimi => TokenizerKind::CharacterBased,    // Kimi tokenizer 未开源
            Self::Glm => TokenizerKind::ClaudeApprox,      // GLM 与 Claude 分词接近
            // v2.54 P28：OpenCode Zen 计划新家族 tokenizer 暂用兜底
            Self::Hunyuan => TokenizerKind::CharacterBased,   // 混元 tokenizer 未开源
            Self::Mimo => TokenizerKind::CharacterBased,      // MiMo tokenizer 未开源
            Self::Nimotron => TokenizerKind::CharacterBased,  // Nimotron tokenizer 未开源
            Self::North => TokenizerKind::CharacterBased,     // North tokenizer 未开源
            Self::Local => TokenizerKind::CharacterBased,
            Self::Custom => TokenizerKind::CharacterBased,
        }
    }

    /// 返回所有家族变体（用于遍历）
    pub fn all() -> [Self; 17] {
        [
            Self::Claude,
            Self::Gpt,
            Self::Gemini,
            Self::DeepSeek,
            Self::Qwen,
            Self::Llama,
            Self::Grok,
            Self::Doubao,
            Self::MiniMax,
            Self::Kimi,
            Self::Glm,
            Self::Hunyuan,
            Self::Mimo,
            Self::Nimotron,
            Self::North,
            Self::Local,
            Self::Custom,
        ]
    }
}

impl std::fmt::Display for ModelFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

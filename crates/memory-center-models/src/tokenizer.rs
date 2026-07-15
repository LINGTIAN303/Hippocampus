//! # Tokenizer trait 与 TokenizerKind enum
//!
//! Token 计数抽象层，支持多种分词器实现。
//! 重依赖（tiktoken-rs）隔离在 [`crate::tiktoken_impl`]，未启用时降级为字符级。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Tokenizer 抽象 trait
///
/// 任何能计算文本 token 数的实现都应符合此 trait。
/// 内置实现：
/// - [`crate::tiktoken_impl::TiktokenTokenizer`]：基于 tiktoken-rs（GPT/Claude 近似）
/// - [`crate::char_impl::CharTokenizer`]：字符级兜底（无依赖，中文 1 字 ≈ 1.5 token）
pub trait Tokenizer: Send + Sync {
    /// 计算文本的 token 数
    fn count_tokens(&self, text: &str) -> usize;

    /// 返回 tokenizer 名称（用于调试/日志）
    fn name(&self) -> &str;
}

/// Tokenizer 类型枚举（用于配置选择）
///
/// 对照 2026 年主流模型的分词器：
/// - `O200kBase`：GPT-4o/4-turbo/5/5.2 系列
/// - `Cl100kBase`：GPT-4/3.5 系列（向后兼容）
/// - `ClaudeApprox`：Claude 系列（官方未开源，用 cl100k + 系数 1.05 近似）
/// - `DeepSeekApprox`：DeepSeek 系列（近似 cl100k，系数 1.1）
/// - `CharacterBased`：字符级兜底（中文 1 字 ≈ 1.5 token，英文按词）
/// - `SentencePiece`：SentencePiece 分词器（v2.53 P9 新增，需启用 feature，用于 Gemini/Qwen/Llama）
///
/// # 序列化说明
///
/// 序列化时只存储类型名称（如 `"o200k_base"`），不序列化 `Custom` 变体内部的 trait 对象。
/// 反序列化 `Custom` 时回退为 `CharacterBased`（因为 trait 对象无法反序列化）。
#[derive(Clone)]
pub enum TokenizerKind {
    /// GPT-4o/5 系列分词器（o200k_base）
    O200kBase,

    /// GPT-4/3.5 系列分词器（cl100k_base）
    Cl100kBase,

    /// Claude 近似分词器（cl100k + 系数 1.05）
    ClaudeApprox,

    /// DeepSeek 近似分词器（cl100k + 系数 1.1）
    DeepSeekApprox,

    /// 字符级分词器（无依赖兜底，中文 1 字 ≈ 1.5 token）
    CharacterBased,

    /// SentencePiece 分词器（v2.53 P9 新增）
    ///
    /// 仅在启用 `tokenizer-sentencepiece` feature 时可用。
    /// 需通过环境变量 `MEMORY_CENTER_SPM_MODEL_PATH` 指定 `.model` 文件路径。
    /// 未启用 feature 时此变体不存在，应使用 [`TokenizerKind::spm_or_char`] 自动降级。
    #[cfg(feature = "tokenizer-sentencepiece")]
    SentencePiece,

    /// 自定义分词器（用户注入实现，不可序列化）
    Custom(Arc<dyn Tokenizer>),
}

impl TokenizerKind {
    /// 构建对应的 Tokenizer 实例
    ///
    /// - `O200kBase` / `Cl100kBase` / `ClaudeApprox` / `DeepSeekApprox` → TiktokenTokenizer（失败降级 CharTokenizer）
    /// - `CharacterBased` → CharTokenizer
    /// - `SentencePiece`（启用 feature 时）→ SentencePieceTokenizer（失败降级 CharTokenizer）
    /// - `Custom` → 返回内部 Arc 的克隆
    pub fn build(&self) -> Arc<dyn Tokenizer> {
        match self {
            Self::O200kBase => match crate::tiktoken_impl::TiktokenTokenizer::o200k_base() {
                Ok(tk) => Arc::new(tk),
                Err(e) => {
                    tracing::warn!("o200k_base 初始化失败: {}，降级为 CharTokenizer", e);
                    Arc::new(crate::char_impl::CharTokenizer::new())
                }
            },
            Self::Cl100kBase => match crate::tiktoken_impl::TiktokenTokenizer::cl100k_base() {
                Ok(tk) => Arc::new(tk),
                Err(e) => {
                    tracing::warn!("cl100k_base 初始化失败: {}，降级为 CharTokenizer", e);
                    Arc::new(crate::char_impl::CharTokenizer::new())
                }
            },
            Self::ClaudeApprox => match crate::tiktoken_impl::TiktokenTokenizer::claude_approx() {
                Ok(tk) => Arc::new(tk),
                Err(e) => {
                    tracing::warn!("claude_approx 初始化失败: {}，降级为 CharTokenizer", e);
                    Arc::new(crate::char_impl::CharTokenizer::new())
                }
            },
            Self::DeepSeekApprox => match crate::tiktoken_impl::TiktokenTokenizer::deepseek_approx() {
                Ok(tk) => Arc::new(tk),
                Err(e) => {
                    tracing::warn!("deepseek_approx 初始化失败: {}，降级为 CharTokenizer", e);
                    Arc::new(crate::char_impl::CharTokenizer::new())
                }
            },
            Self::CharacterBased => Arc::new(crate::char_impl::CharTokenizer::new()),
            #[cfg(feature = "tokenizer-sentencepiece")]
            Self::SentencePiece => match crate::sentencepiece_impl::SentencePieceTokenizer::from_env() {
                Ok(sp) => Arc::new(sp),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "SentencePiece 初始化失败，降级为 CharTokenizer（检查 MEMORY_CENTER_SPM_MODEL_PATH 环境变量）"
                    );
                    Arc::new(crate::char_impl::CharTokenizer::new())
                }
            },
            Self::Custom(t) => t.clone(),
        }
    }

    /// 返回类型名称（用于日志/调试/序列化）
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::O200kBase => "o200k_base",
            Self::Cl100kBase => "cl100k_base",
            Self::ClaudeApprox => "claude_approx",
            Self::DeepSeekApprox => "deepseek_approx",
            Self::CharacterBased => "character_based",
            #[cfg(feature = "tokenizer-sentencepiece")]
            Self::SentencePiece => "sentencepiece",
            Self::Custom(_) => "custom",
        }
    }

    /// 从类型名称构建 TokenizerKind（反序列化用）
    ///
    /// `custom` 回退为 `CharacterBased`（因为 trait 对象无法反序列化）
    /// `sentencepiece` 在未启用 feature 时回退为 `CharacterBased`
    pub fn from_type_name(name: &str) -> Self {
        match name {
            "o200k_base" => Self::O200kBase,
            "cl100k_base" => Self::Cl100kBase,
            "claude_approx" => Self::ClaudeApprox,
            "deepseek_approx" => Self::DeepSeekApprox,
            #[cfg(feature = "tokenizer-sentencepiece")]
            "sentencepiece" => Self::SentencePiece,
            #[cfg(not(feature = "tokenizer-sentencepiece"))]
            "sentencepiece" => {
                tracing::warn!(
                    "反序列化 'sentencepiece' 但未启用 tokenizer-sentencepiece feature，降级为 CharacterBased"
                );
                Self::CharacterBased
            }
            "character_based" | "custom" | _ => Self::CharacterBased,
        }
    }

    /// 智能选择 SentencePiece 或 CharTokenizer（v2.53 P9 新增）
    ///
    /// 用于 Gemini / Qwen / Llama 等家族的 `default_tokenizer()`：
    ///
    /// - 启用 `tokenizer-sentencepiece` feature → 返回 [`TokenizerKind::SentencePiece`]
    ///   （运行时若未配置 `MEMORY_CENTER_SPM_MODEL_PATH` 会自动降级为 CharTokenizer，见 [`Self::build`]）
    /// - 未启用 feature → 返回 [`TokenizerKind::CharacterBased`]
    ///
    /// 这样编译时即决定是否依赖 sentencepiece，避免未启用 feature 时编译错误。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use memory_center_models::tokenizer::TokenizerKind;
    ///
    /// // 在 family.rs::default_tokenizer() 中
    /// let kind = TokenizerKind::spm_or_char();
    /// ```
    pub fn spm_or_char() -> Self {
        #[cfg(feature = "tokenizer-sentencepiece")]
        {
            Self::SentencePiece
        }
        #[cfg(not(feature = "tokenizer-sentencepiece"))]
        {
            Self::CharacterBased
        }
    }
}

impl Default for TokenizerKind {
    fn default() -> Self {
        // 默认用字符级（无依赖，向后兼容）
        Self::CharacterBased
    }
}

/// 手动实现 Debug（因为 `Custom(Arc<dyn Tokenizer>)` 无法自动 derive）
impl std::fmt::Debug for TokenizerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::O200kBase => write!(f, "TokenizerKind(O200kBase)"),
            Self::Cl100kBase => write!(f, "TokenizerKind(Cl100kBase)"),
            Self::ClaudeApprox => write!(f, "TokenizerKind(ClaudeApprox)"),
            Self::DeepSeekApprox => write!(f, "TokenizerKind(DeepSeekApprox)"),
            Self::CharacterBased => write!(f, "TokenizerKind(CharacterBased)"),
            #[cfg(feature = "tokenizer-sentencepiece")]
            Self::SentencePiece => write!(f, "TokenizerKind(SentencePiece)"),
            Self::Custom(_) => write!(f, "TokenizerKind(Custom(<tokenizer>))"),
        }
    }
}

/// 手动实现 Serialize（只存储类型名称）
impl Serialize for TokenizerKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.type_name())
    }
}

/// 手动实现 Deserialize（从类型名称重建，custom 回退为 character_based）
impl<'de> Deserialize<'de> for TokenizerKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let name = String::deserialize(deserializer)?;
        Ok(Self::from_type_name(&name))
    }
}

impl std::fmt::Display for TokenizerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.type_name())
    }
}

// ============================================================================
// Token 估算器便利构造（v2.52 阶段 4 新增）
// ============================================================================

/// 从 Tokenizer 构建 Token 估算器闭包
///
/// 返回的闭包类型 `Arc<dyn Fn(&str) -> usize + Send + Sync>` 可直接注入
/// `memory-center-archive-core::ArchiveEngine::with_token_estimator`，
/// 用于替换 archive-core 的 `chars/3` 简化估算（仅对 Agent 未传 token_count 的轮次生效）。
///
/// ## 设计动机
///
/// archive-core 不依赖 models crate（星型拓扑约束），因此无法直接调用
/// `ModelVariant::count_tokens`。通过将 tokenizer 封装为闭包，让调用方
/// （server/mcp 等已依赖 models 的 crate）负责构造并注入，解耦依赖。
///
/// ## 使用示例
///
/// ```ignore
/// use memory_center_models::{ModelVariant, ModelRegistry, build_token_estimator};
/// use memory_center_archive_core::ArchiveEngine;
/// use std::env;
///
/// // 默认用 deepseek-v4-flash（DeepSeekApprox tokenizer，中文场景 token 估算更贴近实际）
/// // 可通过环境变量 MEMORY_CENTER_TOKENIZER_MODEL 覆盖
/// let model = env::var("MEMORY_CENTER_TOKENIZER_MODEL")
///     .ok()
///     .and_then(|n| ModelRegistry::find(&n))
///     .unwrap_or_else(ModelVariant::deepseek_v4_flash);
/// let tokenizer = model.build_tokenizer();
/// let estimator = build_token_estimator(tokenizer);
/// let engine = ArchiveEngine::new(storage_root).with_token_estimator(estimator);
/// ```
pub fn build_token_estimator(
    tokenizer: Arc<dyn Tokenizer>,
) -> Arc<dyn Fn(&str) -> usize + Send + Sync> {
    Arc::new(move |text: &str| tokenizer.count_tokens(text))
}

// ============================================================================
// 单元测试（v2.53 P9 新增：SentencePiece 变体与 spm_or_char）
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spm_or_char_returns_correct_kind_based_on_feature() {
        let kind = TokenizerKind::spm_or_char();
        #[cfg(feature = "tokenizer-sentencepiece")]
        {
            assert!(matches!(kind, TokenizerKind::SentencePiece),
                "启用 feature 时应返回 SentencePiece，实际: {:?}", kind);
        }
        #[cfg(not(feature = "tokenizer-sentencepiece"))]
        {
            assert!(matches!(kind, TokenizerKind::CharacterBased),
                "未启用 feature 时应返回 CharacterBased，实际: {:?}", kind);
        }
    }

    #[test]
    fn test_sentencepiece_type_name_roundtrip() {
        // 序列化往返测试：from_type_name(type_name()) 应返回等价变体
        #[cfg(feature = "tokenizer-sentencepiece")]
        {
            let kind = TokenizerKind::SentencePiece;
            let name = kind.type_name();
            assert_eq!(name, "sentencepiece");
            let restored = TokenizerKind::from_type_name(name);
            assert!(matches!(restored, TokenizerKind::SentencePiece),
                "序列化往返应保持 SentencePiece，实际: {:?}", restored);
        }
        #[cfg(not(feature = "tokenizer-sentencepiece"))]
        {
            // 未启用 feature 时，"sentencepiece" 应降级为 CharacterBased
            let restored = TokenizerKind::from_type_name("sentencepiece");
            assert!(matches!(restored, TokenizerKind::CharacterBased),
                "未启用 feature 时 'sentencepiece' 应降级为 CharacterBased，实际: {:?}", restored);
        }
    }

    #[test]
    fn test_sentencepiece_build_does_not_panic_without_model() {
        // 未配置环境变量时，build() 应降级为 CharTokenizer 而不 panic
        // 此测试保证 archive 主链路稳定
        #[cfg(feature = "tokenizer-sentencepiece")]
        {
            // 保存原值
            let original = std::env::var(crate::sentencepiece_impl::ENV_SPM_MODEL_PATH).ok();
            std::env::remove_var(crate::sentencepiece_impl::ENV_SPM_MODEL_PATH);

            let kind = TokenizerKind::SentencePiece;
            let tokenizer = kind.build();
            // 应返回 CharTokenizer（name 为 "character_based"）
            assert_eq!(tokenizer.name(), "character_based",
                "未配置 SPM 模型路径时应降级为 CharTokenizer");

            // 恢复原值
            if let Some(val) = original {
                std::env::set_var(crate::sentencepiece_impl::ENV_SPM_MODEL_PATH, val);
            }
        }
        #[cfg(not(feature = "tokenizer-sentencepiece"))]
        {
            // 未启用 feature 时此测试无意义（变体不存在）
        }
    }

    #[test]
    fn test_debug_format_includes_sentencepiece() {
        #[cfg(feature = "tokenizer-sentencepiece")]
        {
            let kind = TokenizerKind::SentencePiece;
            let s = format!("{:?}", kind);
            assert!(s.contains("SentencePiece"), "Debug 输出应含 SentencePiece，实际: {}", s);
        }
        // 未启用 feature 时无需测试（变体不存在）
    }

    #[test]
    fn test_all_variants_type_name_consistency() {
        // 验证所有可构造变体的 type_name/from_type_name 一致性
        let variants = vec![
            TokenizerKind::O200kBase,
            TokenizerKind::Cl100kBase,
            TokenizerKind::ClaudeApprox,
            TokenizerKind::DeepSeekApprox,
            TokenizerKind::CharacterBased,
        ];
        for v in variants {
            let name = v.type_name();
            let restored = TokenizerKind::from_type_name(name);
            // 不直接比较（Custom 无法往返），仅验证 type_name 一致
            assert_eq!(restored.type_name(), name,
                "往返后 type_name 应一致: {} vs {}", restored.type_name(), name);
        }
    }
}

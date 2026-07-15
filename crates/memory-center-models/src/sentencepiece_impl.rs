//! # SentencePiece 分词器实现（v2.53 P9 新增）
//!
//! 基于 `sentencepiece` crate（C++ 绑定版 0.13.1 + static feature）。
//! 用于 Gemini / Qwen / Llama 等非 OpenAI/Claude 家族模型的精确 token 计数。
//!
//! ## 设计动机
//!
//! 之前的实现中，Gemini/Qwen/Llama 家族默认用 [`crate::char_impl::CharTokenizer`]
//! （字符级近似，CJK × 1.5 / 拉丁 × 1.3 / 标点 × 0.5），中文场景估算偏差可达 20-30%。
//! 启用 SentencePiece 后可用真实模型分词器替代字符级近似，提升归档触发时机判断精度。
//!
//! ## Feature 依赖
//!
//! 此模块仅在启用 `tokenizer-sentencepiece` feature 时编译：
//!
//! ```toml
//! # Cargo.toml（消费方）
//! memory-center-models = { path = "...", features = ["tokenizer-sentencepiece"] }
//! ```
//!
//! 未启用时 [`crate::tokenizer::TokenizerKind::spm_or_char`] 返回 [`crate::char_impl::CharTokenizer`]，
//! 保持向后兼容。
//!
//! ## 模型文件（.model）
//!
//! SentencePiece 需要预训练的 `.model` 文件（protobuf 序列化），不随二进制打包。
//! 用户通过环境变量指定路径：
//!
//! ```bash
//! export MEMORY_CENTER_SPM_MODEL_PATH=/path/to/spiece.model
//! ```
//!
//! 推荐模型：
//! - **mT5-base** (`spiece.model`, ~2.4MB)：多语言 101 种，中文友好
//! - **NLLB-200** (`sentencepiece.bpe.model`, ~2.5MB)：200+ 语言
//! - **T5-base** (`spiece.model`, ~500KB)：纯英文场景
//!
//! 下载：`huggingface-cli download google/mt5-base spiece.model`
//!
//! ## 降级链
//!
//! 1. 启用 feature 且 `MEMORY_CENTER_SPM_MODEL_PATH` 已设置 + 文件可加载 → SentencePiece
//! 2. 启用 feature 但环境变量未设置或加载失败 → CharTokenizer（带 warn 日志）
//! 3. 未启用 feature → CharTokenizer（编译时即降级）
//!
//! ## 与 tiktoken-rs 共存
//!
//! SentencePiece 与 tiktoken 互不干扰：
//! - OpenAI/Claude/DeepSeek 家族继续用 tiktoken（O200kBase / Cl100kBase / ClaudeApprox / DeepSeekApprox）
//! - Gemini/Qwen/Llama 家族用 SentencePiece（feature 启用时）或 CharTokenizer（未启用时）
//!
//! 详见 `docs/sentencepiece-guide.md` 完整使用教程。

use crate::tokenizer::Tokenizer;

/// 环境变量名：SentencePiece 模型文件路径
pub const ENV_SPM_MODEL_PATH: &str = "MEMORY_CENTER_SPM_MODEL_PATH";

/// SentencePiece 分词器
///
/// 内部持有 `sentencepiece::SentencePieceProcessor` 实例，
/// 通过 `.model` 文件加载预训练模型实现精确 token 计数。
///
/// # 构造方式
///
/// - [`SentencePieceTokenizer::from_file`]：从指定路径加载
/// - [`SentencePieceTokenizer::from_env`]：从环境变量 `MEMORY_CENTER_SPM_MODEL_PATH` 加载
///
/// # Debug 输出
///
/// 派生 `Debug`：内部 `SentencePieceProcessor` 已实现 `Debug`（来自 sentencepiece crate），
/// 调试输出包含模型元信息（词表大小、特殊 token id 等），不包含完整词表内容。
#[derive(Debug)]
pub struct SentencePieceTokenizer {
    /// 内部分词器实例
    sp: sentencepiece::SentencePieceProcessor,
    /// 类型名称（用于日志/调试）
    kind_name: &'static str,
}

impl SentencePieceTokenizer {
    /// 从 `.model` 文件路径加载分词器
    ///
    /// # 参数
    /// - `path`：`.model` 文件路径（protobuf 序列化的 SentencePiece 模型）
    ///
    /// # 错误
    /// - 文件不存在或格式错误时返回 Err（错误消息含路径和底层错误）
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use memory_center_models::SentencePieceTokenizer;
    ///
    /// let sp = SentencePieceTokenizer::from_file("/path/to/spiece.model")?;
    /// let count = sp.count_tokens("你好世界");
    /// ```
    pub fn from_file(path: &str) -> Result<Self, String> {
        let sp = sentencepiece::SentencePieceProcessor::open(path)
            .map_err(|e| format!("SentencePiece 加载失败 (path={}): {}", path, e))?;
        Ok(Self {
            sp,
            kind_name: "sentencepiece",
        })
    }

    /// 从环境变量 `MEMORY_CENTER_SPM_MODEL_PATH` 加载分词器
    ///
    /// # 环境变量
    /// - `MEMORY_CENTER_SPM_MODEL_PATH`：`.model` 文件绝对路径
    ///
    /// # 错误
    /// - 环境变量未设置 → Err("MEMORY_CENTER_SPM_MODEL_PATH 环境变量未设置")
    /// - 文件加载失败 → Err（含路径和底层错误）
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use memory_center_models::SentencePieceTokenizer;
    ///
    /// // 前提：export MEMORY_CENTER_SPM_MODEL_PATH=/path/to/spiece.model
    /// let sp = SentencePieceTokenizer::from_env()?;
    /// ```
    pub fn from_env() -> Result<Self, String> {
        let path = std::env::var(ENV_SPM_MODEL_PATH).map_err(|_| {
            format!(
                "环境变量 {} 未设置，无法加载 SentencePiece 模型",
                ENV_SPM_MODEL_PATH
            )
        })?;
        Self::from_file(&path)
    }
}

impl Tokenizer for SentencePieceTokenizer {
    fn count_tokens(&self, text: &str) -> usize {
        // sentencepiece 0.13.1: encode() 返回 Result<Vec<PieceWithId>, SentencePieceError>
        // 每个 PieceWithId 对应一个 token，长度即为 token 数
        // 失败时降级为字符级近似（避免归档中断）
        match self.sp.encode(text) {
            Ok(pieces) => pieces.len(),
            Err(e) => {
                // 编码失败（如含非法 UTF-8 或控制字符）时降级为字符数估算
                // 不 panic，保证 archive 主链路稳定
                tracing::warn!(
                    error = %e,
                    text_len = text.chars().count(),
                    "SentencePiece 编码失败，降级为字符级估算 (chars × 1.5)"
                );
                ((text.chars().count() as f32) * 1.5).round() as usize
            }
        }
    }

    fn name(&self) -> &str {
        self.kind_name
    }
}

// ============================================================================
// 单元测试
// ============================================================================
//
// 注意：以下测试需要 .model 文件才能完整运行。CI 默认禁用 feature 时不编译此模块。
// 启用 feature 后，若未设置 MEMORY_CENTER_SPM_MODEL_PATH，from_env 测试会跳过。
//
// 测试覆盖策略：
// - from_file 不存在的路径应返回 Err
// - from_env 环境变量未设置应返回 Err
// - 加载成功后的 count_tokens 行为（需 .model 文件，用 `#[ignore]` 标记需手动运行）

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_file_nonexistent_returns_err() {
        let result = SentencePieceTokenizer::from_file("/nonexistent/path/to/model.model");
        assert!(result.is_err(), "不存在路径应返回 Err");
        let err = result.unwrap_err();
        assert!(
            err.contains("SentencePiece 加载失败"),
            "错误消息应含加载失败字样，实际: {}",
            err
        );
        assert!(
            err.contains("/nonexistent/path/to/model.model"),
            "错误消息应含路径，实际: {}",
            err
        );
    }

    #[test]
    fn test_from_env_unset_returns_err() {
        // 保存原值，测试结束后恢复（避免污染其他测试）
        let original = std::env::var(ENV_SPM_MODEL_PATH).ok();
        std::env::remove_var(ENV_SPM_MODEL_PATH);

        let result = SentencePieceTokenizer::from_env();
        assert!(result.is_err(), "环境变量未设置应返回 Err");
        let err = result.unwrap_err();
        assert!(
            err.contains(ENV_SPM_MODEL_PATH),
            "错误消息应含环境变量名，实际: {}",
            err
        );

        // 恢复原值
        if let Some(val) = original {
            std::env::set_var(ENV_SPM_MODEL_PATH, val);
        }
    }

    #[test]
    fn test_env_constant_value() {
        assert_eq!(ENV_SPM_MODEL_PATH, "MEMORY_CENTER_SPM_MODEL_PATH");
    }

    /// 加载真实 .model 文件并验证 count_tokens 行为
    ///
    /// 运行方式（需手动准备 .model 文件）：
    /// ```bash
    /// # 1. 下载 mT5-base 的 spiece.model
    /// huggingface-cli download google/mt5-base spiece.model \
    ///   --local-dir ./tests/fixtures
    ///
    /// # 2. 设置环境变量并运行测试
    /// MEMORY_CENTER_SPM_MODEL_PATH=./tests/fixtures/spiece.model \
    ///   cargo test -p memory-center-models --features tokenizer-sentencepiece \
    ///   test_from_env_loads_real_model -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "需 .model 文件，手动运行（见函数注释）"]
    fn test_from_env_loads_real_model() {
        let sp = SentencePieceTokenizer::from_env()
            .expect("应成功加载（前提：环境变量已设置且 .model 文件有效）");

        // 英文短文本应能计数
        let en_count = sp.count_tokens("Hello, world!");
        assert!(en_count > 0, "英文短文本 token 数应 > 0");

        // 中文短文本应能计数
        let zh_count = sp.count_tokens("你好世界");
        assert!(zh_count > 0, "中文短文本 token 数应 > 0");

        // 空字符串应返回 0
        let empty_count = sp.count_tokens("");
        assert_eq!(empty_count, 0, "空字符串 token 数应为 0");

        // name() 应返回 "sentencepiece"
        assert_eq!(sp.name(), "sentencepiece");
    }

    /// 验证 SentencePiece 与 CharTokenizer 在中文场景的差异
    ///
    /// 启用 feature 后，SentencePiece 通常比 CharTokenizer 更精确
    /// （CharTokenizer CJK × 1.5 是固定系数，SentencePiece 用真实模型分词）
    #[test]
    #[ignore = "需 .model 文件，手动运行"]
    fn test_sentencepiece_vs_char_tokenizer_chinese() {
        let sp = SentencePieceTokenizer::from_env().expect("应成功加载");
        let char_tk = crate::char_impl::CharTokenizer::new();

        let text = "这是一段用于测试的中文文本，包含足够长度以观察分词差异。";
        let sp_count = sp.count_tokens(text);
        let char_count = char_tk.count_tokens(text);

        // 两者都应 > 0
        assert!(sp_count > 0, "SentencePiece 计数应 > 0");
        assert!(char_count > 0, "CharTokenizer 计数应 > 0");

        // 打印对比（需 --nocapture 查看）
        println!(
            "text='{}'\n  SentencePiece: {} tokens\n  CharTokenizer: {} tokens (CJK × 1.5)",
            text, sp_count, char_count
        );
    }
}

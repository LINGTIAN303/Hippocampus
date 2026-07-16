//! # Tiktoken 分词器实现
//!
//! 基于 tiktoken-rs 的 BPE 分词器，支持 OpenAI 系列模型与 Claude/DeepSeek 近似。
//!
//! ## 支持的 encoding
//!
//! - `o200k_base`：GPT-4o/4-turbo/5/5.2 系列
//! - `cl100k_base`：GPT-4/3.5 系列（向后兼容）
//! - `ClaudeApprox`：cl100k_base + 系数 1.20（v2.54 P24 校准，Claude 官方未开源）
//! - `DeepSeekApprox`：cl100k_base + 系数 0.95（v2.54 P24 实测校准，DeepSeek V4 对中文优化）
//!
//! ## v2.54 P24 校准说明
//!
//! - **Claude 系数 1.20**（暂定，基于公开资料推理）：
//!   - Sonnet 5 / Opus 4.7+ 共享新 tokenizer，官方系统卡承认涨幅 1.0~1.35 倍（相对旧 tokenizer）
//!   - 中位约 1.17 倍，叠加旧 Claude tokenizer ≈ cl100k × 1.05 的经验值
//!   - 推理值 ≈ 1.05 × 1.17 ≈ 1.23，取保守暂定 1.20
//!   - 待 OpenRouter 充值后实测校准
//! - **DeepSeek 系数 0.95**（实测校准，2026-07-16）：
//!   - 100 条中英代码混合样本实测，V4 Pro 和 V4 Flash 共享同一 tokenizer
//!   - 整体平均系数 1.0115（被短样本 API 开销拉高）
//!   - 长文本（>100 tokens）平均系数约 0.94（更贴近归档场景）
//!   - DeepSeek V4 对中文优化明显（中文 token 比 cl100k 少 25-40%），英文/代码略多（+5-15%）
//!   - 取 0.95 作为整体折中值
//!
//! ## 降级策略
//!
//! tiktoken-rs 初始化失败（如缺少词表文件）时返回 Err，
//! 由 [`crate::tokenizer::TokenizerKind::build`] 降级为 [`crate::char_impl::CharTokenizer`]。

use crate::tokenizer::Tokenizer;

/// Tiktoken 分词器
///
/// 内部持有 tiktoken-rs 的 Core BPE 实例，支持多种 encoding。
pub struct TiktokenTokenizer {
    /// BPE 编码器
    bpe: tiktoken_rs::CoreBPE,
    /// 类型名称
    kind_name: &'static str,
    /// 系数（用于 Claude/DeepSeek 近似，1.0 表示无调整）
    coefficient: f32,
}

impl TiktokenTokenizer {
    /// 创建 o200k_base 分词器（GPT-4o/5 系列）
    pub fn o200k_base() -> Result<Self, String> {
        let bpe = tiktoken_rs::o200k_base().map_err(|e| format!("o200k_base 初始化失败: {}", e))?;
        Ok(Self {
            bpe,
            kind_name: "o200k_base",
            coefficient: 1.0,
        })
    }

    /// 创建 cl100k_base 分词器（GPT-4/3.5 系列）
    pub fn cl100k_base() -> Result<Self, String> {
        let bpe = tiktoken_rs::cl100k_base().map_err(|e| format!("cl100k_base 初始化失败: {}", e))?;
        Ok(Self {
            bpe,
            kind_name: "cl100k_base",
            coefficient: 1.0,
        })
    }

    /// 创建 Claude 近似分词器（cl100k_base + 系数 1.20）
    ///
    /// Claude 官方未开源 tokenizer。v2.54 P24 基于公开资料推理：
    /// Sonnet 5 / Opus 4.7+ 共享新 tokenizer，官方系统卡承认涨幅 1.0~1.35 倍
    /// （相对旧 tokenizer），中位约 1.17 倍，叠加旧系数 1.05，取保守暂定 1.20。
    /// 待 OpenRouter 充值后实测校准。
    pub fn claude_approx() -> Result<Self, String> {
        let bpe = tiktoken_rs::cl100k_base().map_err(|e| format!("cl100k_base 初始化失败: {}", e))?;
        Ok(Self {
            bpe,
            kind_name: "claude_approx",
            coefficient: 1.20,
        })
    }

    /// 创建 DeepSeek 近似分词器（cl100k_base + 系数 0.95）
    ///
    /// v2.54 P24 实测校准（2026-07-16）：100 条中英代码混合样本通过 DeepSeek 官方 API
    /// 实测 prompt_tokens，V4 Pro 和 V4 Flash 共享同一 tokenizer。整体平均系数 1.0115
    /// （被短样本 API 开销拉高），长文本平均约 0.94，取 0.95 作为折中值。
    /// DeepSeek V4 对中文优化明显（中文 token 比 cl100k 少 25-40%）。
    pub fn deepseek_approx() -> Result<Self, String> {
        let bpe = tiktoken_rs::cl100k_base().map_err(|e| format!("cl100k_base 初始化失败: {}", e))?;
        Ok(Self {
            bpe,
            kind_name: "deepseek_approx",
            coefficient: 0.95,
        })
    }
}

impl Tokenizer for TiktokenTokenizer {
    fn count_tokens(&self, text: &str) -> usize {
        // tiktoken-rs 0.6: encode_with_special_tokens 返回 Vec<usize>（token id 列表）
        let tokens = self.bpe.encode_with_special_tokens(text);
        let raw_count = tokens.len();

        // 应用系数（Claude/DeepSeek 近似）
        // 系数 != 1.0 时应用调整（Claude ×1.20 上调，DeepSeek ×0.95 下调）
        if self.coefficient != 1.0 {
            ((raw_count as f32) * self.coefficient).round() as usize
        } else {
            raw_count
        }
    }

    fn name(&self) -> &str {
        self.kind_name
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_o200k_base_english() {
        let tk = TiktokenTokenizer::o200k_base().expect("o200k_base 应可用");
        let count = tk.count_tokens("Hello, world!");
        assert!(count > 0, "英文应能正确计数");
        assert!(count < 10, "短英文 token 数应 < 10");
    }

    #[test]
    fn test_o200k_base_chinese() {
        let tk = TiktokenTokenizer::o200k_base().expect("o200k_base 应可用");
        let count = tk.count_tokens("你好，世界！");
        assert!(count > 0, "中文应能正确计数");
    }

    #[test]
    fn test_cl100k_base_english() {
        let tk = TiktokenTokenizer::cl100k_base().expect("cl100k_base 应可用");
        let count = tk.count_tokens("Hello, world!");
        assert!(count > 0);
    }

    #[test]
    fn test_claude_approx_higher_than_cl100k() {
        // v2.54 P24：Claude 系数 1.20（暂定，基于公开推理），token 数应高于 cl100k 原始计数
        let cl100k = TiktokenTokenizer::cl100k_base().expect("cl100k_base 应可用");
        let claude = TiktokenTokenizer::claude_approx().expect("claude_approx 应可用");

        let text = "这是一段测试文本，用于验证 Claude 近似分词器的系数是否生效。Hello world.";
        let raw = cl100k.count_tokens(text);
        let approx = claude.count_tokens(text);

        // 系数 1.20，近似值应 > 原始值（涨幅约 20%）
        assert!(
            approx > raw,
            "Claude 近似 ({}) 应 > cl100k 原始 ({})（系数 1.20）",
            approx,
            raw
        );
    }

    #[test]
    fn test_deepseek_approx_lower_than_cl100k() {
        // v2.54 P24：DeepSeek 系数 0.95（实测校准），token 数应略低于 cl100k 原始计数
        // DeepSeek V4 对中文优化，token 数比 cl100k 少约 5%
        let cl100k = TiktokenTokenizer::cl100k_base().expect("cl100k_base 应可用");
        let deepseek = TiktokenTokenizer::deepseek_approx().expect("deepseek_approx 应可用");

        let text = "这是一段测试文本，用于验证 DeepSeek 近似分词器的系数是否生效。Hello world.";
        let raw = cl100k.count_tokens(text);
        let approx = deepseek.count_tokens(text);

        // 系数 0.95，近似值应 < 原始值（降幅约 5%）
        assert!(
            approx < raw,
            "DeepSeek 近似 ({}) 应 < cl100k 原始 ({})（系数 0.95）",
            approx,
            raw
        );
    }

    #[test]
    fn test_p24_claude_coefficient_value() {
        // v2.54 P24：验证 Claude 系数为 1.20
        let claude = TiktokenTokenizer::claude_approx().expect("claude_approx 应可用");
        let cl100k = TiktokenTokenizer::cl100k_base().expect("cl100k_base 应可用");

        // 用足够长的文本（避免短文本舍入误差）
        let text = "这是一段足够长的测试文本，用于验证 Claude 近似分词器的系数是否为 1.20。"
            .repeat(5);
        let raw = cl100k.count_tokens(&text);
        let approx = claude.count_tokens(&text);

        // 验证系数约为 1.20（允许 ±2% 误差，因浮点舍入）
        let ratio = approx as f64 / raw as f64;
        assert!(
            (ratio - 1.20).abs() < 0.02,
            "Claude 系数应为 1.20，实际 {:.4}",
            ratio
        );
    }

    #[test]
    fn test_p24_deepseek_coefficient_value() {
        // v2.54 P24：验证 DeepSeek 系数为 0.95
        let deepseek = TiktokenTokenizer::deepseek_approx().expect("deepseek_approx 应可用");
        let cl100k = TiktokenTokenizer::cl100k_base().expect("cl100k_base 应可用");

        let text = "这是一段足够长的测试文本，用于验证 DeepSeek 近似分词器的系数是否为 0.95。"
            .repeat(5);
        let raw = cl100k.count_tokens(&text);
        let approx = deepseek.count_tokens(&text);

        // 验证系数约为 0.95（允许 ±2% 误差）
        let ratio = approx as f64 / raw as f64;
        assert!(
            (ratio - 0.95).abs() < 0.02,
            "DeepSeek 系数应为 0.95，实际 {:.4}",
            ratio
        );
    }

    #[test]
    fn test_name() {
        let tk = TiktokenTokenizer::o200k_base().expect("o200k_base 应可用");
        assert_eq!(tk.name(), "o200k_base");
    }

    #[test]
    fn test_long_text() {
        let tk = TiktokenTokenizer::o200k_base().expect("o200k_base 应可用");
        let long_text = "Hello world. ".repeat(1000);
        let count = tk.count_tokens(&long_text);
        assert!(count > 1000, "长文本 token 数应 > 1000");
    }
}

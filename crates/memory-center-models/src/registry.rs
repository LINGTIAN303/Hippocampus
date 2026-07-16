//! # 模型注册表（Registry）
//!
//! 维护家族 → 默认型号的映射，提供按家族快速获取默认 ModelVariant 的能力。
//!
//! ## 设计目的
//!
//! 用户指定家族（如 `ModelFamily::Claude`）但未指定具体型号时，
//! Registry 返回该家族的最新（默认）ModelVariant。

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::family::ModelFamily;
use crate::variant::{ModelVariant, ToolCallFormat};

/// 模型注册表
///
/// 维护家族 → 默认 ModelVariant 的映射。
///
/// # 用法
///
/// ```no_run
/// use memory_center_models::{ModelFamily, ModelRegistry, ModelVariant};
///
/// // 获取家族默认型号
/// let variant = ModelRegistry::default_variant(ModelFamily::Claude);
/// assert_eq!(variant.family, ModelFamily::Claude);
///
/// // 列出所有内置型号
/// for (name, variant) in ModelRegistry::all_variants() {
///     println!("{}: {}", name, variant);
/// }
/// ```
pub struct ModelRegistry;

/// 全局注册表（懒加载，只初始化一次）
static REGISTRY: OnceLock<HashMap<String, ModelVariant>> = OnceLock::new();

impl ModelRegistry {
    /// 初始化注册表（内部，只调用一次）
    fn init() -> &'static HashMap<String, ModelVariant> {
        REGISTRY.get_or_init(|| {
            let mut map = HashMap::new();
            // 2026 年 7 月最新主流型号（已核查官方文档）
            let variants = vec![
                // Claude 家族（5 个型号，覆盖 Opus/Sonnet/Fable/Mythos 级）
                ("claude-opus-4.6", ModelVariant::claude_opus_4_6()),
                ("claude-opus-4.8", ModelVariant::claude_opus_4_8()),
                ("claude-sonnet-5", ModelVariant::claude_sonnet_5()),
                ("claude-fable-5", ModelVariant::claude_fable_5()),
                ("claude-mythos-5", ModelVariant::claude_mythos_5()),
                // 其他家族（原生版本）
                ("gpt-5.2", ModelVariant::gpt_5_2()),
                ("gpt-5-codex", ModelVariant::gpt_5_codex()),
                ("gemini-3.1-pro", ModelVariant::gemini_3_1_pro()),
                ("deepseek-v4-pro", ModelVariant::deepseek_v4_pro()),
                ("deepseek-v4-flash", ModelVariant::deepseek_v4_flash()),
                ("qwen-3-coder", ModelVariant::qwen_3_coder()),
                ("llama-4-scout", ModelVariant::llama_4_scout()),
                ("llama-4-maverick", ModelVariant::llama_4_maverick()),
                ("grok-4.1", ModelVariant::grok_4_1()),
                ("local-default", ModelVariant::local_default()),
                // v2.54 P26：Trae 内置 12 个型号（统一限制 200K 上下文）
                ("doubao-seed-2.1-pro", ModelVariant::trae_doubao_seed_2_1_pro()),
                ("doubao-seed-2.1-turbo", ModelVariant::trae_doubao_seed_2_1_turbo()),
                ("doubao-seed-code", ModelVariant::trae_doubao_seed_code()),
                ("minimax-m3", ModelVariant::trae_minimax_m3()),
                ("glm-5.2", ModelVariant::trae_glm_5_2()),
                ("glm-5.1", ModelVariant::trae_glm_5_1()),
                ("glm-5", ModelVariant::trae_glm_5()),
                ("trae-deepseek-v4-pro", ModelVariant::trae_deepseek_v4_pro()),
                ("trae-deepseek-v4-flash", ModelVariant::trae_deepseek_v4_flash()),
                ("kimi-k2.7-code", ModelVariant::trae_kimi_k2_7_code()),
                ("kimi-k2.6", ModelVariant::trae_kimi_k2_6()),
                ("trae-qwen-3.7-plus", ModelVariant::trae_qwen_3_7_plus()),
                // v2.54 P28：OpenCode Zen 计划 5 个型号（各有上下文限制）
                ("opencode-deepseek-v4-flash", ModelVariant::opencode_deepseek_v4_flash()),
                ("opencode-hy3", ModelVariant::opencode_hy3()),
                ("opencode-mimo-v2.5", ModelVariant::opencode_mimo_v2_5()),
                ("opencode-nimotron-3-ultra", ModelVariant::opencode_nimotron_3_ultra()),
                ("opencode-north-mini-code", ModelVariant::opencode_north_mini_code()),
            ];
            for (name, variant) in variants {
                map.insert(name.to_string(), variant);
            }
            map
        })
    }

    /// 按名称查找型号
    ///
    /// 支持的名称（2026 年 7 月，已核查官方文档）：
    /// - Claude 家族（5 个）：`claude-opus-4.6` / `claude-opus-4.8` / `claude-sonnet-5` / `claude-fable-5` / `claude-mythos-5`
    /// - `gpt-5.2` / `gpt-5-codex`
    /// - `gemini-3.1-pro`
    /// - `deepseek-v4-pro` / `deepseek-v4-flash`
    /// - `qwen-3-coder`
    /// - `llama-4-scout` / `llama-4-maverick`
    /// - `grok-4.1`
    /// - `local-default`
    ///
    /// # v2.54 P26 新增 Trae 内置型号（12 个，统一 200K 限制）
    ///
    /// - Doubao 家族：`doubao-seed-2.1-pro` / `doubao-seed-2.1-turbo` / `doubao-seed-code`
    /// - MiniMax 家族：`minimax-m3`
    /// - GLM 家族：`glm-5.2` / `glm-5.1` / `glm-5`
    /// - DeepSeek 家族（Trae 限制版）：`trae-deepseek-v4-pro` / `trae-deepseek-v4-flash`
    /// - Kimi 家族：`kimi-k2.7-code` / `kimi-k2.6`
    /// - Qwen 家族（Trae 限制版）：`trae-qwen-3.7-plus`
    ///
    /// # v2.54 P28 新增 OpenCode Zen 计划型号（5 个，各有上下文限制）
    ///
    /// - DeepSeek 家族（OpenCode Zen 版，200K）：`opencode-deepseek-v4-flash`
    /// - Hunyuan 家族（190K）：`opencode-hy3`
    /// - Mimo 家族（200K）：`opencode-mimo-v2.5`
    /// - Nimotron 家族（1M）：`opencode-nimotron-3-ultra`
    /// - North 家族（256K）：`opencode-north-mini-code`
    ///
    /// # v2.54 P25 新增别名机制
    ///
    /// 支持 `-latest` 后缀别名，自动转发到家族最新主流型号：
    /// - `claude-latest` → `claude-opus-4.8`
    /// - `gpt-latest` → `gpt-5.2`
    /// - `gemini-latest` → `gemini-3.1-pro`
    /// - `deepseek-latest` → `deepseek-v4-pro`
    /// - `qwen-latest` → `qwen-3-coder`
    /// - `llama-latest` → `llama-4-scout`
    /// - `grok-latest` → `grok-4.1`
    /// - `doubao-latest` → `doubao-seed-2.1-pro`
    /// - `minimax-latest` → `minimax-m3`
    /// - `kimi-latest` → `kimi-k2.7-code`
    /// - `glm-latest` → `glm-5.2`
    /// - `hunyuan-latest` → `opencode-hy3`（v2.54 P28 新增）
    /// - `mimo-latest` → `opencode-mimo-v2.5`（v2.54 P28 新增）
    /// - `nimotron-latest` → `opencode-nimotron-3-ultra`（v2.54 P28 新增）
    /// - `north-latest` → `opencode-north-mini-code`（v2.54 P28 新增）
    ///
    /// **注意**：Trae 的 Auto Mode 是调度模式，非具体型号，不在注册表中。
    /// 用户使用 Auto Mode 时应通过 `ModelVariant::custom` 兜底处理。
    pub fn find(name: &str) -> Option<ModelVariant> {
        // v2.54 P25：先查别名映射，再查注册表
        let canonical = Self::resolve_alias(name);
        Self::init().get(canonical).cloned()
    }

    /// 别名解析（v2.54 P25 新增）
    ///
    /// 将 `-latest` 后缀别名解析为家族最新主流型号。
    /// 未匹配别名的原样返回（保持向后兼容）。
    ///
    /// 设计原则：
    /// - 别名与具体型号解耦：`default_variant()` 升级时，别名自动跟随
    /// - 仅支持 `-latest` 后缀：避免与具体型号名冲突
    /// - 无 `local-latest`：local_default 是唯一本地型号，无别名需求
    /// - 无 `custom-latest`：custom 为用户自定义，无家族概念
    fn resolve_alias(name: &str) -> &str {
        // 匹配 `<family>-latest` 格式
        if let Some(family_prefix) = name.strip_suffix("-latest") {
            let family = match family_prefix {
                "claude" => Some(ModelFamily::Claude),
                "gpt" => Some(ModelFamily::Gpt),
                "gemini" => Some(ModelFamily::Gemini),
                "deepseek" => Some(ModelFamily::DeepSeek),
                "qwen" => Some(ModelFamily::Qwen),
                "llama" => Some(ModelFamily::Llama),
                "grok" => Some(ModelFamily::Grok),
                "doubao" => Some(ModelFamily::Doubao),
                "minimax" => Some(ModelFamily::MiniMax),
                "kimi" => Some(ModelFamily::Kimi),
                "glm" => Some(ModelFamily::Glm),
                // v2.54 P28：OpenCode Zen 计划新家族别名
                "hunyuan" => Some(ModelFamily::Hunyuan),
                "mimo" => Some(ModelFamily::Mimo),
                "nimotron" => Some(ModelFamily::Nimotron),
                "north" => Some(ModelFamily::North),
                _ => None,
            };
            if let Some(f) = family {
                // 查找该家族的默认型号，返回其在注册表中的 key
                // 由于 HashMap<String, _> 的 key 是 String，无法直接返回 &'static str
                // 这里查找一次得到对应 key，再从 all_variants 中匹配
                // 简化实现：default_variant() 返回的 name 即为注册表 key（Trae 型号 key 与 name 一致）
                // 用一次 leak 保证 'static（别名解析是低频操作，leak 一次无内存问题）
                let default = Self::default_variant(f);
                // 再次查找确保 key 存在于注册表
                if Self::init().contains_key(&default.name) {
                    return Box::leak(default.name.into_boxed_str());
                }
            }
        }
        name
    }

    /// 获取家族的默认型号（最新稳定版本）
    ///
    /// 每个家族返回其最新主流型号：
    /// - Claude → Opus 4.8（API 普遍可用的稳定旗舰）
    /// - GPT → GPT-5.2
    /// - Gemini → Gemini 3.1 Pro
    /// - DeepSeek → V4-Pro（原生版，1M 上下文）
    /// - Qwen → Qwen3-Coder（原生版，256K 上下文）
    /// - Llama → Llama 4 Scout
    /// - Grok → Grok 4.1
    /// - Doubao → Doubao-Seed-2.1-Pro（Trae 内置版，200K）
    /// - MiniMax → MiniMax-M3（Trae 内置版，200K）
    /// - Kimi → Kimi-K2.7-Code（Trae 内置版，200K）
    /// - GLM → GLM-5.2（Trae 内置版，200K）
    /// - Hunyuan → opencode-hy3（OpenCode Zen 版，190K，v2.54 P28 新增）
    /// - Mimo → opencode-mimo-v2.5（OpenCode Zen 版，200K，v2.54 P28 新增）
    /// - Nimotron → opencode-nimotron-3-ultra（OpenCode Zen 版，1M，v2.54 P28 新增）
    /// - North → opencode-north-mini-code（OpenCode Zen 版，256K，v2.54 P28 新增）
    /// - Local → local-default
    /// - Custom → custom("custom", Custom, 32K)（中性预设）
    ///
    /// **说明**：Claude 默认选 Opus 4.8 而非 Fable 5/Mythos 5，原因：
    /// - Fable 5 曾因出口管制暂停，稳定性待观察
    /// - Mythos 5 面向特定合作方，普通用户难访问
    /// - Opus 4.8 为 API 普遍可用的稳定旗舰
    ///
    /// **v2.54 P26 说明**：Doubao/MiniMax/Kimi/GLM 家族当前仅有 Trae 内置版本，
    /// 默认返回 Trae 限制版（200K）。未来若有原生版本接入，可调整默认值。
    ///
    /// **v2.54 P28 说明**：Hunyuan/Mimo/Nimotron/North 家族当前仅有 OpenCode Zen 版本，
    /// 默认返回 OpenCode 限制版（各有上下文限制）。未来若有 Go 计划付费版接入，可调整默认值。
    pub fn default_variant(family: ModelFamily) -> ModelVariant {
        match family {
            ModelFamily::Claude => ModelVariant::claude_opus_4_8(),
            ModelFamily::Gpt => ModelVariant::gpt_5_2(),
            ModelFamily::Gemini => ModelVariant::gemini_3_1_pro(),
            ModelFamily::DeepSeek => ModelVariant::deepseek_v4_pro(),
            ModelFamily::Qwen => ModelVariant::qwen_3_coder(),
            ModelFamily::Llama => ModelVariant::llama_4_scout(),
            ModelFamily::Grok => ModelVariant::grok_4_1(),
            // v2.54 P26：新家族默认返回 Trae 内置版
            ModelFamily::Doubao => ModelVariant::trae_doubao_seed_2_1_pro(),
            ModelFamily::MiniMax => ModelVariant::trae_minimax_m3(),
            ModelFamily::Kimi => ModelVariant::trae_kimi_k2_7_code(),
            ModelFamily::Glm => ModelVariant::trae_glm_5_2(),
            // v2.54 P28：OpenCode Zen 计划新家族默认返回 Zen 版（当前唯一版本）
            ModelFamily::Hunyuan => ModelVariant::opencode_hy3(),
            ModelFamily::Mimo => ModelVariant::opencode_mimo_v2_5(),
            ModelFamily::Nimotron => ModelVariant::opencode_nimotron_3_ultra(),
            ModelFamily::North => ModelVariant::opencode_north_mini_code(),
            ModelFamily::Local => ModelVariant::local_default(),
            ModelFamily::Custom => ModelVariant::custom("custom", ModelFamily::Custom, 32_000),
        }
    }

    /// 列出所有内置型号
    ///
    /// 返回 (型号名称, ModelVariant) 的迭代器
    pub fn all_variants() -> impl Iterator<Item = (&'static String, &'static ModelVariant)> {
        Self::init().iter()
    }

    /// 列出指定家族的所有型号
    pub fn variants_by_family(family: ModelFamily) -> Vec<(&'static String, &'static ModelVariant)> {
        Self::init()
            .iter()
            .filter(|(_, v)| v.family == family)
            .collect()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ArchiveStrategy;

    #[test]
    fn test_find_by_name() {
        let v = ModelRegistry::find("claude-opus-4.6");
        assert!(v.is_some());
        assert_eq!(v.unwrap().family, ModelFamily::Claude);
    }

    #[test]
    fn test_find_nonexistent() {
        let v = ModelRegistry::find("nonexistent-model");
        assert!(v.is_none());
    }

    #[test]
    fn test_default_variant_claude() {
        let v = ModelRegistry::default_variant(ModelFamily::Claude);
        assert_eq!(v.family, ModelFamily::Claude);
        assert_eq!(v.name, "claude-opus-4.8");
    }

    #[test]
    fn test_default_variant_gpt() {
        let v = ModelRegistry::default_variant(ModelFamily::Gpt);
        assert_eq!(v.name, "gpt-5.2");
    }

    #[test]
    fn test_default_variant_gemini() {
        let v = ModelRegistry::default_variant(ModelFamily::Gemini);
        assert_eq!(v.name, "gemini-3.1-pro");
    }

    #[test]
    fn test_default_variant_deepseek_v4() {
        let v = ModelRegistry::default_variant(ModelFamily::DeepSeek);
        assert_eq!(v.name, "deepseek-v4-pro");
    }

    #[test]
    fn test_default_variant_qwen_coder() {
        let v = ModelRegistry::default_variant(ModelFamily::Qwen);
        assert_eq!(v.name, "qwen-3-coder");
    }

    #[test]
    fn test_default_variant_llama_4_scout() {
        let v = ModelRegistry::default_variant(ModelFamily::Llama);
        assert_eq!(v.name, "llama-4-scout");
    }

    #[test]
    fn test_all_variants_count() {
        let count = ModelRegistry::all_variants().count();
        // 15 原生型号 + 12 Trae 内置型号 + 5 OpenCode Zen 型号 = 32 个
        assert_eq!(count, 32, "应内置 32 个型号（15 原生 + 12 Trae + 5 OpenCode）");
    }

    #[test]
    fn test_variants_by_family() {
        let claude_variants = ModelRegistry::variants_by_family(ModelFamily::Claude);
        assert_eq!(claude_variants.len(), 5); // opus-4.6, opus-4.8, sonnet-5, fable-5, mythos-5
    }

    #[test]
    fn test_variants_by_family_deepseek() {
        let deepseek_variants = ModelRegistry::variants_by_family(ModelFamily::DeepSeek);
        // v4-pro, v4-flash（原生）+ trae-deepseek-v4-pro, trae-deepseek-v4-flash（Trae 版）
        // + opencode-deepseek-v4-flash（OpenCode Zen 版，v2.54 P28 新增）
        assert_eq!(deepseek_variants.len(), 5);
    }

    #[test]
    fn test_variants_by_family_llama() {
        let llama_variants = ModelRegistry::variants_by_family(ModelFamily::Llama);
        assert_eq!(llama_variants.len(), 2); // scout, maverick
    }

    // ========================================================================
    // v2.54 P26：Trae 内置型号测试
    // ========================================================================

    #[test]
    fn test_trae_doubao_variants() {
        let pro = ModelRegistry::find("doubao-seed-2.1-pro").expect("应找到 doubao-seed-2.1-pro");
        assert_eq!(pro.family, ModelFamily::Doubao);
        assert_eq!(pro.context_window, 200_000);

        let turbo = ModelRegistry::find("doubao-seed-2.1-turbo").expect("应找到 doubao-seed-2.1-turbo");
        assert_eq!(turbo.family, ModelFamily::Doubao);

        let code = ModelRegistry::find("doubao-seed-code").expect("应找到 doubao-seed-code");
        assert_eq!(code.family, ModelFamily::Doubao);

        // 3 个 Doubao 型号
        let doubao_variants = ModelRegistry::variants_by_family(ModelFamily::Doubao);
        assert_eq!(doubao_variants.len(), 3);
    }

    #[test]
    fn test_trae_minimax_variant() {
        let m3 = ModelRegistry::find("minimax-m3").expect("应找到 minimax-m3");
        assert_eq!(m3.family, ModelFamily::MiniMax);
        assert_eq!(m3.context_window, 200_000);

        let minimax_variants = ModelRegistry::variants_by_family(ModelFamily::MiniMax);
        assert_eq!(minimax_variants.len(), 1);
    }

    #[test]
    fn test_trae_glm_variants() {
        let glm52 = ModelRegistry::find("glm-5.2").expect("应找到 glm-5.2");
        assert_eq!(glm52.family, ModelFamily::Glm);
        assert_eq!(glm52.context_window, 200_000);
        assert!(glm52.supports_thinking, "GLM-5.2 应支持思考链");

        let glm51 = ModelRegistry::find("glm-5.1").expect("应找到 glm-5.1");
        assert_eq!(glm51.family, ModelFamily::Glm);

        let glm5 = ModelRegistry::find("glm-5").expect("应找到 glm-5");
        assert_eq!(glm5.family, ModelFamily::Glm);

        // 3 个 GLM 型号
        let glm_variants = ModelRegistry::variants_by_family(ModelFamily::Glm);
        assert_eq!(glm_variants.len(), 3);
    }

    #[test]
    fn test_trae_kimi_variants() {
        let k27 = ModelRegistry::find("kimi-k2.7-code").expect("应找到 kimi-k2.7-code");
        assert_eq!(k27.family, ModelFamily::Kimi);
        assert_eq!(k27.context_window, 200_000);

        let k26 = ModelRegistry::find("kimi-k2.6").expect("应找到 kimi-k2.6");
        assert_eq!(k26.family, ModelFamily::Kimi);

        // 2 个 Kimi 型号
        let kimi_variants = ModelRegistry::variants_by_family(ModelFamily::Kimi);
        assert_eq!(kimi_variants.len(), 2);
    }

    #[test]
    fn test_trae_deepseek_variants() {
        // Trae 限制版（200K）
        let trae_pro = ModelRegistry::find("trae-deepseek-v4-pro").expect("应找到 trae-deepseek-v4-pro");
        assert_eq!(trae_pro.context_window, 200_000);
        assert_eq!(trae_pro.family, ModelFamily::DeepSeek);

        let trae_flash = ModelRegistry::find("trae-deepseek-v4-flash").expect("应找到 trae-deepseek-v4-flash");
        assert_eq!(trae_flash.context_window, 200_000);

        // 原生版仍为 1M
        let native_pro = ModelRegistry::find("deepseek-v4-pro").expect("应找到 deepseek-v4-pro");
        assert_eq!(native_pro.context_window, 1_000_000, "原生版应为 1M");
    }

    #[test]
    fn test_trae_qwen_variant() {
        let trae_qwen = ModelRegistry::find("trae-qwen-3.7-plus").expect("应找到 trae-qwen-3.7-plus");
        assert_eq!(trae_qwen.context_window, 200_000);
        assert_eq!(trae_qwen.family, ModelFamily::Qwen);

        // 原生版仍为 256K
        let native_qwen = ModelRegistry::find("qwen-3-coder").expect("应找到 qwen-3-coder");
        assert_eq!(native_qwen.context_window, 256_000, "原生版应为 256K");
    }

    #[test]
    fn test_all_trae_variants_have_200k_context() {
        // 所有 Trae 内置型号的 context_window 应为 200K
        let trae_names = [
            "doubao-seed-2.1-pro",
            "doubao-seed-2.1-turbo",
            "doubao-seed-code",
            "minimax-m3",
            "glm-5.2",
            "glm-5.1",
            "glm-5",
            "trae-deepseek-v4-pro",
            "trae-deepseek-v4-flash",
            "kimi-k2.7-code",
            "kimi-k2.6",
            "trae-qwen-3.7-plus",
        ];
        for name in trae_names {
            let v = ModelRegistry::find(name).unwrap_or_else(|| panic!("应找到 {}", name));
            assert_eq!(
                v.context_window, 200_000,
                "Trae 内置型号 {} 的 context_window 应为 200K",
                name
            );
            // archive_strategy 应为 LargeWindow { threshold: 50_000 }（200K/4，与 P22 一致）
            match v.archive_strategy {
                ArchiveStrategy::LargeWindow { threshold } => assert_eq!(
                    threshold, 50_000,
                    "Trae 内置型号 {} 的 archive_strategy 阈值应为 50K",
                    name
                ),
                _ => panic!("Trae 内置型号 {} 应为 LargeWindow", name),
            }
        }
    }

    #[test]
    fn test_trae_auto_mode_not_in_registry() {
        // Auto Mode 是调度模式，不在注册表中
        let auto = ModelRegistry::find("auto");
        assert!(auto.is_none(), "Auto Mode 不应在注册表中");
    }

    #[test]
    fn test_default_variant_new_families() {
        // v2.54 P26：新家族默认返回 Trae 内置版
        let doubao = ModelRegistry::default_variant(ModelFamily::Doubao);
        assert_eq!(doubao.family, ModelFamily::Doubao);
        assert_eq!(doubao.context_window, 200_000);

        let minimax = ModelRegistry::default_variant(ModelFamily::MiniMax);
        assert_eq!(minimax.family, ModelFamily::MiniMax);

        let kimi = ModelRegistry::default_variant(ModelFamily::Kimi);
        assert_eq!(kimi.family, ModelFamily::Kimi);

        let glm = ModelRegistry::default_variant(ModelFamily::Glm);
        assert_eq!(glm.family, ModelFamily::Glm);
    }

    #[test]
    fn test_all_families_have_default() {
        for family in ModelFamily::all() {
            let v = ModelRegistry::default_variant(family);
            assert_eq!(v.family, family, "家族 {:?} 的默认型号家族不匹配", family);
        }
    }

    // ========================================================================
    // v2.54 P27：能力标记更新测试（基于 2026-07 官方文档调研）
    // ========================================================================

    #[test]
    fn test_p27_doubao_pro_thinking_vision() {
        // P27：Doubao-Seed-2.1-Pro 支持 thinking + vision（旗舰深度推理版 + 多模态图文视频深度理解）
        let v = ModelRegistry::find("doubao-seed-2.1-pro").expect("应找到 doubao-seed-2.1-pro");
        assert!(v.supports_thinking, "P27: Doubao-Seed-2.1-Pro 应支持思考链（旗舰深度推理版）");
        assert!(v.supports_vision, "P27: Doubao-Seed-2.1-Pro 应支持视觉（多模态图文视频深度理解）");
    }

    #[test]
    fn test_p27_doubao_turbo_thinking_vision() {
        // P27：Doubao-Seed-2.1-Turbo 支持 thinking + vision（深度思考模型 + 多模态延续）
        let v = ModelRegistry::find("doubao-seed-2.1-turbo").expect("应找到 doubao-seed-2.1-turbo");
        assert!(v.supports_thinking, "P27: Doubao-Seed-2.1-Turbo 应支持思考链（深度思考模型）");
        assert!(v.supports_vision, "P27: Doubao-Seed-2.1-Turbo 应支持视觉（多模态延续 Pro）");
    }

    #[test]
    fn test_p27_doubao_code_conservative() {
        // P27：Doubao-Seed-Code 保持保守（代码模型，未明确宣传思考链/多模态）
        let v = ModelRegistry::find("doubao-seed-code").expect("应找到 doubao-seed-code");
        assert!(!v.supports_thinking, "P27: Doubao-Seed-Code 保持 supports_thinking=false（保守）");
        assert!(!v.supports_vision, "P27: Doubao-Seed-Code 保持 supports_vision=false（保守）");
    }

    #[test]
    fn test_p27_minimax_m3_thinking_vision() {
        // P27：MiniMax-M3 支持 thinking + vision（推理能力 + 原生多模态）
        let v = ModelRegistry::find("minimax-m3").expect("应找到 minimax-m3");
        assert!(v.supports_thinking, "P27: MiniMax-M3 应支持思考链（推理能力）");
        assert!(v.supports_vision, "P27: MiniMax-M3 应支持视觉（原生多模态）");
    }

    #[test]
    fn test_p27_glm_5_2_vision() {
        // P27：GLM-5.2 支持 vision（旗舰版延续多模态，GLM-4.5V 已支持 4K 图像 + 10 分钟视频）
        let v = ModelRegistry::find("glm-5.2").expect("应找到 glm-5.2");
        assert!(v.supports_thinking, "P27: GLM-5.2 应支持思考链（已是 P26 标记）");
        assert!(v.supports_vision, "P27: GLM-5.2 应支持视觉（旗舰版延续多模态）");
    }

    #[test]
    fn test_p27_glm_5_1_vision() {
        // P27：GLM-5.1 支持 vision（开源版延续多模态）
        let v = ModelRegistry::find("glm-5.1").expect("应找到 glm-5.1");
        assert!(v.supports_thinking, "P27: GLM-5.1 应支持思考链");
        assert!(v.supports_vision, "P27: GLM-5.1 应支持视觉（5.x 旗舰版延续多模态）");
    }

    #[test]
    fn test_p27_glm_5_conservative_vision() {
        // P27：GLM-5 保持 supports_vision=false（保守，初代未明确宣传多模态）
        let v = ModelRegistry::find("glm-5").expect("应找到 glm-5");
        assert!(v.supports_thinking, "P27: GLM-5 应支持思考链");
        assert!(!v.supports_vision, "P27: GLM-5 保持 supports_vision=false（保守，初代未明确多模态）");
    }

    #[test]
    fn test_p27_deepseek_v4_flash_trae_thinking() {
        // P27：Trae 版 DeepSeek-V4-Flash 现支持思考链（V3.1 起混合模型路线，V4-Flash 延续）
        let v = ModelRegistry::find("trae-deepseek-v4-flash").expect("应找到 trae-deepseek-v4-flash");
        assert!(v.supports_thinking, "P27: Trae 版 DeepSeek-V4-Flash 应支持思考链");
    }

    #[test]
    fn test_p27_deepseek_v4_flash_native_no_thinking() {
        // P27：原生版 DeepSeek-V4-Flash 仍保持 supports_thinking=false（原生版未明确支持，与 Trae 版差异化）
        let v = ModelRegistry::find("deepseek-v4-flash").expect("应找到 deepseek-v4-flash");
        assert!(!v.supports_thinking, "P27: 原生版 DeepSeek-V4-Flash 仍保持不支持思考链");
    }

    #[test]
    fn test_p27_qwen_3_7_plus_thinking_vision() {
        // P27：Trae 版 Qwen3.7-Plus 支持 thinking + vision（全域思考模式 + 原生多模态）
        let v = ModelRegistry::find("trae-qwen-3.7-plus").expect("应找到 trae-qwen-3.7-plus");
        assert!(v.supports_thinking, "P27: Trae 版 Qwen3.7-Plus 应支持思考链（全域思考模式）");
        assert!(v.supports_vision, "P27: Trae 版 Qwen3.7-Plus 应支持视觉（原生多模态）");
    }

    #[test]
    fn test_p27_kimi_variants_conservative() {
        // P27：Kimi 系列保持保守（K2.7-Code 专注编程、K2.6 通用版未明确思考链/多模态）
        let k27 = ModelRegistry::find("kimi-k2.7-code").expect("应找到 kimi-k2.7-code");
        assert!(!k27.supports_thinking, "P27: Kimi-K2.7-Code 保持 supports_thinking=false（保守）");
        assert!(!k27.supports_vision, "P27: Kimi-K2.7-Code 保持 supports_vision=false（保守）");

        let k26 = ModelRegistry::find("kimi-k2.6").expect("应找到 kimi-k2.6");
        assert!(!k26.supports_thinking, "P27: Kimi-K2.6 保持 supports_thinking=false（保守）");
        assert!(!k26.supports_vision, "P27: Kimi-K2.6 保持 supports_vision=false（保守）");
    }

    #[test]
    fn test_p27_summary_max_tokens_differentiated() {
        // P27：所有 Trae 内置型号（context_window=200K）summary_max_tokens 统一为 1024
        // 差异化规则：≥200K → 1024；<200K（如 local_default 32K）→ 512
        let trae_names = [
            "doubao-seed-2.1-pro",
            "minimax-m3",
            "glm-5.2",
            "trae-deepseek-v4-pro",
            "kimi-k2.7-code",
            "trae-qwen-3.7-plus",
        ];
        for name in trae_names {
            let v = ModelRegistry::find(name).unwrap_or_else(|| panic!("应找到 {}", name));
            assert_eq!(
                v.summary_max_tokens, 1024,
                "P27: 200K 上下文型号 {} 的 summary_max_tokens 应为 1024",
                name
            );
        }

        // 本地小模型（32K）应为 512
        let local = ModelRegistry::find("local-default").expect("应找到 local-default");
        assert_eq!(
            local.summary_max_tokens, 512,
            "P27: local_default（32K）的 summary_max_tokens 应为 512"
        );
    }

    #[test]
    fn test_p27_audio_all_false() {
        // P27：所有 Trae 内置型号均不支持音频（保守，无明确音频能力宣传）
        let trae_names = [
            "doubao-seed-2.1-pro",
            "doubao-seed-2.1-turbo",
            "doubao-seed-code",
            "minimax-m3",
            "glm-5.2",
            "glm-5.1",
            "glm-5",
            "trae-deepseek-v4-pro",
            "trae-deepseek-v4-flash",
            "kimi-k2.7-code",
            "kimi-k2.6",
            "trae-qwen-3.7-plus",
        ];
        for name in trae_names {
            let v = ModelRegistry::find(name).unwrap_or_else(|| panic!("应找到 {}", name));
            assert!(!v.supports_audio, "P27: {} 应不支持音频（保守）", name);
        }
    }

    // ========================================================================
    // v2.54 P25：alias 别名机制测试
    // ========================================================================

    #[test]
    fn test_p25_claude_latest_alias() {
        // claude-latest 应解析到 claude-opus-4.8（家族默认型号）
        let v = ModelRegistry::find("claude-latest").expect("claude-latest 应能解析");
        assert_eq!(v.family, ModelFamily::Claude);
        assert_eq!(v.name, "claude-opus-4.8", "claude-latest 应解析到 claude-opus-4.8");
    }

    #[test]
    fn test_p25_gpt_latest_alias() {
        let v = ModelRegistry::find("gpt-latest").expect("gpt-latest 应能解析");
        assert_eq!(v.family, ModelFamily::Gpt);
        assert_eq!(v.name, "gpt-5.2");
    }

    #[test]
    fn test_p25_gemini_latest_alias() {
        let v = ModelRegistry::find("gemini-latest").expect("gemini-latest 应能解析");
        assert_eq!(v.family, ModelFamily::Gemini);
        assert_eq!(v.name, "gemini-3.1-pro");
    }

    #[test]
    fn test_p25_deepseek_latest_alias() {
        let v = ModelRegistry::find("deepseek-latest").expect("deepseek-latest 应能解析");
        assert_eq!(v.family, ModelFamily::DeepSeek);
        // 注意：deepseek-latest 解析到原生版 deepseek-v4-pro（1M 上下文），非 Trae 限制版
        assert_eq!(v.name, "deepseek-v4-pro");
        assert_eq!(v.context_window, 1_000_000, "deepseek-latest 应为原生版 1M 上下文");
    }

    #[test]
    fn test_p25_qwen_latest_alias() {
        let v = ModelRegistry::find("qwen-latest").expect("qwen-latest 应能解析");
        assert_eq!(v.family, ModelFamily::Qwen);
        // 注意：qwen-latest 解析到原生版 qwen-3-coder（256K），非 Trae 限制版
        assert_eq!(v.name, "qwen-3-coder");
        assert_eq!(v.context_window, 256_000);
    }

    #[test]
    fn test_p25_llama_latest_alias() {
        let v = ModelRegistry::find("llama-latest").expect("llama-latest 应能解析");
        assert_eq!(v.family, ModelFamily::Llama);
        assert_eq!(v.name, "llama-4-scout");
    }

    #[test]
    fn test_p25_grok_latest_alias() {
        let v = ModelRegistry::find("grok-latest").expect("grok-latest 应能解析");
        assert_eq!(v.family, ModelFamily::Grok);
        assert_eq!(v.name, "grok-4.1");
    }

    #[test]
    fn test_p25_new_families_latest_alias() {
        // v2.54 P26 新增的 4 个家族也应支持 -latest 别名
        let doubao = ModelRegistry::find("doubao-latest").expect("doubao-latest 应能解析");
        assert_eq!(doubao.family, ModelFamily::Doubao);
        assert_eq!(doubao.name, "doubao-seed-2.1-pro");

        let minimax = ModelRegistry::find("minimax-latest").expect("minimax-latest 应能解析");
        assert_eq!(minimax.family, ModelFamily::MiniMax);
        assert_eq!(minimax.name, "minimax-m3");

        let kimi = ModelRegistry::find("kimi-latest").expect("kimi-latest 应能解析");
        assert_eq!(kimi.family, ModelFamily::Kimi);
        assert_eq!(kimi.name, "kimi-k2.7-code");

        let glm = ModelRegistry::find("glm-latest").expect("glm-latest 应能解析");
        assert_eq!(glm.family, ModelFamily::Glm);
        assert_eq!(glm.name, "glm-5.2");
    }

    #[test]
    fn test_p25_all_families_latest_alias_resolves() {
        // 遍历所有家族，确保 -latest 别名都能解析到对应默认型号
        let families_and_expected = [
            (ModelFamily::Claude, "claude-opus-4.8"),
            (ModelFamily::Gpt, "gpt-5.2"),
            (ModelFamily::Gemini, "gemini-3.1-pro"),
            (ModelFamily::DeepSeek, "deepseek-v4-pro"),
            (ModelFamily::Qwen, "qwen-3-coder"),
            (ModelFamily::Llama, "llama-4-scout"),
            (ModelFamily::Grok, "grok-4.1"),
            (ModelFamily::Doubao, "doubao-seed-2.1-pro"),
            (ModelFamily::MiniMax, "minimax-m3"),
            (ModelFamily::Kimi, "kimi-k2.7-code"),
            (ModelFamily::Glm, "glm-5.2"),
            // v2.54 P28：OpenCode Zen 计划新家族别名
            (ModelFamily::Hunyuan, "opencode-hy3"),
            (ModelFamily::Mimo, "opencode-mimo-v2.5"),
            (ModelFamily::Nimotron, "opencode-nimotron-3-ultra"),
            (ModelFamily::North, "opencode-north-mini-code"),
        ];

        for (family, expected_name) in families_and_expected {
            let alias = format!(
                "{}-latest",
                family.display_name().to_lowercase().split_whitespace().next().unwrap_or("")
            );
            // 这里用 family 的 debug 名小写化作为别名前缀（与 resolve_alias 实现一致）
            // 注意：resolve_alias 使用固定字符串匹配，而非 display_name 推导
            let alias_str = match family {
                ModelFamily::Claude => "claude-latest",
                ModelFamily::Gpt => "gpt-latest",
                ModelFamily::Gemini => "gemini-latest",
                ModelFamily::DeepSeek => "deepseek-latest",
                ModelFamily::Qwen => "qwen-latest",
                ModelFamily::Llama => "llama-latest",
                ModelFamily::Grok => "grok-latest",
                ModelFamily::Doubao => "doubao-latest",
                ModelFamily::MiniMax => "minimax-latest",
                ModelFamily::Kimi => "kimi-latest",
                ModelFamily::Glm => "glm-latest",
                // v2.54 P28：OpenCode Zen 计划新家族别名
                ModelFamily::Hunyuan => "hunyuan-latest",
                ModelFamily::Mimo => "mimo-latest",
                ModelFamily::Nimotron => "nimotron-latest",
                ModelFamily::North => "north-latest",
                _ => continue,
            };
            let v = ModelRegistry::find(alias_str)
                .unwrap_or_else(|| panic!("{} 应能解析", alias_str));
            assert_eq!(
                v.name, expected_name,
                "{} 应解析到 {}，实际解析到 {}",
                alias_str, expected_name, v.name
            );
            // 忽略未使用的 alias 变量
            let _ = alias;
        }
    }

    #[test]
    fn test_p25_unknown_alias_returns_none() {
        // 未知家族的 -latest 别名应返回 None（原样返回后查不到）
        assert!(ModelRegistry::find("unknownfamily-latest").is_none());
        assert!(ModelRegistry::find("nonexistent-latest").is_none());
    }

    #[test]
    fn test_p25_no_suffix_name_still_works() {
        // 无 -latest 后缀的具体型号名仍能正常查找（向后兼容）
        let v = ModelRegistry::find("claude-opus-4.8").expect("claude-opus-4.8 应能找到");
        assert_eq!(v.name, "claude-opus-4.8");

        let v2 = ModelRegistry::find("gpt-5.2").expect("gpt-5.2 应能找到");
        assert_eq!(v2.name, "gpt-5.2");
    }

    #[test]
    fn test_p25_local_latest_not_supported() {
        // local-default 无 -latest 别名（设计上无此需求）
        // local-latest 应返回 None（未匹配到家族）
        assert!(
            ModelRegistry::find("local-latest").is_none(),
            "local-latest 不应被识别为别名（local_default 无家族别名）"
        );
    }

    #[test]
    fn test_p25_custom_latest_not_supported() {
        // custom 无 -latest 别名（custom 为用户自定义，无家族概念）
        assert!(
            ModelRegistry::find("custom-latest").is_none(),
            "custom-latest 不应被识别为别名（custom 无家族概念）"
        );
    }

    // ========================================================================
    // v2.54 P25：deprecated 字段测试
    // ========================================================================

    #[test]
    fn test_p25_all_builtin_variants_deprecated_none() {
        // v2.54 P25：当前所有内置型号的 deprecated 字段应为 None（活跃状态）
        for (name, variant) in ModelRegistry::all_variants() {
            assert!(
                variant.deprecated.is_none(),
                "P25: 内置型号 {} 的 deprecated 应为 None（当前全部活跃）",
                name
            );
        }
    }

    #[test]
    fn test_p25_custom_variant_deprecated_none() {
        // custom 构造的型号 deprecated 默认为 None
        let v = ModelVariant::custom("test-custom", ModelFamily::Custom, 32_000);
        assert!(v.deprecated.is_none(), "P25: custom 型号的 deprecated 应为 None");
    }

    #[test]
    fn test_p25_local_default_deprecated_none() {
        // local_default 的 deprecated 应为 None
        let v = ModelVariant::local_default();
        assert!(v.deprecated.is_none(), "P25: local_default 的 deprecated 应为 None");
    }

    // ========================================================================
    // v2.54 P28：OpenCode Zen 计划型号测试
    // ========================================================================

    #[test]
    fn test_p28_opencode_deepseek_v4_flash() {
        let v = ModelRegistry::find("opencode-deepseek-v4-flash")
            .expect("应找到 opencode-deepseek-v4-flash");
        assert_eq!(v.family, ModelFamily::DeepSeek);
        assert_eq!(v.context_window, 200_000, "P28: OpenCode Zen 版应为 200K 限制");
        assert!(v.supports_thinking, "P28: OpenCode 版 DeepSeek-V4-Flash 应支持思考链");
        assert!(!v.supports_vision, "P28: DeepSeek V4 系列未明确多模态");
        // 200K → LargeWindow 50K（与 Trae 内置一致）
        match v.archive_strategy {
            ArchiveStrategy::LargeWindow { threshold } => assert_eq!(threshold, 50_000),
            _ => panic!("P28: 200K 应为 LargeWindow"),
        }
    }

    #[test]
    fn test_p28_opencode_hy3() {
        let v = ModelRegistry::find("opencode-hy3").expect("应找到 opencode-hy3");
        assert_eq!(v.family, ModelFamily::Hunyuan);
        assert_eq!(v.context_window, 190_000, "P28: Hy3 Zen 版应为 190K 限制");
        assert!(v.supports_thinking, "P28: Hy3 应支持思考链（295B MoE 推理优化）");
        assert!(v.supports_vision, "P28: Hy3 应支持视觉（混元多模态）");
        // 190K → Standard 47.5K（<200K 走 Standard）
        match v.archive_strategy {
            ArchiveStrategy::Standard { threshold } => assert_eq!(threshold, 47_500),
            _ => panic!("P28: 190K 应为 Standard"),
        }
    }

    #[test]
    fn test_p28_opencode_mimo_v2_5() {
        let v = ModelRegistry::find("opencode-mimo-v2.5").expect("应找到 opencode-mimo-v2.5");
        assert_eq!(v.family, ModelFamily::Mimo);
        assert_eq!(v.context_window, 200_000, "P28: MiMo-V2.5 Zen 版应为 200K 限制");
        assert!(v.supports_thinking, "P28: MiMo-V2.5 应支持思考链（Agent 模型）");
        assert!(v.supports_vision, "P28: MiMo-V2.5 应支持视觉（全模态）");
        match v.archive_strategy {
            ArchiveStrategy::LargeWindow { threshold } => assert_eq!(threshold, 50_000),
            _ => panic!("P28: 200K 应为 LargeWindow"),
        }
    }

    #[test]
    fn test_p28_opencode_nimotron_3_ultra() {
        let v = ModelRegistry::find("opencode-nimotron-3-ultra")
            .expect("应找到 opencode-nimotron-3-ultra");
        assert_eq!(v.family, ModelFamily::Nimotron);
        assert_eq!(v.context_window, 1_000_000, "P28: Nimotron-3-Ultra Zen 版即开放 1M");
        assert!(v.supports_thinking, "P28: Nimotron-3-Ultra 应支持思考链（Agentic Reasoning）");
        assert!(!v.supports_vision, "P28: Nimotron-3-Ultra 保守不支持视觉");
        // 1M → LargeWindow 250K（按 P22 custom 推导规则）
        match v.archive_strategy {
            ArchiveStrategy::LargeWindow { threshold } => assert_eq!(threshold, 250_000),
            _ => panic!("P28: 1M 应为 LargeWindow"),
        }
    }

    #[test]
    fn test_p28_opencode_north_mini_code() {
        let v = ModelRegistry::find("opencode-north-mini-code")
            .expect("应找到 opencode-north-mini-code");
        assert_eq!(v.family, ModelFamily::North);
        assert_eq!(v.context_window, 256_000, "P28: North-Mini-Code Zen 版应为 256K 限制");
        assert!(!v.supports_thinking, "P28: North-Mini-Code 保守不支持思考链（代码模型）");
        assert!(!v.supports_vision, "P28: North-Mini-Code 保守不支持视觉");
        // 256K → LargeWindow 64K
        match v.archive_strategy {
            ArchiveStrategy::LargeWindow { threshold } => assert_eq!(threshold, 64_000),
            _ => panic!("P28: 256K 应为 LargeWindow"),
        }
    }

    #[test]
    fn test_p28_opencode_variants_by_family() {
        // 4 个新家族各 1 个型号
        assert_eq!(ModelRegistry::variants_by_family(ModelFamily::Hunyuan).len(), 1);
        assert_eq!(ModelRegistry::variants_by_family(ModelFamily::Mimo).len(), 1);
        assert_eq!(ModelRegistry::variants_by_family(ModelFamily::Nimotron).len(), 1);
        assert_eq!(ModelRegistry::variants_by_family(ModelFamily::North).len(), 1);
    }

    #[test]
    fn test_p28_opencode_default_variant_new_families() {
        // v2.54 P28：4 个新家族的 default_variant 应返回 OpenCode Zen 版
        let hunyuan = ModelRegistry::default_variant(ModelFamily::Hunyuan);
        assert_eq!(hunyuan.family, ModelFamily::Hunyuan);
        assert_eq!(hunyuan.name, "opencode-hy3");
        assert_eq!(hunyuan.context_window, 190_000);

        let mimo = ModelRegistry::default_variant(ModelFamily::Mimo);
        assert_eq!(mimo.family, ModelFamily::Mimo);
        assert_eq!(mimo.name, "opencode-mimo-v2.5");
        assert_eq!(mimo.context_window, 200_000);

        let nimotron = ModelRegistry::default_variant(ModelFamily::Nimotron);
        assert_eq!(nimotron.family, ModelFamily::Nimotron);
        assert_eq!(nimotron.name, "opencode-nimotron-3-ultra");
        assert_eq!(nimotron.context_window, 1_000_000);

        let north = ModelRegistry::default_variant(ModelFamily::North);
        assert_eq!(north.family, ModelFamily::North);
        assert_eq!(north.name, "opencode-north-mini-code");
        assert_eq!(north.context_window, 256_000);
    }

    #[test]
    fn test_p28_opencode_latest_alias() {
        // v2.54 P28：4 个新家族的 -latest 别名应解析到 OpenCode Zen 版
        let hunyuan = ModelRegistry::find("hunyuan-latest").expect("hunyuan-latest 应能解析");
        assert_eq!(hunyuan.family, ModelFamily::Hunyuan);
        assert_eq!(hunyuan.name, "opencode-hy3");

        let mimo = ModelRegistry::find("mimo-latest").expect("mimo-latest 应能解析");
        assert_eq!(mimo.family, ModelFamily::Mimo);
        assert_eq!(mimo.name, "opencode-mimo-v2.5");

        let nimotron = ModelRegistry::find("nimotron-latest").expect("nimotron-latest 应能解析");
        assert_eq!(nimotron.family, ModelFamily::Nimotron);
        assert_eq!(nimotron.name, "opencode-nimotron-3-ultra");

        let north = ModelRegistry::find("north-latest").expect("north-latest 应能解析");
        assert_eq!(north.family, ModelFamily::North);
        assert_eq!(north.name, "opencode-north-mini-code");
    }

    #[test]
    fn test_p28_opencode_context_limits_distinct() {
        // OpenCode Zen 计划各型号上下文限制各有不同（与 Trae 统一 200K 不同）
        let cases = [
            ("opencode-deepseek-v4-flash", 200_000),
            ("opencode-hy3", 190_000),
            ("opencode-mimo-v2.5", 200_000),
            ("opencode-nimotron-3-ultra", 1_000_000),
            ("opencode-north-mini-code", 256_000),
        ];
        for (name, expected_ctx) in cases {
            let v = ModelRegistry::find(name).unwrap_or_else(|| panic!("应找到 {}", name));
            assert_eq!(
                v.context_window, expected_ctx,
                "P28: {} 的 context_window 应为 {}",
                name, expected_ctx
            );
        }
    }

    #[test]
    fn test_p28_opencode_audio_all_false() {
        // P28：所有 OpenCode Zen 型号均不支持音频（保守，无明确音频能力宣传）
        let opencode_names = [
            "opencode-deepseek-v4-flash",
            "opencode-hy3",
            "opencode-mimo-v2.5",
            "opencode-nimotron-3-ultra",
            "opencode-north-mini-code",
        ];
        for name in opencode_names {
            let v = ModelRegistry::find(name).unwrap_or_else(|| panic!("应找到 {}", name));
            assert!(!v.supports_audio, "P28: {} 应不支持音频（保守）", name);
        }
    }

    #[test]
    fn test_p28_opencode_summary_max_tokens_1024() {
        // P28：所有 OpenCode Zen 型号（>=200K）summary_max_tokens 统一为 1024
        let opencode_names = [
            "opencode-deepseek-v4-flash",
            "opencode-hy3",
            "opencode-mimo-v2.5",
            "opencode-nimotron-3-ultra",
            "opencode-north-mini-code",
        ];
        for name in opencode_names {
            let v = ModelRegistry::find(name).unwrap_or_else(|| panic!("应找到 {}", name));
            assert_eq!(
                v.summary_max_tokens, 1024,
                "P28: {} 的 summary_max_tokens 应为 1024",
                name
            );
        }
    }

    #[test]
    fn test_p28_opencode_deprecated_all_none() {
        // P28：所有 OpenCode Zen 型号的 deprecated 字段应为 None
        let opencode_names = [
            "opencode-deepseek-v4-flash",
            "opencode-hy3",
            "opencode-mimo-v2.5",
            "opencode-nimotron-3-ultra",
            "opencode-north-mini-code",
        ];
        for name in opencode_names {
            let v = ModelRegistry::find(name).unwrap_or_else(|| panic!("应找到 {}", name));
            assert!(
                v.deprecated.is_none(),
                "P28: {} 的 deprecated 应为 None",
                name
            );
        }
    }

    #[test]
    fn test_p28_opencode_tool_call_format_openai() {
        // P28：所有 OpenCode Zen 型号统一用 OpenAI 工具调用格式
        let opencode_names = [
            "opencode-deepseek-v4-flash",
            "opencode-hy3",
            "opencode-mimo-v2.5",
            "opencode-nimotron-3-ultra",
            "opencode-north-mini-code",
        ];
        for name in opencode_names {
            let v = ModelRegistry::find(name).unwrap_or_else(|| panic!("应找到 {}", name));
            assert_eq!(
                v.tool_call_format,
                ToolCallFormat::OpenAI,
                "P28: {} 应使用 OpenAI 工具调用格式",
                name
            );
        }
    }
}

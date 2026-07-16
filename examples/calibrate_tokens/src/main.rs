//! # Token 系数校准工具（v2.54 P24）
//!
//! 通过 LLM Provider API 实测真实 `prompt_tokens`，对比 tiktoken-rs `cl100k_base`
//! 原始计数，反推 Claude/DeepSeek 各型号的实际系数。
//!
//! ## 支持的 Provider
//!
//! - **OpenRouter**（端点 `https://openrouter.ai/api/v1/chat/completions`）：用于 Claude 系列
//!   - 环境变量：`OPENROUTER_API_KEY`
//! - **DeepSeek 官方 API**（端点 `https://api.deepseek.com/v1/chat/completions`）：用于 DeepSeek V4 系列
//!   - 环境变量：`DEEPSEEK_API_KEY`
//!
//! ## 使用方式
//!
//! ```bash
//! # DeepSeek V4 Pro（使用 DEEPSEEK_API_KEY 环境变量）
//! set DEEPSEEK_API_KEY=sk-xxxxx
//! cargo run -p calibrate_tokens -- --model deepseek-v4-pro
//!
//! # Claude Opus 4.8（使用 OPENROUTER_API_KEY 环境变量）
//! set OPENROUTER_API_KEY=sk-or-v1-xxxxx
//! cargo run -p calibrate_tokens -- --model claude-opus-4.8
//!
//! # 快速预览（限制前 20 条）
//! cargo run -p calibrate_tokens -- --model deepseek-v4-flash --limit 20
//! ```
//!
//! ## 输出
//!
//! Markdown 报告输出到 `fixtures/calibration/reports/<model>-<timestamp>.md`，
//! 包含每个样本的实测对比、按类别的系数分布、整体统计结论。

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use chrono::Local;
use clap::Parser;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

// ============================================================================
// 常量与默认配置
// ============================================================================

/// OpenRouter API 端点（用于 Claude 系列）
const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// DeepSeek 官方 API 端点（OpenAI 兼容格式，用于 DeepSeek V4 系列）
const DEEPSEEK_API_URL: &str = "https://api.deepseek.com/v1/chat/completions";

/// 项目名称（OpenRouter 推荐传入，用于展示来源）
const OPENROUTER_APP_TITLE: &str = "MemoryCenter Token Calibration";

/// 项目 GitHub 地址（OpenRouter 推荐，用于展示来源）
const OPENROUTER_REFERER: &str = "https://github.com/LINGTIAN303/MemoryCenter";

/// 默认样本文件路径（相对于工作区根目录）
const DEFAULT_SAMPLES_PATH: &str = "fixtures/calibration/samples.jsonl";

/// 默认报告输出目录
const DEFAULT_REPORTS_DIR: &str = "fixtures/calibration/reports";

/// 请求间隔（毫秒），避免触发速率限制
const DEFAULT_DELAY_MS: u64 = 500;

/// 单样本最大重试次数（遇到 429 速率限制或网络错误时）
const MAX_RETRIES: u32 = 3;

/// 429 速率限制时的初始等待时间
const RATE_LIMIT_WAIT_SECS: u64 = 10;

// ============================================================================
// Provider 与模型映射
// ============================================================================

/// LLM Provider 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Provider {
    /// OpenRouter 聚合平台（用于 Claude 系列）
    OpenRouter,
    /// DeepSeek 官方 API（用于 DeepSeek V4 系列）
    DeepSeekOfficial,
}

impl Provider {
    /// 该 provider 的 API 端点
    fn api_url(&self) -> &'static str {
        match self {
            Provider::OpenRouter => OPENROUTER_API_URL,
            Provider::DeepSeekOfficial => DEEPSEEK_API_URL,
        }
    }

    /// 该 provider 对应的环境变量名
    fn env_var_name(&self) -> &'static str {
        match self {
            Provider::OpenRouter => "OPENROUTER_API_KEY",
            Provider::DeepSeekOfficial => "DEEPSEEK_API_KEY",
        }
    }

    /// 该 provider 是否需要在请求头加入 OpenRouter 特定的字段
    fn needs_openrouter_headers(&self) -> bool {
        matches!(self, Provider::OpenRouter)
    }

    /// 显示名
    fn display_name(&self) -> &'static str {
        match self {
            Provider::OpenRouter => "OpenRouter",
            Provider::DeepSeekOfficial => "DeepSeek 官方",
        }
    }
}

/// 模型简称 → (Provider, API 模型 ID) 映射
///
/// 2026-07-16 状态：
/// - Claude 系列：通过 OpenRouter 代理调用
/// - DeepSeek V4 系列：直接用官方 API
fn model_mapping(short: &str) -> Option<(Provider, &'static str)> {
    match short {
        // Claude 系列（通过 OpenRouter 代理）
        "claude-opus-4.8" => Some((Provider::OpenRouter, "anthropic/claude-opus-4.8")),
        "claude-sonnet-5" => Some((Provider::OpenRouter, "anthropic/claude-sonnet-5")),
        // DeepSeek V4 系列（通过 DeepSeek 官方 API）
        // 2026-07-15 DeepSeek V4 正式上线，模型 ID 为 deepseek-v4-pro / deepseek-v4-flash
        "deepseek-v4-pro" => Some((Provider::DeepSeekOfficial, "deepseek-v4-pro")),
        "deepseek-v4-flash" => Some((Provider::DeepSeekOfficial, "deepseek-v4-flash")),
        _ => None,
    }
}

// ============================================================================
// CLI 参数定义
// ============================================================================

/// Token 系数校准工具（v2.54 P24）
#[derive(Parser, Debug)]
#[command(name = "calibrate_tokens", version, about)]
struct Cli {
    /// 模型简称（如 claude-opus-4.8 / claude-sonnet-5 / deepseek-v4-pro / deepseek-v4-flash）
    #[arg(long, short = 'm')]
    model: String,

    /// API 模型 ID（可选，覆盖默认映射）
    /// 例如：anthropic/claude-opus-4.8（OpenRouter）或 deepseek-v4-pro（DeepSeek 官方）
    #[arg(long)]
    api_model_id: Option<String>,

    /// 样本文件路径（默认 fixtures/calibration/samples.jsonl）
    #[arg(long, default_value = DEFAULT_SAMPLES_PATH)]
    samples: PathBuf,

    /// 报告输出目录（默认 fixtures/calibration/reports/）
    #[arg(long, default_value = DEFAULT_REPORTS_DIR)]
    output_dir: PathBuf,

    /// API Key（默认根据模型自动选择环境变量：OPENROUTER_API_KEY 或 DEEPSEEK_API_KEY）
    #[arg(long)]
    api_key: Option<String>,

    /// 请求间隔（毫秒，默认 500）
    #[arg(long, default_value_t = DEFAULT_DELAY_MS)]
    delay_ms: u64,

    /// 请求超时（秒，默认 60）
    #[arg(long, default_value_t = 60)]
    timeout_secs: u64,

    /// 只测试前 N 条样本（调试用，默认全部）
    #[arg(long)]
    limit: Option<usize>,

    /// 仅运行干跑（不调用 API，只输出 cl100k_base 计数用于核对样本）
    #[arg(long)]
    dry_run: bool,
}

// ============================================================================
// 数据结构
// ============================================================================

/// 样本条目（与 fixtures/calibration/samples.jsonl 格式一致）
#[derive(Debug, Deserialize)]
struct Sample {
    id: String,
    category: String,
    text: String,
}

/// OpenRouter 请求体（OpenAI Chat Completions 兼容格式）
#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    /// 设为 1，最小化输出 token 数量，降低成本
    /// 仅关注 input token 计数（prompt_tokens）
    max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

/// OpenRouter 响应体（仅解析 usage 字段）
#[derive(Debug, Deserialize)]
struct ChatResponse {
    usage: Usage,
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: usize,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    message: String,
}

/// 单样本校准结果
#[derive(Debug, Clone)]
struct SampleResult {
    id: String,
    category: String,
    text_preview: String,
    /// cl100k_base 原始 token 数（未乘系数）
    cl100k_raw: usize,
    /// OpenRouter API 返回的真实 prompt_tokens
    real_tokens: usize,
    /// 实际系数 = real_tokens / cl100k_raw
    coefficient: f64,
    /// 偏差百分比 = (real - cl100k_raw) / cl100k_raw × 100%
    /// 正数表示真实多于 cl100k 原始计数，负数表示少于
    deviation_pct: f64,
}

// ============================================================================
// 主函数
// ============================================================================

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    println!("=== Token 系数校准工具 (v2.54 P24) ===");
    println!("模型简称: {}", cli.model);

    // 解析 Provider 和 API 模型 ID
    let (provider, default_api_model_id) = match model_mapping(&cli.model) {
        Some((p, id)) => (p, id),
        None => {
            eprintln!(
                "错误：未识别的模型简称 '{}'。支持的模型：claude-opus-4.8 / claude-sonnet-5 / deepseek-v4-pro / deepseek-v4-flash",
                cli.model
            );
            return ExitCode::FAILURE;
        }
    };

    let api_model_id = cli
        .api_model_id
        .clone()
        .unwrap_or_else(|| default_api_model_id.to_string());

    println!("Provider: {}", provider.display_name());
    println!("API 模型 ID: {}", api_model_id);

    // 加载样本
    let samples = match load_samples(&cli.samples, cli.limit) {
        Ok(s) => {
            println!("已加载样本: {} 条（来源: {}）", s.len(), cli.samples.display());
            s
        }
        Err(e) => {
            eprintln!("错误：加载样本失败: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 初始化 tiktoken cl100k_base
    let bpe = match tiktoken_rs::cl100k_base() {
        Ok(bpe) => {
            println!("已初始化 tiktoken cl100k_base");
            bpe
        }
        Err(e) => {
            eprintln!("错误：初始化 cl100k_base 失败: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 干跑模式：只输出 cl100k_base 计数，不调用 API
    if cli.dry_run {
        println!("\n[干跑模式] 仅输出 cl100k_base 计数：\n");
        for s in &samples {
            let tokens = bpe.encode_with_special_tokens(&s.text).len();
            println!("  {:<6} [{:<32}] {} tokens", s.id, s.category, tokens);
        }
        return ExitCode::SUCCESS;
    }

    // 获取 API Key（优先 --api-key，其次根据 provider 选择环境变量）
    let api_key = match cli.api_key.clone() {
        Some(k) => k,
        None => match env::var(provider.env_var_name()) {
            Ok(k) => k,
            Err(_) => {
                eprintln!(
                    "错误：未提供 API Key。请通过 --api-key 或环境变量 {} 提供",
                    provider.env_var_name()
                );
                return ExitCode::FAILURE;
            }
        },
    };
    println!(
        "API Key: {}...{}（来源: {}）",
        &api_key[..6],
        &api_key[api_key.len() - 4..],
        provider.env_var_name()
    );

    // 构造 HTTP 客户端
    let client = Client::builder()
        .timeout(Duration::from_secs(cli.timeout_secs))
        .build()
        .expect("HTTP 客户端构造失败");

    // 逐个样本调用 API
    println!(
        "\n开始校准（{} 条样本，间隔 {}ms，预计耗时 ~{} 秒）...\n",
        samples.len(),
        cli.delay_ms,
        (samples.len() as u64 * (cli.delay_ms + 1500)) / 1000
    );

    let mut results: Vec<SampleResult> = Vec::with_capacity(samples.len());
    let total = samples.len();
    for (i, sample) in samples.iter().enumerate() {
        let progress = format!("[{:>3}/{:>3}]", i + 1, total);

        let cl100k_raw = bpe.encode_with_special_tokens(&sample.text).len();

        match call_api_with_retry(
            &client,
            &api_key,
            provider,
            &api_model_id,
            &sample.text,
            MAX_RETRIES,
            cli.delay_ms,
        )
        .await
        {
            Ok(real_tokens) => {
                let coefficient = real_tokens as f64 / cl100k_raw as f64;
                let deviation_pct =
                    (real_tokens as f64 - cl100k_raw as f64) / cl100k_raw as f64 * 100.0;

                println!(
                    "{} {:<6} [{:<30}] cl100k={:>5}  real={:>5}  系数={:.4}  偏差={:+.1}%",
                    progress,
                    sample.id,
                    sample.category,
                    cl100k_raw,
                    real_tokens,
                    coefficient,
                    deviation_pct
                );

                results.push(SampleResult {
                    id: sample.id.clone(),
                    category: sample.category.clone(),
                    text_preview: preview_text(&sample.text, 50),
                    cl100k_raw,
                    real_tokens,
                    coefficient,
                    deviation_pct,
                });
            }
            Err(e) => {
                eprintln!(
                    "{} {:<6} [{:<30}] 失败: {}",
                    progress, sample.id, sample.category, e
                );
                // 失败的样本跳过，不影响其他样本
            }
        }

        // 请求间隔（最后一次不用等）
        if i + 1 < total {
            tokio::time::sleep(Duration::from_millis(cli.delay_ms)).await;
        }
    }

    if results.is_empty() {
        eprintln!("\n错误：所有样本均失败，无法生成报告");
        return ExitCode::FAILURE;
    }

    // 生成报告
    let report = generate_report(&cli.model, provider, &api_model_id, &results);
    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    let report_filename = format!("{}-{}.md", cli.model, timestamp);

    // 确保输出目录存在
    if let Err(e) = fs::create_dir_all(&cli.output_dir) {
        eprintln!("错误：创建输出目录失败: {}", e);
        return ExitCode::FAILURE;
    }

    let report_path = cli.output_dir.join(&report_filename);
    match fs::write(&report_path, &report) {
        Ok(_) => {
            println!("\n=== 校准完成 ===");
            println!("报告路径: {}", report_path.display());
            println!("有效样本: {} / {}", results.len(), total);

            // 输出汇总
            let avg_coef = results.iter().map(|r| r.coefficient).sum::<f64>() / results.len() as f64;
            println!("平均系数: {:.4}（当前 cl100k_base 系数 1.0）", avg_coef);
            println!("建议值:  {:.4}（保留 3 位小数）", round_to_3_decimals(avg_coef));

            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("错误：写入报告失败: {}", e);
            ExitCode::FAILURE
        }
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 加载样本文件
fn load_samples(path: &PathBuf, limit: Option<usize>) -> Result<Vec<Sample>, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("读取文件失败: {}", e))?;

    let mut samples: Vec<Sample> = Vec::new();
    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Sample>(line) {
            Ok(s) => samples.push(s),
            Err(e) => {
                return Err(format!("第 {} 行解析失败: {}", line_num + 1, e));
            }
        }
    }

    if let Some(n) = limit {
        samples.truncate(n);
    }

    Ok(samples)
}

/// 调用 LLM Provider API（带重试）
///
/// 支持 OpenRouter 和 DeepSeek 官方两个 provider，端点和请求头根据 provider 自动适配。
async fn call_api_with_retry(
    client: &Client,
    api_key: &str,
    provider: Provider,
    model: &str,
    text: &str,
    max_retries: u32,
    delay_ms: u64,
) -> Result<usize, String> {
    let mut last_err = String::new();

    for attempt in 0..max_retries {
        if attempt > 0 {
            // 重试前等待（指数退避）
            let wait = delay_ms * 2u64.pow(attempt);
            tokio::time::sleep(Duration::from_millis(wait)).await;
        }

        let req = ChatRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user",
                content: text.to_string(),
            }],
            max_tokens: 1,
        };

        // 根据 provider 构造请求（OpenRouter 需要额外两个 header）
        let mut builder = client
            .post(provider.api_url())
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json");

        if provider.needs_openrouter_headers() {
            builder = builder
                .header("HTTP-Referer", OPENROUTER_REFERER)
                .header("X-Title", OPENROUTER_APP_TITLE);
        }

        let resp = builder.json(&req).send().await;

        match resp {
            Ok(r) => {
                let status = r.status();

                // 429 速率限制：等待后重试
                if status == StatusCode::TOO_MANY_REQUESTS {
                    last_err = format!("429 速率限制");
                    eprintln!("    [重试 {}/{}] 429 速率限制，等待 {} 秒",
                        attempt + 1, max_retries, RATE_LIMIT_WAIT_SECS);
                    tokio::time::sleep(Duration::from_secs(RATE_LIMIT_WAIT_SECS)).await;
                    continue;
                }

                // 5xx 服务器错误：重试
                if status.is_server_error() {
                    last_err = format!("{} 服务器错误", status);
                    eprintln!("    [重试 {}/{}] {} 服务器错误",
                        attempt + 1, max_retries, status);
                    continue;
                }

                // 4xx 客户端错误（非 429）：不重试
                if status.is_client_error() && status != StatusCode::TOO_MANY_REQUESTS {
                    let body = r.text().await.unwrap_or_default();
                    return Err(format!("{} 客户端错误: {}", status, body));
                }

                // 解析响应
                match r.json::<ChatResponse>().await {
                    Ok(parsed) => {
                        if let Some(err) = parsed.error {
                            return Err(format!("API 错误: {}", err.message));
                        }
                        return Ok(parsed.usage.prompt_tokens);
                    }
                    Err(e) => {
                        last_err = format!("响应解析失败: {}", e);
                        eprintln!("    [重试 {}/{}] {}",
                            attempt + 1, max_retries, last_err);
                        continue;
                    }
                }
            }
            Err(e) => {
                last_err = format!("网络错误: {}", e);
                eprintln!("    [重试 {}/{}] {}",
                    attempt + 1, max_retries, last_err);
                continue;
            }
        }
    }

    Err(format!("重试 {} 次后仍失败: {}", max_retries, last_err))
}

/// 截取文本预览（避免过长）
fn preview_text(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().take(max_chars).collect();
    let mut s: String = chars.into_iter().collect();
    if text.chars().count() > max_chars {
        s.push('…');
    }
    // 替换换行和制表符，保持单行
    s = s.replace('\n', " ").replace('\r', " ").replace('\t', " ");
    s
}

/// 保留 3 位小数
fn round_to_3_decimals(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

// ============================================================================
// 报告生成
// ============================================================================

/// 生成 Markdown 格式的校准报告
fn generate_report(
    model_short: &str,
    provider: Provider,
    api_model_id: &str,
    results: &[SampleResult],
) -> String {
    let total = results.len();
    let avg_coef: f64 = results.iter().map(|r| r.coefficient).sum::<f64>() / total as f64;
    let avg_deviation: f64 =
        results.iter().map(|r| r.deviation_pct).sum::<f64>() / total as f64;

    let max_coef = results.iter().map(|r| r.coefficient).fold(0.0f64, f64::max);
    let min_coef = results.iter().map(|r| r.coefficient).fold(f64::MAX, f64::min);

    // 标准差
    let variance: f64 = results
        .iter()
        .map(|r| (r.coefficient - avg_coef).powi(2))
        .sum::<f64>()
        / total as f64;
    let std_dev = variance.sqrt();

    // 按类别分组统计
    let mut categories: std::collections::HashMap<&str, Vec<&SampleResult>> =
        std::collections::HashMap::new();
    for r in results {
        categories.entry(r.category.as_str()).or_default().push(r);
    }
    let mut cat_stats: Vec<(&str, usize, f64, f64)> = categories
        .iter()
        .map(|(cat, items)| {
            let n = items.len();
            let avg = items.iter().map(|r| r.coefficient).sum::<f64>() / n as f64;
            let dev = items.iter().map(|r| r.deviation_pct).sum::<f64>() / n as f64;
            (*cat, n, avg, dev)
        })
        .collect();
    cat_stats.sort_by_key(|x| x.0);

    // 偏差分布
    let within_5 = results.iter().filter(|r| r.deviation_pct.abs() <= 5.0).count();
    let within_10 = results.iter().filter(|r| r.deviation_pct.abs() <= 10.0).count();
    let within_15 = results.iter().filter(|r| r.deviation_pct.abs() <= 15.0).count();
    let within_20 = results.iter().filter(|r| r.deviation_pct.abs() <= 20.0).count();
    let beyond_20 = results.iter().filter(|r| r.deviation_pct.abs() > 20.0).count();

    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let suggested_coef = round_to_3_decimals(avg_coef);

    let mut md = String::new();
    md.push_str(&format!("# Token 系数校准报告\n\n"));
    md.push_str(&format!("- **模型简称**: {}\n", model_short));
    md.push_str(&format!("- **Provider**: {}\n", provider.display_name()));
    md.push_str(&format!("- **API 模型 ID**: `{}`\n", api_model_id));
    md.push_str(&format!("- **校准时间**: {}\n", timestamp));
    md.push_str(&format!("- **有效样本**: {} 条\n", total));
    md.push_str(&format!("- **基准分词器**: tiktoken-rs `cl100k_base`（原始计数，未乘系数）\n\n"));

    md.push_str("## 1. 总体结论\n\n");
    md.push_str(&format!(
        "| 指标 | 数值 |\n|---|---|\n"
    ));
    md.push_str(&format!("| 平均系数 | **{:.4}** |\n", avg_coef));
    md.push_str(&format!("| 建议系数（3 位小数）| **{}** |\n", suggested_coef));
    md.push_str(&format!("| 最小系数 | {:.4} |\n", min_coef));
    md.push_str(&format!("| 最大系数 | {:.4} |\n", max_coef));
    md.push_str(&format!("| 标准差 | {:.4} |\n", std_dev));
    md.push_str(&format!("| 平均偏差 | {:+.2}% |\n\n", avg_deviation));

    md.push_str("### 1.1 建议操作\n\n");
    md.push_str(&format!(
        "若采用建议系数 `{}`，需在以下文件中更新对应构造器的 `coefficient` 字段：\n\n",
        suggested_coef
    ));
    md.push_str("- `crates/memory-center-models/src/tiktoken_impl.rs` 的 `claude_approx()` 或 `deepseek_approx()`\n");
    md.push_str("- `crates/memory-center-models/src/variant.rs` 中对应 Claude/DeepSeek 型号构造器\n\n");

    md.push_str("## 2. 偏差分布\n\n");
    md.push_str(&format!(
        "| 偏差范围 | 样本数 | 占比 |\n|---|---|---|\n"
    ));
    md.push_str(&format!(
        "| ±5% | {} | {:.1}% |\n",
        within_5,
        within_5 as f64 / total as f64 * 100.0
    ));
    md.push_str(&format!(
        "| ±10% | {} | {:.1}% |\n",
        within_10,
        within_10 as f64 / total as f64 * 100.0
    ));
    md.push_str(&format!(
        "| ±15% | {} | {:.1}% |\n",
        within_15,
        within_15 as f64 / total as f64 * 100.0
    ));
    md.push_str(&format!(
        "| ±20% | {} | {:.1}% |\n",
        within_20,
        within_20 as f64 / total as f64 * 100.0
    ));
    md.push_str(&format!(
        "| >20% | {} | {:.1}% |\n\n",
        beyond_20,
        beyond_20 as f64 / total as f64 * 100.0
    ));

    md.push_str("## 3. 按类别分组统计\n\n");
    md.push_str("| 类别 | 样本数 | 平均系数 | 平均偏差 |\n|---|---|---|---|\n");
    for (cat, n, avg, dev) in &cat_stats {
        md.push_str(&format!(
            "| {} | {} | {:.4} | {:+.2}% |\n",
            cat, n, avg, dev
        ));
    }
    md.push_str("\n");

    md.push_str("## 4. 详细样本数据\n\n");
    md.push_str("| ID | 类别 | cl100k_base | 实测 token | 系数 | 偏差 | 文本预览 |\n");
    md.push_str("|---|---|---|---|---|---|---|\n");
    for r in results {
        md.push_str(&format!(
            "| {} | {} | {} | {} | {:.4} | {:+.1}% | {} |\n",
            r.id,
            r.category,
            r.cl100k_raw,
            r.real_tokens,
            r.coefficient,
            r.deviation_pct,
            r.text_preview
        ));
    }

    md.push_str("\n---\n\n");
    md.push_str("*本报告由 `examples/calibrate_tokens` 工具自动生成。*\n");
    md.push_str(&format!(
        "*系数计算公式: `real_prompt_tokens / cl100k_base_raw_count`，建议系数保留 3 位小数。*\n"
    ));

    md
}

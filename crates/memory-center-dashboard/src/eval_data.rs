//! 评测数据加载 - 读取本地 JSON 文件展示 V2.35/V2.36 结果

use serde::Deserialize;
use std::path::PathBuf;

/// V2.36 LongMemEval 结果
#[derive(Debug, Clone, Deserialize)]
pub struct LongMemEvalSummary {
    pub siliconflow: EvalModelData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvalModelData {
    pub baseline: EvalCondition,
    pub memory_center: EvalCondition,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvalCondition {
    #[serde(flatten)]
    pub types: std::collections::HashMap<String, EvalTypeStat>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvalTypeStat {
    pub accuracy: f64,
    pub count: u32,
}

/// 内嵌的评测数据（当本地 JSON 不存在时使用）
pub fn builtin_eval_data() -> Vec<EvalRow> {
    vec![
        EvalRow {
            dataset: "LoCoMo F1".to_string(),
            model: "sensenova".to_string(),
            baseline: 0.1036,
            memory_center: 0.1465,
            improvement: 41.4,
            judge: "纯算法".to_string(),
        },
        EvalRow {
            dataset: "LoCoMo F1".to_string(),
            model: "step".to_string(),
            baseline: 0.1105,
            memory_center: 0.1345,
            improvement: 21.7,
            judge: "纯算法".to_string(),
        },
        EvalRow {
            dataset: "R@5 检索".to_string(),
            model: "s_cleaned".to_string(),
            baseline: 0.0,
            memory_center: 1.0,
            improvement: 999.0,
            judge: "纯算法".to_string(),
        },
        EvalRow {
            dataset: "nDCG@5".to_string(),
            model: "s_cleaned".to_string(),
            baseline: 0.0,
            memory_center: 0.9405,
            improvement: 999.0,
            judge: "纯算法".to_string(),
        },
        EvalRow {
            dataset: "LongMemEval V2.3".to_string(),
            model: "sensenova".to_string(),
            baseline: 0.7333,
            memory_center: 0.7333,
            improvement: 0.0,
            judge: "DeepSeek-flash".to_string(),
        },
        EvalRow {
            dataset: "LongMemEval V2.36".to_string(),
            model: "Qwen2.5-7B".to_string(),
            baseline: 0.0667,
            memory_center: 0.0667,
            improvement: 0.0,
            judge: "Qwen2.5-7B(自判)".to_string(),
        },
        EvalRow {
            dataset: "响应速度(smoke)".to_string(),
            model: "Qwen2.5-7B".to_string(),
            baseline: 127.1,
            memory_center: 87.6,
            improvement: 31.0,
            judge: "客观计时(s)".to_string(),
        },
    ]
}

/// 评测行数据
pub struct EvalRow {
    pub dataset: String,
    pub model: String,
    pub baseline: f64,
    pub memory_center: f64,
    pub improvement: f64,
    pub judge: String,
}

/// 加载本地评测 JSON 文件
pub fn load_local_eval(eval_dir: &str) -> Vec<EvalRow> {
    let mut rows = builtin_eval_data();

    // 尝试加载 V2.36 LongMemEval 结果
    let v236_path = PathBuf::from(eval_dir).join("longmemeval_summary_v2.36.json");
    if let Ok(content) = std::fs::read_to_string(&v236_path) {
        if let Ok(summary) = serde_json::from_str::<serde_json::Value>(&content) {
            // 尝试解析并替换内嵌数据
            if let Some(siliconflow) = summary.get("siliconflow") {
                if let (Some(base), Some(mc)) =
                    (siliconflow.get("baseline"), siliconflow.get("memory_center"))
                {
                    if let (Some(base_overall), Some(mc_overall)) =
                        (base.get("overall"), mc.get("overall"))
                    {
                        if let (Some(b_acc), Some(m_acc)) = (
                            base_overall.get("accuracy"),
                            mc_overall.get("accuracy"),
                        ) {
                            if let (Some(b), Some(m)) =
                                (b_acc.as_f64(), m_acc.as_f64())
                            {
                                // 替换 V2.36 行
                                if let Some(row) = rows.iter_mut().find(|r| {
                                    r.dataset == "LongMemEval V2.36"
                                }) {
                                    row.baseline = b;
                                    row.memory_center = m;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    rows
}

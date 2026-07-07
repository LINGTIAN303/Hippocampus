//! # BM25 关键词检索（WASM 版，v2.35 Task 6）
//!
//! 与 `bm25.rs`（native 版）保持相同的公共接口，但内部用简易字符分词
//! 替代 `jieba-rs` 中文分词，以兼容 `wasm32-unknown-unknown` target。
//!
//! ## 分词策略（简易 fallback）
//!
//! - ASCII 字母数字序列：作为一个 token，转小写
//! - 中文字符（CJK Unified Ideograph）：每个字符作为一个 token
//! - 其他字符（标点、空格、符号）：作为分隔符，跳过
//!
//! 相比 jieba 分词，简易分词对中文的召回率略低（单字粒度），
//! 但对短查询和精确匹配仍有效，且无外部词典依赖，适合 WASM 环境。
//!
//! ## 算法
//!
//! BM25（Best Matching 25）经典文本检索算法：
//!
//! ```text
//! score(q, d) = Σ_{qi in q} IDF(qi) * (f(qi, d) * (k1 + 1)) /
//!               (f(qi, d) + k1 * (1 - b + b * |d| / avgdl))
//!
//! IDF(qi) = ln((N - n(qi) + 0.5) / (n(qi) + 0.5) + 1)
//! ```
//!
//! - `N`：文档总数
//! - `n(qi)`：包含 qi 的文档数
//! - `f(qi, d)`：qi 在文档 d 中的词频
//! - `|d|`：文档 d 长度（词数）
//! - `avgdl`：平均文档长度
//! - `k1`：词频饱和参数（默认 1.2）
//! - `b`：长度归一化参数（默认 0.75）

use crate::semantic::{KeywordSearcher, RetrievalSource, SearchHit};
use std::collections::HashMap;
use std::sync::RwLock;

// ============================================================================
// 分词器（WASM 简易版，无 jieba 依赖）
// ============================================================================

/// 简易分词器（无外部依赖，WASM 兼容）
///
/// 分词策略：
/// - ASCII 字母数字序列 → 一个 token（转小写）
/// - 中文字符 → 每个字符一个 token
/// - 其他字符 → 分隔符，跳过
struct Tokenizer;

impl Tokenizer {
    fn new() -> Self {
        Self
    }

    /// 分词：ASCII 按词切分，中文按单字切分
    ///
    /// 返回小写化的 token 列表（去停用词）。
    fn tokenize(&self, text: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current_ascii = String::new();

        for ch in text.chars() {
            if ch.is_ascii_alphanumeric() {
                // ASCII 字母数字：累积成词
                current_ascii.push(ch);
            } else {
                // 非 ASCII 字母数字：先 flush 累积的 ASCII 词
                if !current_ascii.is_empty() {
                    tokens.push(current_ascii.to_lowercase());
                    current_ascii.clear();
                }
                // 中文字符：每个字作为一个 token
                if is_cjk_char(ch) {
                    tokens.push(ch.to_string());
                }
                // 其他字符（标点、空格、符号）：作为分隔符，跳过
            }
        }
        // flush 末尾的 ASCII 词
        if !current_ascii.is_empty() {
            tokens.push(current_ascii.to_lowercase());
        }

        // 去停用词
        tokens
            .into_iter()
            .filter(|t| !STOPWORDS.contains(&t.as_str()))
            .collect()
    }
}

/// 判断字符是否为 CJK 统一汉字（U+4E00 - U+9FFF）
///
/// 覆盖常用中文字符，不包含扩展区（足够用于简易分词）。
fn is_cjk_char(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs（常用汉字）
        | '\u{3400}'..='\u{4DBF}' // CJK Extension A
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
    )
}

/// 中文停用词表（常见无意义词，与 native 版一致）
const STOPWORDS: &[&str] = &[
    "的", "了", "在", "是", "我", "有", "和", "就", "不", "人", "都", "一", "一个",
    "上", "也", "很", "到", "说", "要", "去", "你", "会", "着", "没有", "看", "好",
    "这", "那", "它", "他", "她", "们", "与", "或", "但", "而", "如果", "因为",
    "所以", "但是", "然后", "可以", "什么", "怎么", "为什么", "哪里", "哪个",
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "must", "can", "to", "of", "in", "on", "at",
    "for", "with", "by", "from", "as", "into", "through", "during",
    "and", "or", "but", "if", "then", "else", "when", "where", "why",
    "how", "all", "any", "both", "each", "few", "more", "most", "other",
    "some", "such", "no", "not", "only", "own", "same", "so", "than",
    "too", "very", "just", "this", "that", "these", "those",
];

// ============================================================================
// Bm25Searcher（与 native 版公共接口完全一致）
// ============================================================================

/// 文档索引项
#[derive(Debug, Clone)]
struct DocEntry {
    /// 记忆文件 ID
    memory_id: String,
    /// 文档长度（词数）
    doc_len: usize,
    /// 词频表：term → tf
    term_freq: HashMap<String, u32>,
}

/// BM25 关键词检索器（WASM 版）
///
/// 默认实现 [`KeywordSearcher`] trait。
///
/// ## 参数
///
/// - `k1`：词频饱和参数（1.2-2.0，默认 1.2）
/// - `b`：长度归一化参数（0-1，默认 0.75）
///
/// ## 并发
///
/// 内部用 `RwLock` 保证并发安全：读操作（search）可并发，写操作（index/remove）串行化。
///
/// ## 与 native 版差异
///
/// 仅分词器不同（简易字符分词 vs jieba），BM25 算法和公共接口完全一致。
pub struct Bm25Searcher {
    /// BM25 参数 k1
    k1: f64,
    /// BM25 参数 b
    b: f64,
    /// 分词器（WASM 简易版，无状态）
    tokenizer: Tokenizer,
    /// 文档索引：hook_id → DocEntry
    docs: RwLock<HashMap<String, DocEntry>>,
    /// 倒排索引：term → [(hook_id, tf)]
    inverted_index: RwLock<HashMap<String, Vec<(String, u32)>>>,
}

impl Bm25Searcher {
    /// 创建新的 BM25 检索器（默认参数 k1=1.2, b=0.75）
    pub fn new() -> Self {
        Self::with_params(1.2, 0.75)
    }

    /// 创建新的 BM25 检索器（自定义参数）
    pub fn with_params(k1: f64, b: f64) -> Self {
        Self {
            k1,
            b,
            tokenizer: Tokenizer::new(),
            docs: RwLock::new(HashMap::new()),
            inverted_index: RwLock::new(HashMap::new()),
        }
    }

    /// 计算 IDF（Inverse Document Frequency）
    ///
    /// `IDF(qi) = ln((N - n(qi) + 0.5) / (n(qi) + 0.5) + 1)`
    ///
    /// 加 1 防止负值（BM25+ 变体）。
    fn idf(&self, n_qi: usize, n_total: usize) -> f64 {
        let numerator = (n_total as f64) - (n_qi as f64) + 0.5;
        let denominator = (n_qi as f64) + 0.5;
        (numerator / denominator + 1.0).ln()
    }
}

impl Default for Bm25Searcher {
    fn default() -> Self {
        Self::new()
    }
}

impl KeywordSearcher for Bm25Searcher {
    fn index(&self, hook_id: &str, memory_id: &str, text: &str) {
        // 分词
        let tokens = self.tokenizer.tokenize(text);
        let doc_len = tokens.len();

        // 统计词频
        let mut term_freq: HashMap<String, u32> = HashMap::new();
        for token in &tokens {
            *term_freq.entry(token.clone()).or_insert(0) += 1;
        }

        // 写入文档索引
        {
            let mut docs = self.docs.write().unwrap();
            // 若已存在，先移除旧索引（避免重复）
            if docs.contains_key(hook_id) {
                drop(docs);
                self.remove(hook_id);
                docs = self.docs.write().unwrap();
            }
            docs.insert(
                hook_id.to_string(),
                DocEntry {
                    memory_id: memory_id.to_string(),
                    doc_len,
                    term_freq: term_freq.clone(),
                },
            );
        }

        // 写入倒排索引
        {
            let mut inverted = self.inverted_index.write().unwrap();
            for (term, tf) in &term_freq {
                inverted
                    .entry(term.clone())
                    .or_insert_with(Vec::new)
                    .push((hook_id.to_string(), *tf));
            }
        }
    }

    fn search(&self, query: &str, top_k: usize) -> Vec<SearchHit> {
        let query_tokens = self.tokenizer.tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        let docs = self.docs.read().unwrap();
        let inverted = self.inverted_index.read().unwrap();

        let n_total = docs.len();
        if n_total == 0 {
            return Vec::new();
        }

        // 计算平均文档长度
        let avgdl: f64 = docs.values().map(|d| d.doc_len as f64).sum::<f64>() / n_total as f64;

        // 对每个文档计算 BM25 分数
        let mut scores: Vec<(String, String, f32)> = Vec::new();

        for (hook_id, doc) in docs.iter() {
            let mut score = 0.0_f64;

            for term in &query_tokens {
                // 词频
                let tf = doc.term_freq.get(term).copied().unwrap_or(0);
                if tf == 0 {
                    continue;
                }

                // 包含该词的文档数
                let n_qi = inverted.get(term).map(|v| v.len()).unwrap_or(0);
                if n_qi == 0 {
                    continue;
                }

                // IDF
                let idf = self.idf(n_qi, n_total);

                // BM25 分数
                let tf_f = tf as f64;
                let doc_len = doc.doc_len as f64;
                let denom = tf_f + self.k1 * (1.0 - self.b + self.b * doc_len / avgdl);
                let term_score = idf * (tf_f * (self.k1 + 1.0)) / denom;

                score += term_score;
            }

            if score > 0.0 {
                scores.push((hook_id.clone(), doc.memory_id.clone(), score as f32));
            }
        }

        // 按分数降序排列，取 top_k
        scores.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);

        scores
            .into_iter()
            .map(|(hook_id, memory_id, score)| SearchHit {
                hook_id,
                memory_id,
                score,
                source: RetrievalSource::Keyword,
            })
            .collect()
    }

    fn remove(&self, hook_id: &str) {
        // 从文档索引移除，并获取旧文档的 term_freq
        let old_doc = {
            let mut docs = self.docs.write().unwrap();
            docs.remove(hook_id)
        };

        // 从倒排索引移除对应条目
        if let Some(doc) = old_doc {
            let mut inverted = self.inverted_index.write().unwrap();
            for term in doc.term_freq.keys() {
                if let Some(postings) = inverted.get_mut(term) {
                    postings.retain(|(hid, _)| hid != hook_id);
                    if postings.is_empty() {
                        inverted.remove(term);
                    }
                }
            }
        }
    }

    fn len(&self) -> usize {
        self.docs.read().unwrap().len()
    }

    fn clear(&self) {
        self.docs.write().unwrap().clear();
        self.inverted_index.write().unwrap().clear();
    }
}

// ============================================================================
// 单元测试（仅在非 native feature 下编译，不影响 native 测试）
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_wasm_basic_search() {
        let searcher = Bm25Searcher::new();

        // 索引 3 个文档
        searcher.index("h1", "m1", "Rust 是一门系统编程语言，强调安全性和性能");
        searcher.index("h2", "m2", "Python 是动态语言，适合数据分析和机器学习");
        searcher.index("h3", "m3", "Rust 的所有权机制保证内存安全");

        // 搜索 "Rust"（简易分词下 "Rust" 作为完整 ASCII 词被切出）
        let results = searcher.search("Rust", 3);
        assert!(!results.is_empty(), "应返回结果");

        // h1 和 h3 都含 "Rust"，应排在前面
        let top_ids: Vec<&str> = results.iter().map(|h| h.hook_id.as_str()).collect();
        assert!(top_ids.contains(&"h1") || top_ids.contains(&"h3"));
        assert_eq!(results[0].source, RetrievalSource::Keyword);
    }

    #[test]
    fn test_bm25_wasm_chinese_single_char() {
        let searcher = Bm25Searcher::new();

        searcher.index("h1", "m1", "记忆库是 Agent 的核心组件，负责存储和检索历史对话");
        searcher.index("h2", "m2", "向量检索通过 embedding 实现语义匹配");
        searcher.index("h3", "m3", "Agent 记忆库的归档机制基于 token 阈值触发");

        // 搜索 "记忆库"（简易分词下切为 ["记", "忆", "库"]）
        let results = searcher.search("记忆库", 3);
        assert!(!results.is_empty(), "中文单字分词应仍能匹配");

        // h1 和 h3 都含 "记" "忆" "库" 三个字，应排前面
        let top_ids: Vec<&str> = results.iter().map(|h| h.hook_id.as_str()).collect();
        assert!(top_ids.contains(&"h1"));
        assert!(top_ids.contains(&"h3"));
    }

    #[test]
    fn test_bm25_wasm_no_match() {
        let searcher = Bm25Searcher::new();
        searcher.index("h1", "m1", "Rust 编程语言");

        let results = searcher.search("Python", 5);
        assert!(results.is_empty(), "无匹配应返回空");
    }

    #[test]
    fn test_bm25_wasm_remove() {
        let searcher = Bm25Searcher::new();
        searcher.index("h1", "m1", "Rust 语言");
        searcher.index("h2", "m2", "Rust 安全");

        assert_eq!(searcher.len(), 2);

        // 删除 h1
        searcher.remove("h1");
        assert_eq!(searcher.len(), 1);

        // 搜索应只剩 h2
        let results = searcher.search("Rust", 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hook_id, "h2");
    }

    #[test]
    fn test_bm25_wasm_clear() {
        let searcher = Bm25Searcher::new();
        searcher.index("h1", "m1", "测试文档");
        searcher.index("h2", "m2", "另一个文档");

        searcher.clear();
        assert!(searcher.is_empty());

        let results = searcher.search("文档", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_wasm_reindex_same_hook() {
        // 同一 hook_id 重新索引应覆盖旧内容
        let searcher = Bm25Searcher::new();
        searcher.index("h1", "m1", "Rust 语言");
        searcher.index("h1", "m1", "Python 语言"); // 覆盖

        assert_eq!(searcher.len(), 1, "应只有 1 个文档");

        // 搜索 Rust 应无结果（已被覆盖）
        let rust_results = searcher.search("Rust", 5);
        assert!(rust_results.is_empty(), "Rust 应已被覆盖");

        // 搜索 Python 应有结果
        let py_results = searcher.search("Python", 5);
        assert!(!py_results.is_empty(), "Python 应可搜到");
    }

    #[test]
    fn test_bm25_wasm_empty_query() {
        let searcher = Bm25Searcher::new();
        searcher.index("h1", "m1", "测试文档");

        let results = searcher.search("", 5);
        assert!(results.is_empty(), "空查询应返回空");
    }

    #[test]
    fn test_bm25_wasm_top_k_limit() {
        let searcher = Bm25Searcher::new();

        // 索引 5 个含相同词的文档
        for i in 0..5 {
            searcher.index(
                &format!("h{}", i),
                &format!("m{}", i),
                &format!("Rust 编程 测试 {}", i),
            );
        }

        let results = searcher.search("Rust", 3);
        assert_eq!(results.len(), 3, "应返回 top 3");
    }

    #[test]
    fn test_bm25_wasm_score_ordering() {
        let searcher = Bm25Searcher::new();

        // h1 含 "Rust" 1 次
        searcher.index("h1", "m1", "Rust 是编程语言");
        // h2 含 "Rust" 3 次（词频更高）
        searcher.index("h2", "m2", "Rust Rust Rust 性能优秀");

        let results = searcher.search("Rust", 2);
        assert_eq!(results.len(), 2);
        // h2 词频更高，应排第一
        assert_eq!(results[0].hook_id, "h2");
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn test_tokenizer_wasm_mixed_content() {
        let tokenizer = Tokenizer::new();

        // 中英文混合
        let tokens = tokenizer.tokenize("Rust 是一门 systems programming 语言");
        assert!(tokens.contains(&"rust".to_string()));
        assert!(tokens.contains(&"systems".to_string()));
        assert!(tokens.contains(&"programming".to_string()));
        // 中文按单字切分
        assert!(tokens.contains(&"语".to_string()));
        assert!(tokens.contains(&"言".to_string()));
        // "是" 是停用词，应被过滤
        assert!(!tokens.contains(&"是".to_string()));
    }

    #[test]
    fn test_bm25_wasm_concurrent_safe() {
        use std::sync::Arc;
        use std::thread;

        let searcher = Arc::new(Bm25Searcher::new());

        let mut handles = Vec::new();
        for i in 0..4 {
            let s = searcher.clone();
            handles.push(thread::spawn(move || {
                for j in 0..10 {
                    s.index(
                        &format!("h-{}-{}", i, j),
                        &format!("m-{}-{}", i, j),
                        &format!("并发测试 Rust {}", j),
                    );
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(searcher.len(), 40);

        // 并发搜索
        let results = searcher.search("Rust", 5);
        assert!(!results.is_empty());
    }
}

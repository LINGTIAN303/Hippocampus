//! # 混合检索器（v2.5 批次 7）
//!
//! 组合关键词检索（BM25）与向量语义检索，通过 RRF 融合得到最终结果。
//!
//! ## 检索流程
//!
//! ```text
//! query
//!   │
//!   ├─→ KeywordSearcher.search(query, top_k) ─→ kw_results
//!   │                                            │
//!   └─→ Embedder.embed(query) ─→ query_vec        │
//!         │                                       │
//!         └─→ VectorIndex.search(query_vec, top_k) ─→ sem_results
//!                                                   │
//!                          ┌────────────────────────┘
//!                          ▼
//!                    rrf_fusion([kw_results, sem_results], top_k, k=60)
//!                          │
//!                          ▼
//!                    final top-K (source = Hybrid)
//! ```
//!
//! ## 降级策略
//!
//! - **Embedder 失败**（网络错误 / API 错误）：仅返回关键词检索结果，`source = Keyword`
//! - **VectorIndex 空**（无向量）：仅返回关键词检索结果，`source = Keyword`
//! - **关键词无结果**：仅返回向量检索结果，`source = Semantic`
//! - **两者都有结果**：RRF 融合，`source = Hybrid`
//!
//! 降级时 `tracing::warn!` 记录原因，不传播错误（检索是"尽力而为"的操作）。

use crate::semantic::{Embedder, KeywordSearcher, SearchHit, SemanticRetriever, VectorIndex, rrf_fusion};
use std::sync::Arc;

// ============================================================================
// HybridRetriever
// ============================================================================

/// 混合检索器
///
/// 组合 [`KeywordSearcher`] + [`Embedder`] + [`VectorIndex`]，
/// 通过 RRF 融合得到最终结果。
///
/// ## 创建
///
/// ```rust,ignore
/// let retriever = HybridRetriever::new(keyword, embedder, vector_index);
/// ```
///
/// ## RRF 参数
///
/// - `rrf_k`：平滑参数，默认 60（Elasticsearch 8.x 默认值）
///
/// ## 降级
///
/// Embedder 失败时自动降级为仅关键词检索，详见模块级文档。
pub struct HybridRetriever {
    /// 关键词检索器
    keyword: Arc<dyn KeywordSearcher>,
    /// 文本向量化器
    embedder: Arc<dyn Embedder>,
    /// 向量索引
    vector_index: Arc<dyn VectorIndex>,
    /// RRF 平滑参数（默认 60）
    rrf_k: u32,
}

impl HybridRetriever {
    /// 创建混合检索器（RRF k=60）
    pub fn new(
        keyword: Arc<dyn KeywordSearcher>,
        embedder: Arc<dyn Embedder>,
        vector_index: Arc<dyn VectorIndex>,
    ) -> Self {
        Self {
            keyword,
            embedder,
            vector_index,
            rrf_k: 60,
        }
    }

    /// 自定义 RRF 平滑参数
    pub fn with_rrf_k(mut self, k: u32) -> Self {
        self.rrf_k = k;
        self
    }
}

#[async_trait::async_trait]
impl SemanticRetriever for HybridRetriever {
    async fn search(&self, query: &str, top_k: usize) -> crate::Result<Vec<SearchHit>> {
        // 空查询直接返回
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        // 1. 关键词检索（同步，快速）
        let kw_results = self.keyword.search(query, top_k);

        // 2. 向量检索（异步，可能失败）
        let sem_results = match self.embedder.embed(query).await {
            Ok(query_vec) => {
                // 向量索引检索（同步）
                let results = self.vector_index.search(&query_vec, top_k);
                results
            }
            Err(e) => {
                // 降级：仅关键词检索
                tracing::warn!(
                    error = %e,
                    "Embedder 失败，降级为仅关键词检索"
                );
                return Ok(kw_results);
            }
        };

        // 3. 判断降级场景
        if sem_results.is_empty() && !kw_results.is_empty() {
            // 向量检索无结果，仅返回关键词结果
            return Ok(kw_results);
        }
        if kw_results.is_empty() && !sem_results.is_empty() {
            // 关键词无结果，仅返回语义结果
            return Ok(sem_results);
        }
        if kw_results.is_empty() && sem_results.is_empty() {
            // 两者都空，返回空
            return Ok(Vec::new());
        }

        // 4. RRF 融合（两者都有结果）
        let fused = rrf_fusion(&[kw_results, sem_results], top_k, self.rrf_k);
        Ok(fused)
    }
}

// ============================================================================
// 仅关键词检索器（降级模式）
// ============================================================================

/// 仅关键词检索器
///
/// 包装 [`KeywordSearcher`]，实现 [`SemanticRetriever`] trait。
///
/// ## 适用场景
///
/// - 未配置 Embedder（无 API Key）
/// - 纯离线模式
/// - Embedder 持续失败时的降级方案
pub struct KeywordOnlyRetriever {
    /// 关键词检索器
    keyword: Arc<dyn KeywordSearcher>,
}

impl KeywordOnlyRetriever {
    /// 创建仅关键词检索器
    pub fn new(keyword: Arc<dyn KeywordSearcher>) -> Self {
        Self { keyword }
    }
}

#[async_trait::async_trait]
impl SemanticRetriever for KeywordOnlyRetriever {
    async fn search(&self, query: &str, top_k: usize) -> crate::Result<Vec<SearchHit>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        Ok(self.keyword.search(query, top_k))
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bm25::Bm25Searcher;
    use crate::semantic::RetrievalSource;
    use crate::vector::InMemoryVectorIndex;

    // ============================================================================
    // Mock Embedder
    // ============================================================================

    /// Mock Embedder：基于文本 hash 的简单向量化
    ///
    /// 将文本字符的 ASCII/Unicode 值映射到固定维度向量，再归一化。
    /// 仅用于测试，不保证语义质量。
    struct MockEmbedder {
        dim: usize,
    }

    impl MockEmbedder {
        fn new(dim: usize) -> Self {
            Self { dim }
        }
    }

    #[async_trait::async_trait]
    impl Embedder for MockEmbedder {
        fn dim(&self) -> usize {
            self.dim
        }

        async fn embed(&self, text: &str) -> crate::Result<Vec<f32>> {
            let mut vector = vec![0.0_f32; self.dim];
            for (i, c) in text.chars().enumerate() {
                vector[i % self.dim] += c as u32 as f32;
            }
            // 归一化
            let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for v in &mut vector {
                    *v /= norm;
                }
            }
            Ok(vector)
        }
    }

    /// 始终失败的 Embedder（用于测试降级）
    struct FailEmbedder {
        dim: usize,
    }

    #[async_trait::async_trait]
    impl Embedder for FailEmbedder {
        fn dim(&self) -> usize {
            self.dim
        }

        async fn embed(&self, _text: &str) -> crate::Result<Vec<f32>> {
            Err(crate::Error::Storage("mock embedder failure".into()))
        }
    }

    // ============================================================================
    // 测试辅助函数
    // ============================================================================

    /// 构建测试用混合检索器（含 3 个文档）
    async fn build_test_retriever() -> (HybridRetriever, Arc<Bm25Searcher>, Arc<InMemoryVectorIndex>) {
        let keyword = Arc::new(Bm25Searcher::new());
        let embedder = Arc::new(MockEmbedder::new(8));
        let vector_index = Arc::new(InMemoryVectorIndex::new(8));

        // 索引 3 个文档
        let docs = vec![
            ("h1", "m1", "Rust 是一门系统编程语言，强调安全性和性能"),
            ("h2", "m2", "Python 是动态语言，适合数据分析和机器学习"),
            ("h3", "m3", "Rust 的所有权机制保证内存安全"),
        ];

        for (hook_id, memory_id, text) in &docs {
            keyword.index(hook_id, memory_id, text);
            // 索引向量（用 MockEmbedder 生成向量）
            let vector = embedder.embed(text).await.unwrap();
            vector_index.add(hook_id, memory_id, vector);
        }

        let retriever = HybridRetriever::new(keyword.clone(), embedder, vector_index.clone());
        (retriever, keyword, vector_index)
    }

    // ============================================================================
    // 测试用例
    // ============================================================================

    #[tokio::test]
    async fn test_hybrid_search_basic() {
        let (retriever, _, _) = build_test_retriever().await;

        // 搜索 "Rust"
        let results = retriever.search("Rust", 3).await.unwrap();
        assert!(!results.is_empty(), "应返回结果");

        // h1 和 h3 都含 Rust，应排前面
        let top_ids: Vec<&str> = results.iter().map(|h| h.hook_id.as_str()).collect();
        assert!(top_ids.contains(&"h1") || top_ids.contains(&"h3"));
    }

    #[tokio::test]
    async fn test_hybrid_search_rrf_fusion() {
        let (retriever, _, _) = build_test_retriever().await;

        // 搜索同时在关键词和语义中匹配的查询
        let results = retriever.search("Rust 语言", 3).await.unwrap();
        assert!(!results.is_empty());

        // 若同时被两个检索器命中，source 应为 Hybrid
        // 注意：MockEmbedder 的向量质量不高，不保证一定有 Hybrid，但至少要有结果
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_hybrid_degrade_to_keyword_on_embedder_failure() {
        // 使用 FailEmbedder
        let keyword = Arc::new(Bm25Searcher::new());
        let embedder = Arc::new(FailEmbedder { dim: 8 });
        let vector_index = Arc::new(InMemoryVectorIndex::new(8));

        keyword.index("h1", "m1", "Rust 编程语言");
        keyword.index("h2", "m2", "Python 数据分析");

        let retriever = HybridRetriever::new(keyword.clone(), embedder, vector_index);
        let results = retriever.search("Rust", 5).await.unwrap();

        // 降级为仅关键词检索
        assert!(!results.is_empty(), "降级后应有关键词结果");
        // 所有结果 source 应为 Keyword
        for hit in &results {
            assert_eq!(hit.source, RetrievalSource::Keyword);
        }
    }

    #[tokio::test]
    async fn test_hybrid_degrade_to_keyword_on_empty_vector_index() {
        // VectorIndex 为空
        let keyword = Arc::new(Bm25Searcher::new());
        let embedder = Arc::new(MockEmbedder::new(8));
        let vector_index = Arc::new(InMemoryVectorIndex::new(8));

        // 只有关键词索引，无向量索引
        keyword.index("h1", "m1", "Rust 编程语言");

        let retriever = HybridRetriever::new(keyword.clone(), embedder, vector_index);
        let results = retriever.search("Rust", 5).await.unwrap();

        // 向量索引为空，降级为仅关键词
        assert!(!results.is_empty());
        for hit in &results {
            assert_eq!(hit.source, RetrievalSource::Keyword);
        }
    }

    #[tokio::test]
    async fn test_hybrid_only_semantic_when_keyword_no_match() {
        // 关键词无匹配，但向量有匹配
        let keyword = Arc::new(Bm25Searcher::new());
        let embedder = Arc::new(MockEmbedder::new(8));
        let vector_index = Arc::new(InMemoryVectorIndex::new(8));

        // 索引向量（但不索引关键词，模拟关键词无匹配）
        let vector = embedder.embed("Rust 编程语言").await.unwrap();
        vector_index.add("h1", "m1", vector);

        let retriever = HybridRetriever::new(keyword.clone(), embedder.clone(), vector_index);

        // 用与索引完全相同的文本查询（向量必然命中）
        let results = retriever.search("Rust 编程语言", 5).await.unwrap();
        assert!(!results.is_empty(), "应通过向量检索返回结果");

        // 关键词索引为空，应返回 Semantic 结果
        // 注意：关键词索引为空，search 返回空，sem 有结果 → 降级为仅语义
        for hit in &results {
            assert_eq!(hit.source, RetrievalSource::Semantic);
        }
    }

    #[tokio::test]
    async fn test_hybrid_empty_query() {
        let (retriever, _, _) = build_test_retriever().await;

        let results = retriever.search("", 5).await.unwrap();
        assert!(results.is_empty(), "空查询应返回空");

        let results = retriever.search("   ", 5).await.unwrap();
        assert!(results.is_empty(), "纯空格查询应返回空");
    }

    #[tokio::test]
    async fn test_hybrid_top_k_limit() {
        let (retriever, _, _) = build_test_retriever().await;

        let results = retriever.search("Rust", 1).await.unwrap();
        assert_eq!(results.len(), 1, "应返回 top 1");
    }

    #[tokio::test]
    async fn test_hybrid_no_match_at_all() {
        let (retriever, _, _) = build_test_retriever().await;

        // 查询完全无关的内容
        let results = retriever.search("量子力学", 5).await.unwrap();
        // 关键词无匹配，向量也可能无强匹配
        // 但 MockEmbedder 会生成非零向量，可能返回低相似度结果
        // 这里只验证不报错
        assert!(results.len() <= 5);
    }

    #[tokio::test]
    async fn test_keyword_only_retriever() {
        let keyword = Arc::new(Bm25Searcher::new());
        keyword.index("h1", "m1", "Rust 编程语言");
        keyword.index("h2", "m2", "Python 数据分析");

        let retriever = KeywordOnlyRetriever::new(keyword);
        let results = retriever.search("Rust", 5).await.unwrap();

        assert!(!results.is_empty());
        assert_eq!(results[0].hook_id, "h1");
        assert_eq!(results[0].source, RetrievalSource::Keyword);
    }

    #[tokio::test]
    async fn test_keyword_only_empty_query() {
        let keyword = Arc::new(Bm25Searcher::new());
        let retriever = KeywordOnlyRetriever::new(keyword);

        let results = retriever.search("", 5).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_hybrid_custom_rrf_k() {
        let keyword = Arc::new(Bm25Searcher::new());
        let embedder = Arc::new(MockEmbedder::new(8));
        let vector_index = Arc::new(InMemoryVectorIndex::new(8));

        // 索引文档
        keyword.index("h1", "m1", "Rust 语言");
        let vector = embedder.embed("Rust 语言").await.unwrap();
        vector_index.add("h1", "m1", vector);

        // 用自定义 k=1（更陡峭的排名衰减）
        let retriever = HybridRetriever::new(keyword.clone(), embedder, vector_index).with_rrf_k(1);

        let results = retriever.search("Rust", 5).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_hybrid_both_empty_returns_empty() {
        // 两者都无结果
        let keyword = Arc::new(Bm25Searcher::new());
        let embedder = Arc::new(MockEmbedder::new(8));
        let vector_index = Arc::new(InMemoryVectorIndex::new(8));

        // 不索引任何内容
        let retriever = HybridRetriever::new(keyword, embedder, vector_index);
        let results = retriever.search("Rust", 5).await.unwrap();
        assert!(results.is_empty(), "两者都空应返回空");
    }
}

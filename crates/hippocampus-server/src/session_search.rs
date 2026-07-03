//! # Session 级索引隔离路由器（v2.8）+ LRU/TTL 内存管理（v2.9）+ 严格 LRU 模式（v2.14）
//!
//! 解决 v2.5 批次 7 遗留的全局单例问题：BM25 索引和向量索引原为全局共享，
//! 任意 session 的 /search 会返回其他 session 的结果。
//!
//! ## 架构
//!
//! ```text
//! archive handler                          search handler
//!   │                                        │
//!   └─→ SessionSearchRouter.index_hook(sid, hook)
//!         │                                  └─→ SessionSearchRouter.search(sid, query, top_k)
//!         │                                        │
//!         ├─→ 获取/创建 sid 的 SessionIndices      └─→ 获取/创建 sid 的 SessionIndices
//!         │                                        │
//!         ├─→ keyword.index(hook_id, text)         └─→ retriever.search(query, top_k)
//!         └─→ embedder.embed → vector.add
//! ```
//!
//! ## 隔离策略
//!
//! - 每个 session_id 拥有独立的 `Bm25Searcher` + `InMemoryVectorIndex`
//! - 索引和查询完全隔离，不跨 session 返回结果
//! - session 首次访问时懒加载创建索引器
//! - 未配置 Embedder 时降级为 `KeywordOnlyRetriever`
//!
//! ## 内存管理
//!
//! ### TinyLFU 模式（默认，v2.9）
//!
//! - **LRU 淘汰**：session 数量超过 `max_sessions` 时，淘汰最久未访问的 session
//! - **TTL 过期**：session 索引在 `session_ttl` 时间内未被访问则自动释放
//! - 默认：`max_sessions = 1000`，`session_ttl = 1 小时`
//! - 底层使用 `moka::dash::Cache`，无锁并发 + 异步清理
//! - TinyLFU 是频率敏感的准入策略，高频访问的更易保留（不是严格 LRU）
//!
//! ### StrictLRU 模式（v2.14 新增）
//!
//! - **严格 LRU 淘汰**：最久未访问的 session 优先淘汰
//! - **不支持 TTL**：仅按访问顺序淘汰，不支持时间过期
//! - 底层使用 `lru::LruCache` + `tokio::sync::Mutex`，简单确定性
//! - 适用于需要确定性淘汰策略的场景（如测试、严格容量控制）
//! - 通过 [`SessionSearchRouterConfig::eviction_policy`] 切换

use hippocampus_core::bm25::Bm25Searcher;
use hippocampus_core::hybrid::{HybridRetriever, KeywordOnlyRetriever};
use hippocampus_core::model::IndexHook;
use hippocampus_core::semantic::{
    Embedder, KeywordSearcher, SearchHit, SemanticRetriever, VectorIndex,
};
use hippocampus_core::vector::InMemoryVectorIndex;
use moka::future::Cache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

// ============================================================================
// SessionIndices：单个 session 的索引器集合
// ============================================================================

/// 单个 session 的索引器集合
///
/// 每个 session 独立持有：
/// - `keyword`：BM25 关键词索引器（写入 + 查询共享）
/// - `vector`：向量索引器（写入 + 查询共享，未配置 Embedder 时为 None）
/// - `retriever`：语义检索器（Hybrid 或 KeywordOnly，内部共享同一组 keyword/vector）
///
/// v2.9：派生 Clone 以适配 moka 缓存（Arc clone 廉价，无需深拷贝）
#[derive(Clone)]
struct SessionIndices {
    /// 关键词索引器（index_hook 写入 + retriever 查询共享）
    keyword: Arc<dyn KeywordSearcher>,
    /// 向量索引器（index_hook 写入 + retriever 查询共享，降级模式为 None）
    vector: Option<Arc<dyn VectorIndex>>,
    /// 语义检索器（Hybrid 或 KeywordOnly）
    retriever: Arc<dyn SemanticRetriever>,
}

// ============================================================================
// EvictionPolicy：驱逐策略（v2.14 新增）
// ============================================================================

/// Session 索引缓存驱逐策略（v2.14 新增）
///
/// 控制 [`SessionSearchRouter`] 内部缓存的淘汰行为。
///
/// ## 策略对比
///
/// | 策略 | 底层实现 | LRU 淘汰 | TTL 过期 | 并发模型 | 适用场景 |
/// |------|---------|---------|---------|---------|---------|
/// | `TinyLFU`（默认） | moka | 近似（频率敏感） | 支持 | 无锁 | 高并发生产环境 |
/// | `StrictLRU` | lru crate | 严格 LRU | 不支持 | Mutex | 确定性淘汰、测试 |
///
/// ## 选择建议
///
/// - **默认用 `TinyLFU`**：moka 无锁并发性能好，TinyLFU 在大多数场景下命中率优于严格 LRU
/// - **测试或需要确定性淘汰时用 `StrictLRU`**：严格按访问顺序淘汰，行为可预测
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionPolicy {
    /// TinyLFU（moka 默认）：频率敏感的准入策略，高频访问的更易保留
    ///
    /// - 支持 LRU 淘汰（`max_sessions`）+ TTL 过期（`session_ttl`）
    /// - 无锁并发，高吞吐
    /// - TinyLFU 不是严格 LRU，新插入的 session 可能被拒绝准入
    TinyLFU,
    /// 严格 LRU：最久未访问的优先淘汰
    ///
    /// - 仅支持 LRU 淘汰（`max_sessions`），不支持 TTL 过期
    /// - `tokio::sync::Mutex` 保护，简单确定性
    /// - 适用于需要确定性淘汰策略的场景（如测试、严格容量控制）
    StrictLRU,
}

impl Default for EvictionPolicy {
    fn default() -> Self {
        Self::TinyLFU
    }
}

// ============================================================================
// SessionSearchRouterConfig：配置
// ============================================================================

/// Session 索引路由器配置（v2.9 新增，v2.14 扩展）
///
/// 控制 LRU 淘汰上限、TTL 过期时长和驱逐策略。
///
/// ## 默认值
///
/// - `max_sessions = 1000`：最多缓存 1000 个 session 的索引
/// - `session_ttl = 1 小时`：session 索引空闲 1 小时后自动释放（仅 TinyLFU 模式生效）
/// - `dim = 0`：向量维度（与 embedder 配合）
/// - `eviction_policy = TinyLFU`：驱逐策略（v2.14 新增）
#[derive(Debug, Clone)]
pub struct SessionSearchRouterConfig {
    /// 最大 session 数（LRU 上限，超过则淘汰最久未访问的）
    pub max_sessions: usize,
    /// 单个 session 的空闲 TTL（自最后一次访问起算）
    ///
    /// 仅 `EvictionPolicy::TinyLFU` 模式生效，`StrictLRU` 模式忽略此字段。
    pub session_ttl: Duration,
    /// 向量维度（embedder 存在时使用）
    pub dim: usize,
    /// 驱逐策略（v2.14 新增，默认 `TinyLFU`）
    pub eviction_policy: EvictionPolicy,
}

impl Default for SessionSearchRouterConfig {
    fn default() -> Self {
        Self {
            max_sessions: 1000,
            session_ttl: Duration::from_secs(3600), // 1 小时
            dim: 0,
            eviction_policy: EvictionPolicy::default(),
        }
    }
}

// ============================================================================
// SessionCache：内部缓存实现（v2.14 新增，enum 包装两种策略）
// ============================================================================

/// Session 缓存实现（v2.14 新增）
///
/// 根据 [`EvictionPolicy`] 选择底层缓存：
///
/// - `TinyLFU`：`moka::future::Cache`，无锁并发 + LRU + TTL
/// - `StrictLRU`：`lru::LruCache` + `tokio::sync::Mutex`，严格 LRU
enum SessionCache {
    /// TinyLFU 模式（moka）：支持 LRU + TTL，无锁并发
    TinyLFU(Cache<String, SessionIndices>),
    /// 严格 LRU 模式（lru crate）：仅 LRU 淘汰，Mutex 保护
    StrictLRU(Mutex<lru::LruCache<String, SessionIndices>>),
}

// ============================================================================
// SessionSearchRouter
// ============================================================================

/// Session 级索引隔离路由器
///
/// 按 session_id 路由到独立的子索引器，实现 session 间完全隔离。
/// 替代 v2.5 的全局单例 `SearchIndexer` + `SemanticRetriever`。
///
/// ## 内存管理
///
/// ### TinyLFU 模式（默认，v2.9）
///
/// - **LRU 淘汰**：session 数量超过 `max_sessions` 时，自动淘汰最久未访问的 session
/// - **TTL 过期**：session 索引在 `session_ttl` 时间内未被访问则自动释放
/// - 底层使用 `moka::dash::Cache`，无锁并发 + 异步清理
/// - TinyLFU 是频率敏感的准入策略，高频访问的更易保留（不是严格 LRU）
///
/// ### StrictLRU 模式（v2.14 新增）
///
/// - **严格 LRU 淘汰**：最久未访问的 session 优先淘汰
/// - **不支持 TTL**：仅按访问顺序淘汰
/// - 底层使用 `lru::LruCache` + `tokio::sync::Mutex`
/// - 适用于需要确定性淘汰策略的场景
///
/// ## 创建
///
/// 通常由 `main.rs` 从环境变量构造，注入到 `AppState.session_search`：
///
/// ```rust,ignore
/// // 默认配置（TinyLFU, max=1000, ttl=1h）
/// let router = SessionSearchRouter::new(Some(embedder), dim);
///
/// // 自定义配置（TinyLFU）
/// let router = SessionSearchRouter::with_config(
///     Some(embedder),
///     SessionSearchRouterConfig {
///         max_sessions: 500,
///         session_ttl: Duration::from_secs(1800),
///         dim,
///         eviction_policy: EvictionPolicy::TinyLFU,
///     },
/// );
///
/// // 严格 LRU 模式（v2.14）
/// let router = SessionSearchRouter::with_config(
///     Some(embedder),
///     SessionSearchRouterConfig {
///         max_sessions: 500,
///         session_ttl: Duration::from_secs(3600), // StrictLRU 忽略此字段
///         dim,
///         eviction_policy: EvictionPolicy::StrictLRU,
///     },
/// );
/// ```
pub struct SessionSearchRouter {
    /// Embedder（可选，None 时降级为仅关键词检索）
    embedder: Option<Arc<dyn Embedder>>,
    /// 向量维度（embedder 存在时使用）
    dim: usize,
    /// session → 独立索引器集合（根据 eviction_policy 选择底层实现）
    sessions: SessionCache,
}

impl SessionSearchRouter {
    /// 创建 Session 级索引路由器（默认配置：TinyLFU 模式）
    ///
    /// - `embedder`：文本向量化器（None 时降级为仅关键词检索）
    /// - `dim`：向量维度（embedder 存在时使用）
    ///
    /// 默认：`max_sessions = 1000`，`session_ttl = 1 小时`，`eviction_policy = TinyLFU`
    pub fn new(embedder: Option<Arc<dyn Embedder>>, dim: usize) -> Self {
        Self::with_config(
            embedder,
            SessionSearchRouterConfig {
                dim,
                ..Default::default()
            },
        )
    }

    /// 创建 Session 级索引路由器（自定义配置）
    ///
    /// v2.9 新增，支持自定义 LRU 上限和 TTL 时长。
    /// v2.14 扩展，支持 `eviction_policy` 选择 TinyLFU 或 StrictLRU。
    pub fn with_config(
        embedder: Option<Arc<dyn Embedder>>,
        config: SessionSearchRouterConfig,
    ) -> Self {
        let sessions = match config.eviction_policy {
            EvictionPolicy::TinyLFU => {
                let cache = Cache::builder()
                    .max_capacity(config.max_sessions as u64)
                    .time_to_idle(config.session_ttl)
                    .build();
                SessionCache::TinyLFU(cache)
            }
            EvictionPolicy::StrictLRU => {
                // max_sessions 至少为 1（NonZeroUsize 要求）
                let cap = NonZeroUsize::new(config.max_sessions.max(1))
                    .expect("max_sessions 至少为 1");
                let cache = lru::LruCache::new(cap);
                SessionCache::StrictLRU(Mutex::new(cache))
            }
        };
        Self {
            embedder,
            dim: config.dim,
            sessions,
        }
    }

    /// 获取或创建指定 session 的索引器集合
    ///
    /// 首次访问时懒加载创建独立的 keyword + vector + retriever。
    /// `KeywordSearcher` 和 `VectorIndex` 在 indexer（写入）与 retriever（查询）间共享 Arc。
    ///
    /// - TinyLFU 模式：moka 的 `try_get_with`（async），原子性避免重复创建
    /// - StrictLRU 模式：Mutex 保护，`get` 更新访问顺序，`put` 自动淘汰 LRU 端
    async fn get_or_create(&self, sid: &str) -> SessionIndices {
        match &self.sessions {
            SessionCache::TinyLFU(cache) => {
                // clone embedder/dim 以便 move 进异步闭包
                let embedder = self.embedder.clone();
                let dim = self.dim;
                cache
                    .try_get_with(sid.to_string(), async move {
                        Ok::<SessionIndices, std::convert::Infallible>(
                            Self::create_indices(&embedder, dim),
                        )
                    })
                    .await
                    .expect("Infallible 不会失败")
                    .clone()
            }
            SessionCache::StrictLRU(cache) => {
                let mut guard = cache.lock().await;
                // get 会更新访问顺序（移到 MRU 端）
                if let Some(indices) = guard.get(sid) {
                    return indices.clone();
                }
                // 首次访问：创建并插入
                // put 在容量满时自动淘汰 LRU 端（最久未访问的）
                let indices = Self::create_indices(&self.embedder, self.dim);
                guard.put(sid.to_string(), indices.clone());
                indices
            }
        }
    }

    /// 创建单个 session 的索引器集合（无 IO，纯内存构造）
    fn create_indices(
        embedder: &Option<Arc<dyn Embedder>>,
        dim: usize,
    ) -> SessionIndices {
        let keyword: Arc<dyn KeywordSearcher> = Arc::new(Bm25Searcher::new());

        let (vector, retriever): (
            Option<Arc<dyn VectorIndex>>,
            Arc<dyn SemanticRetriever>,
        ) = match embedder {
            Some(embedder) => {
                // 完整模式：HybridRetriever（关键词 + 向量 + RRF 融合）
                let vector_index: Arc<dyn VectorIndex> = Arc::new(InMemoryVectorIndex::new(dim));
                let retriever: Arc<dyn SemanticRetriever> = Arc::new(HybridRetriever::new(
                    keyword.clone(),
                    embedder.clone(),
                    vector_index.clone(),
                ));
                (Some(vector_index), retriever)
            }
            None => {
                // 降级模式：KeywordOnlyRetriever（仅关键词）
                let retriever: Arc<dyn SemanticRetriever> =
                    Arc::new(KeywordOnlyRetriever::new(keyword.clone()));
                (None, retriever)
            }
        };

        SessionIndices {
            keyword,
            vector,
            retriever,
        }
    }

    /// 归档后触发索引（按 session 隔离）
    ///
    /// 将 hook 的摘要文本索引到该 session 的关键词索引和向量索引。
    /// Embedder 失败时跳过向量索引，不影响关键词索引。
    pub async fn index_hook(&self, sid: &str, hook: &IndexHook) {
        let text = extract_index_text(hook);
        let hook_id = hook.id.to_string();
        let memory_id = hook.memory_id.clone();

        let indices = self.get_or_create(sid).await;

        // 1. 关键词索引（必执行）
        indices.keyword.index(&hook_id, &memory_id, &text);

        // 2. 向量索引（仅当 embedder 和 vector 都存在时执行）
        if let (Some(embedder), Some(vi)) = (&self.embedder, &indices.vector) {
            match embedder.embed(&text).await {
                Ok(vector) => {
                    vi.add(&hook_id, &memory_id, vector);
                }
                Err(e) => {
                    tracing::warn!(
                        session = %sid,
                        hook_id = %hook_id,
                        error = %e,
                        "Embedder 失败，跳过向量索引（关键词索引已更新）"
                    );
                }
            }
        }

        tracing::debug!(
            session = %sid,
            hook_id = %hook_id,
            memory_id = %memory_id,
            text_len = text.len(),
            "session 索引完成"
        );
    }

    /// 语义检索（按 session 隔离）
    ///
    /// 只搜索该 session 的索引，不返回其他 session 的结果。
    pub async fn search(
        &self,
        sid: &str,
        query: &str,
        top_k: usize,
    ) -> hippocampus_core::Result<Vec<SearchHit>> {
        let indices = self.get_or_create(sid).await;
        indices.retriever.search(query, top_k).await
    }

    /// 获取已注册的 session 数量（供监控/测试）
    ///
    /// 注意：
    /// - TinyLFU 模式：moka 的 `entry_count` 是近似值，可能略高于实际有效条目。
    ///   精确数量需调用 `run_pending_tasks().await` 后再读取。
    /// - StrictLRU 模式：`lru::LruCache::len` 是精确值。
    pub fn session_count(&self) -> usize {
        match &self.sessions {
            SessionCache::TinyLFU(cache) => cache.entry_count() as usize,
            SessionCache::StrictLRU(cache) => {
                // try_lock 避免在同步上下文中阻塞
                cache.try_lock().map(|g| g.len()).unwrap_or(0)
            }
        }
    }

    /// 移除指定 session 的索引（手动清理）
    ///
    /// 返回是否成功移除（true = 之前存在）。
    ///
    /// - TinyLFU 模式：moka 的 `invalidate` 是 async
    /// - StrictLRU 模式：`lru::LruCache::pop` 是同步操作
    pub async fn remove_session(&self, sid: &str) -> bool {
        match &self.sessions {
            SessionCache::TinyLFU(cache) => {
                let existed = cache.contains_key(sid);
                cache.invalidate(sid).await;
                existed
            }
            SessionCache::StrictLRU(cache) => {
                let mut guard = cache.lock().await;
                guard.pop(sid).is_some()
            }
        }
    }

    /// 强制执行待处理的清理任务（测试用）
    ///
    /// - TinyLFU 模式：moka 的 LRU/TTL 清理是异步的，测试中需要调用本方法确保清理已执行。
    /// - StrictLRU 模式：lru 的淘汰是同步的，本方法为兼容性保留（no-op）。
    ///
    /// 生产环境无需调用。
    pub async fn run_pending_tasks(&self) {
        match &self.sessions {
            SessionCache::TinyLFU(cache) => {
                cache.run_pending_tasks().await;
            }
            SessionCache::StrictLRU(_) => {
                // lru 的淘汰是同步的，无需异步清理
            }
        }
    }
}

// ============================================================================
// 辅助函数：提取索引文本
// ============================================================================

/// 从 IndexHook 提取用于索引的文本
///
/// 组合摘要的多维信息：title + abstract + key_facts + key_entities + tags
fn extract_index_text(hook: &IndexHook) -> String {
    let mut parts: Vec<String> = Vec::new();

    parts.push(hook.summary.title.clone());

    if let Some(abs) = &hook.summary.abstract_text {
        if !abs.trim().is_empty() {
            parts.push(abs.clone());
        }
    }

    if !hook.summary.key_facts.is_empty() {
        parts.push(hook.summary.key_facts.join(" "));
    }

    if !hook.summary.key_entities.is_empty() {
        parts.push(hook.summary.key_entities.join(" "));
    }

    if !hook.tags.is_empty() {
        let tag_str: Vec<String> = hook.tags.iter().map(|t| t.to_string()).collect();
        parts.push(tag_str.join(" "));
    }

    parts.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use hippocampus_core::model::{ArchivePeriod, Summary, Tag};
    use chrono::Utc;
    use uuid::Uuid;

    // ============================================================================
    // Mock Embedder
    // ============================================================================

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

        async fn embed(&self, text: &str) -> hippocampus_core::Result<Vec<f32>> {
            let mut vector = vec![0.0_f32; self.dim];
            for (i, c) in text.chars().enumerate() {
                vector[i % self.dim] += c as u32 as f32;
            }
            let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for v in &mut vector {
                    *v /= norm;
                }
            }
            Ok(vector)
        }
    }

    // ============================================================================
    // 测试辅助
    // ============================================================================

    fn make_hook(title: &str, key_facts: Vec<String>) -> IndexHook {
        IndexHook {
            id: Uuid::new_v4(),
            memory_id: format!("mem-{}", Uuid::new_v4()),
            summary: Summary {
                title: title.to_string(),
                abstract_text: None,
                key_facts,
                key_entities: Vec::new(),
                clue_anchors: Vec::new(),
            },
            tags: vec![Tag::Text],
            archived_at: Utc::now(),
            period: ArchivePeriod::Daily,
            token_count: 100,
        }
    }

    // ============================================================================
    // 基础测试（兼容性回归：v2.8 行为保持不变）
    // ============================================================================

    #[test]
    fn test_extract_index_text_basic() {
        let hook = make_hook("测试标题", vec![]);
        let text = extract_index_text(&hook);
        assert!(text.contains("测试标题"));
    }

    #[test]
    fn test_router_session_count_initial() {
        let router = SessionSearchRouter::new(None, 0);
        assert_eq!(router.session_count(), 0);
    }

    #[tokio::test]
    async fn test_router_keyword_only_search() {
        // 未配置 Embedder → 降级为仅关键词检索
        let router = SessionSearchRouter::new(None, 0);

        let hook = make_hook("Rust 安全编程", vec!["所有权机制".into()]);
        router.index_hook("sess-1", &hook).await;

        let results = router.search("sess-1", "Rust", 5).await.unwrap();
        assert!(!results.is_empty(), "应能搜索到已索引的内容");
        assert_eq!(results[0].hook_id, hook.id.to_string());
    }

    #[tokio::test]
    async fn test_router_session_isolation() {
        // 核心：不同 session 的索引完全隔离
        let router = SessionSearchRouter::new(None, 0);

        let hook1 = make_hook("Rust 编程语言", vec![]);
        router.index_hook("sess-1", &hook1).await;

        let hook2 = make_hook("Python 编程语言", vec![]);
        router.index_hook("sess-2", &hook2).await;

        // session-1 搜索 "Rust" → 应找到 hook1
        let results1 = router.search("sess-1", "Rust", 5).await.unwrap();
        assert!(!results1.is_empty(), "sess-1 应找到 Rust");
        assert_eq!(results1[0].hook_id, hook1.id.to_string());

        // session-1 搜索 "Python" → 不应找到 hook2（隔离）
        let results1_py = router.search("sess-1", "Python", 5).await.unwrap();
        assert!(
            results1_py.is_empty()
                || !results1_py.iter().any(|r| r.hook_id == hook2.id.to_string()),
            "sess-1 不应搜到 sess-2 的 Python 内容"
        );

        // session-2 搜索 "Python" → 应找到 hook2
        let results2 = router.search("sess-2", "Python", 5).await.unwrap();
        assert!(!results2.is_empty(), "sess-2 应找到 Python");
        assert_eq!(results2[0].hook_id, hook2.id.to_string());

        // session-2 搜索 "Rust" → 不应找到 hook1（隔离）
        let results2_rs = router.search("sess-2", "Rust", 5).await.unwrap();
        assert!(
            results2_rs.is_empty()
                || !results2_rs.iter().any(|r| r.hook_id == hook1.id.to_string()),
            "sess-2 不应搜到 sess-1 的 Rust 内容"
        );
    }

    #[tokio::test]
    async fn test_router_session_count_after_index() {
        let router = SessionSearchRouter::new(None, 0);

        router
            .index_hook("sess-a", &make_hook("标题A", vec![]))
            .await;
        router
            .index_hook("sess-b", &make_hook("标题B", vec![]))
            .await;
        router
            .index_hook("sess-a", &make_hook("标题A2", vec![]))
            .await;

        router.run_pending_tasks().await;
        assert_eq!(
            router.session_count(),
            2,
            "应有 2 个 session（a, b）"
        );
    }

    #[tokio::test]
    async fn test_router_with_embedder() {
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(8));
        let router = SessionSearchRouter::new(Some(embedder), 8);

        let hook = make_hook("Rust 安全编程", vec![]);
        router.index_hook("sess-1", &hook).await;

        let results = router.search("sess-1", "Rust", 5).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_router_remove_session() {
        let router = SessionSearchRouter::new(None, 0);

        router
            .index_hook("sess-1", &make_hook("标题", vec![]))
            .await;
        router.run_pending_tasks().await;
        assert_eq!(router.session_count(), 1);

        assert!(router.remove_session("sess-1").await);
        router.run_pending_tasks().await;
        assert_eq!(router.session_count(), 0);

        // 移除后重新搜索 → 应返回空（新建空索引）
        let results = router.search("sess-1", "标题", 5).await.unwrap();
        assert!(results.is_empty(), "移除后重建索引应为空");
    }

    #[tokio::test]
    async fn test_router_multiple_hooks_same_session() {
        let router = SessionSearchRouter::new(None, 0);

        for i in 0..3 {
            let hook = make_hook(&format!("文档 {}", i), vec![]);
            router.index_hook("sess-1", &hook).await;
        }

        let results = router.search("sess-1", "文档", 10).await.unwrap();
        assert_eq!(results.len(), 3, "应找到 3 个文档");
    }

    // ============================================================================
    // v2.9 新增：LRU + TTL 测试
    // ============================================================================

    #[tokio::test]
    async fn test_router_lru_eviction() {
        // max_sessions = 2，插入 3 个 session 后应淘汰最久未访问的
        // TinyLFU 模式（moka）：频率敏感的准入策略
        let router = SessionSearchRouter::with_config(
            None,
            SessionSearchRouterConfig {
                max_sessions: 2,
                session_ttl: Duration::from_secs(3600), // 长 TTL，只测 LRU
                dim: 0,
                eviction_policy: EvictionPolicy::TinyLFU,
            },
        );

        router
            .index_hook("sess-1", &make_hook("标题1", vec![]))
            .await;
        router
            .index_hook("sess-2", &make_hook("标题2", vec![]))
            .await;
        router
            .index_hook("sess-3", &make_hook("标题3", vec![]))
            .await;

        // 强制执行清理任务
        router.run_pending_tasks().await;

        // 由于 max=2，sess-1（最久未访问）应被淘汰
        let count = router.session_count();
        assert!(
            count <= 2,
            "session_count 应不超过 max_sessions=2，实际 {}",
            count
        );

        // sess-2 和 sess-3 应仍可搜索到内容
        let results2 = router.search("sess-2", "标题2", 5).await.unwrap();
        assert!(
            !results2.is_empty(),
            "sess-2 应仍可搜索（LRU 保留最近访问）"
        );

        let results3 = router.search("sess-3", "标题3", 5).await.unwrap();
        assert!(
            !results3.is_empty(),
            "sess-3 应仍可搜索（最新访问）"
        );
    }

    #[tokio::test]
    async fn test_router_ttl_expiry() {
        // session_ttl = 100ms，等待 200ms 后应被清理
        // TinyLFU 模式（moka）支持 TTL 过期
        let router = SessionSearchRouter::with_config(
            None,
            SessionSearchRouterConfig {
                max_sessions: 1000, // 足够大，只测 TTL
                session_ttl: Duration::from_millis(100),
                dim: 0,
                eviction_policy: EvictionPolicy::TinyLFU,
            },
        );

        router
            .index_hook("sess-1", &make_hook("标题", vec![]))
            .await;
        router.run_pending_tasks().await;
        assert_eq!(router.session_count(), 1);

        // 等待 TTL 过期
        tokio::time::sleep(Duration::from_millis(200)).await;
        router.run_pending_tasks().await;

        assert_eq!(
            router.session_count(),
            0,
            "TTL 过期后 session 应被清理"
        );
    }

    #[tokio::test]
    async fn test_router_default_config_compatible() {
        // 验证 new() 默认配置可用（max=1000, ttl=1h）
        let router = SessionSearchRouter::new(None, 0);

        router
            .index_hook("sess-default", &make_hook("默认配置", vec![]))
            .await;
        router.run_pending_tasks().await;

        assert_eq!(router.session_count(), 1);

        let results = router
            .search("sess-default", "默认", 5)
            .await
            .unwrap();
        assert!(!results.is_empty(), "默认配置应可正常搜索");
    }

    #[tokio::test]
    async fn test_router_config_default_impl() {
        // 验证 SessionSearchRouterConfig::default()
        let config = SessionSearchRouterConfig::default();
        assert_eq!(config.max_sessions, 1000);
        assert_eq!(config.session_ttl, Duration::from_secs(3600));
        assert_eq!(config.dim, 0);
        // v2.14：默认驱逐策略为 TinyLFU
        assert_eq!(config.eviction_policy, EvictionPolicy::TinyLFU);
    }

    #[test]
    fn test_eviction_policy_default() {
        // v2.14：EvictionPolicy::default() == TinyLFU
        assert_eq!(EvictionPolicy::default(), EvictionPolicy::TinyLFU);
    }

    #[tokio::test]
    async fn test_router_remove_session_returns_false_for_nonexistent() {
        // 移除不存在的 session 应返回 false
        let router = SessionSearchRouter::new(None, 0);
        assert!(
            !router.remove_session("never-exists").await,
            "移除不存在的 session 应返回 false"
        );
    }

    #[tokio::test]
    async fn test_router_lru_keeps_frequently_accessed() {
        // moka 使用 TinyLFU（频率敏感的准入策略），高频访问的 session 更容易被保留
        // 本测试验证：多次访问的 sess-1 在容量压力下仍能保留
        let router = SessionSearchRouter::with_config(
            None,
            SessionSearchRouterConfig {
                max_sessions: 2,
                session_ttl: Duration::from_secs(3600),
                dim: 0,
                eviction_policy: EvictionPolicy::TinyLFU,
            },
        );

        router
            .index_hook("sess-1", &make_hook("标题1", vec![]))
            .await;
        router
            .index_hook("sess-2", &make_hook("标题2", vec![]))
            .await;

        // 多次访问 sess-1（提高其在 TinyLFU 中的频率）
        for _ in 0..5 {
            let _ = router.search("sess-1", "标题1", 5).await.unwrap();
        }

        // 插入 sess-3（触发容量压力）
        router
            .index_hook("sess-3", &make_hook("标题3", vec![]))
            .await;
        router.run_pending_tasks().await;

        // sess-1 多次访问过，应被 TinyLFU 保留
        let results1 = router.search("sess-1", "标题1", 5).await.unwrap();
        assert!(
            !results1.is_empty(),
            "sess-1 多次访问过，应被 TinyLFU 保留"
        );
    }

    // ============================================================================
    // v2.14 新增：StrictLRU 模式测试
    // ============================================================================

    #[tokio::test]
    async fn test_router_strict_lru_eviction() {
        // StrictLRU 模式：max_sessions=2，插入 3 个 session 后，
        // 最久未访问的 sess-1 应被**严格**淘汰（与 TinyLFU 的频率敏感行为不同）
        let router = SessionSearchRouter::with_config(
            None,
            SessionSearchRouterConfig {
                max_sessions: 2,
                session_ttl: Duration::from_secs(3600), // StrictLRU 忽略此字段
                dim: 0,
                eviction_policy: EvictionPolicy::StrictLRU,
            },
        );

        router
            .index_hook("sess-1", &make_hook("标题1", vec![]))
            .await;
        router
            .index_hook("sess-2", &make_hook("标题2", vec![]))
            .await;

        // 访问 sess-1，使其成为最近访问（MRU 端）
        // 这样 sess-2 成为 LRU 端
        let _ = router.search("sess-1", "标题1", 5).await.unwrap();

        // 插入 sess-3（触发容量压力）
        // 严格 LRU 应淘汰 sess-2（最久未访问）
        router
            .index_hook("sess-3", &make_hook("标题3", vec![]))
            .await;

        // StrictLRU 淘汰是同步的，无需 run_pending_tasks
        assert_eq!(
            router.session_count(),
            2,
            "StrictLRU 应严格保持 max_sessions=2"
        );

        // sess-1 是最近访问过的，应保留
        let results1 = router.search("sess-1", "标题1", 5).await.unwrap();
        assert!(
            !results1.is_empty(),
            "sess-1 最近访问过，应被 StrictLRU 保留"
        );

        // sess-3 是最新插入的，应保留
        let results3 = router.search("sess-3", "标题3", 5).await.unwrap();
        assert!(
            !results3.is_empty(),
            "sess-3 最新插入，应被 StrictLRU 保留"
        );

        // sess-2 是最久未访问的，应被严格淘汰
        // 淘汰后重新 search 会创建新的空索引，返回空结果
        let results2 = router.search("sess-2", "标题2", 5).await.unwrap();
        assert!(
            results2.is_empty(),
            "sess-2 应被 StrictLRU 严格淘汰（淘汰后重建为空索引）"
        );
    }

    #[tokio::test]
    async fn test_router_strict_lru_no_ttl() {
        // StrictLRU 模式不支持 TTL 过期
        // 即使设置了短 TTL，session 在 max_sessions 范围内也不会因超时被清理
        let router = SessionSearchRouter::with_config(
            None,
            SessionSearchRouterConfig {
                max_sessions: 1000, // 足够大，不触发 LRU 淘汰
                session_ttl: Duration::from_millis(100), // 短 TTL，但 StrictLRU 应忽略
                dim: 0,
                eviction_policy: EvictionPolicy::StrictLRU,
            },
        );

        router
            .index_hook("sess-1", &make_hook("标题", vec![]))
            .await;
        assert_eq!(router.session_count(), 1);

        // 等待远超 TTL 的时长
        tokio::time::sleep(Duration::from_millis(300)).await;

        // StrictLRU 不支持 TTL，session 应仍存在
        assert_eq!(
            router.session_count(),
            1,
            "StrictLRU 模式不支持 TTL，session 应仍存在"
        );

        // 且仍可搜索到内容（未被清理）
        let results = router.search("sess-1", "标题", 5).await.unwrap();
        assert!(
            !results.is_empty(),
            "StrictLRU 模式下 session 内容应保留（不受 TTL 影响）"
        );
    }

    #[tokio::test]
    async fn test_router_strict_lru_remove_session() {
        // StrictLRU 模式下移除 session
        let router = SessionSearchRouter::with_config(
            None,
            SessionSearchRouterConfig {
                max_sessions: 100,
                session_ttl: Duration::from_secs(3600),
                dim: 0,
                eviction_policy: EvictionPolicy::StrictLRU,
            },
        );

        router
            .index_hook("sess-1", &make_hook("标题", vec![]))
            .await;
        assert_eq!(router.session_count(), 1);

        // 移除存在的 session → 返回 true
        assert!(
            router.remove_session("sess-1").await,
            "移除存在的 session 应返回 true"
        );
        assert_eq!(router.session_count(), 0);

        // 移除不存在的 session → 返回 false
        assert!(
            !router.remove_session("sess-1").await,
            "移除不存在的 session 应返回 false"
        );
    }

    #[tokio::test]
    async fn test_router_strict_lru_session_isolation() {
        // StrictLRU 模式下验证 session 隔离（与 TinyLFU 行为一致）
        let router = SessionSearchRouter::with_config(
            None,
            SessionSearchRouterConfig {
                max_sessions: 100,
                session_ttl: Duration::from_secs(3600),
                dim: 0,
                eviction_policy: EvictionPolicy::StrictLRU,
            },
        );

        let hook1 = make_hook("Rust 编程", vec![]);
        let hook2 = make_hook("Python 编程", vec![]);

        router.index_hook("sess-1", &hook1).await;
        router.index_hook("sess-2", &hook2).await;

        // 隔离验证：sess-1 搜索 Rust 应找到 hook1
        let r1 = router.search("sess-1", "Rust", 5).await.unwrap();
        assert!(!r1.is_empty(), "sess-1 应找到 Rust");
        assert_eq!(r1[0].hook_id, hook1.id.to_string());

        // 隔离验证：sess-1 搜索 Python 不应找到 hook2
        let r1_py = router.search("sess-1", "Python", 5).await.unwrap();
        assert!(
            r1_py.is_empty()
                || !r1_py.iter().any(|r| r.hook_id == hook2.id.to_string()),
            "StrictLRU 模式下 sess-1 不应搜到 sess-2 的内容"
        );
    }

    #[tokio::test]
    async fn test_router_strict_lru_with_embedder() {
        // StrictLRU 模式 + Embedder 完整模式验证
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(8));
        let router = SessionSearchRouter::with_config(
            Some(embedder),
            SessionSearchRouterConfig {
                max_sessions: 100,
                session_ttl: Duration::from_secs(3600),
                dim: 8,
                eviction_policy: EvictionPolicy::StrictLRU,
            },
        );

        let hook = make_hook("Rust 安全编程", vec!["所有权".into()]);
        router.index_hook("sess-1", &hook).await;

        let results = router.search("sess-1", "Rust", 5).await.unwrap();
        assert!(
            !results.is_empty(),
            "StrictLRU + Embedder 模式应可正常搜索"
        );
        assert_eq!(results[0].hook_id, hook.id.to_string());
    }
}

//! # SQLite 向量索引（v2.5 批次 7）
//!
//! SQLite BLOB 持久化 + 内存缓存的向量索引实现。
//!
//! ## 设计
//!
//! - **持久化**：向量以 BLOB（小端字节序）存储在 SQLite `embeddings` 表
//! - **内存缓存**：查询时走 [`InMemoryVectorIndex`]（O(N) 暴力扫描，但内存命中快）
//! - **写穿透**：`add` / `remove` / `clear` 同时写 SQLite 和内存
//! - **启动加载**：构造时从 SQLite 加载所有向量到内存
//!
//! ## 表结构
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS embeddings (
//!     hook_id    TEXT PRIMARY KEY,
//!     memory_id  TEXT NOT NULL,
//!     vector     BLOB NOT NULL
//! );
//! ```
//!
//! ## 向量序列化
//!
//! `Vec<f32>` ↔ `Vec<u8>`（小端字节序，每个 f32 占 4 字节）
//!
//! ## 适用场景
//!
//! - 需要持久化的语义检索（重启后无需重新 embed）
//! - 记忆数 < 10K（内存可容纳）
//! - 单机部署

use crate::semantic::{SearchHit, VectorIndex};
use crate::vector::InMemoryVectorIndex;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::path::PathBuf;

/// SQL 初始化语句
const INIT_SQL: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;

CREATE TABLE IF NOT EXISTS embeddings (
    hook_id    TEXT PRIMARY KEY,
    memory_id  TEXT NOT NULL,
    vector     BLOB NOT NULL
);
"#;

/// SQLite 向量索引（持久化 + 内存缓存）
///
/// 实现 [`VectorIndex`] trait，内部组合 SQLite BLOB 持久化 + [`InMemoryVectorIndex`] 缓存。
///
/// ## 创建
///
/// ```rust,ignore
/// let index = SqliteVectorIndex::new("./data/embeddings.db", 1536)?;
/// ```
///
/// ## 写穿透
///
/// - `add`：INSERT OR REPLACE + 内存 add
/// - `remove`：DELETE + 内存 remove
/// - `clear`：DELETE ALL + 内存 clear
///
/// 查询走内存缓存，保证检索速度。
pub struct SqliteVectorIndex {
    /// r2d2 连接池
    pool: Pool<SqliteConnectionManager>,
    /// 向量维度
    dim: usize,
    /// 内存缓存（查询走这里）
    cache: InMemoryVectorIndex,
}

impl std::fmt::Debug for SqliteVectorIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteVectorIndex")
            .field("dim", &self.dim)
            .field("pool_state", &self.pool.state())
            .field("cache_len", &self.cache.len())
            .finish()
    }
}

impl SqliteVectorIndex {
    /// 创建新的 SQLite 向量索引
    ///
    /// - `db_path`：数据库文件路径（如 `./data/embeddings.db`）
    /// - `dim`：向量维度（如 1536）
    ///
    /// 启动时会从 SQLite 加载所有已有向量到内存缓存。
    pub fn new(db_path: impl Into<PathBuf>, dim: usize) -> crate::Result<Self> {
        let db_path = db_path.into();

        // 确保父目录存在
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                crate::Error::Storage(format!("创建数据库目录失败: {}", e))
            })?;
        }

        let manager = SqliteConnectionManager::file(&db_path)
            .with_init(|c| c.execute_batch(INIT_SQL));

        let pool = Pool::builder()
            .max_size(4) // 向量索引写少读多，4 个连接足够
            .build(manager)
            .map_err(|e| crate::Error::Storage(format!("创建连接池失败: {}", e)))?;

        let cache = InMemoryVectorIndex::new(dim);

        let index = Self {
            pool,
            dim,
            cache,
        };

        // 从 SQLite 加载已有向量到内存
        index.load_from_sqlite()?;

        Ok(index)
    }

    /// 从 SQLite 加载所有向量到内存缓存
    fn load_from_sqlite(&self) -> crate::Result<()> {
        let conn = self
            .pool
            .get()
            .map_err(|e| crate::Error::Storage(format!("获取连接失败: {}", e)))?;

        let mut stmt = conn
            .prepare("SELECT hook_id, memory_id, vector FROM embeddings")
            .map_err(|e| crate::Error::Storage(format!("查询 embeddings 失败: {}", e)))?;

        let rows: Vec<(String, String, Vec<u8>)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(|e| crate::Error::Storage(format!("查询 embeddings 失败: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        let count = rows.len();
        for (hook_id, memory_id, vector_bytes) in rows {
            let vector = bytes_to_vector(&vector_bytes);
            // 直接写入内存缓存（不走 add，避免重复写 SQLite）
            self.cache.add(&hook_id, &memory_id, vector);
        }

        if count > 0 {
            tracing::info!(count, "从 SQLite 加载向量到内存缓存");
        }

        Ok(())
    }

    /// 获取内存缓存中的向量数量（用于测试）
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }
}

impl VectorIndex for SqliteVectorIndex {
    fn add(&self, hook_id: &str, memory_id: &str, vector: Vec<f32>) {
        // 维度校验
        if vector.len() != self.dim {
            tracing::warn!(
                hook_id,
                memory_id,
                expected = self.dim,
                got = vector.len(),
                "向量维度不匹配，忽略此条目"
            );
            return;
        }

        // 序列化为 BLOB
        let vector_bytes = vector_to_bytes(&vector);

        // 写入 SQLite
        match self.pool.get() {
            Ok(conn) => {
                if let Err(e) = conn.execute(
                    "INSERT OR REPLACE INTO embeddings (hook_id, memory_id, vector) VALUES (?1, ?2, ?3)",
                    params![hook_id, memory_id, vector_bytes],
                ) {
                    tracing::error!(hook_id, error = %e, "写入 embeddings 失败");
                }
            }
            Err(e) => {
                tracing::error!(hook_id, error = %e, "获取连接失败");
            }
        }

        // 写入内存缓存
        self.cache.add(hook_id, memory_id, vector);
    }

    fn search(&self, query: &[f32], top_k: usize) -> Vec<SearchHit> {
        // 查询走内存缓存
        self.cache.search(query, top_k)
    }

    fn remove(&self, hook_id: &str) {
        // 从 SQLite 删除
        match self.pool.get() {
            Ok(conn) => {
                if let Err(e) = conn.execute(
                    "DELETE FROM embeddings WHERE hook_id = ?1",
                    params![hook_id],
                ) {
                    tracing::error!(hook_id, error = %e, "删除 embedding 失败");
                }
            }
            Err(e) => {
                tracing::error!(hook_id, error = %e, "获取连接失败");
            }
        }

        // 从内存缓存删除
        self.cache.remove(hook_id);
    }

    fn len(&self) -> usize {
        self.cache.len()
    }

    fn clear(&self) {
        // 清空 SQLite
        match self.pool.get() {
            Ok(conn) => {
                if let Err(e) = conn.execute("DELETE FROM embeddings", []) {
                    tracing::error!(error = %e, "清空 embeddings 失败");
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "获取连接失败");
            }
        }

        // 清空内存缓存
        self.cache.clear();
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

// ============================================================================
// 向量序列化（f32 数组 ↔ BLOB）
// ============================================================================

/// 将 f32 向量序列化为小端字节序 BLOB
///
/// 每个 f32 占 4 字节，总长度 = dim * 4。
fn vector_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for f in v {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

/// 将 BLOB 反序列化为 f32 向量
///
/// 每个 f32 占 4 字节，长度 = bytes.len() / 4。
fn bytes_to_vector(bytes: &[u8]) -> Vec<f32> {
    let dim = bytes.len() / 4;
    let mut vector = Vec::with_capacity(dim);
    for i in 0..dim {
        let offset = i * 4;
        if offset + 4 <= bytes.len() {
            let arr: [u8; 4] = bytes[offset..offset + 4].try_into().unwrap_or([0; 4]);
            vector.push(f32::from_le_bytes(arr));
        }
    }
    vector
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::RetrievalSource;
    use tempfile::TempDir;

    /// 创建临时数据库路径
    fn temp_db_path() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("embeddings.db");
        (dir, path)
    }

    #[test]
    fn test_vector_serialization_roundtrip() {
        let original = vec![1.0_f32, -2.5, 3.14, 0.0, -0.001];
        let bytes = vector_to_bytes(&original);
        assert_eq!(bytes.len(), original.len() * 4);

        let restored = bytes_to_vector(&bytes);
        assert_eq!(restored.len(), original.len());
        for (a, b) in original.iter().zip(restored.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn test_vector_serialization_empty() {
        let bytes = vector_to_bytes(&[]);
        assert!(bytes.is_empty());
        let v = bytes_to_vector(&bytes);
        assert!(v.is_empty());
    }

    #[test]
    fn test_sqlite_vector_index_basic() {
        let (_dir, path) = temp_db_path();
        let index = SqliteVectorIndex::new(&path, 3).unwrap();

        index.add("h1", "m1", vec![1.0, 0.0, 0.0]);
        index.add("h2", "m2", vec![0.0, 1.0, 0.0]);

        assert_eq!(index.len(), 2);

        let results = index.search(&[1.0, 0.0, 0.0], 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hook_id, "h1");
        assert_eq!(results[0].source, RetrievalSource::Semantic);
    }

    #[test]
    fn test_sqlite_vector_index_persistence() {
        let (_dir, path) = temp_db_path();

        // 第一次创建：写入数据
        {
            let index = SqliteVectorIndex::new(&path, 3).unwrap();
            index.add("h1", "m1", vec![1.0, 0.0, 0.0]);
            index.add("h2", "m2", vec![0.0, 1.0, 0.0]);
            assert_eq!(index.len(), 2);
        }

        // 第二次创建：从 SQLite 加载
        {
            let index = SqliteVectorIndex::new(&path, 3).unwrap();
            assert_eq!(index.len(), 2, "重启后应从 SQLite 加载向量");

            let results = index.search(&[1.0, 0.0, 0.0], 5);
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].hook_id, "h1");
        }
    }

    #[test]
    fn test_sqlite_vector_index_remove() {
        let (_dir, path) = temp_db_path();
        let index = SqliteVectorIndex::new(&path, 3).unwrap();

        index.add("h1", "m1", vec![1.0, 0.0, 0.0]);
        index.add("h2", "m2", vec![0.0, 1.0, 0.0]);

        index.remove("h1");
        assert_eq!(index.len(), 1);

        // 验证 SQLite 也已删除
        drop(index);
        let index2 = SqliteVectorIndex::new(&path, 3).unwrap();
        assert_eq!(index2.len(), 1, "SQLite 中 h1 应已删除");
    }

    #[test]
    fn test_sqlite_vector_index_clear() {
        let (_dir, path) = temp_db_path();
        let index = SqliteVectorIndex::new(&path, 3).unwrap();

        index.add("h1", "m1", vec![1.0, 0.0, 0.0]);
        index.add("h2", "m2", vec![0.0, 1.0, 0.0]);

        index.clear();
        assert!(index.is_empty());

        // 验证 SQLite 也已清空
        drop(index);
        let index2 = SqliteVectorIndex::new(&path, 3).unwrap();
        assert!(index2.is_empty(), "SQLite 应已清空");
    }

    #[test]
    fn test_sqlite_vector_index_reindex_same_hook() {
        let (_dir, path) = temp_db_path();
        let index = SqliteVectorIndex::new(&path, 3).unwrap();

        // 同一 hook_id 重新索引应覆盖
        index.add("h1", "m1", vec![1.0, 0.0, 0.0]);
        index.add("h1", "m1", vec![0.0, 1.0, 0.0]);

        assert_eq!(index.len(), 1);

        // 验证 SQLite 也已覆盖
        drop(index);
        let index2 = SqliteVectorIndex::new(&path, 3).unwrap();
        assert_eq!(index2.len(), 1);

        let results = index2.search(&[0.0, 1.0, 0.0], 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hook_id, "h1");
    }

    #[test]
    fn test_sqlite_vector_index_dim_mismatch_ignored() {
        let (_dir, path) = temp_db_path();
        let index = SqliteVectorIndex::new(&path, 3).unwrap();

        // 维度不符应被忽略
        index.add("h1", "m1", vec![1.0, 0.0]); // dim=2 != 3
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_sqlite_vector_index_dim() {
        let (_dir, path) = temp_db_path();
        let index = SqliteVectorIndex::new(&path, 1536).unwrap();
        assert_eq!(index.dim(), 1536);
    }

    #[test]
    fn test_sqlite_vector_index_empty_db() {
        let (_dir, path) = temp_db_path();
        let index = SqliteVectorIndex::new(&path, 3).unwrap();

        // 空数据库应正常工作
        assert!(index.is_empty());
        let results = index.search(&[1.0, 0.0, 0.0], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_sqlite_vector_index_batch_add() {
        let (_dir, path) = temp_db_path();
        let index = SqliteVectorIndex::new(&path, 3).unwrap();

        let items = vec![
            ("h1".into(), "m1".into(), vec![1.0, 0.0, 0.0]),
            ("h2".into(), "m2".into(), vec![0.0, 1.0, 0.0]),
            ("h3".into(), "m3".into(), vec![0.0, 0.0, 1.0]),
        ];
        index.add_batch(items);

        assert_eq!(index.len(), 3);

        // 验证持久化
        drop(index);
        let index2 = SqliteVectorIndex::new(&path, 3).unwrap();
        assert_eq!(index2.len(), 3);
    }

    #[test]
    fn test_sqlite_vector_index_high_dim() {
        let (_dir, path) = temp_db_path();
        let dim = 1536;
        let index = SqliteVectorIndex::new(&path, dim).unwrap();

        let mut v1 = vec![0.0; dim];
        v1[0] = 1.0;
        index.add("h1", "m1", v1.clone());

        let results = index.search(&v1, 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hook_id, "h1");
    }
}

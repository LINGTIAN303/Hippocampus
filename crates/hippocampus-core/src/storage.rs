//! # 存储模块
//!
//! 可插拔存储后端 trait。
//!
//! ## 设计
//!
//! - [`Storage`] trait：存储后端接口，可插拔
//! - [`LocalStorage`]：默认实现，本地文件树结构
//! - **并发策略**：RwLock 串行化写操作（读可并发）
//! - **原子写入**：temp + rename，防止崩溃导致文件损坏
//! - **索引追加**：读-改-写（read → add_hook → write back）
//!
//! ## 记忆库文件树结构
//!
//! ```text
//! memory_store/
//! ├── sessions/
//! │   └── {session_id}/
//! │       ├── daily/
//! │       │   ├── 2026-07-02_143052.json   # 天级记忆文件（日期_时间戳）
//! │       │   └── 2026-07-02_150230.json
//! │       ├── weekly/
//! │       │   └── 2026-W27.json           # 周级合并文件（ISO 周数）
//! │       ├── monthly/
//! │       │   └── 2026-07.json              # 月级主记忆文件
//! │       └── index/
//! │           ├── daily_index.json         # 天级索引文档
//! │           ├── weekly_index.json        # 周级索引文档
//! │           └── monthly_index.json       # 月级索引文档
//! └── projects/
//!     └── {project_id}/
//!         └── ... (同 sessions 结构)
//! ```
//!
//! ## 路径约定
//!
//! - 所有 `write_*` 方法返回**相对路径**（POSIX 分隔符 `/`，跨平台一致）
//! - 所有 `read_*` / `delete_*` 方法接受相对路径
//! - `read_index` / `list_memories` 按 session_id + period 查找（无需路径）

use crate::model::{ArchivePeriod, IndexDocument, IndexHook, MemoryFile};
use chrono::{Datelike, NaiveDateTime};
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

/// 存储后端 trait
///
/// 所有存储后端（本地文件树、SQLite、S3 等）需实现此 trait。
/// 设计为单写多读：写入操作串行化，读取操作可并发。
#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    /// 写入记忆文件，返回相对路径
    async fn write_memory(&self, file: &MemoryFile) -> crate::Result<String>;

    /// 读取记忆文件（按相对路径）
    async fn read_memory(&self, path: &str) -> crate::Result<MemoryFile>;

    /// 删除记忆文件
    async fn delete_memory(&self, path: &str) -> crate::Result<()>;

    /// 写入索引文档（全量覆盖写）
    async fn write_index(&self, doc: &IndexDocument) -> crate::Result<String>;

    /// 读取索引文档（按 session + period 查找）
    ///
    /// 返回 `Ok(None)` 表示文档不存在
    async fn read_index(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
    ) -> crate::Result<Option<IndexDocument>>;

    /// 追加钩子到索引文档（读-改-写便利方法）
    ///
    /// 内部实现：读取现有索引文档 → 追加钩子 → 写回。
    /// 若文档不存在则创建新的。
    async fn append_hook(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
        hook: IndexHook,
    ) -> crate::Result<()>;

    /// 列出指定会话/项目下某周期层级的所有记忆文件路径
    async fn list_memories(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
    ) -> crate::Result<Vec<String>>;
}

/// 本地文件树存储后端
///
/// 将记忆文件以 JSON 格式存储在本地文件系统中。
/// 文件树结构见模块文档。
///
/// ## 并发
///
/// 内部用 [`RwLock`] 串行化写操作，读操作无锁可并发。
/// 跨进程并发需由调用方保证（如文件锁）。
pub struct LocalStorage {
    /// 根目录路径
    root: PathBuf,
    /// 写操作串行化锁（读操作无需获取）
    write_lock: RwLock<()>,
}

impl LocalStorage {
    /// 创建新的本地存储后端
    ///
    /// 注意：不会立即创建根目录，延迟到首次写入时创建。
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            write_lock: RwLock::new(()),
        }
    }

    /// 根目录
    pub fn root(&self) -> &Path {
        &self.root
    }

    // ========================================================================
    // 路径生成（纯函数，无 IO）
    // ========================================================================

    /// 生成 scope 根目录（sessions/{id} 或 projects/{id}）
    fn scope_dir(&self, session_id: &str, project_id: Option<&str>) -> PathBuf {
        if let Some(pid) = project_id {
            PathBuf::from("projects").join(pid)
        } else {
            PathBuf::from("sessions").join(session_id)
        }
    }

    /// 生成某周期层级的目录路径（相对）
    fn period_dir(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
    ) -> PathBuf {
        self.scope_dir(session_id, project_id)
            .join(period.as_dir_name())
    }

    /// 生成记忆文件的相对路径
    fn memory_relative_path(&self, file: &MemoryFile) -> PathBuf {
        let dir = self.period_dir(&file.session_id, file.project_id.as_deref(), file.period);
        dir.join(self.memory_filename(file))
    }

    /// 生成记忆文件名
    ///
    /// - Daily: `{YYYY-MM-DD}_{HHMMSS}_{mmm}.json`（日期+秒级时间戳+毫秒，避免并发冲突）
    /// - Weekly: `{YYYY}-W{WW}.json`（ISO 周数）
    /// - Monthly: `{YYYY}-{MM}.json`
    ///
    /// # 毫秒精度的理由
    ///
    /// 秒级精度在快速连续归档场景（如单元测试、批量回填）下会冲突覆盖。
    /// 毫秒精度足以区分正常归档节奏，且可在文件名中保留可读性。
    fn memory_filename(&self, file: &MemoryFile) -> String {
        let dt: NaiveDateTime = file.archived_at.naive_utc();
        match file.period {
            ArchivePeriod::Daily => format!("{}.json", dt.format("%Y-%m-%d_%H%M%S_%3f")),
            ArchivePeriod::Weekly => {
                let iso = file.archived_at.iso_week();
                format!("{:04}-W{:02}.json", iso.year(), iso.week())
            }
            ArchivePeriod::Monthly => format!("{:04}-{:02}.json", dt.year(), dt.month()),
        }
    }

    /// 生成索引文档的相对路径
    fn index_relative_path(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
    ) -> PathBuf {
        self.scope_dir(session_id, project_id)
            .join("index")
            .join(format!("{}_index.json", period.as_dir_name()))
    }

    /// 拼接根目录得到绝对路径
    fn abs_path(&self, relative: &Path) -> PathBuf {
        self.root.join(relative)
    }

    /// 将相对路径转换为 POSIX 分隔符字符串（跨平台一致）
    fn to_posix_string(relative: &Path) -> String {
        relative.to_string_lossy().replace('\\', "/")
    }

    // ========================================================================
    // IO 辅助方法
    // ========================================================================

    /// 确保目标文件的父目录存在
    async fn ensure_parent_dir(&self, path: &Path) -> crate::Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| crate::Error::Storage(format!("创建目录失败 {:?}: {}", parent, e)))?;
        }
        Ok(())
    }

    /// 原子写入（temp + rename）
    ///
    /// 流程：写入 `{filename}.tmp` → rename 到目标路径
    /// rename 在 Windows/Linux/macOS 上均原子替换目标文件
    async fn atomic_write(&self, path: &Path, content: &[u8]) -> crate::Result<()> {
        let tmp_name = format!(
            "{}.tmp",
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("tmp")
        );
        let tmp_path = path.with_file_name(tmp_name);

        // 写入临时文件
        tokio::fs::write(&tmp_path, content)
            .await
            .map_err(|e| crate::Error::Storage(format!("写入临时文件失败 {:?}: {}", tmp_path, e)))?;

        // 原子 rename（覆盖目标）
        tokio::fs::rename(&tmp_path, path)
            .await
            .map_err(|e| crate::Error::Storage(format!("重命名失败 {:?} → {:?}: {}", tmp_path, path, e)))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl Storage for LocalStorage {
    async fn write_memory(&self, file: &MemoryFile) -> crate::Result<String> {
        let _guard = self.write_lock.write().await;

        let relative = self.memory_relative_path(file);
        let abs = self.abs_path(&relative);
        self.ensure_parent_dir(&abs).await?;

        let json = serde_json::to_vec_pretty(file)
            .map_err(|e| crate::Error::Serialize(format!("序列化 MemoryFile 失败: {}", e)))?;

        self.atomic_write(&abs, &json).await?;

        Ok(Self::to_posix_string(&relative))
    }

    async fn read_memory(&self, path: &str) -> crate::Result<MemoryFile> {
        let abs = self.root.join(path);
        let content = tokio::fs::read(&abs)
            .await
            .map_err(|e| crate::Error::Storage(format!("读取记忆文件失败 {:?}: {}", path, e)))?;

        serde_json::from_slice(&content)
            .map_err(|e| crate::Error::Serialize(format!("反序列化 MemoryFile 失败: {}", e)))
    }

    async fn delete_memory(&self, path: &str) -> crate::Result<()> {
        let _guard = self.write_lock.write().await;
        let abs = self.root.join(path);
        tokio::fs::remove_file(&abs)
            .await
            .map_err(|e| crate::Error::Storage(format!("删除记忆文件失败 {:?}: {}", path, e)))?;
        Ok(())
    }

    async fn write_index(&self, doc: &IndexDocument) -> crate::Result<String> {
        let _guard = self.write_lock.write().await;

        let relative = self.index_relative_path(&doc.session_id, doc.project_id.as_deref(), doc.period);
        let abs = self.abs_path(&relative);
        self.ensure_parent_dir(&abs).await?;

        let json = serde_json::to_vec_pretty(doc)
            .map_err(|e| crate::Error::Serialize(format!("序列化 IndexDocument 失败: {}", e)))?;

        self.atomic_write(&abs, &json).await?;

        Ok(Self::to_posix_string(&relative))
    }

    async fn read_index(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
    ) -> crate::Result<Option<IndexDocument>> {
        let relative = self.index_relative_path(session_id, project_id, period);
        let abs = self.abs_path(&relative);

        match tokio::fs::read(&abs).await {
            Ok(content) => {
                let doc: IndexDocument = serde_json::from_slice(&content)
                    .map_err(|e| crate::Error::Serialize(format!("反序列化 IndexDocument 失败: {}", e)))?;
                Ok(Some(doc))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(crate::Error::Storage(format!(
                "读取索引文档失败 {:?}: {}",
                path_display(&relative),
                e
            ))),
        }
    }

    async fn append_hook(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
        hook: IndexHook,
    ) -> crate::Result<()> {
        let _guard = self.write_lock.write().await;

        let relative = self.index_relative_path(session_id, project_id, period);
        let abs = self.abs_path(&relative);

        // 读-改-写
        let mut doc: IndexDocument = match tokio::fs::read(&abs).await {
            Ok(content) => serde_json::from_slice(&content).map_err(|e| {
                crate::Error::Serialize(format!("反序列化 IndexDocument 失败: {}", e))
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // 文档不存在则创建新的
                IndexDocument::new(session_id.to_string(), project_id.map(String::from), period)
            }
            Err(e) => {
                return Err(crate::Error::Storage(format!(
                    "读取索引文档失败 {:?}: {}",
                    path_display(&relative),
                    e
                )))
            }
        };

        doc.add_hook(hook);

        let json = serde_json::to_vec_pretty(&doc)
            .map_err(|e| crate::Error::Serialize(format!("序列化 IndexDocument 失败: {}", e)))?;

        self.ensure_parent_dir(&abs).await?;
        self.atomic_write(&abs, &json).await?;

        Ok(())
    }

    async fn list_memories(
        &self,
        session_id: &str,
        project_id: Option<&str>,
        period: ArchivePeriod,
    ) -> crate::Result<Vec<String>> {
        let relative = self.period_dir(session_id, project_id, period);
        let abs = self.abs_path(&relative);

        let mut entries = match tokio::fs::read_dir(&abs).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(crate::Error::Storage(format!(
                    "读取目录失败 {:?}: {}",
                    path_display(&relative),
                    e
                )))
            }
        };

        let mut paths = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| crate::Error::Storage(format!("遍历目录失败: {}", e)))?
        {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("json") {
                // 转为相对路径（POSIX 分隔符）
                let rel = p
                    .strip_prefix(&self.root)
                    .map_err(|e| crate::Error::Storage(format!("路径截取失败: {}", e)))?;
                paths.push(Self::to_posix_string(rel));
            }
        }

        paths.sort();
        Ok(paths)
    }
}

/// 路径显示辅助（用于错误信息）
fn path_display(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ArchivePeriod, IndexDocument, IndexHook, MemoryFile, MessageContent, MessageTurn, Tag,
    };
    use chrono::Utc;
    use tempfile::TempDir;
    use uuid::Uuid;

    /// 构造测试用的 MemoryFile
    fn make_test_memory(period: ArchivePeriod, session_id: &str) -> MemoryFile {
        let turn = MessageTurn {
            id: Uuid::new_v4(),
            user_message: MessageContent {
                text: Some("用户问：如何实现一个记忆库？".into()),
                attachments: Vec::new(),
                tool_calls: Vec::new(),
                thinking: None,
            },
            llm_message: MessageContent {
                text: Some("LLM 答：可以通过归档+索引+检索三级机制实现...".into()),
                attachments: Vec::new(),
                tool_calls: Vec::new(),
                thinking: None,
            },
            tags: vec![Tag::Text, Tag::CodeBlock],
            timestamp: Utc::now(),
            token_count: 100,
        };
        MemoryFile::new(session_id, None, vec![turn], period)
    }

    /// 构造测试用的 MemoryFile（带 project_id）
    fn make_test_memory_with_project(period: ArchivePeriod) -> MemoryFile {
        let mut file = make_test_memory(period, "test-session");
        file.project_id = Some("proj-001".into());
        file
    }

    #[tokio::test]
    async fn test_write_and_read_memory() {
        let tmp = TempDir::new().unwrap();
        let storage = LocalStorage::new(tmp.path());

        let original = make_test_memory(ArchivePeriod::Daily, "sess-001");
        let path = storage.write_memory(&original).await.unwrap();

        // 验证返回的相对路径（POSIX 分隔符）
        assert!(path.contains("sessions/sess-001/daily/"));
        assert!(path.ends_with(".json"));
        assert!(!path.contains('\\'));

        // 读回验证
        let read_back = storage.read_memory(&path).await.unwrap();
        assert_eq!(read_back.session_id, "sess-001");
        assert_eq!(read_back.period, ArchivePeriod::Daily);
        assert_eq!(read_back.turns.len(), 1);
        assert_eq!(read_back.total_tokens, 100);
        assert!(read_back.tags.contains(&Tag::Text));
        assert!(read_back.tags.contains(&Tag::CodeBlock));
    }

    #[tokio::test]
    async fn test_read_memory_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let storage = LocalStorage::new(tmp.path());

        let result = storage.read_memory("nonexistent.json").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, crate::Error::Storage(_)));
    }

    #[tokio::test]
    async fn test_delete_memory() {
        let tmp = TempDir::new().unwrap();
        let storage = LocalStorage::new(tmp.path());

        let file = make_test_memory(ArchivePeriod::Daily, "sess-del");
        let path = storage.write_memory(&file).await.unwrap();

        // 删除
        storage.delete_memory(&path).await.unwrap();

        // 再读应失败
        let result = storage.read_memory(&path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_memories() {
        let tmp = TempDir::new().unwrap();
        let storage = LocalStorage::new(tmp.path());

        // 写入 3 个 daily 记忆文件
        for _ in 0..3 {
            let file = make_test_memory(ArchivePeriod::Daily, "sess-list");
            // 加一点延迟避免时间戳冲突
            tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
            storage.write_memory(&file).await.unwrap();
        }

        let paths = storage
            .list_memories("sess-list", None, ArchivePeriod::Daily)
            .await
            .unwrap();
        assert_eq!(paths.len(), 3);

        // 验证路径已排序
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted);
    }

    #[tokio::test]
    async fn test_list_memories_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let storage = LocalStorage::new(tmp.path());

        // 目录不存在时返回空数组（而非错误）
        let paths = storage
            .list_memories("nonexistent-session", None, ArchivePeriod::Daily)
            .await
            .unwrap();
        assert!(paths.is_empty());
    }

    #[tokio::test]
    async fn test_append_hook_new_doc() {
        let tmp = TempDir::new().unwrap();
        let storage = LocalStorage::new(tmp.path());

        // 先写入一个记忆文件
        let file = make_test_memory(ArchivePeriod::Daily, "sess-hook");
        let memory_path = storage.write_memory(&file).await.unwrap();

        // 生成钩子并追加
        let hook = IndexHook::from_memory_file(&file, memory_path.clone());
        storage
            .append_hook("sess-hook", None, ArchivePeriod::Daily, hook)
            .await
            .unwrap();

        // 读回索引文档验证
        let doc = storage
            .read_index("sess-hook", None, ArchivePeriod::Daily)
            .await
            .unwrap();
        assert!(doc.is_some());
        let doc = doc.unwrap();
        assert_eq!(doc.hooks.len(), 1);
        assert_eq!(doc.hooks[0].memory_file_path, memory_path);
    }

    #[tokio::test]
    async fn test_append_hook_multiple() {
        let tmp = TempDir::new().unwrap();
        let storage = LocalStorage::new(tmp.path());

        // 追加 3 个钩子到同一个索引文档
        for _ in 0..3 {
            let file = make_test_memory(ArchivePeriod::Daily, "sess-multi");
            let path = storage.write_memory(&file).await.unwrap();
            let hook = IndexHook::from_memory_file(&file, path);
            tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
            storage
                .append_hook("sess-multi", None, ArchivePeriod::Daily, hook)
                .await
                .unwrap();
        }

        let doc = storage
            .read_index("sess-multi", None, ArchivePeriod::Daily)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(doc.hooks.len(), 3);
    }

    #[tokio::test]
    async fn test_read_index_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let storage = LocalStorage::new(tmp.path());

        let result = storage
            .read_index("nonexistent", None, ArchivePeriod::Daily)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_write_index_overwrite() {
        let tmp = TempDir::new().unwrap();
        let storage = LocalStorage::new(tmp.path());

        // 写入一个索引文档
        let mut doc = IndexDocument::new("sess-overwrite", None, ArchivePeriod::Weekly);
        let file = make_test_memory(ArchivePeriod::Weekly, "sess-overwrite");
        let path = storage.write_memory(&file).await.unwrap();
        doc.add_hook(IndexHook::from_memory_file(&file, path));
        storage.write_index(&doc).await.unwrap();

        // 覆盖写入
        let mut doc2 = IndexDocument::new("sess-overwrite", None, ArchivePeriod::Weekly);
        doc2.add_hook(IndexHook::from_memory_file(&file, "new-path".into()));
        storage.write_index(&doc2).await.unwrap();

        // 读回验证只剩新的钩子
        let read_back = storage
            .read_index("sess-overwrite", None, ArchivePeriod::Weekly)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(read_back.hooks.len(), 1);
        assert_eq!(read_back.hooks[0].memory_file_path, "new-path");
    }

    #[tokio::test]
    async fn test_project_id_path() {
        let tmp = TempDir::new().unwrap();
        let storage = LocalStorage::new(tmp.path());

        let file = make_test_memory_with_project(ArchivePeriod::Daily);
        let path = storage.write_memory(&file).await.unwrap();

        // 验证路径走 projects/ 而非 sessions/
        assert!(path.starts_with("projects/proj-001/daily/"));
        assert!(!path.contains("sessions/"));

        // list_memories 用 project_id 参数
        let paths = storage
            .list_memories("ignored", Some("proj-001"), ArchivePeriod::Daily)
            .await
            .unwrap();
        assert_eq!(paths.len(), 1);
    }

    #[tokio::test]
    async fn test_memory_filename_daily_format() {
        let storage = LocalStorage::new(std::path::Path::new("/tmp/test"));
        let mut file = make_test_memory(ArchivePeriod::Daily, "x");
        // 固定时间验证格式
        file.archived_at = chrono::DateTime::parse_from_rfc3339("2026-07-02T14:30:52Z")
            .unwrap()
            .with_timezone(&Utc);

        let name = storage.memory_filename(&file);
        assert_eq!(name, "2026-07-02_143052_000.json");
    }

    #[tokio::test]
    async fn test_memory_filename_weekly_format() {
        let storage = LocalStorage::new(std::path::Path::new("/tmp/test"));
        let mut file = make_test_memory(ArchivePeriod::Weekly, "x");
        // 2026-07-02 是 ISO 第 27 周
        file.archived_at = chrono::DateTime::parse_from_rfc3339("2026-07-02T14:30:52Z")
            .unwrap()
            .with_timezone(&Utc);

        let name = storage.memory_filename(&file);
        assert_eq!(name, "2026-W27.json");
    }

    #[tokio::test]
    async fn test_memory_filename_monthly_format() {
        let storage = LocalStorage::new(std::path::Path::new("/tmp/test"));
        let mut file = make_test_memory(ArchivePeriod::Monthly, "x");
        file.archived_at = chrono::DateTime::parse_from_rfc3339("2026-07-02T14:30:52Z")
            .unwrap()
            .with_timezone(&Utc);

        let name = storage.memory_filename(&file);
        assert_eq!(name, "2026-07.json");
    }

    #[tokio::test]
    async fn test_atomic_write_survives_overwrite() {
        // 验证原子写入能正确覆盖已有文件
        let tmp = TempDir::new().unwrap();
        let storage = LocalStorage::new(tmp.path());

        let file1 = make_test_memory(ArchivePeriod::Monthly, "sess-atomic");
        let path1 = storage.write_memory(&file1).await.unwrap();

        // 同一月份覆盖（monthly 文件名相同）
        let file2 = make_test_memory(ArchivePeriod::Monthly, "sess-atomic");
        let path2 = storage.write_memory(&file2).await.unwrap();

        assert_eq!(path1, path2);

        // 读回应该是 file2 的内容
        let read_back = storage.read_memory(&path2).await.unwrap();
        assert_eq!(read_back.id, file2.id);
    }
}

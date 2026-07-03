//! # 检索模块
//!
//! 混合检索机制：摘要钩子注入 + tool 主动检索。
//!
//! ## 两种检索模式
//!
//! 1. **摘要钩子注入**：将索引钩子的摘要信息（标题+标签+时间戳）注入到
//!    system prompt，让 LLM 知道"有哪些记忆"，轻量
//! 2. **Tool 主动检索**：LLM 根据需要通过 tool 调用检索详细记忆文件，
//!    返回完整上下文
//!
//! ## 分层钩子设计
//!
//! - [`IndexHook`] 包含完整信息
//! - [`SummaryView`] 是轻量摘要视图（用于注入 system prompt）
//! - 详细检索返回完整 [`MemoryFile`]
//!
//! ## 摘要来源
//!
//! 每次调用 [`Retriever::get_summaries`] 时实时从 [`Storage`] 读取所有周期
//! （daily/weekly/monthly）的索引文档，提取所有钩子转为摘要视图。
//! 保证与存储的一致性。

use crate::model::{ArchivePeriod, IndexHook, MemoryFile};
use crate::storage::Storage;
use std::sync::Arc;

/// 摘要视图（用于注入 system prompt）
///
/// v2.4 升级：暴露完整结构化摘要字段，支持分级渲染。
/// - 日级：仅 title（启发式生成）
/// - 周级：title + abstract_text + key_facts + key_entities（LLM 生成）
/// - 月级：全字段含 clue_anchors（LLM 生成）
#[derive(Debug, Clone, serde::Serialize)]
pub struct SummaryView {
    /// 钩子 ID（UUID 字符串形式）
    pub hook_id: String,
    /// 指向的记忆文件 ID（LocalStorage 为路径，SQLite 为 UUID）
    pub memory_id: String,
    /// 摘要标题（从 IndexHook.summary.title 提取，向后兼容）
    pub summary_title: String,
    /// 抽象摘要（2-3 句话，提炼主题；日级为 None）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abstract_text: Option<String>,
    /// 关键事实（事实级别，可被直接引用；日级为空）
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub key_facts: Vec<String>,
    /// 关键实体（人名/项目名/技术名词等；日级为空）
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub key_entities: Vec<String>,
    /// 线索锚点（用于检索匹配的关键词；月级才有）
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub clue_anchors: Vec<String>,
    /// 标签集合（中文显示，通过 Tag Display 转换）
    pub tags: Vec<String>,
    /// 归档时间（RFC3339）
    pub archived_at: String,
    /// 周期层级（daily/weekly/monthly）
    pub period: String,
    /// Token 数
    pub token_count: usize,
    /// 是否为高级摘要（含 abstract 或 key_facts）
    #[serde(skip)]
    pub is_rich: bool,
}

impl From<&IndexHook> for SummaryView {
    fn from(hook: &IndexHook) -> Self {
        Self {
            hook_id: hook.id.to_string(),
            memory_id: hook.memory_id.clone(),
            summary_title: hook.summary.title.clone(),
            abstract_text: hook.summary.abstract_text.clone(),
            key_facts: hook.summary.key_facts.clone(),
            key_entities: hook.summary.key_entities.clone(),
            clue_anchors: hook.summary.clue_anchors.clone(),
            tags: hook.tags.iter().map(|t| t.to_string()).collect(),
            archived_at: hook.archived_at.to_rfc3339(),
            period: hook.period.as_dir_name().to_string(),
            token_count: hook.token_count,
            is_rich: hook.summary.is_rich(),
        }
    }
}

/// 检索器
///
/// 持有 [`Storage`] 引用，从存储实时读取索引文档和记忆文件。
pub struct Retriever {
    /// 存储后端
    storage: Arc<dyn Storage>,
    /// 会话 ID
    session_id: String,
    /// 项目 ID（可选）
    project_id: Option<String>,
}

impl Retriever {
    /// 创建新的检索器
    pub fn new(
        storage: Arc<dyn Storage>,
        session_id: impl Into<String>,
        project_id: Option<String>,
    ) -> Self {
        Self {
            storage,
            session_id: session_id.into(),
            project_id,
        }
    }

    /// 获取所有周期的摘要视图（用于注入 system prompt）
    ///
    /// 实时从 Storage 读取 daily/weekly/monthly 三个周期的索引文档，
    /// 合并所有钩子转为摘要视图。
    pub async fn get_summaries(&self) -> crate::Result<Vec<SummaryView>> {
        let mut all_summaries = Vec::new();

        for period in ArchivePeriod::all() {
            // v2.4: 有 project_id 时走 project 级聚合索引（跨 session 共享）
            // 无 project_id 时走 session 级索引（隔离）
            let doc = if let Some(pid) = &self.project_id {
                self.storage.read_project_index(pid, period).await?
            } else {
                self.storage
                    .read_index(&self.session_id, None, period)
                    .await?
            };

            if let Some(doc) = doc {
                for hook in &doc.hooks {
                    all_summaries.push(SummaryView::from(hook));
                }
            }
        }

        // 按归档时间排序（旧→新）
        all_summaries.sort_by(|a, b| a.archived_at.cmp(&b.archived_at));

        Ok(all_summaries)
    }

    /// 渲染摘要视图为 system prompt 文本（v2.4 分级渲染）
    ///
    /// **分级渲染策略**：
    /// - 日级（daily）：仅标题 + 标签（轻量，避免上下文膨胀）
    /// - 周级（weekly）：标题 + abstract + key_facts + key_entities（结构化摘要）
    /// - 月级（monthly）：全字段含 clue_anchors（最详细）
    ///
    /// **高价值片段自动展开**：
    /// - 含 ToolCall/Thinking/CodeBlock 等标签的 daily 钩子，自动展开 key_facts（若有）
    /// - 高价值判定：tags 含 "工具调用"/"思考过程"/"代码块"/"文件附件"/"图片"/"视频"
    ///
    /// 格式：按周期分组，每个钩子按层级展示。
    /// 若无任何记忆，返回空字符串。
    pub async fn render_to_system_prompt(&self) -> crate::Result<String> {
        let summaries = self.get_summaries().await?;

        if summaries.is_empty() {
            return Ok(String::new());
        }

        let mut out = String::from("# 可用记忆索引\n\n");
        out.push_str("以下是可用的历史记忆摘要，可直接基于此信息回答用户问题：\n\n");

        // 高价值标签集合（自动展开判定）
        const HIGH_VALUE_TAGS: &[&str] = &[
            "工具调用",
            "思考过程",
            "代码块",
            "文件附件",
            "图片",
            "视频",
        ];

        // 按周期分组
        for period in ArchivePeriod::all() {
            let period_name = period.as_dir_name();
            let hooks: Vec<&SummaryView> = summaries
                .iter()
                .filter(|s| s.period == period_name)
                .collect();

            if hooks.is_empty() {
                continue;
            }

            let period_label = match period {
                ArchivePeriod::Daily => "近期记忆",
                ArchivePeriod::Weekly => "周度记忆",
                ArchivePeriod::Monthly => "月度记忆",
            };
            out.push_str(&format!("## {}（{}）\n\n", period_label, period_name));

            for s in hooks {
                let tags_str = if s.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", s.tags.join(", "))
                };
                out.push_str(&format!(
                    "- **{}**{}（{} tokens, at {}）\n",
                    s.summary_title, tags_str, s.token_count, s.archived_at
                ));
                out.push_str(&format!("  - 记忆 ID: `{}`\n", s.hook_id));

                // 分级渲染：根据周期层级展示不同详细度
                let should_expand = match period {
                    ArchivePeriod::Daily => {
                        // 日级：仅高价值片段展开 key_facts
                        s.tags.iter().any(|t| HIGH_VALUE_TAGS.contains(&t.as_str()))
                            && !s.key_facts.is_empty()
                    }
                    ArchivePeriod::Weekly => s.is_rich,
                    ArchivePeriod::Monthly => true, // 月级全展开
                };

                if should_expand {
                    if let Some(abs) = &s.abstract_text {
                        out.push_str(&format!("  - 摘要：{}\n", abs));
                    }
                    if !s.key_facts.is_empty() {
                        out.push_str("  - 关键事实：\n");
                        for fact in &s.key_facts {
                            out.push_str(&format!("    - {}\n", fact));
                        }
                    }
                    if !s.key_entities.is_empty() {
                        out.push_str(&format!("  - 关键实体：{}\n", s.key_entities.join(", ")));
                    }
                    if !s.clue_anchors.is_empty() {
                        out.push_str(&format!("  - 线索锚点：{}\n", s.clue_anchors.join(", ")));
                    }
                }
            }
            out.push('\n');
        }

        Ok(out)
    }

    /// 按钩子 ID 检索完整记忆文件（tool 调用入口）
    ///
    /// 流程：
    /// 1. 从所有周期的索引文档中查找对应 hook_id
    /// 2. 获取该钩子指向的 memory_id
    /// 3. 从 Storage 读取完整 MemoryFile
    pub async fn retrieve_memory(&self, hook_id: &str) -> crate::Result<MemoryFile> {
        // 在所有周期中查找钩子
        for period in ArchivePeriod::all() {
            if let Some(doc) = self
                .storage
                .read_index(&self.session_id, self.project_id.as_deref(), period)
                .await?
            {
                for hook in &doc.hooks {
                    if hook.id.to_string() == hook_id {
                        // 找到钩子，读取对应的记忆文件
                        return self.storage.read_memory(&hook.memory_id).await;
                    }
                }
            }
        }

        Err(crate::Error::Index(format!(
            "未找到钩子 ID: {}",
            hook_id
        )))
    }

    /// 按 hook_id 查找对应的 memory_id（v2.4 批次 3 新增）
    ///
    /// 用于 update_memory 场景：先通过 hook_id 定位到 memory_id，
    /// 再调用 Storage::update_memory 执行更新。
    ///
    /// 返回 None 表示未找到对应钩子。
    pub async fn find_memory_id_by_hook(&self, hook_id: &str) -> Option<String> {
        for period in ArchivePeriod::all() {
            if let Ok(Some(doc)) = self
                .storage
                .read_index(&self.session_id, self.project_id.as_deref(), period)
                .await
            {
                for hook in &doc.hooks {
                    if hook.id.to_string() == hook_id {
                        return Some(hook.memory_id.clone());
                    }
                }
            }
        }
        None
    }

    /// 按 session + period 获取索引文档（高级接口）
    ///
    /// 供调用方需要直接操作 IndexDocument 时使用。
    pub async fn get_index_document(
        &self,
        period: ArchivePeriod,
    ) -> crate::Result<Option<crate::model::IndexDocument>> {
        self.storage
            .read_index(&self.session_id, self.project_id.as_deref(), period)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::Archiver;
    use crate::model::{ArchiveConfig, MessageContent, MessageTurn, Tag};
    use crate::storage::LocalStorage;
    use chrono::Utc;
    use tempfile::TempDir;
    use uuid::Uuid;

    /// 构造测试用 MessageTurn
    fn make_turn(text: &str, token_count: usize) -> MessageTurn {
        MessageTurn {
            id: Uuid::new_v4(),
            user_message: MessageContent {
                text: Some(text.into()),
                attachments: Vec::new(),
                tool_calls: Vec::new(),
                thinking: None,
            },
            llm_message: MessageContent {
                text: Some("LLM 回复".into()),
                attachments: Vec::new(),
                tool_calls: Vec::new(),
                thinking: None,
            },
            tags: vec![Tag::Text, Tag::CodeBlock],
            timestamp: Utc::now(),
            token_count,
        }
    }

    #[tokio::test]
    async fn test_retriever_empty_summaries() {
        let tmp = TempDir::new().unwrap();
        let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(tmp.path()));
        let retriever = Retriever::new(storage, "sess-empty", None);

        let summaries = retriever.get_summaries().await.unwrap();
        assert!(summaries.is_empty());

        let prompt = retriever.render_to_system_prompt().await.unwrap();
        assert!(prompt.is_empty());
    }

    #[tokio::test]
    async fn test_retriever_after_archive() {
        let tmp = TempDir::new().unwrap();
        let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(tmp.path()));

        // 用 Archiver 归档一次
        let config = ArchiveConfig {
            token_threshold: 100,
            force_truncate_limit: 150,
            wait_for_turn_completion: true,
        };
        let mut archiver = Archiver::new(config, storage.clone(), "sess-r1", None);
        archiver.push_turn(make_turn("第一次对话内容", 60));
        archiver.push_turn(make_turn("第二次对话内容", 50));
        let (_memory, hook) = archiver.archive().await.unwrap();

        // 用 Retriever 检索
        let retriever = Retriever::new(storage.clone(), "sess-r1", None);
        let summaries = retriever.get_summaries().await.unwrap();
        assert_eq!(summaries.len(), 1);

        let s = &summaries[0];
        assert_eq!(s.hook_id, hook.id.to_string());
        // v2.4: memory_id 是相对路径（如 sessions/sess-r1/daily/xxx.json），不是 UUID
        assert!(s.memory_id.starts_with("sessions/sess-r1/daily/"));
        assert!(s.memory_id.ends_with(".json"));
        assert!(s.summary_title.contains("第一次对话内容"));
        assert!(s.tags.contains(&"文本消息".to_string()));
        assert!(s.tags.contains(&"代码块".to_string()));
        assert_eq!(s.period, "daily");
        assert_eq!(s.token_count, 110);
    }

    #[tokio::test]
    async fn test_retriever_render_prompt() {
        let tmp = TempDir::new().unwrap();
        let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(tmp.path()));

        let config = ArchiveConfig {
            token_threshold: 100,
            force_truncate_limit: 150,
            wait_for_turn_completion: true,
        };
        let mut archiver = Archiver::new(config, storage.clone(), "sess-r2", None);
        archiver.push_turn(make_turn("讨论记忆库设计", 60));
        archiver.push_turn(make_turn("确定三级周期", 50));
        archiver.archive().await.unwrap();

        let retriever = Retriever::new(storage, "sess-r2", None);
        let prompt = retriever.render_to_system_prompt().await.unwrap();

        assert!(prompt.contains("# 可用记忆索引"));
        assert!(prompt.contains("## 近期记忆（daily）"));
        assert!(prompt.contains("讨论记忆库设计"));
        assert!(prompt.contains("文本消息"));
        assert!(prompt.contains("代码块"));
    }

    #[tokio::test]
    async fn test_retriever_retrieve_memory() {
        let tmp = TempDir::new().unwrap();
        let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(tmp.path()));

        let config = ArchiveConfig {
            token_threshold: 100,
            force_truncate_limit: 150,
            wait_for_turn_completion: true,
        };
        let mut archiver = Archiver::new(config, storage.clone(), "sess-r3", None);
        archiver.push_turn(make_turn("可检索的内容", 110));
        let (original_memory, hook) = archiver.archive().await.unwrap();

        let retriever = Retriever::new(storage, "sess-r3", None);

        // 按钩子 ID 检索
        let retrieved = retriever
            .retrieve_memory(&hook.id.to_string())
            .await
            .unwrap();

        assert_eq!(retrieved.id, original_memory.id);
        assert_eq!(retrieved.session_id, "sess-r3");
        assert_eq!(retrieved.turns.len(), 1);
        assert_eq!(retrieved.total_tokens, 110);
    }

    #[tokio::test]
    async fn test_retriever_retrieve_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(tmp.path()));
        let retriever = Retriever::new(storage, "sess-r4", None);

        let result = retriever.retrieve_memory("nonexistent-id").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_retriever_multiple_archives() {
        let tmp = TempDir::new().unwrap();
        let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(tmp.path()));

        let config = ArchiveConfig {
            token_threshold: 100,
            force_truncate_limit: 150,
            wait_for_turn_completion: true,
        };
        let mut archiver = Archiver::new(config, storage.clone(), "sess-r5", None);

        // 归档 3 次
        let mut hooks = Vec::new();
        for i in 1..=3 {
            archiver.push_turn(make_turn(&format!("话题 {}", i), 60));
            archiver.push_turn(make_turn(&format!("续接 {}", i), 50));
            let (_, hook) = archiver.archive().await.unwrap();
            hooks.push(hook);
        }

        let retriever = Retriever::new(storage, "sess-r5", None);
        let summaries = retriever.get_summaries().await.unwrap();
        assert_eq!(summaries.len(), 3);

        // 验证按时间排序（旧→新）
        assert!(summaries[0].archived_at <= summaries[1].archived_at);
        assert!(summaries[1].archived_at <= summaries[2].archived_at);

        // 检索第二个记忆
        let retrieved = retriever
            .retrieve_memory(&hooks[1].id.to_string())
            .await
            .unwrap();
        assert!(retrieved.turns[0]
            .user_message
            .text
            .as_ref()
            .unwrap()
            .contains("话题 2"));
    }
}

# 内部代码逻辑调用过程

> 记录 5 个核心操作（archive / retrieve / summaries / prompt / compaction）
> 从接口层到 Core 的完整调用链，含代码引用与行号。
> 代码引用变更时，同步更新本文件。

## 整体调用链概览

```
┌──────────────────────────────────────────────────────────────────────┐
│ 调用方（Agent / 应用）                                                │
└──────────────────────────────────────────────────────────────────────┘
       │           │           │           │
       ▼           ▼           ▼           ▼
   ┌───────┐  ┌────────┐  ┌────────┐  ┌────────────┐
   │ C ABI │  │  HTTP  │  │ Python │  │    MCP     │
   │  FFI  │  │  REST  │  │  PyO3  │  │  Server    │
   └───┬───┘  └────┬───┘  └────┬───┘  └─────┬──────┘
       │           │           │              │
       │  block_on │           │  block_on    │  async
       │           │           │              │
       └───────────┴───────────┴──────────────┘
                            │
                            ▼
              ┌─────────────────────────────┐
              │  hippocampus-core           │
              │  ┌────────┐  ┌──────────┐   │
              │  │Archiver│  │Retriever │   │
              │  └────┬───┘  └────┬─────┘   │
              │       │           │         │
              │  ┌────┴───┐  ┌────┴─────┐   │
              │  │Compactor│  │  Scorer  │   │
              │  └────┬───┘  └──────────┘   │
              │       │                     │
              │  ┌────▼─────────────────┐   │
              │  │  Storage (trait)     │   │
              │  │  ┌────────────┐      │   │
              │  │  │ LocalStorage│     │   │
              │  │  │ SqliteStorage│     │   │
              │  │  │ CachedStorage│     │   │
              │  │  └────────────┘      │   │
              │  └─────────────────────┘   │
              └─────────────────────────────┘
```

---

## 1. archive 调用链（归档）

### 1.1 入口层（4 种接口形态）

#### MCP Server 入口

[crates/hippocampus-mcp/src/lib.rs:278-311](../../crates/hippocampus-mcp/src/lib.rs#L278-L311)

```rust
#[tool(description = "归档对话轮次到 Hippocampus 记忆库...")]
async fn archive(
    &self,
    Parameters(params): Parameters<ArchiveParams>,
) -> Result<String, McpError> {
    // 1. 解析 turns_json → Vec<MessageTurn>
    let turns: Vec<MessageTurn> = serde_json::from_str(&params.turns_json)?;

    // 2. 创建无状态 Storage（每次 tool 调用独立）
    let storage = self.create_storage();  // Arc<dyn Storage>

    // 3. 构造 Archiver
    let mut archiver = Archiver::new(
        ArchiveConfig::default(),
        storage,
        &params.session_id,
        params.project_id,
    );

    // 4. 推入所有 turn
    for turn in turns {
        archiver.push_turn(turn);
    }

    // 5. 执行归档
    let (_, hook) = archiver.archive().await?;

    // 6. 返回 SummaryView JSON
    let summary = SummaryView::from(&hook);
    Ok(serde_json::to_string(&summary)?)
}
```

#### C ABI 入口

[crates/hippocampus-ffi/src/lib.rs:309-355](../../crates/hippocampus-ffi/src/lib.rs#L309-L355)

```rust
#[no_mangle]
pub unsafe extern "C" fn hippocampus_archive(
    handle: *mut HippocampusHandle,
    turns_json: *const c_char,
) -> *mut HippocampusResult {
    // 1. C 字符串 → &str（UTF-8 校验）
    let json_str = CStr::from_ptr(turns_json).to_str()?;

    // 2. JSON → Vec<MessageTurn>
    let turns: Vec<MessageTurn> = serde_json::from_str(json_str)?;

    // 3. 构造 Archiver（复用 handle 内的 storage + runtime）
    let mut archiver = Archiver::new(
        handle.config.clone(),
        handle.storage.clone(),
        handle.session_id.clone(),
        handle.project_id.clone(),
    );

    // 4. 推入 turn
    for turn in turns {
        archiver.push_turn(turn);
    }

    // 5. block_on 执行异步归档（current_thread runtime）
    match handle.runtime.block_on(archiver.archive()) {
        Ok((_memory, hook)) => {
            let summary = SummaryView::from(&hook);
            ok_result(&summary)  // 返回 *mut HippocampusResult
        }
        Err(e) => err_from_core(e),
    }
}
```

#### HTTP REST 入口

[crates/hippocampus-server/src/handlers.rs](../../crates/hippocampus-server/src/handlers.rs)

```rust
// POST /api/v1/sessions/{sid}/archive
async fn archive_handler(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    Json(payload): Json<ArchivePayload>,
) -> Result<Json<SummaryView>, AppError> {
    // 1. 创建无状态 Storage
    let storage = Arc::new(LocalStorage::new(&state.storage_root));

    // 2. 构造 Archiver（sid 来自 URL，project_id 来自 body）
    let mut archiver = Archiver::new(
        ArchiveConfig::default(),
        storage,
        &sid,
        payload.project_id,
    );

    // 3. 推入 turn
    for turn in payload.turns {
        archiver.push_turn(turn);
    }

    // 4. 执行归档（tokio async，无需 block_on）
    let (_, hook) = archiver.archive().await?;

    // 5. 返回 JSON
    Ok(Json(SummaryView::from(&hook)))
}
```

#### Python 入口

[crates/hippocampus-python/src/lib.rs](../../crates/hippocampus-python/src/lib.rs)

```rust
// PyO3 绑定，Python 调用 hp.archive(turns)
fn archive(&self, turns: Vec<PyDict>) -> PyResult<Py<PyDict>> {
    // 1. Python dict → Vec<MessageTurn>
    let turns_json = serde_json::to_string(&turns)?;
    let turns: Vec<MessageTurn> = serde_json::from_str(&turns_json)?;

    // 2. 构造 Archiver
    let mut archiver = Archiver::new(...);

    // 3. block_on 执行归档
    let (_, hook) = self.runtime.block_on(async {
        archiver.archive().await
    })?;

    // 4. 返回 Python dict
    Ok(SummaryView::from(&hook).to_python())
}
```

### 1.2 Core 层：Archiver::archive()

[crates/hippocampus-core/src/archive.rs:138-200](../../crates/hippocampus-core/src/archive.rs#L138-L200)

```rust
pub async fn archive(&mut self) -> crate::Result<(MemoryFile, IndexHook)> {
    // 1. 消费 pending_turns
    let turns = std::mem::take(&mut self.pending_turns);
    let was_over_limit = self.current_tokens >= self.config.force_truncate_limit;
    let total_tokens = self.current_tokens;
    self.current_tokens = 0;

    // 2. 生成 MemoryFile（自动计算标签并集 + total_tokens）
    let mut memory_file = MemoryFile::new(
        self.session_id.clone(),
        self.project_id.clone(),
        turns,
        ArchivePeriod::Daily,
    );

    // 3. 若超过硬上限，标记 truncated=true
    if was_over_limit {
        memory_file.mark_truncated();
    }

    // 4. 写入 Storage（原子写入：temp + rename）
    let memory_path = self.storage.write_memory(&memory_file).await?;

    // 5. 生成 IndexHook 指向该记忆文件
    let hook = IndexHook::from_memory_file(&memory_file, memory_path);

    // 6. 追加钩子到 daily 索引文档（session 级）
    self.storage.append_hook(
        &self.session_id,
        self.project_id.as_deref(),
        ArchivePeriod::Daily,
        hook.clone(),
    ).await?;

    // 7. v2.4 双写：若有 project_id，同时追加到 project 级聚合索引
    if let Some(pid) = &self.project_id {
        self.storage.append_project_hook(
            pid, ArchivePeriod::Daily, hook.clone()
        ).await?;
    }

    Ok((memory_file, hook))
}
```

### 1.3 Storage 层：LocalStorage::write_memory + append_hook

[crates/hippocampus-core/src/storage.rs](../../crates/hippocampus-core/src/storage.rs)

```rust
// 写入记忆文件（原子写入）
async fn write_memory(&self, memory: &MemoryFile) -> Result<String> {
    let path = self.memory_path(&memory.session_id, &memory.project_id, memory.period, memory.id);
    let json = serde_json::to_string_pretty(memory)?;

    // 原子写入：先写 temp 文件，再 rename（防崩溃损坏）
    let tmp_path = path.with_extension("tmp");
    tokio::fs::write(&tmp_path, json).await?;
    tokio::fs::rename(&tmp_path, &path).await?;

    Ok(relative_path)
}

// 追加钩子到索引文档（read-modify-write）
async fn append_hook(&self, session_id: &str, project_id: Option<&str>,
                     period: ArchivePeriod, hook: IndexHook) -> Result<()> {
    // 1. 读取现有索引文档
    let mut doc = self.read_index(session_id, project_id, period).await?
        .unwrap_or_else(|| IndexDocument::new(...));

    // 2. 追加新钩子
    doc.add_hook(hook);

    // 3. 写回（RwLock 写锁保护，原子写入）
    self.write_index(&doc).await?;
    Ok(())
}
```

### 1.4 归档完整调用链总结

```
调用方
  ↓ turns_json (JSON 字符串)
接口层（MCP/FFI/HTTP/Python）
  ↓ Vec<MessageTurn>
Archiver::push_turn() × N
  ↓ pending_turns + current_tokens
Archiver::archive()
  ↓ MemoryFile（含完整 turns + 标签并集 + total_tokens）
Storage::write_memory()  ← 原子写入 temp+rename
  ↓ memory_path (相对路径)
IndexHook::from_memory_file()
  ↓ IndexHook（含 hook_id + memory_id + summary + tags）
Storage::append_hook()  ← read-modify-write，session 级索引
  ↓
Storage::append_project_hook()  ← v2.4 双写，project 级聚合索引
  ↓
返回 SummaryView JSON（含 hook_id）
```

---

## 2. retrieve 调用链（检索）

### 2.1 入口层

#### MCP Server 入口

[crates/hippocampus-mcp/src/lib.rs:315-331](../../crates/hippocampus-mcp/src/lib.rs#L315-L331)

```rust
#[tool(description = "按 hook_id 检索完整记忆文件...")]
async fn retrieve(&self, Parameters(params): Parameters<RetrieveParams>) -> Result<String, McpError> {
    let storage = self.create_storage();
    let retriever = Retriever::new(storage, &params.session_id, params.project_id);

    // 调用 Core 检索
    let memory = retriever.retrieve_memory(&params.hook_id).await?;

    Ok(serde_json::to_string(&memory)?)
}
```

### 2.2 Core 层：Retriever::retrieve_memory()

[crates/hippocampus-core/src/retrieve.rs:248-269](../../crates/hippocampus-core/src/retrieve.rs#L248-L269)

```rust
pub async fn retrieve_memory(&self, hook_id: &str) -> crate::Result<MemoryFile> {
    // 1. 遍历所有周期（daily/weekly/monthly）的索引文档
    for period in ArchivePeriod::all() {
        if let Some(doc) = self.storage
            .read_index(&self.session_id, self.project_id.as_deref(), period)
            .await?
        {
            // 2. 在索引文档中查找匹配的 hook_id
            for hook in &doc.hooks {
                if hook.id.to_string() == hook_id {
                    // 3. 找到钩子，读取对应的完整记忆文件
                    return self.storage.read_memory(&hook.memory_id).await;
                }
            }
        }
    }

    Err(crate::Error::Index(format!("未找到钩子 ID: {}", hook_id)))
}
```

### 2.3 检索完整调用链

```
调用方
  ↓ hook_id (UUID 字符串)
接口层
  ↓
Retriever::retrieve_memory(hook_id)
  ↓ 遍历 daily/weekly/monthly
Storage::read_index(session_id, project_id, period)  × 3 周期
  ↓ IndexDocument
匹配 hook.id == hook_id
  ↓ 找到 hook.memory_id（相对路径）
Storage::read_memory(memory_id)
  ↓
返回 MemoryFile JSON（含完整 turns）
```

---

## 3. summaries 调用链（获取摘要列表）

### 3.1 Core 层：Retriever::get_summaries()

[crates/hippocampus-core/src/retrieve.rs:116-141](../../crates/hippocampus-core/src/retrieve.rs#L116-L141)

```rust
pub async fn get_summaries(&self) -> crate::Result<Vec<SummaryView>> {
    let mut all_summaries = Vec::new();

    // 1. 遍历三个周期
    for period in ArchivePeriod::all() {
        // v2.4: 有 project_id 时走 project 级聚合索引（跨 session 共享）
        let doc = if let Some(pid) = &self.project_id {
            self.storage.read_project_index(pid, period).await?
        } else {
            self.storage.read_index(&self.session_id, None, period).await?
        };

        if let Some(doc) = doc {
            // 2. 每个钩子转为 SummaryView
            for hook in &doc.hooks {
                all_summaries.push(SummaryView::from(hook));
            }
        }
    }

    // 3. 按归档时间排序（旧→新）
    all_summaries.sort_by(|a, b| a.archived_at.cmp(&b.archived_at));

    Ok(all_summaries)
}
```

### 3.2 SummaryView 数据结构

[crates/hippocampus-core/src/retrieve.rs:34-65](../../crates/hippocampus-core/src/retrieve.rs#L34-L65)

```rust
pub struct SummaryView {
    pub hook_id: String,           // 钩子 ID
    pub memory_id: String,         // 指向的记忆文件 ID
    pub summary_title: String,     // 摘要标题
    pub abstract_text: Option<String>,  // 抽象摘要（2-3 句，日级为 None）
    pub key_facts: Vec<String>,    // 关键事实（事实级，日级为空）
    pub key_entities: Vec<String>, // 关键实体（人名/项目名/技术名词）
    pub clue_anchors: Vec<String>, // 线索锚点（月级才有）
    pub tags: Vec<String>,         // 标签集合（中文显示）
    pub archived_at: String,       // 归档时间
    pub period: String,            // 周期层级
    pub token_count: usize,        // Token 数
    pub is_rich: bool,             // 是否为高级摘要
}
```

---

## 4. prompt 调用链（渲染 system prompt）

### 4.1 Core 层：Retriever::render_to_system_prompt()

[crates/hippocampus-core/src/retrieve.rs:156-240](../../crates/hippocampus-core/src/retrieve.rs#L156-L240)

```rust
pub async fn render_to_system_prompt(&self) -> crate::Result<String> {
    let summaries = self.get_summaries().await?;

    if summaries.is_empty() {
        return Ok(String::new());  // 无记忆返回空字符串
    }

    let mut out = String::from("# 可用记忆索引\n\n");

    // 高价值标签集合（自动展开判定）
    const HIGH_VALUE_TAGS: &[&str] = &[
        "工具调用", "思考过程", "代码块", "文件附件", "图片", "视频",
    ];

    // 按周期分组渲染
    for period in ArchivePeriod::all() {
        let hooks: Vec<&SummaryView> = summaries.iter()
            .filter(|s| s.period == period.as_dir_name())
            .collect();

        if hooks.is_empty() { continue; }

        // 渲染周期标题
        out.push_str(&format!("## {}（{}）\n\n", period_label, period_name));

        for s in hooks {
            // 渲染基础信息：标题 + 标签 + token + 时间 + hook_id
            out.push_str(&format!("- **{}**[{}]（{} tokens, at {}）\n",
                s.summary_title, s.tags.join(", "), s.token_count, s.archived_at));
            out.push_str(&format!("  - 记忆 ID: `{}`\n", s.hook_id));

            // 分级渲染策略
            let should_expand = match period {
                ArchivePeriod::Daily => {
                    // 日级：仅高价值片段展开 key_facts
                    s.tags.iter().any(|t| HIGH_VALUE_TAGS.contains(&t.as_str()))
                        && !s.key_facts.is_empty()
                }
                ArchivePeriod::Weekly => s.is_rich,
                ArchivePeriod::Monthly => true,  // 月级全展开
            };

            if should_expand {
                // 展开 abstract_text / key_facts / key_entities / clue_anchors
                if let Some(abs) = &s.abstract_text {
                    out.push_str(&format!("  - 摘要：{}\n", abs));
                }
                if !s.key_facts.is_empty() {
                    out.push_str("  - 关键事实：\n");
                    for fact in &s.key_facts {
                        out.push_str(&format!("    - {}\n", fact));
                    }
                }
                // ... key_entities / clue_anchors
            }
        }
    }

    Ok(out)
}
```

### 4.2 prompt 渲染示例输出

```markdown
# 可用记忆索引

以下是可用的历史记忆摘要，可直接基于此信息回答用户问题：

## 近期记忆（daily）

- **商品表设计讨论**[CodeBlock, Text, URL]（80000 tokens, at 2026-07-07T14:00:00Z）
  - 记忆 ID: `uuid-h001`
  - 关键事实：
    - 商品表设计讨论

## 周度记忆（weekly）

- **周度合并（4 个记忆）**[CodeBlock, Text, URL, ToolCall]（850000 tokens, at 2026-07-13T23:00:00Z）
  - 记忆 ID: `uuid-wh001`
  - 摘要：本周合并了 4 个日级记忆：商品表设计讨论；Product CRUD 实现；...
  - 关键事实：
    - 商品表设计讨论
    - Product CRUD 实现
  - 关键实体：CodeBlock, Text, URL, ToolCall

## 月度记忆（monthly）

- **2026-07: 电商后台开发**[CodeBlock, Text, URL, ToolCall, Plan]（1200000 tokens, at 2026-07-28T23:00:00Z）
  - 记忆 ID: `uuid-mh001`
  - 摘要：本月合并了 3 个周级记忆...
  - 关键事实：
    - 支付核心逻辑
    - 认证中间件
  - 线索锚点：JWT, PostgreSQL, Axum
```

---

## 5. compaction 调用链（周期任务）

### 5.1 入口层

[crates/hippocampus-mcp/src/lib.rs:371-401](../../crates/hippocampus-mcp/src/lib.rs#L371-L401)

```rust
#[tool(description = "触发周期任务...")]
async fn compaction(&self, Parameters(params): Parameters<CompactionParams>) -> Result<String, McpError> {
    let storage = self.create_storage();
    let compactor = Compactor::new(
        storage,
        Box::new(DefaultScorer::new()),  // 启发式评分器
        &params.session_id,
        params.project_id,
    );

    let (memory, index_doc) = match params.period.as_str() {
        "weekly" => compactor.weekly_merge().await,
        "monthly" => compactor.monthly_evict().await,
        other => return Err(McpError::invalid_params(...)),
    }?;

    // 返回 CompactionResult JSON
    Ok(serde_json::json!({
        "memory_file_id": memory.id,
        "total_turns": memory.turns.len(),
        "total_tokens": memory.total_tokens,
        "hooks_count": index_doc.hooks.len(),
        "period": params.period,
    }).to_string())
}
```

### 5.2 weekly_merge 调用链

[crates/hippocampus-core/src/compact.rs:136-269](../../crates/hippocampus-core/src/compact.rs#L136-L269)

```rust
pub async fn weekly_merge(&self) -> crate::Result<(MemoryFile, IndexDocument)> {
    // 1. 读取所有 daily 记忆文件路径
    let daily_paths = self.storage.list_memories(
        &self.session_id, self.project_id.as_deref(), ArchivePeriod::Daily
    ).await?;

    // 2. 读取所有 daily MemoryFile，过滤寒暄 turn，合并 turns
    let mut all_turns = Vec::new();
    let mut removed_count = 0;

    for path in &daily_paths {
        let file = self.storage.read_memory(path).await?;
        for turn in &file.turns {
            if Self::is_chitchat(turn) {
                removed_count += 1;  // 寒暄剥离
            } else {
                all_turns.push(turn.clone());
            }
        }
    }

    // 3. 生成合并后的 MemoryFile（Weekly）
    let merged_memory = MemoryFile::new(
        self.session_id.clone(),
        self.project_id.clone(),
        all_turns,
        ArchivePeriod::Weekly,
    );

    // 4. 写入 Storage
    let memory_path = self.storage.write_memory(&merged_memory).await?;

    // 5. 合并 daily 索引文档的钩子到 weekly 索引
    let daily_index = self.storage.read_index(
        &self.session_id, self.project_id.as_deref(), ArchivePeriod::Daily
    ).await?;

    let mut weekly_index = IndexDocument::new(..., ArchivePeriod::Weekly);

    if let Some(daily_doc) = daily_index {
        // v2.4: 为 weekly 钩子生成 richer Summary
        let abstract_text = Some(format!("本周合并了 {} 个日级记忆：{}",
            daily_doc.hooks.len(),
            daily_doc.hooks.iter().map(|h| h.summary.title.clone())
                .collect::<Vec<_>>().join("；")));

        for hook in &daily_doc.hooks {
            let mut new_hook = hook.clone();
            new_hook.memory_id = memory_path.clone();  // 指向新的 weekly 文件
            new_hook.period = ArchivePeriod::Weekly;
            new_hook.summary = Summary {
                title: format!("周度合并（{} 个记忆）", daily_doc.hooks.len()),
                abstract_text: abstract_text.clone(),
                key_facts: daily_doc.hooks.iter().map(|h| h.summary.title.clone()).collect(),
                key_entities: ...,
                clue_anchors: Vec::new(),
            };
            weekly_index.add_hook(new_hook);
        }
    }

    // 6. 写入 weekly 索引
    self.storage.write_index(&weekly_index).await?;

    Ok((merged_memory, weekly_index))
}
```

#### 寒暄判定规则

[crates/hippocampus-core/src/compact.rs:87-122](../../crates/hippocampus-core/src/compact.rs#L87-L122)

```rust
fn is_chitchat(turn: &MessageTurn) -> bool {
    // 规则 1：完全空内容的 turn
    let user_empty = turn.user_message.text.is_none()
        && turn.user_message.attachments.is_empty()
        && turn.user_message.tool_calls.is_empty()
        && turn.user_message.thinking.is_none();
    let llm_empty = ...;
    if user_empty && llm_empty { return true; }

    // 规则 2：极短用户消息（≤3 字符，如「嗯」「哦」「好」）
    if let Some(text) = &turn.user_message.text {
        if text.trim().chars().count() <= 3 { return true; }
    }

    // 规则 3：匹配寒暄模式（你好/谢谢/好的/再见等，≤10 字符）
    if let Some(text) = &turn.user_message.text {
        let lower = text.trim().to_lowercase();
        if lower.chars().count() <= 10 {
            for pattern in CHITCHAT_PATTERNS {
                if lower == *pattern || lower.starts_with(pattern) {
                    return true;
                }
            }
        }
    }

    false
}
```

### 5.3 monthly_evict 调用链

[crates/hippocampus-core/src/compact.rs:285-404](../../crates/hippocampus-core/src/compact.rs#L285-L404)

```rust
pub async fn monthly_evict(&self) -> crate::Result<(MemoryFile, IndexDocument)> {
    // 1. 读取所有 weekly 记忆文件
    let weekly_paths = self.storage.list_memories(
        &self.session_id, self.project_id.as_deref(), ArchivePeriod::Weekly
    ).await?;

    // 2. 读取所有 weekly MemoryFile 并评分
    let mut scored: Vec<(MemoryFile, f64)> = weekly_files.into_iter()
        .map(|f| {
            let score = self.scorer.score(&f);  // 4 维加权评分
            (f, score)
        })
        .collect();

    // 3. 按分数从高到低排序
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Equal));

    // 4. 选最高分的作为主记忆
    let (mut main_memory, main_score) = scored.remove(0);

    // 5. 从其余文件中挑选高价值 Turn
    let mut high_value_turns = Vec::new();
    for (file, _) in &scored {
        for turn in &file.turns {
            if Self::is_high_value_turn(turn) {
                high_value_turns.push(turn.clone());
            }
        }
    }

    // 6. 追加高价值 Turn 到主记忆
    for turn in high_value_turns {
        main_memory.turns.push(turn);
    }
    main_memory.total_tokens = main_memory.turns.iter().map(|t| t.token_count).sum();
    main_memory.period = ArchivePeriod::Monthly;

    // 7. 写入 Storage
    let memory_path = self.storage.write_memory(&main_memory).await?;

    // 8. 合并 weekly 索引到 monthly 索引
    let weekly_index = self.storage.read_index(..., ArchivePeriod::Weekly).await?;
    let mut monthly_index = IndexDocument::new(..., ArchivePeriod::Monthly);

    if let Some(weekly_doc) = weekly_index {
        for hook in &weekly_doc.hooks {
            let mut new_hook = hook.clone();
            new_hook.memory_id = memory_path.clone();
            new_hook.period = ArchivePeriod::Monthly;
            monthly_index.add_hook(new_hook);
        }
    }

    self.storage.write_index(&monthly_index).await?;

    Ok((main_memory, monthly_index))
}
```

#### 高价值 Turn 判定

[crates/hippocampus-core/src/compact.rs:414-431](../../crates/hippocampus-core/src/compact.rs#L414-L431)

```rust
fn is_high_value_turn(turn: &MessageTurn) -> bool {
    let valuable_tags = [
        Tag::ToolCall,      // 工具调用信息
        Tag::Thinking,      // 思考过程
        Tag::AgentTool,     // Agent 工具使用记录
        Tag::CodeBlock,     // 代码块，技术信息
        Tag::FileAttachment,// 附件信息
        Tag::Image,         // 图片
        Tag::Video,         // 视频
    ];
    for tag in &turn.tags {
        if valuable_tags.contains(tag) {
            return true;
        }
    }
    false
}
```

### 5.4 评分调用链

[crates/hippocampus-core/src/score.rs:68-120](../../crates/hippocampus-core/src/score.rs#L68-L120)

```rust
impl Scorer for DefaultScorer {
    fn score(&self, file: &MemoryFile) -> f64 {
        // 4 维加权评分（P3 实现 3 维，topic_relevance 留 v2）
        let timeliness_score = self.timeliness_score(file.archived_at);  // 0-100
        let access_score = self.access_score(file.access_count);         // 0-100
        let topic_score = 0.0;  // v2 实现
        let user_score = file.importance as f64;                         // 0-100

        // 加权平均
        self.weights.timeliness * timeliness_score
            + self.weights.access_frequency * access_score
            + self.weights.topic_relevance * topic_score
            + self.weights.user_marked * user_score
    }
}

// 时效性：半衰期衰减
// score = 100 * 0.5^(age_days / half_life_days)
// - 当前时间归档：100 分
// - 半衰期（7 天）前归档：50 分
// - 两周前归档：25 分
fn timeliness_score(&self, archived_at: DateTime<Utc>) -> f64 {
    let age_days = (self.now - archived_at).num_seconds() as f64 / 86400.0;
    100.0 * 0.5_f64.powf(age_days / self.half_life_days)
}

// 访问频率：归一化，10 次满分
fn access_score(&self, access_count: usize) -> f64 {
    (access_count as f64 / self.access_full_score_threshold * 100.0).min(100.0)
}
```

---

## 6. 关键数据结构流转

### 6.1 数据流转图

```
用户输入 turns
     ↓
MessageTurn { id, user_message, llm_message, tags, timestamp, token_count }
     ↓ Archiver::push_turn() × N
pending_turns: Vec<MessageTurn> + current_tokens
     ↓ Archiver::archive()
MemoryFile {
    id: Uuid,
    schema_version: 1,
    archived_at: DateTime,
    session_id, project_id,
    turns: Vec<MessageTurn>,  ← 完整保留
    tags: Vec<Tag>,           ← 自动计算并集
    total_tokens: usize,      ← 自动求和
    truncated: bool,          ← 硬上限时标记
    period: Daily,
    access_count: 0,
    importance: 0,
}
     ↓ Storage::write_memory()
文件路径：sessions/{sid}/[projects/{pid}/]daily/{timestamp}_{uuid}.json
     ↓ IndexHook::from_memory_file()
IndexHook {
    id: Uuid,                 ← 新生成的 hook_id
    memory_id: String,        ← 指向 MemoryFile 的相对路径
    summary: Summary {
        title: String,        ← 从首个 user_message 提取
        abstract_text: None,  ← 日级为空，周级/月级有
        key_facts: [],        ← 日级为空，周级/月级有
        key_entities: [],     ← 日级为空，周级/月级有
        clue_anchors: [],     ← 仅月级有
    },
    tags: Vec<Tag>,           ← 从 MemoryFile.tags 复制
    archived_at: DateTime,
    period: Daily,
    token_count: usize,
}
     ↓ Storage::append_hook()
IndexDocument {
    session_id, project_id, period,
    hooks: Vec<IndexHook>,   ← 追加新钩子
}
     ↓ SummaryView::from(&hook)
SummaryView {
    hook_id, memory_id, summary_title,
    abstract_text, key_facts, key_entities, clue_anchors,
    tags: Vec<String>,        ← Tag 转中文显示
    archived_at, period, token_count, is_rich,
}
     ↓ 序列化
返回 JSON 字符串给调用方
```

### 6.2 标签流转

```
MessageTurn.tags: Vec<Tag>     ← 每轮的标签（可叠加）
     ↓
MemoryFile.tags: Vec<Tag>      ← 所有 turns 标签的并集
     ↓
IndexHook.tags: Vec<Tag>       ← 从 MemoryFile 复制
     ↓
SummaryView.tags: Vec<String>  ← Tag::to_string() 转中文显示
     ↓
render_to_system_prompt()      ← "[CodeBlock, Text, URL]"
```

---

## 7. 接口层对比与调用差异

| 维度 | C ABI (FFI) | HTTP REST | Python | MCP |
|------|-------------|-----------|--------|-----|
| 入口函数 | `hippocampus_archive(handle, json)` | `POST /archive` | `hp.archive(turns)` | `archive` tool |
| 状态 | 有状态（handle） | 无状态（每请求） | 有状态（实例） | 无状态（每 tool） |
| 异步执行 | `runtime.block_on()` | tokio async | `runtime.block_on()` | async |
| Runtime | current_thread | rt-multi-thread | current_thread | current_thread |
| 错误处理 | `HippocampusResult*` | `{error:{code,msg}}` | `PyValueError` | `McpError` |
| Storage 创建 | handle 持有 | 每请求新建 | 实例持有 | 每 tool 调用新建 |

### 调用链关键差异

**FFI / Python**（有状态）：
- handle/实例持有 `Arc<dyn Storage>` 和 `tokio Runtime`
- 多次调用复用同一 storage，但需串行化
- 适合嵌入式 / 单进程应用

**HTTP / MCP**（无状态）：
- 每次请求/tool 调用新建 `LocalStorage`
- 天然支持并发，水平扩展
- 适合远程访问 / AI 编程客户端

---

## 8. 维护说明

### 修改调用链时的检查清单

1. Core 层接口变更 → 同步更新 4 个接口层（FFI/HTTP/Python/MCP）
2. Storage trait 新增方法 → LocalStorage/SqliteStorage/CachedStorage 三处实现
3. SummaryView 字段变更 → 序列化兼容性测试（旧 JSON 必须能反序列化）
4. 新增标签 → 更新 `Tag` enum + `Display` 实现 + `is_high_value_turn` 判定
5. 评分维度扩展 → 更新 `ScoreWeights` 默认值 + `DefaultScorer` 实现

### 代码引用更新规则

- 重命名文件时，全文搜索旧路径，批量替换
- 行号变化时，通过 Grep 重新定位关键函数
- 新增模块时，在本文件添加对应调用链章节

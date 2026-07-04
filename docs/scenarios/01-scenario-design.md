# 场景设定集

> 列出所有推演场景的设定参数、用户画像、配置项。
> 新场景接入前，先在本文件登记设定，再编写推演文档。

## 场景一：AI 编程助手 MCP 集成（已推演）

### 用户画像

- **用户**：独立开发者小林
- **技术水平**：熟悉 Rust + Axum，5 年后端经验
- **使用工具**：集成 Hippocampus MCP Server 的 Claude Code
- **开发习惯**：长会话（4+ 小时）、跨会话频繁、需要追溯历史决策

### 项目设定

| 参数 | 值 | 说明 |
|------|----|----|
| 项目名 | `shop-backend` | 电商后台 API |
| 技术栈 | Rust + Axum 0.8 + SQLx + PostgreSQL | 后端服务 |
| 会话 ID | `shop-backend-session` | 绑定项目，跨会话复用 |
| 项目 ID | `shop-backend` | 用于 project 级聚合索引 |
| 接入方式 | MCP Server（stdio） | Claude Code 自动拉起子进程 |

### Hippocampus 配置

| 配置项 | 值 | 说明 |
|--------|----|----|
| `token_threshold` | 400K | 软阈值，达到后等待当前轮次完成 |
| `force_truncate_limit` | 600K | 硬上限（1.5 倍），强制截断 |
| `wait_for_turn_completion` | true | 等待 Agent 任务/工具调用链完成 |
| 周期任务触发 | 每周一 23:00 / 每月 1 日 23:00 | 手动或定时调用 compaction |
| 存储后端 | LocalStorage | 文件树 + RwLock + 原子写入 |
| 评分器 | DefaultScorer | 启发式 3 维（时效性/访问频率/importance） |

### 推演时间线

- **Day 1**（周一）：首次会话，需求讨论（80K token）
- **Day 2-3**：实现 Product CRUD + Order 创建（330K token）
- **Day 5**（周五）：触发首次硬上限截断（600K token）
- **Week 1 末**：触发 weekly_merge（4 个 daily → 1 个 weekly）
- **Week 2-3**：持续累积（每周 3 个 daily 文件）
- **Week 4 末**：触发 monthly_evict（3 个 weekly → 1 个 monthly，淘汰 1.35M）

### 关键验证点

- [x] 首次会话 `prompt` tool 返回空字符串
- [x] 跨会话 `retrieve` 按需加载历史细节
- [x] 硬上限截断标记 `truncated=true`
- [x] weekly_merge 寒暄剥离 + 无损去重
- [x] monthly_evict 4 维评分淘汰 + 高价值 Turn 保留
- [x] 冲突检测（JWT 过期时间从 1h 改 24h 触发 DirectContradict）

完整推演见 [03-mcp-coding-assistant.md](./03-mcp-coding-assistant.md)。

---

## 场景二：RAG 框架时序记忆后端（待推演）

### 用户画像

- **用户**：数据科学家小王
- **使用工具**：LlamaIndex + Hippocampus ChatStore 适配器
- **场景特点**：多用户并发查询、需要跨会话保留用户偏好

### 项目设定

| 参数 | 值 | 说明 |
|------|----|----|
| 项目名 | `customer-service-bot` | 客服机器人 |
| 技术栈 | Python + LlamaIndex + Hippocampus Python 绑定 | RAG 应用 |
| 接入方式 | Python 原生绑定（PyO3） | 直接 `import hippocampus_python` |
| 并发模型 | 多实例（每会话一个 Hippocampus 对象） | GIL 约束下的串行调用 |

### Hippocampus 配置

| 配置项 | 值 | 说明 |
|--------|----|----|
| `token_threshold` | 100K | 客服会话较短，阈值降低 |
| `force_truncate_limit` | 150K | 1.5 倍硬上限 |
| 存储后端 | SqliteStorage | WAL + r2d2 连接池，支持并发读 |
| 缓存层 | CachedStorage<SqliteStorage> | moka 三级缓存 |

### 待验证点

- [ ] 多会话并发归档不串号
- [ ] CachedStorage 缓存命中率
- [ ] SQLite WAL 并发读性能
- [ ] project 级聚合检索跨 session 查询

---

## 场景三：多 Agent 编排统一记忆层（待推演）

### 用户画像

- **用户**：AI 平台工程师
- **使用工具**：LangGraph + Hippocampus HTTP REST API
- **场景特点**：多个 Agent 共享同一记忆库，需要隔离 + 共享

### 项目设定

| 参数 | 值 | 说明 |
|------|----|----|
| 项目名 | `multi-agent-platform` | 多 Agent 协作平台 |
| 技术栈 | Python + LangGraph + Hippocampus HTTP REST | 分布式 |
| 接入方式 | HTTP REST API | 远程访问，多语言客户端 |
| 部署 | hippocampus-server 多实例 + 负载均衡 | 水平扩展 |

### Hippocampus 配置

| 配置项 | 值 | 说明 |
|--------|----|----|
| 会话隔离 | 每 Agent 独立 session_id | Agent 间不互相干扰 |
| 项目共享 | 同一 project_id | 项目级聚合检索 |
| 存储后端 | LocalStorage（共享文件系统） | 多实例无状态 |
| 语义检索 | HybridRetriever（BM25 + 向量） | 配置 Embedder API |

### 待验证点

- [ ] HTTP 无状态设计的水平扩展能力
- [ ] 多 Agent 通过 project_id 共享记忆
- [ ] HybridRetriever 语义检索效果
- [ ] Session 级索引隔离（v2.8 SessionSearchRouter）

---

## 场景四：嵌入式 / 桌面应用本地记忆（待推演）

### 用户画像

- **用户**：桌面应用开发者
- **使用工具**：Tauri + Hippocampus C ABI
- **场景特点**：零外部依赖、单二进制部署、本地隐私

### 项目设定

| 参数 | 值 | 说明 |
|------|----|----|
| 项目名 | `local-memo-app` | 本地笔记 + AI 助手应用 |
| 技术栈 | Tauri (Rust) + Hippocampus C ABI | 桌面应用 |
| 接入方式 | C ABI 直接嵌入宿主进程 | 零外部依赖 |
| 部署 | 单二进制（hippocampus.dll 随应用分发） | 隐私优先 |

### Hippocampus 配置

| 配置项 | 值 | 说明 |
|--------|----|----|
| `token_threshold` | 50K | 桌面应用会话较短 |
| 存储后端 | LocalStorage | 用户本地目录 |
| 评分器 | DefaultScorer | 纯算法，无 LLM 依赖 |

### 待验证点

- [ ] C ABI 嵌入宿主进程的内存占用
- [ ] 单二进制分发流程
- [ ] 跨平台（Windows/Linux/macOS）兼容性
- [ ] 本地隐私（无网络请求）

---

## 场景五：合规审计场景（待推演）

### 用户画像

- **用户**：金融企业合规审计员
- **使用工具**：Hippocampus HTTP REST API + 审计脚本
- **场景特点**：完整对话保真归档、不可篡改、可追溯

### 项目设定

| 参数 | 值 | 说明 |
|------|----|----|
| 项目名 | `compliance-archive` | 合规审计系统 |
| 技术栈 | Rust + Hippocampus HTTP REST + 审计工具链 | 企业级 |
| 接入方式 | HTTP REST API | 集成现有审计系统 |
| 部署 | hippocampus-server + 定期备份 | 高可用 |

### Hippocampus 配置

| 配置项 | 值 | 说明 |
|--------|----|----|
| `token_threshold` | 200K | 审计场景需完整保留 |
| `force_truncate_limit` | 300K | 1.5 倍硬上限 |
| 存储后端 | SqliteStorage + 定期 WAL 备份 | 数据持久化 |
| 周期任务 | 禁用 monthly_evict | 审计场景不淘汰 |

### 待验证点

- [ ] 完整对话保真归档（非摘要）
- [ ] daily 文件不删除（可追溯）
- [ ] SQLite 存储的不可篡改性
- [ ] 冲突检测记录历史演进

---

## 场景六：Agent 编程工具全流程（已推演）

### 用户画像

- **用户**：独立开发者小李
- **使用工具**：Codex CLI（GPT-5.5）+ Hippocampus MCP Server
- **场景特点**：从零开始生产完整项目，7 天短周期高密度开发

### 项目设定

| 参数 | 值 | 说明 |
|------|----|----|
| 项目名 | `blog-backend` | 博客系统后端 API |
| 技术栈 | Rust + Axum 0.8 + SQLx + PostgreSQL | 后端服务 |
| 会话 ID | `blog-backend-session` | 跨会话复用 |
| 项目 ID | `blog-backend` | project 级聚合索引 |
| Agent 工具 | Codex CLI（GPT-5.5） | 支持 MCP 协议 |
| 预计周期 | 7 天 | 模拟真实开发节奏 |

### Hippocampus 配置

| 配置项 | 值 | 说明 |
|--------|----|----|
| `token_threshold` | 200K | Codex 单次会话较短 |
| `force_truncate_limit` | 300K | 1.5 倍硬上限 |
| 接入方式 | MCP Server（stdio） | Codex 自动拉起子进程 |
| 触发模式 | Agent 自主决策 | Codex 判断达阈值后调用 archive |

### 推演时间线

- **Day 1**：需求沟通 + 项目初始化 + 数据库 Schema 设计（2 次归档）
- **Day 2**：用户认证模块（含 Argon2→bcrypt 冲突检测）
- **Day 3**：文章 CRUD + 标签系统（2 次 retrieve）
- **Day 4**：评论系统 + 点赞功能
- **Day 5**：测试用例 + 性能优化（3 次 retrieve + 1 次 batch_retrieve）
- **Day 6**：部署 + 文档 + Bug 修复
- **Day 7（周日 23:00）**：weekly_merge（7 个 daily → 1 个 weekly，剥离 68 寒暄）

### 关键验证点

- [x] Codex 启动时自动调用 prompt
- [x] 跨会话延续无需用户重述
- [x] retrieve 按需加载历史细节
- [x] 冲突检测在决策变更时触发
- [x] batch_retrieve 减少多次调用开销
- [x] weekly_merge 寒暄剥离

完整推演见 [04-agent-coding-workflow.md](./04-agent-coding-workflow.md)。

---

## 场景登记规则

新增场景时，按以下模板登记：

```markdown
## 场景N：场景名称（状态：已推演/待推演）

### 用户画像
- **用户**：xxx
- **使用工具**：xxx
- **场景特点**：xxx

### 项目设定
| 参数 | 值 | 说明 |
|------|----|----|
| ... | ... | ... |

### Hippocampus 配置
| 配置项 | 值 | 说明 |
|--------|----|----|
| ... | ... | ... |

### 验证点
- [ ] xxx
```

登记后，创建对应的推演文档 `0N-scenario-name.md`，并在 `README.md` 目录中登记。

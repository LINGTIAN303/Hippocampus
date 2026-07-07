# Changelog

> MemoryCenter 版本演进摘要。完整变更详情请查阅 [CHANGELOG.md](https://github.com/lingtian303/MemoryCenter/blob/main/CHANGELOG.md)。
>
> 版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)，变更格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

## 当前稳定版

**v2.37（2026-07-08）—— install_rules 远程模式**

核心亮点：HTTPS MCP 模式下 `install_rules` 工具可用。当 MCP server 无法访问客户端本地路径时，返回模板内容让 LLM 用客户端 Write 工具自行创建文件。

---

## 版本时间线（倒序）

### v2.37 - install_rules 远程模式（2026-07-08）

- **新增远程模式**：MCP server 端 `project_root` 不存在时返回 `action=remote_template` + `files[]` 模板数组
- 触发场景：HTTPS MCP 模式下 DeepSeek 网页端等远程客户端接入
- 支持 catpaw / trae / claude-code 三种客户端模板，`mode` 区分 `create` 与 `append_with_markers`
- 新增 5 个测试用例（三种客户端 + 不支持类型 + 本地模式兼容性）
- **向后兼容**：本地 stdio 模式行为完全不变

### v2.36 - MCP Streamable HTTP 传输（2026-07-07）

- **新增 `/mcp` 端点**：基于 rmcp 1.8 Streamable HTTP Server，支持 POST/GET(SSE)/DELETE
- 与 REST API 共享同一个 Axum 服务，无需独立进程
- 抽离 `bootstrap` 模块供 stdio + HTTP 双入口复用 5 个 `build_*` 函数
- 新增 4 个环境变量（`MEMORY_CENTER_MCP_ENABLED` / `MEMORY_CENTER_MCP_STATEFUL` / `MEMORY_CENTER_MCP_ALLOWED_HOSTS` / `MEMORY_CENTER_MCP_ALLOWED_ORIGINS`）
- **向后兼容**：未启用 MCP 时 HTTP Server 行为与 v2.35 一致

### v2.35 - WASM 组件支持（2026-07-07）

- 新建 `memory-center-core-logic` crate（纯逻辑，可编译为 WASM）
- 新建 `memory-center-wasm` crate（wasm-bindgen 绑定 + JsStorage 注入式存储）
- `memory-center-core` 改为 facade，重导出 core-logic + 保留原生 IO 实现
- feature flag：`native`（jieba-rs+dashmap）/ `wasm`（简易字符分词 BM25）
- WASM target：wasm32-unknown-unknown，wasm-pack build 验证通过

### v2.34 - pre_compress_hook 工具（2026-07-07）

- **新增 `pre_compress_hook` MCP 工具**：压缩前一次性完整归档（与 archive 平级）
- 双轨处理：`raw_context` 原样保存 + 解析 turns 复用 Archiver
- Storage trait 新增 `write_raw_context` / `read_raw_context` / `delete_raw_context` 三方法
- 新增 `context_parser` 模块：JSON 数组 + User:/Assistant: 分隔符双解析器
- 新增 HTTP 端点 `POST /api/v1/sessions/:sid/pre-compress`

### v2.33 - 场景识别功能（2026-07-07）

- 首次 archive 时从对话内容识别场景（Coding/Writing/Research 等 7 类），写入 session 元数据
- 三种 Detector：`KeywordScenarioDetector`（关键词）/ `HttpScenarioDetector`（LLM 兜底）/ `HybridScenarioDetector`（串联）
- `resolve_effective_scenario` 4 级优先级链：用户显式 > session_meta > 识别 > Agent 默认
- Storage trait 扩展 `write_session_meta` / `read_session_meta`（默认实现向后兼容）
- 识别失败永不阻塞 archive

### v2.32 - 运行时配置查询工具 get_config（2026-07-06）

- **新增 `get_config` MCP 工具**：让 LLM 主动查询运行时配置快照
- 支持 4 种 scope：`runtime` / `preset` / `degraded` / `all`
- 新增 `RuntimeStatus` struct 记录三组件（conflict_detector / semantic_search / summary_generator）降级模式
- `with_runtime_status` 链式注入方法，启动时汇总注入

### v2.31 - Agent 上下文感知与同步归档（2026-07-06）

- **install_rules 写 AGENTS.md**：治本方案，所有客户端通用，标记隔离支持 created/updated/appended/skipped
- **prompt 返回 session 列表**：兜底方案，引导 LLM 选择正确 session_id
- 新增 `TaskStateSnapshot`：archive 时传入任务状态快照，prompt 时返回供压缩后校准
- 新增 `update_project_memory` / `get_project_memory` MCP 工具：让记忆主动流入 IDE 第 7 层 Memory Context
- Bug 修复：retrieve 不存在 hook_id 返回 500 → 404；batch_delete 死锁（RwLock 重入）
- 清理 AppState deprecated 字段（retriever / search_indexer）

### v2.29 - Presets Create 全链路落地（2026-07-05）

- 让 PresetBuilder 真正影响 archive 行为（覆盖 core / HTTP API / MCP / Python 4 层）
- archive 请求体新增可选 `preset` 字段，服务端 build 后应用 `archive_threshold` + `summary_template`
- 优先级链：用户 > scenario > model > 默认 400K
- 新增 4 个 preset_* MCP 工具（`preset_build` / `preset_list_agents` / `preset_list_scenarios` / `preset_list_models`）
- HTTP API 新增 4 个 `/api/v1/presets/*` 端点
- **向后兼容**：旧请求不传 `preset` 字段保持原行为

### v2.28 - HybridDetector 字段级 merge（2026-07-05）

- 合并 LLM 与启发式冲突报告时，从「二选一丢弃 LLM」升级为「字段级 merge」
- 字段合并规则：`severity` 取更严重 / `description` 优先 LLM / `existing_fact` 优先 LLM
- 新增 `find_duplicate_index` + `merge_conflict_fields` 方法
- 新增 9 个单元测试，50 个 conflict 模块测试全部通过

### v2.27.1 - batch_update/update_memory key_facts 注入统一（2026-07-05）

- `update_memory` 与 `batch_update` 改用 `find_hook_by_id` 获取完整 IndexHook
- 从 `IndexHook.summary.key_facts` 注入虚拟 `MemoryUpdateRecord`，逐条 `add_fact` 保持事实粒度
- 解决批量更新时 `historical_facts` 为空导致检测失效的问题

### v2.27 - 服务器端 detect_conflicts HTTP 端点（2026-07-05）

- 新增 `POST /api/v1/sessions/{sid}/memories/{hook_id}/detect-conflicts`：仅检测不写入（预检测）
- 与 MCP 端 `detect_conflicts` tool 行为一致，复用 `UpdateMemoryRequest` 请求体
- 新增生产环境 LLM 配置脚本 `deploy/setup-llm-env.sh`

### v2.26 - 自动部署配置（2026-07-05）

- 新增 `deploy/setup-auto-deploy.sh`：服务器端一次性配置裸仓库 + post-receive hook
- 新增 `deploy/post-receive.sh`：checkout → cargo build → stop → cp → start → verify
- 解决 "Text file busy" 问题：先 `systemctl stop` 再 `cp` 再 `start`
- 日常部署：`git push production main`（约 5 分钟自动编译+重启）

### v2.25 - Detector 检测失效修复 + LLM 思考模式（2026-07-05）

- **v2.24 修复**：3 个 LLM 客户端请求体加 `"thinking": {"type": "disabled"}`
  - 根因：DeepSeek V4 Flash 默认启用思考模式，输出进入 `reasoning_content` 而 `content` 为空
- **v2.25 修复**：从 `IndexHook.summary.key_facts` 注入历史事实，解决 archive 只写 turns 不写 updates 的设计缺陷
- 新增 `find_hook_by_id()` 返回完整 IndexHook
- v2.25.1：逐条 `add_fact` 替代 `join("\n")`，避免多条 key_facts 被合并成粗粒度事实

### v2.24 - API Key 鉴权中间件 + 生产部署文档（2026-07-05）

- **API Key 鉴权中间件**：`Authorization: Bearer <key>` 头校验，环境变量驱动，常量时间比对防时序侧信道
- 未配置 `MEMORY_CENTER_API_KEY` 时跳过鉴权（向后兼容）
- 新增 `docs/DEPLOY.md` 完整生产部署指南（编译 → systemd → Nginx 反代 → 验证）
- 新增 E2E 测试脚本 `deploy/test_e2e.py`（归档/检索/摘要/Prompt/公网反代 5 项）
- 新增 Nginx 配置示例

### 型号库更新（2026-07-04）

- 核查 Anthropic / OpenAI / Google / DeepSeek / Alibaba / Meta / xAI 官方文档
- 删除 7 个过期型号，新增 10 个新型号（含 Claude Opus 4.8 / Sonnet 5 / Gemini 3.1 Pro / DeepSeek V4 等）
- **破坏性变更**：5 个家族 default_variant 映射变更（详见下方破坏性变更汇总）

### v2.3 - MCP Server + 差异化定位（2026-07-03）

- **新增 `memory-center-mcp` crate**（rmcp 1.8 + tokio，stdio 传输）
- 5 个 MCP tools：`archive` / `retrieve` / `summaries` / `prompt` / `compaction`
- 每个 tool 内部创建 LocalStorage，无状态设计
- 新增 `docs/POSITIONING.md`：竞品对比矩阵 + 蓝海象限图 + 四大护城河分析
- workspace `rust-version` 从 1.83 升至 1.85

### v2.2 - Python 原生绑定（2026-07-03）

- **新增 `memory-center-python` crate**（PyO3 0.29 + maturin，cdylib）
- `MemoryCenter` pyclass：OOP 风格 + 上下文管理器 + `__repr__`
- 5 个方法（与 FFI/HTTP 一一对应）：`archive` / `retrieve` / `summaries` / `prompt` / `compaction`
- 数据类型映射：dict 字典（JSON 中间转换，零样板代码）
- 20 个 pytest 集成测试

### v2.1 - HTTP REST API（2026-07-03）

- **新增 `memory-center-server` crate**（Axum 0.8 + tower-http 0.7）
- 5 个 REST 端点（路径前缀 `/api/v1/sessions/{sid}/...`）
- 无状态设计：每次请求创建 LocalStorage，天然支持水平扩展
- 统一错误响应：`{error:{code,message}}`
- 环境变量配置：`MEMORY_CENTER_HOST` / `MEMORY_CENTER_PORT` / `MEMORY_CENTER_ROOT`
- 14 个 HTTP 集成测试

### v1.0 - MVP（2026-07-02）

- **核心库 + C ABI 动态库完整实现**
- Cargo workspace 双 crate 架构（`memory-center-core` + `memory-center-ffi`）
- 核心数据结构：`MemoryFile` / `IndexHook` / `IndexDocument` / `MessageTurn` / `Tag`（17 类标签）
- `Storage` trait + `LocalStorage`（RwLock + 原子写入 temp+rename）
- `Archiver`（归档触发）+ `Retriever`（3 核心方法）+ `Compactor`（周合并 + 月淘汰）
- 5 个 C ABI 操作 + C 头文件 `memory_center.h`
- 74 测试全部通过（51 单元 + 6 集成 + 17 FFI）

---

## 里程碑版本

| 版本 | 日期 | 里程碑 |
|------|------|--------|
| MVP | 2026-07-02 | C ABI + 核心 crate（Rust 单二进制可嵌入） |
| v2.1 | 2026-07-03 | HTTP REST API 服务 |
| v2.2 | 2026-07-03 | Python 原生绑定（PyO3） |
| v2.3 | 2026-07-03 | MCP Server（stdio）+ 预设系统 |
| v2.35 | 2026-07-07 | WASM 组件支持（浏览器/Edge 场景） |
| v2.36 | 2026-07-07 | MCP Streamable HTTP（远程多客户端共享） |
| v2.37 | 2026-07-08 | install_rules 远程模式（HTTPS MCP 全场景可用） |

---

## 破坏性变更汇总

| 版本 | 变更 | 迁移方式 |
|------|------|----------|
| 型号库更新（2026-07-04） | 删除 7 个旧型号构造器 | `claude_opus_4_5()` → `claude_opus_4_8()`；`claude_sonnet_4_5()` → `claude_sonnet_5()`；`gemini_3_pro()` → `gemini_3_1_pro()`；`deepseek_v3_2()` / `deepseek_r1()` → `deepseek_v4_pro()` 或 `deepseek_v4_flash()`；`qwen_3()` → `qwen_3_coder()`；`llama_4()` → `llama_4_scout()` 或 `llama_4_maverick()` |
| 型号库更新（2026-07-04） | 5 个家族 default_variant 映射变更 | Claude → opus-4.8；Gemini → 3.1-pro；DeepSeek → v4-pro；Qwen → 3-coder；Llama → 4-scout |
| v2.3（2026-07-03） | workspace `rust-version` 从 1.83 升至 1.85 | 升级 Rust 工具链至 1.85+ |
| v2.2（2026-07-03） | workspace `rust-version` 从 1.75 升至 1.83 | 升级 Rust 工具链至 1.83+ |

> 其余版本（v2.24 / v2.27 / v2.29 / v2.31 ~ v2.37）均保持向后兼容：旧请求不传新字段时行为不变，新增 Storage trait 方法提供默认实现，feature flag 默认 `native`。

---

## 路线图

### 即将到来

- **v2.4+**：Node.js / Go / Java 绑定（待 WASM 生态成熟后推进）

### 长期目标

- 更多 Agent 预设（覆盖主流 IDE / CLI Agent）
- 场景扩展（金融分析 / 法律研究 / 医疗等垂直领域）
- 存储后端扩展（PostgreSQL / 向量库可选集成）

---

## 下一步

- 返回 [Home](Home)
- 查看 [Architecture](Architecture)
- 查看 [Deployment](Deployment)

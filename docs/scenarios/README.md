# MemoryCenter 推演文档集

> 记录项目在真实场景中被使用的流程，以及内部代码逻辑被调用的全过程。
> 本文档集用于辅助后续开发、验证架构合理性、辅助新场景接入设计。

## 文档目的

1. **架构验证**：通过真实场景推演，验证三层架构（Core / Interface / Bindings）的合理性
2. **开发指引**：新功能开发时，可参照已有场景的调用链快速定位修改点
3. **场景设计**：为新接入场景（RAG 框架 / 多 Agent 编排 / 嵌入式应用）提供参考模板
4. **教学参考**：帮助理解 MemoryCenter 内部代码逻辑如何被层层调用

## 文档目录

| 文档 | 内容 | 用途 |
|------|------|------|
| [01-scenario-design.md](./01-scenario-design.md) | 场景设定集 | 列出所有推演场景的设定参数、用户画像、配置项 |
| [02-internal-call-flow.md](./02-internal-call-flow.md) | 内部代码逻辑调用过程 | 5 个核心操作从接口层到 Core 的完整调用链（含代码引用） |
| [03-mcp-coding-assistant.md](./03-mcp-coding-assistant.md) | AI 编程助手 MCP 场景 4 周推演 | 跨 4 周的记忆生命周期演化全过程 |
| [04-agent-coding-workflow.md](./04-agent-coding-workflow.md) | Agent 编程工具全流程推演 | Codex + GPT-5.5 从零生产项目 7 天全流程，MemoryCenter 在 Agent 工作流中的定位 |

## 阅读顺序建议

### 第一次阅读（理解项目）

1. `01-scenario-design.md` → 了解场景设定与用户画像
2. `02-internal-call-flow.md` → 理解代码调用链
3. `04-agent-coding-workflow.md` → 看 Agent 编程工具全流程（最贴近用户视角）
4. `03-mcp-coding-assistant.md` → 看长期演化的 4 周推演

### 开发新功能时

1. 先查 `02-internal-call-flow.md` 定位调用链
2. 参考相关场景的推演文档，理解现有行为
3. 设计新功能的调用链路，评估是否需要扩展接口层

### 接入新场景时

1. 在 `01-scenario-design.md` 新增场景设定
2. 参照 `03-mcp-coding-assistant.md` 格式编写新推演文档
3. 交叉验证内部调用链是否覆盖新场景需求

## 后续扩展计划

随着项目演进，将逐步补充以下场景推演：

- **RAG 框架时序记忆后端**：LlamaIndex ChatStore 适配器接入推演
- **多 Agent 编排统一记忆层**：LangGraph 多 Agent 共享记忆推演
- **嵌入式 / 桌面应用**：C ABI 直接嵌入 Electron/Tauri 应用推演
- **合规审计场景**：完整对话保真归档 + 不可篡改推演

## 维护规则

- 每次大版本（v2.x）发布后，补充对应场景的推演
- 内部代码调用链变更时，同步更新 `02-internal-call-flow.md`
- 新场景接入后，在 `01-scenario-design.md` 登记设定
- 文档中所有代码引用必须带文件路径与行号，便于跳转

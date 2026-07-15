# MemoryCenter 特配 Crate 定位与架构设计

> 本文档定位 5 个特配 crate（agents / scenarios / skills / windows / models）在项目中的架构位置，并规划后续更新策略。
>
> 配置清单见姊妹文档：[preset-crates-inventory.md](preset-crates-inventory.md)（Wiki 镜像：[Preset Crates](https://github.com/LINGTIAN303/MemoryCenter/wiki/Preset-Crates)）。

## 概述

MemoryCenter 通过 5 个特配 crate 提供配置能力，覆盖 **Agent、场景、技能、窗口、模型** 5 个维度。这些维度最终由 `memory-center-presets` 组合层统一装配成 `CombinedProfile`，驱动归档、检索、评分等行为。

本文档基于源码全项目调研（2026-07-15）形成，列出 5 个 crate 的依赖拓扑、影响面、定位边界、待改进问题与更新策略，作为后续 crate 更新工作的规划依据。

**适用读者**：

- 想理解 5 个特配 crate 在整体架构中位置的**开发者**
- 想为 5 个 crate 贡献扩展的**贡献者**
- 想评估 5 个 crate 改动影响面的**维护者**

**文档版本**：2026-07-15 v1（基于源码同步）

---

## 章节 1：依赖拓扑

### 1.1 分层依赖图

```
┌─────────────────────────────────────────────────────────────┐
│  Layer 0：核心层（无反向依赖）                                 │
│   memory-center-core ←── memory-center-core-logic            │
│         ▲                                                     │
│         │（5 个特配 crate 平行向下依赖 core）                   │
├─────────────────────────────────────────────────────────────┤
│  Layer 1：特配层（平行拓扑，互不依赖）                           │
│   agents   scenarios   skills   windows   models             │
│   (重导出     (10 场景   (15 技能  (6 压缩    (15 模型         │
│   11 family)  + Custom)  + Custom) + NoComp)  + tiktoken)    │
│         ▲                                                     │
│         │（presets 唯一组合层，依赖全部 5 个 + llm + reqwest）  │
├─────────────────────────────────────────────────────────────┤
│  Layer 2：组合层                                              │
│   memory-center-presets (PresetBuilder + CombinedProfile)    │
└────────────┬────────────────────────────────────────────────┘
             │
             ▼  (archive-core 依赖 presets + agents)
   memory-center-archive-core
             ▲
             │
   ┌─────────┴──────────┬──────────────┬──────────────┐
   │                    │              │              │
 server              sidecar          mcp          adapter
```

### 1.2 关键设计原则

1. **平行拓扑**：5 个 crate 互相 0 依赖（Cargo.toml 验证无误），全部仅向下依赖 `memory-center-core`
2. **联动归位**：5 个 crate 之间的联动**全部由 `memory-center-presets` 组合层处理**，特配层互不感知
3. **重依赖隔离**：`tiktoken-rs 0.6` 仅在 `memory-center-models` crate，不污染 core/presets
4. **家族/型号分离**：family（稳定枚举）+ variant（高频字符串），符合开闭原则
5. **Custom 兜底统一**：5 个 crate 都有 `Custom(String)` 变体，外部扩展无需等发版
6. **核心层解耦**：`core` / `core-logic` 与特配层完全无关联，5 个特配反向依赖 core，但 core 不反向依赖

### 1.3 archive-core 的特殊位置

`memory-center-archive-core` 是归档核心引擎，**直接依赖 `memory-center-agents`**（不只是通过 presets 间接）：

- 文件：`crates/memory-center-archive-core/src/lib.rs`
- 行 405/555：`.and_then(memory_center_agents::AgentFamily::from_str)`
- 行 407/557：`memory_center_agents::AgentFamily::Custom("unknown".to_string())`

**含义**：agents crate 的任何 `AgentFamily` / `AgentProfile` 字段变更会直接影响归档主链路，需重点防护。

---

## 章节 2：影响面矩阵

### 2.1 按被依赖广度排序

| 特配 crate | 被依赖 crate 数 | 直接消费者 | 修改风险 |
|---|---|---|---|
| **agents** | 6 | adapter / archive-core / sidecar / mcp / python / server | 🔴 最高 |
| **scenarios** | 3 | mcp / python / server | 🟡 中 |
| **models** | 3 | mcp / python / server | 🟡 中（tiktoken 重依赖） |
| **skills** | 2 | python / server（server 仅声明未用） | 🟢 低 |
| **windows** | 2 | python / server（server 仅声明未用） | 🟢 低 |

### 2.2 调用密度（实际 use 语句 + 内联调用）

基于 Grep 全项目扫描结果（不含测试代码与文档注释）：

| 特配 crate | 实际调用点（非测试） | 主要调用方 |
|---|---|---|
| agents | ~15 处 | adapter/lib.rs、archive-core/lib.rs、sidecar/opencode_db.rs、mcp/lib.rs、mcp/bootstrap.rs、presets/{builder,combined,linkage,detect,scenario_detect}.rs、server/presets.rs |
| scenarios | ~10 处 | mcp/bootstrap.rs、presets/{builder,combined,scenario_detect}.rs、server/presets.rs |
| models | ~5 处 | mcp/lib.rs、presets/{builder,combined}.rs、server/presets.rs、python/lib.rs |
| skills | ~3 处 | presets/{builder,combined}.rs、python/lib.rs |
| windows | ~3 处 | presets/{linkage,combined,builder}.rs、python/lib.rs |

### 2.3 关键观察

1. **agents 是被调用最广的特配 crate**（6 个消费者 + ~15 处调用点）——任何字段变更需联动检查 6 个 crate
2. **archive-core 直接依赖 agents**——`AgentFamily::from_str` 在归档主链路被直接调用
3. **sidecar 仅依赖 agents**——简化了 sidecar 维护，但 agents 改动会立即波及 sidecar
4. **server 声明依赖 5 个但仅用 3 个**——skills/windows 在 server 源码中未发现任何 `use` 或内联调用，属冗余依赖
5. **presets 不重导出底层类型**——调用方需各自 `use` 引入，特配层 API 变更需全项目搜索调用点
6. **测试代码占比高**——mcp/lib.rs 中对 agents/scenarios 的 use 大部分位于 `#[cfg(test)]` 模块（行 4733-5280 区间）

---

## 章节 3：定位与边界

### 3.1 定位矩阵

| crate | 维度 | 稳定性 | 迭代频率 | 主要扩展点 |
|---|---|---|---|---|
| agents | "谁在用" | family 枚举稳定 | variant 高频 | 7 个 generic 补专属 Profile/指纹/HookMode |
| scenarios | "在做什么" | 10 场景稳定 | ScoreWeights 微调 | Custom 链 + 场景自动识别 |
| models | "用什么模型" | family 稳定 | variant 高频 | Tokenizer 接入 archive-core |
| windows | "怎么压缩" | 6 scheme 稳定 | Cooperative 未实现 | Cooperative 协作模式 |
| skills | "调什么工具" | 15 技能稳定 | MemoryLink v2 | validate() + StandaloneMemory |

### 3.2 边界划分（该做 / 不该做）

| crate | ✅ 该做 | ❌ 不该做 |
|---|---|---|
| agents | Agent 识别 + family 枚举 + HookMode 分类 + AgentProfile 预设 | 不感知 scenarios/windows，不维护联动映射 |
| scenarios | 场景识别 + 5 维特配（focus/weights/tags/strategy/threshold） | 不调用 models 的 tokenizer，不做 token 计数 |
| models | 模型家族/型号/Tokenizer 抽象与实现 | 不感知 agents/windows，不驱动归档策略选择 |
| windows | 压缩方式枚举 + CooperationMode + WindowProfile | 不主动调用 agents 的 HookMode |
| skills | 技能识别 + MemoryLink + SkillProfile | 不主动决定归档时机（由 archive-core 决定） |

### 3.3 联动机制归属

**核心设计原则**：5 个 crate 之间的联动**全部由 presets 组合层处理**，特配层互不感知。

| 联动 | 实现位置 | 文件 |
|---|---|---|
| Agent → Window（ClaudeCode→180K 等） | presets | `crates/memory-center-presets/src/linkage.rs` |
| Agent → HookMode 分类 | presets（部分内联在 agents） | `agents/src/hook_mode.rs` + `presets/src/scenario_detect.rs` |
| Scenario → ScoreWeights | scenarios 内部 | `scenarios/src/score_weights.rs` |
| Model → ArchiveStrategy | models 内部 | `models/src/variant.rs` |

---

## 章节 4：当前架构优点

1. **真正的平行拓扑**：5 个 crate 互相 0 依赖，Cargo.toml 验证无误
2. **重依赖隔离到位**：tiktoken-rs 0.6 仅在 models crate，不污染 core/presets
3. **家族/型号分离**：family（稳定枚举）+ variant（高频字符串），符合开闭原则
4. **Custom 兜底统一**：5 个 crate 都有 Custom(String) 变体，外部扩展无需等发版
5. **core / core-logic 与特配层完全解耦**：5 个特配反向依赖 core，但 core 不反向依赖，分层清晰

---

## 章节 5：待改进问题

### 5.1 🔴 高优先级问题

| 编号 | 问题 | 影响 |
|---|---|---|
| P1 | **7 个 generic AgentFamily 无专属 AgentProfile**（Zcode/OpenCode/Qoder/WorkBuddy/CatPaw/OpenClaw/Marvis 全走 `generic()` 路径） | OpenCode 已支持 Real Hook 但未体现；sidecar 适配新型号 Agent 缺少 Profile 信息 |
| P2 | **OpenCode 已支持 Real Hook 但无专属 AgentProfile** | archive-core / sidecar 适配 OpenCode 时缺少压缩比/保留轮次等参数 |
| P3 | **models 的 Tokenizer 精确计数能力未被 archive-core 主链路采用**（实际用 `chars/3` 简化估算） | 长对话 token 估算偏差大，影响归档触发时机判断 |

### 5.2 🟡 中优先级问题

| 编号 | 问题 | 影响 |
|---|---|---|
| P4 | 7 个 generic family 补专属 AgentFingerprint | 当前返回空指纹无法被 `detect_agent_client` 自动识别 |
| P5 | 7 个 generic family 补 HookMode 分类 | 当前全部走 Pseudo 默认，未区分开源/闭源 |
| P6 | skills 的 `validate()` 当前永远返回 Ok | 注释标注"预留扩展点"但未实现 destructive 技能强制 AttachedToTurn 校验 |
| P7 | ✅ skills 的 MemoryLink v2 Phase 1+2 已实现（v2.52 阶段 4+5） | Phase 1（阶段 4）：新增 StandaloneMemory / LinkedToProject 变体 + archives() 语义扩展（3 变体归档）+ is_attached_to_turn() 新增（仅 AttachedToTurn 可追溯）+ destructive 校验升级。Phase 2（阶段 5）：Storage trait 扩展 4 方法 + LocalStorage 实现 standalone/linked 路径 + Retriever 新增 retrieve_standalone/linked_memories + MCP/Server/Python/Node retrieve 工具增加 link_type 参数，10 单测全过 |
| P8 | windows 的 Cooperative 协作模式未实现 | `validate()` 主动拒绝，仅 Independent 可用 |
| ✅ P9 | models 集成 sentencepiece（v2.53） | 已完成：feature gating + `spm_or_char()` helper + `MEMORY_CENTER_SPM_MODEL_PATH` 环境变量驱动降级链，详见 [sentencepiece-guide.md](sentencepiece-guide.md) |

### 5.3 🟢 低优先级问题

| 编号 | 问题 | 影响 |
|---|---|---|
| P10 | **server 声明依赖 skills/windows 但源码未实际使用** | Cargo.toml 冗余依赖（行 25-26），可清理 |
| P11 | 5 个 crate 的 Cargo.toml 普遍声明 `thiserror` / `tracing` 但部分未实际使用 | 编译产物略大 |
| P12 | agents description 写"11 主流 Agent family"（实际 11 family 中 4 主流） | 文档措辞小误 |
| P13 | scenarios description 写"7 场景预设"（实际 10 个内置场景） | 文档措辞小误 |
| P14 | ✅ 场景自动识别（HybridScenarioDetector）已接入主链路（v2.50 archive-core 的 pre_compress + archive 两处调用 `resolve_effective_scenario`，server/mcp 3 处初始化点注入） | 已完成 |

---

## 章节 6：更新策略

### 6.1 第一阶段：低风险清理（🟢 文档/依赖整理）

| 任务 | 工作量 | 风险 | 验证方式 |
|---|---|---|---|
| P10 清理 server 冗余依赖（skills/windows） | 5min | 极低 | `cargo build -p memory-center-server` |
| P11 清理 5 个 crate 未用的 thiserror/tracing | 30min | 低 | `cargo build` 全量 |
| P12/P13 修正 description 措辞 | 5min | 极低 | 文档检查 |

### 6.2 第二阶段：中风险扩展（🟡 字段/枚举补充，不破坏 API）

| 任务 | 工作量 | 风险 | 验证方式 |
|---|---|---|---|
| P1 为 7 个 generic family 补专属 AgentProfile | 1h | 中（新增构造器，不破坏 generic） | 单测 + presets 联动测试 |
| P2 OpenCode 补专属 AgentProfile | 15min | 中 | 单测 |
| P4 7 个 generic family 补专属 AgentFingerprint | 1h | 中（影响 detect_agent_client） | 集成测试 |
| P5 7 个 generic family 补 HookMode 分类 | 30min | 低（HookModeResolver 改动小） | 单测 |
| P6 skills 完善 validate() 校验逻辑 | 30min | 中（可能影响现有调用方） | 全量测试 |

### 6.3 第三阶段：高风险架构改动（🔴 主链路接入）

| 任务 | 工作量 | 风险 | 验证方式 |
|---|---|---|---|
| P3 models Tokenizer 接入 archive-core 主链路 | 2-4h | 高（影响 token 估算/归档触发） | 集成测试 + 基准对比 |
| ✅ P7 MemoryLink v2 Phase 1+2 已完成（v2.52 阶段 4+5） | 已完成 | 高（数据模型扩展） | Phase 1：skills 42 单测 + presets 96 单测 + workspace build 全过。Phase 2：Storage trait 4 方法 + LocalStorage 实现 + Retriever 新增方法 + MCP/Server/Python/Node retrieve 增加 link_type 参数 + 10 单测全过 |
| ✅ P7 MemoryLink v2 Phase 3 已完成（v2.52 阶段 6） | 已完成 | 中（4 个入口层新增写入接口） | MCP/Server/Python/Node 各新增 write_standalone_memory + write_linked_memory + build_memory_file 辅助函数，AGENTS.md 新增第 8 章触发协议，workspace 全量测试通过 |
| 📋 P8 Cooperative 协作模式设计 | 已完成设计 | 高（需要双向通信协议） | [设计文档](cooperative-design.md)（15 章节，含状态机/协议/trait 草案/降级链/6 阶段实现路线图），待评审后进入实现 |
| ✅ P9 sentencepiece 集成（v2.53） | 已完成 | 中（重依赖隔离） | 默认 feature 54 tests + 启用 feature 59 tests（57 passed + 2 ignored）全过；feature gating 默认禁用避免强制 cmake 依赖 |

---

## 章节 7：关键风险点

1. **agents crate 影响面最大**（6 个消费者）——任何 `AgentFamily`/`AgentProfile` 字段变更需联动检查：adapter / archive-core / sidecar / mcp / python / server
2. **archive-core 直接依赖 agents**（不只是通过 presets 间接）——`AgentFamily::from_str` 在 `archive-core/src/lib.rs` 行 405/555 被直接调用，agents 改动会直接影响归档主链路
3. **sidecar 仅依赖 agents**——简化了 sidecar 维护，但 agents 改动会立即波及 sidecar
4. **server 声明依赖 5 个但仅用 3 个**——清理冗余时需确认未来是否计划用 skills/windows（如 server 未来是否暴露 skills 管理端点）
5. **presets 不重导出底层类型**——调用方需各自 `use` 引入，特配层 API 变更需全项目搜索调用点
6. **测试代码占比高**——mcp/lib.rs 中对 agents/scenarios 的 use 大部分在 `#[cfg(test)]` 模块，修改 API 时需同步更新测试

---

## 章节 8：推荐执行顺序（谨慎策略）

```
阶段 0（已完成）：定位与架构设计文档（本文档）
   ↓
阶段 1：低风险清理（P10/P11/P12/P13）
   - 单 PR 提交，验证编译通过即可
   ↓
阶段 2：agents crate 补全（P1/P2/P4/P5）
   - 一个 PR 集中处理 agents，避免分散
   - 同步更新 preset-crates-inventory.md 文档
   ↓
阶段 3：skills/windows 扩展（P6/P7/P8）
   - skills validate() 优先（影响面小）✅ P6 已于 v2.52 阶段 3 实现
   - MemoryLink v2 扩展 ✅ P7 Phase 1+2 已于 v2.52 阶段 4+5 实现（Phase 1：enum 4 变体 + is_attached_to_turn() + destructive 校验升级；Phase 2：Storage trait 扩展 4 方法 + LocalStorage 实现 standalone/linked 路径 + Retriever 新增 retrieve_standalone/linked_memories + MCP/Server/Python/Node retrieve 工具增加 link_type 参数，10 单测全过）
   - MemoryLink v2 Phase 3 ✅ 已于 v2.52 阶段 6 实现（MCP/Server/Python/Node 各新增 write_standalone_memory + write_linked_memory + build_memory_file 辅助函数；AGENTS.md 新增第 8 章「主动写入 standalone/linked 记忆」触发协议；workspace 全量测试通过）
   - windows Cooperative 需要先出设计文档（P8 未实现，已标注为未来考虑）
   ↓
阶段 4：models Tokenizer 接入（P3）
   - 单独 PR，需基准对比验证 token 估算精度提升
   - 保留 chars/3 兜底（未配置 tiktoken 时降级）
   - ✅ P3 已于 v2.52 阶段 4 实现（闭包注入避免 archive-core 依赖 models）
   - ✅ P9 sentencepiece 集成已于 v2.53 实现（feature gating 默认禁用；启用 `tokenizer-sentencepiece` feature 后 Gemini/Qwen/Llama 用真实 SentencePiece 模型，未配置时降级 CharTokenizer；详见 [sentencepiece-guide.md](sentencepiece-guide.md)）
```

---

## 贡献指南

- 修改 5 个特配 crate 前，先查阅本文档的「影响面矩阵」（章节 2）确认改动范围
- 新增 family / scenario / skill / scheme / variant 时，遵循各 crate 内现有的 Custom 兜底模式
- 联动逻辑不要内联到特配 crate，统一放到 `memory-center-presets` 组合层
- 字段变更需同步更新 `docs/preset-crates-inventory.md` 配置清单与 Wiki 页面
- 测试代码与文档注释中的 `use` 语句需同步更新

## 相关文档

- [特配 Crate 配置参考](preset-crates-inventory.md)（配置清单，33 个表格）
- [Crate 选择指南](src/crate-guide.md)
- [架构文档（完整版）](ARCHITECTURE.md)
- [GitHub Wiki: Preset Crates](https://github.com/LINGTIAN303/MemoryCenter/wiki/Preset-Crates)
- [GitHub Wiki: Crate Guide](https://github.com/LINGTIAN303/MemoryCenter/wiki/Crate-Guide)

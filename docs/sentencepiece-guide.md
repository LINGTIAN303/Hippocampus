# SentencePiece 分词器启用与使用教程

> v2.53 P9 新增 · 为 Gemini / Qwen / Llama 等非 OpenAI/Claude 家族模型提供精确 token 计数

## 背景

MemoryCenter 默认通过 `CharTokenizer`（字符级近似，CJK × 1.5 / 拉丁 × 1.3 / 标点 × 0.5）估算 Gemini/Qwen/Llama 家族的 token 数。该近似在中文场景下偏差可达 **20-30%**，影响：

- 归档触发时机判断（`archive_threshold` 精度下降）
- `threshold_ratio_percent` 反馈失真
- 伪钩子模式下 LLM 的"token 意识"不准

启用 `tokenizer-sentencepiece` feature 后，可用真实 SentencePiece 模型替代字符级近似，将中文场景估算偏差降至 **5% 以内**（取决于模型选择）。

## 适用场景

| 场景 | 是否推荐启用 | 说明 |
|---|---|---|
| 主要使用 Gemini / Qwen / Llama 家族对话 | ✅ 强烈推荐 | 中文 token 估算精度显著提升 |
| 主要使用 Claude / GPT / DeepSeek 家族 | ❌ 不需要 | 这些家族用 tiktoken（O200kBase / Cl100kBase / DeepSeekApprox），SentencePiece 不参与 |
| 混合使用所有家族 | ✅ 推荐 | 启用后仅影响 Gemini/Qwen/Llama，其他家族无变化 |
| 单二进制部署、追求最小体积 | ❌ 可不启用 | sentencepiece 静态编译约 +2-5MB |
| CI/测试环境 | ❌ 可不启用 | 默认 feature 即可通过所有测试 |

## 启用步骤

### 前置条件

**SentencePiece crate 0.13.1** 使用 C++ 绑定 + `static` feature（C++ 源码静态编译），构建时需要：

- **CMake** ≥ 3.5
- **C++ 编译器**（支持 C++14）：
  - Windows：MSVC（Visual Studio Build Tools）或 MinGW
  - Linux：gcc / g++ ≥ 5
  - macOS：clang（Xcode Command Line Tools）

**验证依赖已就绪**：

```bash
# Windows (PowerShell)
cmake --version    # 应输出 3.5+
cl                 # 应进入 MSVC 编译器（或 g++ --version）

# Linux / macOS
cmake --version    # 应输出 3.5+
g++ --version      # 应输出 5+
```

若未安装 CMake：
- Windows：从 https://cmake.org/download 下载安装，勾选"添加到 PATH"
- Linux（Ubuntu/Debian）：`sudo apt install cmake build-essential`
- macOS：`brew install cmake` 或 `xcode-select --install`

### Step 1：启用 feature

在消费 `memory-center-models` 的 crate 的 `Cargo.toml` 中显式启用 feature：

```toml
[dependencies]
memory-center-models = { path = "../memory-center-models", features = ["tokenizer-sentencepiece"] }
```

**或** 通过 cargo 命令行临时启用（用于测试）：

```bash
cargo build -p memory-center-models --features tokenizer-sentencepiece
cargo test -p memory-center-models --features tokenizer-sentencepiece
```

**或** 在 workspace 根 `Cargo.toml` 中通过 `default-features` 启用（不推荐，会强制所有消费者编译 sentencepiece）：

```toml
# ❌ 不推荐：强制所有消费者编译 sentencepiece
[workspace.dependencies]
memory-center-models = { path = "crates/memory-center-models", default-features = false, features = ["tokenizer-sentencepiece"] }
```

**推荐做法**：保持 `memory-center-models` 默认不启用 feature（`default = []`），由消费方按需启用。

### Step 2：下载 SentencePiece 模型文件

SentencePiece 需要预训练的 `.model` 文件（protobuf 序列化），**不随二进制打包**（避免二进制膨胀）。

**推荐模型**：

| 模型 | 文件 | 大小 | 适用场景 | 下载命令 |
|---|---|---|---|---|
| mT5-base | `spiece.model` | ~2.4MB | 多语言（101 种，中文友好） | `huggingface-cli download google/mt5-base spiece.model --local-dir ./models` |
| NLLB-200 | `sentencepiece.bpe.model` | ~2.5MB | 200+ 语言（含小语种） | `huggingface-cli download facebook/nllb-200-distilled-600M sentencepiece.bpe.model --local-dir ./models` |
| T5-base | `spiece.model` | ~500KB | 纯英文场景 | `huggingface-cli download google-t5/t5-base spiece.model --local-dir ./models` |
| Qwen2.5 | `tokenizer.json` | ~7MB | Qwen 家族原生分词器 | `huggingface-cli download Qwen/Qwen2.5-7B tokenizer.json --local-dir ./models` |

**模型选择建议**：
- 通用场景（多语言混合）：mT5-base
- 中文为主：mT5-base（已含中文）
- 纯 Qwen 项目：Qwen2.5 原生 tokenizer（但需注意：Qwen 实际用 BPE，不是严格 SentencePiece）
- 纯英文：T5-base（体积最小）

**无 huggingface-cli 时**：直接从 Hugging Face 仓库网页下载 `spiece.model` 文件即可。

### Step 3：设置环境变量

SentencePiece 模型路径通过环境变量 `MEMORY_CENTER_SPM_MODEL_PATH` 指定：

```bash
# Linux / macOS
export MEMORY_CENTER_SPM_MODEL_PATH=/path/to/models/spiece.model

# Windows (PowerShell，永久设置)
[Environment]::SetEnvironmentVariable("MEMORY_CENTER_SPM_MODEL_PATH", "D:\models\spiece.model", "User")

# Windows (PowerShell，临时设置)
$env:MEMORY_CENTER_SPM_MODEL_PATH = "D:\models\spiece.model"

# Windows (CMD)
set MEMORY_CENTER_SPM_MODEL_PATH=D:\models\spiece.model
```

**生产环境**：将环境变量写入 systemd unit / Docker env / .env 文件：

```ini
# /etc/systemd/system/memory-center.service（示例）
[Service]
Environment="MEMORY_CENTER_SPM_MODEL_PATH=/opt/models/spiece.model"
```

```yaml
# docker-compose.yml（示例）
services:
  memory-center:
    environment:
      - MEMORY_CENTER_SPM_MODEL_PATH=/models/spiece.model
    volumes:
      - ./models:/models:ro
```

### Step 4：验证启用成功

#### 4.1 编译验证

```bash
cargo build -p memory-center-models --features tokenizer-sentencepiece
```

应输出 `Finished` 无错误。若报 `cmake not found` 或 C++ 编译错误，回到 [前置条件](#前置条件) 检查依赖。

#### 4.2 单元测试验证

```bash
cargo test -p memory-center-models --features tokenizer-sentencepiece --lib
```

应输出类似：

```
running 59 tests
test result: ok. 57 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
```

其中 `2 ignored` 是需要真实 `.model` 文件的手动测试（`test_from_env_loads_real_model` 和 `test_sentencepiece_vs_char_tokenizer_chinese`）。

#### 4.3 真实模型集成验证（需 Step 2-3 完成）

设置环境变量后运行 ignored 测试：

```bash
# 前提：MEMORY_CENTER_SPM_MODEL_PATH 已指向有效 .model 文件
MEMORY_CENTER_SPM_MODEL_PATH=./models/spiece.model \
  cargo test -p memory-center-models --features tokenizer-sentencepiece \
  test_from_env_loads_real_model -- --ignored --nocapture
```

应输出类似：

```
text='这是一段用于测试的中文文本...'
  SentencePiece: 18 tokens
  CharTokenizer: 24 tokens (CJK × 1.5)
```

#### 4.4 运行时日志验证

启动 MemoryCenter 服务后，日志应**不**出现以下 warning：

```
WARN memory_center_models::sentencepiece_impl: SentencePiece 初始化失败，降级为 CharTokenizer
```

若出现此 warning，说明环境变量未设置或 `.model` 文件加载失败，见 [故障排查](#故障排查)。

## 降级机制（容错设计）

SentencePiece 集成采用**多层降级链**，确保任何环节失败都不会中断主链路：

```
┌──────────────────────────────────────────────────────────────┐
│  启用 tokenizer-sentencepiece feature？                       │
│  ├─ 否 → CharTokenizer（编译时降级，无 sentencepiece 依赖）    │
│  └─ 是 ↓                                                      │
│                                                              │
│  MEMORY_CENTER_SPM_MODEL_PATH 已设置？                       │
│  ├─ 否 → CharTokenizer + warn 日志（运行时降级）              │
│  └─ 是 ↓                                                      │
│                                                              │
│  .model 文件加载成功？                                        │
│  ├─ 否 → CharTokenizer + warn 日志（运行时降级）              │
│  └─ 是 ↓                                                      │
│                                                              │
│  ✅ SentencePieceTokenizer（精确 token 计数）                 │
└──────────────────────────────────────────────────────────────┘
```

**降级特征**：

- **编译时降级**（feature 未启用）：`TokenizerKind::spm_or_char()` 返回 `CharacterBased`，`SentencePiece` 变体不存在于枚举中
- **运行时降级**（环境变量未设置 / 文件加载失败）：`TokenizerKind::SentencePiece.build()` 返回 `CharTokenizer`，并打印 warn 日志

**设计原则**：归档主链路（`ArchiveEngine`）永远不 panic，任何 tokenizer 失败都降级为字符级估算。

## 故障排查

### 1. 编译错误：`cmake not found`

**原因**：系统未安装 CMake。

**解决**：

```bash
# Ubuntu / Debian
sudo apt install cmake

# macOS
brew install cmake

# Windows：从 https://cmake.org/download 安装并勾选"添加到 PATH"
```

### 2. 编译错误：`C++ compiler not found`

**原因**：缺少 C++ 编译器。

**解决**：

```bash
# Ubuntu / Debian
sudo apt install build-essential

# macOS
xcode-select --install

# Windows：安装 Visual Studio Build Tools（含 C++ 桌面开发工作负载）
```

### 3. 编译错误：`sentencepiece` 链接失败

**原因**：`static` feature 编译 C++ 源码时资源不足或路径含中文。

**解决**：

- 确保项目路径无中文/空格（`sentencepiece` 0.13.1 的已知问题）
- 增加 swap 空间（Linux 内存不足时）
- 尝试 `cargo clean && cargo build` 清理缓存重编译

### 4. 运行时 warning：`SentencePiece 初始化失败`

**日志示例**：

```
WARN memory_center_models::sentencepiece_impl: SentencePiece 初始化失败，
     降级为 CharTokenizer（检查 MEMORY_CENTER_SPM_MODEL_PATH 环境变量）
```

**排查步骤**：

```bash
# 1. 检查环境变量是否设置
echo $MEMORY_CENTER_SPM_MODEL_PATH

# 2. 检查文件是否存在
ls -la $MEMORY_CENTER_SPM_MODEL_PATH

# 3. 检查文件是否为有效 protobuf（非空且非文本）
file $MEMORY_CENTER_SPM_MODEL_PATH  # 应输出 "data" 或类似，非 "ASCII text"

# 4. 检查进程是否能看到环境变量（systemd 服务常见问题）
sudo systemctl show memory-center --property=Environment
```

### 5. 测试 `test_from_env_loads_real_model` 失败

**原因**：未设置 `MEMORY_CENTER_SPM_MODEL_PATH` 或文件无效。

**解决**：

```bash
# 确保环境变量指向有效 .model 文件
export MEMORY_CENTER_SPM_MODEL_PATH=/absolute/path/to/spiece.model

# 重新运行（注意 --ignored 必须配合 --features）
cargo test -p memory-center-models --features tokenizer-sentencepiece \
  test_from_env_loads_real_model -- --ignored --nocapture
```

### 6. 启用 feature 后其他 crate 编译失败

**原因**：消费方 crate 未正确传递 feature。

**解决**：在消费方 crate 的 `Cargo.toml` 中显式启用 feature：

```toml
[dependencies]
memory-center-models = { path = "../../crates/memory-center-models", features = ["tokenizer-sentencepiece"] }
```

**不要**只在命令行 `--features` 启用（命令行只对当前 crate 生效，不传递给依赖）。

## 开发者参考

### Feature Gating 模式

sentencepiece 模块通过 `#[cfg(feature = "tokenizer-sentencepiece")]` 条件编译，未启用 feature 时**所有 sentencepiece 相关代码不参与编译**：

```rust
// crates/memory-center-models/src/lib.rs
#[cfg(feature = "tokenizer-sentencepiece")]
pub mod sentencepiece_impl;

#[cfg(feature = "tokenizer-sentencepiece")]
pub use sentencepiece_impl::SentencePieceTokenizer;
```

**好处**：
- 未启用 feature 的用户无 cmake / C++ 编译器依赖
- 二进制体积不增加（约节省 2-5MB）
- 编译时间不增加（避免编译 C++ 源码）

### `spm_or_char()` 智能选择 helper

`TokenizerKind::spm_or_char()` 根据是否启用 feature 在**编译时**返回不同变体：

```rust
// crates/memory-center-models/src/tokenizer.rs
pub fn spm_or_char() -> Self {
    #[cfg(feature = "tokenizer-sentencepiece")]
    { Self::SentencePiece }
    #[cfg(not(feature = "tokenizer-sentencepiece"))]
    { Self::CharacterBased }
}
```

**用途**：在 `family.rs` 和 `variant.rs` 中为 Gemini/Qwen/Llama 家族设置默认 tokenizer，无需 `#[cfg]` 包裹：

```rust
// crates/memory-center-models/src/family.rs
Self::Gemini => TokenizerKind::spm_or_char(),
Self::Qwen => TokenizerKind::spm_or_char(),
Self::Llama => TokenizerKind::spm_or_char(),
```

### 序列化行为

`TokenizerKind` 的 `Serialize` / `Deserialize` 实现对 `SentencePiece` 变体有特殊处理：

| 场景 | 序列化值 | 反序列化行为 |
|---|---|---|
| 启用 feature 时序列化 `SentencePiece` | `"sentencepiece"` | 返回 `SentencePiece` 变体 |
| 未启用 feature 时反序列化 `"sentencepiece"` | — | 降级为 `CharacterBased` + warn 日志 |

**设计意图**：保证配置文件跨环境可移植（在启用 feature 的环境保存的配置，复制到未启用 feature 的环境时自动降级，不报错）。

### 文件清单

P9 sentencepiece 集成涉及的文件：

| 文件 | 改动 | 说明 |
|---|---|---|
| `Cargo.toml`（workspace 根） | 新增 `sentencepiece` 到 `[workspace.dependencies]` | 0.13 + static feature |
| `crates/memory-center-models/Cargo.toml` | 新增 `[features]` 段 + sentencepiece 可选依赖 | `tokenizer-sentencepiece = ["dep:sentencepiece"]` |
| `crates/memory-center-models/src/lib.rs` | 新增 `sentencepiece_impl` 模块声明 + 条件导出 | feature gating |
| `crates/memory-center-models/src/sentencepiece_impl.rs` | **新增文件** | SentencePieceTokenizer 实现 + 5 单测 |
| `crates/memory-center-models/src/tokenizer.rs` | 新增 `SentencePiece` 变体 + `spm_or_char()` helper | feature gating + 5 单测 |
| `crates/memory-center-models/src/family.rs` | Gemini/Qwen/Llama 默认 tokenizer 改为 `spm_or_char()` | 智能选择 |
| `crates/memory-center-models/src/variant.rs` | 4 个型号构造器的 tokenizer 字段更新 | gemini_3_1_pro / qwen_3_coder / llama_4_scout / llama_4_maverick |

### API 参考

#### `SentencePieceTokenizer::from_file(path: &str) -> Result<Self, String>`

从指定路径加载 `.model` 文件。

```rust
use memory_center_models::SentencePieceTokenizer;
use memory_center_models::tokenizer::Tokenizer;

let sp = SentencePieceTokenizer::from_file("/path/to/spiece.model")?;
let count = sp.count_tokens("你好世界");
```

#### `SentencePieceTokenizer::from_env() -> Result<Self, String>`

从环境变量 `MEMORY_CENTER_SPM_MODEL_PATH` 加载。

```rust
let sp = SentencePieceTokenizer::from_env()?;
```

#### `Tokenizer::count_tokens(&self, text: &str) -> usize`

计算文本的 token 数。编码失败时降级为字符级估算（`chars × 1.5`），不 panic。

#### `TokenizerKind::spm_or_char() -> Self`

智能选择 SentencePiece 或 CharTokenizer（编译时决定）。

#### `TokenizerKind::SentencePiece.build() -> Arc<dyn Tokenizer>`

构建 SentencePiece 实例。失败时降级为 CharTokenizer + warn 日志。

### 测试覆盖

| 测试名 | 类型 | 是否需要 .model 文件 |
|---|---|---|
| `test_from_file_nonexistent_returns_err` | 错误路径 | 否 |
| `test_from_env_unset_returns_err` | 错误路径 | 否 |
| `test_env_constant_value` | 常量验证 | 否 |
| `test_from_env_loads_real_model` | 真实模型 | ✅ 需要（`#[ignore]`） |
| `test_sentencepiece_vs_char_tokenizer_chinese` | 对比测试 | ✅ 需要（`#[ignore]`） |
| `test_spm_or_char_returns_correct_kind_based_on_feature` | feature gating | 否 |
| `test_sentencepiece_type_name_roundtrip` | 序列化往返 | 否 |
| `test_sentencepiece_build_does_not_panic_without_model` | 降级验证 | 否 |
| `test_debug_format_includes_sentencepiece` | Debug 输出 | 否 |

## 与其他 Tokenizer 的关系

| 家族 | 默认 Tokenizer | 启用 feature 后 | 说明 |
|---|---|---|---|
| Claude | ClaudeApprox | ClaudeApprox（不变） | tiktoken cl100k + 系数 1.05 |
| GPT | O200kBase | O200kBase（不变） | tiktoken o200k_base |
| DeepSeek | DeepSeekApprox | DeepSeekApprox（不变） | tiktoken cl100k + 系数 1.1 |
| Grok | O200kBase | O200kBase（不变） | tiktoken o200k_base |
| **Gemini** | CharacterBased | **SentencePiece** | ✅ P9 提升 |
| **Qwen** | CharacterBased | **SentencePiece** | ✅ P9 提升 |
| **Llama** | CharacterBased | **SentencePiece** | ✅ P9 提升 |
| Local | CharacterBased | CharacterBased（不变） | 本地小模型 |
| Custom | CharacterBased | CharacterBased（不变） | 用户自定义 |

**关键点**：SentencePiece 与 tiktoken 互不干扰，启用 feature 仅影响 Gemini/Qwen/Llama 三个家族。

## 性能考量

### 编译时

- `static` feature 会编译 sentencepiece C++ 源码（约 30-60s 首次编译）
- 后续增量编译无额外开销（缓存命中）
- 二进制体积增加约 **2-5MB**

### 运行时

- `.model` 文件加载：约 **50-200ms**（启动时一次性，mT5-base ~2.4MB 约 80ms）
- `count_tokens()` 调用：约 **0.1-1ms/KB**（取决于文本长度和模型复杂度）
- 内存占用：约 **5-20MB**（词表加载到内存）

**对比 CharTokenizer**：

| 指标 | CharTokenizer | SentencePiece |
|---|---|---|
| 启动耗时 | <1ms | 50-200ms |
| 单次计数 | <0.01ms/KB | 0.1-1ms/KB |
| 内存占用 | 0 | 5-20MB |
| 中文精度偏差 | 20-30% | <5% |

**结论**：对长会话归档触发判断，启动 + 计数的开销远小于精度提升带来的收益（避免误触发归档或漏触发归档）。

## 版本历史

| 版本 | 变更 |
|---|---|
| v2.53 P9 | 初始实现：feature gating + spm_or_char() helper + 环境变量驱动降级链 |

## 相关文档

- [特配 Crate 配置参考](preset-crates-inventory.md) — 5.5 TokenizerKind 枚举、5.8 扩展路线图
- [特配 Crate 架构设计](preset-crates-architecture.md) — P9 状态、推荐执行顺序
- [架构文档（完整版）](ARCHITECTURE.md) — 整体架构
- [sentencepiece crate 文档](https://docs.rs/sentencepiece/0.13.1/sentencepiece/struct.SentencePieceProcessor.html) — API 参考
- [SentencePiece 项目主页](https://github.com/google/sentencepiece) — 模型训练与原理

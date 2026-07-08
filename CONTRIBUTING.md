# 贡献指南

感谢你对 MemoryCenter 的关注！本文档说明如何参与本项目。

## 行为准则

参与本项目即代表你同意遵守 [Code of Conduct](CODE_OF_CONDUCT.md)。请在所有交流中保持尊重与包容。

## 如何贡献

### 报告 Bug

- 使用 [Bug Report 模板](https://github.com/LINGTIAN303/MemoryCenter/issues/new?template=bug_report.yml) 提交 Issue
- 附带可复现的最小示例（代码 + 输入 + 期望输出 + 实际输出）
- 注明环境信息（OS / Rust 版本 / MemoryCenter 版本）

### 提交功能请求

- 使用 [Feature Request 模板](https://github.com/LINGTIAN303/MemoryCenter/issues/new?template=feature_request.yml) 提交 Issue
- 说明使用场景与期望效果，而不只是实现方案

### 提交代码

1. **Fork 仓库** 并创建分支：`feat/<short-name>` 或 `fix/<short-name>`
2. **开发前先开 Issue**：重大改动请先在 Issue 中讨论方案，避免做无用功
3. **遵循 Conventional Commits**：

   ```
   feat(mcp): 新增 pre_compress_hook 工具
   fix(core-logic): 修复 BM25 索引未清空问题
   refactor(search): 重构语义检索路由器
   docs(api): 补全 REST API 文档
   test(python): 增加 5 个 PyO3 集成测试
   chore(deps): 升级 tokio 至 1.45
   ```

4. **代码风格**：
   - Rust：`cargo fmt --all` 格式化，`cargo clippy --all-targets -- -D warnings` 零警告
   - Python：遵循 PEP 8，类型注解必填
   - 中文注释为主，专业术语保留英文
5. **测试要求**：
   - 新增功能必须附带单元测试
   - Bug 修复必须附带回归测试
   - 提交前本地跑通：`cargo test --workspace`
6. **提交 PR**：
   - 使用 [PR 模板](.github/PULL_REQUEST_TEMPLATE.md) 自检清单
   - 一次 PR 只做一件事，避免混合多个无关变更

## 开发环境

```bash
# 克隆仓库
git clone https://github.com/LINGTIAN303/MemoryCenter.git
cd MemoryCenter

# Rust 1.88+（napi 3.x 要求）
rustc --version  # 确认版本

# 构建全部 crate
cargo build --workspace

# 运行全部测试（185+ 测试）
cargo test --workspace

# 构建 Python 绑定（可选）
cd crates/memory-center-python
pip install maturin
maturin develop --release
```

## 项目结构

```
MemoryCenter/
├── crates/                    # 17 个 Rust crate
│   ├── memory-center-core-logic/   # 纯逻辑核心（可 WASM）
│   ├── memory-center-core/         # Facade crate
│   ├── memory-center-ffi/          # C ABI 动态库
│   ├── memory-center-server/       # Axum HTTP + MCP Streamable HTTP
│   ├── memory-center-mcp/          # MCP Server（stdio + HTTP）
│   ├── memory-center-python/       # PyO3 绑定
│   ├── memory-center-node/         # napi-rs 绑定
│   ├── memory-center-wasm/         # wasm-bindgen
│   └── ...                         # 模型/场景/搜索等模块
├── examples/                  # C / Python 示例
├── docs/                      # 架构文档
├── .github/                   # CI / Issue 模板
└── Cargo.toml                 # workspace 根配置
```

## Crate 边界约定

- **`core-logic`**：纯逻辑，无 IO 依赖，可编译为 WASM
- **`core`**：Facade crate，重导出 core-logic + 原生 IO 实现
- **业务 crate** 不可反向依赖入口 crate（server / mcp / ffi）
- **公共 trait** 放在 `core-logic` 或 `core`，不可放在业务 crate
- 新增 crate 需先在 Issue 讨论，确认必要性

## 版本与发布

- 遵循 [SemVer](https://semver.org/lang/zh-CN/) 语义化版本
- 破坏性变更需在 CHANGELOG.md 显著标注，并提供迁移指南
- Release 由 maintainer 通过 git tag 触发 GitHub Actions 自动构建

## License

提交的代码将遵循 [MIT License](LICENSE)。

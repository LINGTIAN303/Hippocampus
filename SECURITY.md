# 安全策略

## 报告漏洞

如果你发现 MemoryCenter 存在安全漏洞，请**不要**通过公开 Issue 报告。

请通过以下方式私密上报：

1. 发送邮件至仓库 owner 的 GitHub 邮箱（见个人主页）
2. 或在 GitHub [Security Advisories](https://github.com/LINGTIAN303/MemoryCenter/security/advisories/new) 提交私密报告

请在报告中包含：

- 漏洞类型（如 SQL 注入 / 路径穿越 / 远程代码执行）
- 受影响的版本与平台
- 复现步骤（最小可复现示例）
- 影响评估与可能的利用方式
- 建议的修复方案（可选）

## 响应时间

- **确认收到**：3 个工作日内
- **初步评估**：7 个工作日内
- **修复发布**：根据严重程度，30 天内发布补丁版本

## 支持的版本

| 版本 | 支持状态 |
|------|---------|
| 0.1.x（最新 main） | ✅ 接收安全修复 |
| 旧版本 | ❌ 不支持 |

由于项目处于 0.x 阶段，仅最新 main 分支接收安全修复。

## 已知安全设计

- **SQL 参数化**：所有数据库查询使用参数化绑定，禁止字符串拼接
- **路径校验**：存储根目录与 session_id 经过路径穿越校验
- **DNS Rebinding 防护**：MCP Streamable HTTP 模式默认仅允许 loopback Host
- **CORS 防护**：MCP HTTP 端点支持 `allowed_origins` 配置
- **敏感配置**：API Key / 密码通过环境变量，禁止硬编码

## 部署安全建议

- 生产环境务必设置 `MEMORY_CENTER_MCP_ALLOWED_HOSTS` 为实际域名
- 启用 `MEMORY_CENTER_MCP_ALLOWED_ORIGINS` 限制跨域来源
- 使用反向代理（Nginx / Caddy）启用 HTTPS
- 数据目录设置最小权限（`chmod 700`）

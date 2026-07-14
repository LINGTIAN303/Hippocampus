#!/bin/bash
# pre-commit 钩子：扫描暂存区文件，防止密钥泄露
# 安装方式：cp scripts/pre-commit-secret-scan.sh .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
#
# 检测的 8 类密钥模式：
#   1. SiliconFlow API Key（sk-xxx）
#   2. OpenAI API Key（sk-proj-xxx / sk-xxx）
#   3. AWS Access Key（AKIAxxx）
#   4. GitHub Token（ghp_xxx / gho_xxx / ghs_xxx / ghu_xxx）
#   5. 通用 API Key 赋值（api_key="xxx" / api_key: xxx）
#   6. SSH 私钥头（-----BEGIN ... PRIVATE KEY-----）
#   7. 服务器 root 登录凭据（root@<ip>）
#   8. Bearer Token（Authorization: Bearer xxx）
#
# 误报处理：在 .secrets.baseline 中加入白名单（detect-secrets baseline）
set -e

# 检测是否有暂存文件
STAGED_FILES=$(git diff --cached --name-only --diff-filter=ACM | grep -v -E '\.(lock|snap|png|jpg|jpeg|gif|svg|woff|woff2|ttf|eot|pdf)$' || true)
if [ -z "$STAGED_FILES" ]; then
    exit 0
fi

echo "🔍 扫描暂存文件中的密钥..."

# 检测函数：在暂存内容中匹配正则
check_pattern() {
    local name="$1"
    local pattern="$2"
    local matches
    matches=$(git diff --cached --diff-filter=ACM -U0 | grep -E "$pattern" || true)
    if [ -n "$matches" ]; then
        echo "❌ 检测到可能的密钥泄露：$name"
        echo "$matches" | head -5
        echo ""
        return 1
    fi
    return 0
}

FAILED=0

# 1. SiliconFlow / 通用 sk- 开头密钥
check_pattern "SiliconFlow/通用 API Key (sk-xxx)" 'sk-[a-zA-Z0-9]{20,}' || FAILED=1

# 2. OpenAI API Key（含 sk-proj- 前缀）
check_pattern "OpenAI API Key (sk-proj-xxx)" 'sk-proj-[a-zA-Z0-9_-]{20,}' || FAILED=1

# 3. AWS Access Key
check_pattern "AWS Access Key (AKIAxxx)" 'AKIA[0-9A-Z]{16}' || FAILED=1

# 4. GitHub Token
check_pattern "GitHub Token (ghp_/gho_/ghs_/ghu_)" 'gh[posu]_[A-Za-z0-9]{36}' || FAILED=1

# 5. 通用 API Key 赋值（api_key="..." / api_key: "..."，排除占位符和 env 读取）
check_pattern "通用 API Key 赋值" '(api_key|apikey|API_KEY)\s*[=:]\s*["\x27][^"\x27$]{16,}["\x27]' || FAILED=1

# 6. SSH 私钥头
check_pattern "SSH 私钥" '-----BEGIN [A-Z ]*PRIVATE KEY-----' || FAILED=1

# 7. 服务器 root 登录凭据
check_pattern "服务器 root 登录凭据 (root@<ip>)" 'root@[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}' || FAILED=1

# 8. Bearer Token（排除明显的占位符 <key>、xxx、example）
check_pattern "Bearer Token" 'Authorization:\s*Bearer\s+[a-zA-Z0-9_-]{20,}' || FAILED=1

# 额外检查：MEMORY_CENTER_API_KEY 硬编码（应通过环境变量）
check_pattern "MEMORY_CENTER_API_KEY 硬编码" 'MEMORY_CENTER_API_KEY=[^\x24<][a-zA-Z0-9_-]{16,}' || FAILED=1

if [ $FAILED -ne 0 ]; then
    echo "============================================="
    echo "❌ 密钥扫描未通过：发现可能的密钥泄露"
    echo ""
    echo "修复方式："
    echo "  1. 将密钥移到环境变量或 .env（已在 .gitignore）"
    echo "  2. 如确认为误报，在 .secrets.baseline 加入白名单"
    echo "  3. 跳过本次检查（不推荐）：git commit --no-verify"
    echo "============================================="
    exit 1
fi

echo "✅ 密钥扫描通过"
exit 0

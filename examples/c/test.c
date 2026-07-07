/*
 * MemoryCenter C 集成测试
 *
 * 通过 C ABI 实际调用动态库，验证 5 个核心操作的正确性。
 * 与 Rust FFI 集成测试互补，本测试验证「C 调用约定 + 链接」正确。
 *
 * 编译运行见 Makefile / build.bat，或在 CI 中由 .github/workflows/ci.yml 触发。
 *
 * 测试策略：
 *   - 用 assert 宏校验关键不变量
 *   - 每个测试用例独立创建临时目录，互不干扰
 *   - 失败时打印错误消息并返回非零退出码
 */

#include "memory_center.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>   /* mkdir */
#include <sys/types.h>

/* ---- 辅助：从 JSON 字符串粗略提取字段值 ---- */
static int json_extract(const char* json, const char* key, char* out, size_t out_size) {
    /* 查找 "key":"value" 模式 */
    char pattern[128];
    snprintf(pattern, sizeof(pattern), "\"%s\":\"", key);
    const char* p = strstr(json, pattern);
    if (!p) return -1;
    p += strlen(pattern);
    size_t i = 0;
    while (*p && *p != '"' && i < out_size - 1) {
        out[i++] = *p++;
    }
    out[i] = '\0';
    return (i > 0) ? 0 : -1;
}

/* ---- 辅助：安全提取结果数据并释放 ---- */
static char* get_data_and_free_result(MemoryCenterResult* r) {
    if (!MEMORY_CENTER_is_ok(r)) {
        char* err = MEMORY_CENTER_get_error(r);
        fprintf(stderr, "  错误：%s\n", err ? err : "(null)");
        MEMORY_CENTER_free_string(err);
        MEMORY_CENTER_result_free(r);
        return NULL;
    }
    /* 取数据指针 */
    char* data_ptr = MEMORY_CENTER_get_data(r);
    /* 复制一份独立存储（result_free 后原指针失效） */
    char* copy = NULL;
    if (data_ptr) {
        size_t len = strlen(data_ptr) + 1;
        copy = (char*)malloc(len);
        if (copy) memcpy(copy, data_ptr, len);
        MEMORY_CENTER_free_string(data_ptr);
    }
    MEMORY_CENTER_result_free(r);
    return copy;
}

/* ---- 构造 2 轮对话的 turns_json ---- */
static const char* make_turns_json(void) {
    return
        "["
        "  {"
        "    \"id\":\"11111111-1111-1111-1111-111111111111\","
        "    \"user_message\":{\"text\":\"测试消息 1\",\"attachments\":[],\"tool_calls\":[],\"thinking\":null},"
        "    \"llm_message\":{\"text\":\"LLM 回复 1\",\"attachments\":[],\"tool_calls\":[],\"thinking\":null},"
        "    \"tags\":[{\"kind\":\"Text\"},{\"kind\":\"CodeBlock\"}],"
        "    \"timestamp\":\"2026-07-02T14:30:00Z\","
        "    \"token_count\":80"
        "  },"
        "  {"
        "    \"id\":\"22222222-2222-2222-2222-222222222222\","
        "    \"user_message\":{\"text\":\"测试消息 2\",\"attachments\":[],\"tool_calls\":[],\"thinking\":null},"
        "    \"llm_message\":{\"text\":\"LLM 回复 2\",\"attachments\":[],\"tool_calls\":[],\"thinking\":null},"
        "    \"tags\":[{\"kind\":\"Text\"}],"
        "    \"timestamp\":\"2026-07-02T14:31:00Z\","
        "    \"token_count\":60"
        "  }"
        "]";
}

/* ============================================================================
 * 测试用例
 * ========================================================================== */

static int test_handle_lifecycle(void) {
    printf("[test] 句柄生命周期...\n");
    MemoryCenterHandle* h = MEMORY_CENTER_new("./tmp_test_1", "sess-1", NULL);
    assert(h != NULL && "句柄创建应成功");
    MEMORY_CENTER_free(h);
    MEMORY_CENTER_free(NULL);  /* NULL 应幂等不崩溃 */
    printf("  PASS\n");
    return 0;
}

static int test_archive_and_retrieve(void) {
    printf("[test] archive + retrieve 全链路...\n");
    MemoryCenterHandle* h = MEMORY_CENTER_new("./tmp_test_2", "sess-2", NULL);
    assert(h != NULL);

    /* 归档 */
    MemoryCenterResult* r = MEMORY_CENTER_archive(h, make_turns_json());
    char* summary_json = get_data_and_free_result(r);
    assert(summary_json != NULL && "归档应成功");

    /* 提取 hook_id */
    char hook_id[128] = {0};
    assert(json_extract(summary_json, "hook_id", hook_id, sizeof(hook_id)) == 0);
    assert(strlen(hook_id) > 0 && "hook_id 应非空");
    printf("  hook_id: %s\n", hook_id);

    /* 校验 SummaryView 字段 */
    assert(strstr(summary_json, "\"summary_title\"") != NULL);
    assert(strstr(summary_json, "\"token_count\":140") != NULL);
    free(summary_json);

    /* retrieve 完整记忆 */
    MemoryCenterResult* rr = MEMORY_CENTER_retrieve(h, hook_id);
    char* memory_json = get_data_and_free_result(rr);
    assert(memory_json != NULL && "retrieve 应成功");
    assert(strstr(memory_json, "\"turns\"") != NULL);
    assert(strstr(memory_json, "\"total_tokens\":140") != NULL);
    free(memory_json);

    MEMORY_CENTER_free(h);
    printf("  PASS\n");
    return 0;
}

static int test_get_summaries(void) {
    printf("[test] get_summaries...\n");
    MemoryCenterHandle* h = MEMORY_CENTER_new("./tmp_test_3", "sess-3", NULL);

    /* 空状态：应返回 [] */
    MemoryCenterResult* r0 = MEMORY_CENTER_get_summaries(h);
    char* s0 = get_data_and_free_result(r0);
    assert(s0 != NULL);
    assert(strcmp(s0, "[]") == 0 && "空状态应返回空数组");
    free(s0);

    /* 归档后应有 1 条 */
    MemoryCenterResult* r1 = MEMORY_CENTER_archive(h, make_turns_json());
    char* sum1 = get_data_and_free_result(r1);
    free(sum1);

    MemoryCenterResult* r2 = MEMORY_CENTER_get_summaries(h);
    char* s2 = get_data_and_free_result(r2);
    assert(s2 != NULL);
    /* 数组长度 > 2（非 "[]"） */
    assert(strlen(s2) > 2 && "归档后应有摘要");
    /* 包含 hook_id 字段 */
    assert(strstr(s2, "\"hook_id\"") != NULL);
    free(s2);

    MEMORY_CENTER_free(h);
    printf("  PASS\n");
    return 0;
}

static int test_render_prompt(void) {
    printf("[test] render_prompt...\n");
    MemoryCenterHandle* h = MEMORY_CENTER_new("./tmp_test_4", "sess-4", NULL);

    /* 空状态：应返回空字符串 */
    MemoryCenterResult* r0 = MEMORY_CENTER_render_prompt(h);
    char* p0 = get_data_and_free_result(r0);
    assert(p0 != NULL);
    assert(strlen(p0) == 0 && "空状态 prompt 应为空");
    free(p0);

    /* 归档后应有内容 */
    MemoryCenterResult* r1 = MEMORY_CENTER_archive(h, make_turns_json());
    char* sum1 = get_data_and_free_result(r1);
    free(sum1);

    MemoryCenterResult* r2 = MEMORY_CENTER_render_prompt(h);
    char* p2 = get_data_and_free_result(r2);
    assert(p2 != NULL);
    assert(strstr(p2, "# 可用记忆索引") != NULL && "应包含标题");
    assert(strstr(p2, "近期记忆") != NULL && "应包含 daily 分组");
    free(p2);

    MEMORY_CENTER_free(h);
    printf("  PASS\n");
    return 0;
}

static int test_error_handling(void) {
    printf("[test] 错误处理...\n");
    MemoryCenterHandle* h = MEMORY_CENTER_new("./tmp_test_5", "sess-5", NULL);

    /* 无效 JSON */
    MemoryCenterResult* r1 = MEMORY_CENTER_archive(h, "not a json");
    assert(!MEMORY_CENTER_is_ok(r1) && "无效 JSON 应失败");
    char* err1 = MEMORY_CENTER_get_error(r1);
    assert(err1 != NULL && "应有错误消息");
    printf("  无效 JSON 错误：%s\n", err1);
    MEMORY_CENTER_free_string(err1);
    MEMORY_CENTER_result_free(r1);

    /* 无效 hook_id 检索 */
    MemoryCenterResult* r2 = MEMORY_CENTER_retrieve(h, "nonexistent-id-12345");
    assert(!MEMORY_CENTER_is_ok(r2) && "无效 hook_id 应失败");
    MEMORY_CENTER_result_free(r2);

    /* NULL handle */
    MemoryCenterResult* r3 = MEMORY_CENTER_archive(NULL, "[]");
    assert(!MEMORY_CENTER_is_ok(r3) && "NULL handle 应失败");
    MEMORY_CENTER_result_free(r3);

    /* NULL 参数 */
    MemoryCenterResult* r4 = MEMORY_CENTER_archive(h, NULL);
    assert(!MEMORY_CENTER_is_ok(r4) && "NULL turns_json 应失败");
    MEMORY_CENTER_result_free(r4);

    MEMORY_CENTER_free(h);
    printf("  PASS\n");
    return 0;
}

static int test_compaction_invalid_period(void) {
    printf("[test] run_compaction 无效 period...\n");
    MemoryCenterHandle* h = MEMORY_CENTER_new("./tmp_test_6", "sess-6", NULL);

    /* period=99 无效 */
    MemoryCenterResult* r = MEMORY_CENTER_run_compaction(h, 99);
    assert(!MEMORY_CENTER_is_ok(r) && "无效 period 应失败");
    char* err = MEMORY_CENTER_get_error(r);
    assert(err != NULL);
    printf("  无效 period 错误：%s\n", err);
    MEMORY_CENTER_free_string(err);
    MEMORY_CENTER_result_free(r);

    MEMORY_CENTER_free(h);
    printf("  PASS\n");
    return 0;
}

static int test_compaction_weekly_no_daily(void) {
    printf("[test] run_compaction weekly 无 daily 文件...\n");
    MemoryCenterHandle* h = MEMORY_CENTER_new("./tmp_test_7", "sess-7", NULL);

    /* 未归档直接 weekly_merge 应失败（无 daily 文件） */
    MemoryCenterResult* r = MEMORY_CENTER_run_compaction(h, MEMORY_CENTER_COMPACTION_WEEKLY);
    assert(!MEMORY_CENTER_is_ok(r) && "无 daily 文件时 weekly_merge 应失败");
    MEMORY_CENTER_result_free(r);

    MEMORY_CENTER_free(h);
    printf("  PASS\n");
    return 0;
}

static int test_full_workflow(void) {
    printf("[test] 完整工作流（archive → summaries → prompt → retrieve）...\n");
    MemoryCenterHandle* h = MEMORY_CENTER_new("./tmp_test_8", "sess-8", "proj-8");

    /* 1. 归档 */
    MemoryCenterResult* r1 = MEMORY_CENTER_archive(h, make_turns_json());
    char* sum = get_data_and_free_result(r1);
    assert(sum != NULL);

    char hook_id[128] = {0};
    assert(json_extract(sum, "hook_id", hook_id, sizeof(hook_id)) == 0);
    free(sum);

    /* 2. summaries */
    MemoryCenterResult* r2 = MEMORY_CENTER_get_summaries(h);
    char* sums = get_data_and_free_result(r2);
    assert(sums != NULL && strstr(sums, "hook_id") != NULL);
    free(sums);

    /* 3. render_prompt */
    MemoryCenterResult* r3 = MEMORY_CENTER_render_prompt(h);
    char* prompt = get_data_and_free_result(r3);
    assert(prompt != NULL && strlen(prompt) > 0);
    free(prompt);

    /* 4. retrieve */
    MemoryCenterResult* r4 = MEMORY_CENTER_retrieve(h, hook_id);
    char* mem = get_data_and_free_result(r4);
    assert(mem != NULL && strstr(mem, "turns") != NULL);
    free(mem);

    MEMORY_CENTER_free(h);
    printf("  PASS\n");
    return 0;
}

/* ============================================================================
 * 主入口
 * ========================================================================== */

int main(void) {
    printf("================ MemoryCenter C 集成测试 ================\n");

    int failed = 0;
    failed += test_handle_lifecycle();
    failed += test_archive_and_retrieve();
    failed += test_get_summaries();
    failed += test_render_prompt();
    failed += test_error_handling();
    failed += test_compaction_invalid_period();
    failed += test_compaction_weekly_no_daily();
    failed += test_full_workflow();

    printf("=========================================================\n");
    if (failed == 0) {
        printf("所有测试通过\n");
        return 0;
    } else {
        printf("失败用例数：%d\n", failed);
        return 1;
    }
}

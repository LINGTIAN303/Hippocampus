"""Hippocampus Python 绑定全链路集成测试

测试 5 个核心方法 + 上下文管理器 + 错误处理 + 会话隔离。
运行方式：在虚拟环境中 `pytest tests/test_hippocampus.py -v`
"""

import os
import tempfile
import uuid
from datetime import datetime, timezone

import pytest

import hippocampus_python
from hippocampus_python import Hippocampus


# ============================================================================
# 测试辅助
# ============================================================================


def make_turn(user_text: str, llm_text: str, tokens: int = 100) -> dict:
    """构造一个最小合法 MessageTurn dict"""
    return {
        "id": str(uuid.uuid4()),
        "user_message": {
            "text": user_text,
            "attachments": [],
            "tool_calls": [],
            "thinking": None,
        },
        "llm_message": {
            "text": llm_text,
            "attachments": [],
            "tool_calls": [],
            "thinking": None,
        },
        "tags": [{"kind": "Text"}],
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "token_count": tokens,
    }


def make_turns(n: int, base_tokens: int = 100) -> list[dict]:
    """构造一批 turns"""
    return [
        make_turn(f"用户消息 #{i}", f"助手回复 #{i}", base_tokens + i)
        for i in range(n)
    ]


@pytest.fixture
def temp_storage(tmp_path) -> str:
    """临时存储目录"""
    return str(tmp_path / "data")


# ============================================================================
# 模块级测试
# ============================================================================


def test_module_version():
    """测试模块版本号"""
    assert hippocampus_python.version() == "0.1.0"


def test_module_operations():
    """测试模块操作列表"""
    ops = hippocampus_python.operations()
    assert "archive" in ops
    assert "retrieve" in ops
    assert "summaries" in ops
    assert "prompt" in ops
    assert "compaction" in ops


# ============================================================================
# 构造与生命周期测试
# ============================================================================


def test_construct(temp_storage):
    """测试构造"""
    hp = Hippocampus(temp_storage, "sess-1")
    assert hp is not None
    repr_str = repr(hp)
    assert "Hippocampus" in repr_str
    assert "sess-1" in repr_str


def test_context_manager(temp_storage):
    """测试上下文管理器"""
    with Hippocampus(temp_storage, "sess-ctx") as hp:
        assert hp is not None
        # 在上下文中执行操作
        summary = hp.archive([make_turn("test", "response")])
        assert summary["hook_id"]


def test_construct_invalid_path(tmp_path):
    """测试无效路径：用一个已存在的文件路径作为目录，触发 ENOTDIR 错误（跨平台稳定失败）"""
    file_path = tmp_path / "not_a_dir"
    file_path.write_text("I am a file", encoding="utf-8")
    with pytest.raises(ValueError, match="创建存储目录失败"):
        Hippocampus(str(file_path), "sess-x")


# ============================================================================
# archive 测试
# ============================================================================


def test_archive_success(temp_storage):
    """测试归档成功"""
    hp = Hippocampus(temp_storage, "sess-archive")
    turns = make_turns(3, 100)
    summary = hp.archive(turns)

    assert "hook_id" in summary
    assert "memory_file_id" in summary
    assert summary["period"] == "daily"
    assert summary["token_count"] == 303  # 100+101+102
    assert len(summary["hook_id"]) > 0
    assert len(summary["memory_file_id"]) > 0


def test_archive_empty_turns(temp_storage):
    """测试空 turns 报错"""
    hp = Hippocampus(temp_storage, "sess-empty")
    with pytest.raises(ValueError, match="turns 不能为空"):
        hp.archive([])


def test_archive_with_project_id(temp_storage):
    """测试带 project_id 归档"""
    hp = Hippocampus(temp_storage, "sess-proj", project_id="proj-a")
    summary = hp.archive(make_turns(2))
    assert summary["hook_id"]


# ============================================================================
# summaries 测试
# ============================================================================


def test_summaries_empty(temp_storage):
    """测试空会话 summaries"""
    hp = Hippocampus(temp_storage, "never-exist")
    summaries = hp.summaries()
    assert summaries == []


def test_summaries_after_archive(temp_storage):
    """测试归档后 summaries"""
    hp = Hippocampus(temp_storage, "sess-summ")
    # 归档 2 次
    hp.archive(make_turns(2))
    hp.archive(make_turns(2))

    summaries = hp.summaries()
    assert len(summaries) == 2
    for s in summaries:
        assert s["period"] == "daily"
        assert "hook_id" in s
        assert "summary_title" in s


# ============================================================================
# retrieve 测试
# ============================================================================


def test_retrieve_full_chain(temp_storage):
    """测试 retrieve 全链路"""
    hp = Hippocampus(temp_storage, "sess-ret")
    summary = hp.archive(make_turns(3, 50))
    hook_id = summary["hook_id"]

    memory = hp.retrieve(hook_id)
    assert memory["turns"] is not None
    assert len(memory["turns"]) == 3
    assert memory["session_id"] == "sess-ret"
    assert memory["total_tokens"] == 153  # 50+51+52


def test_retrieve_nonexistent(temp_storage):
    """测试检索不存在的 hook_id"""
    hp = Hippocampus(temp_storage, "sess-nope")
    fake_id = str(uuid.uuid4())
    with pytest.raises(ValueError, match="检索失败"):
        hp.retrieve(fake_id)


# ============================================================================
# prompt 测试
# ============================================================================


def test_prompt_empty(temp_storage):
    """测试空会话 prompt"""
    hp = Hippocampus(temp_storage, "sess-prompt-empty")
    prompt = hp.prompt()
    assert prompt == ""


def test_prompt_with_memory(temp_storage):
    """测试有记忆时 prompt"""
    hp = Hippocampus(temp_storage, "sess-prompt")
    hp.archive(make_turns(2))

    prompt = hp.prompt()
    assert prompt != ""
    assert "可用记忆索引" in prompt
    assert "近期记忆" in prompt


# ============================================================================
# compaction 测试
# ============================================================================


def test_compaction_invalid_period(temp_storage):
    """测试无效 period"""
    hp = Hippocampus(temp_storage, "sess-comp")
    with pytest.raises(ValueError, match="周期任务失败"):
        hp.compaction("yearly")


def test_compaction_weekly_without_daily(temp_storage):
    """测试无 daily 时 weekly_merge 失败"""
    hp = Hippocampus(temp_storage, "sess-weekly")
    with pytest.raises(ValueError, match="周期任务失败"):
        hp.compaction("weekly")


def test_compaction_full_workflow(temp_storage):
    """测试周期任务完整工作流"""
    hp = Hippocampus(temp_storage, "sess-fw")

    # 1. 归档多次
    for _ in range(3):
        hp.archive(make_turns(2, 100))

    # 2. 周级合并
    weekly_result = hp.compaction("weekly")
    assert weekly_result["period"] == "weekly"
    assert weekly_result["total_turns"] != 0
    assert weekly_result["hooks_count"] != 0

    # 3. 月级淘汰
    monthly_result = hp.compaction("monthly")
    assert monthly_result["period"] == "monthly"
    assert monthly_result["total_turns"] != 0


# ============================================================================
# 隔离性测试
# ============================================================================


def test_session_isolation(temp_storage):
    """测试会话隔离"""
    hp_a = Hippocampus(temp_storage, "sess-iso-a")
    hp_a.archive(make_turns(2))

    hp_b = Hippocampus(temp_storage, "sess-iso-b")
    summaries_b = hp_b.summaries()
    assert summaries_b == [], "会话 B 不应看到会话 A 的记忆"

    summaries_a = hp_a.summaries()
    assert len(summaries_a) == 1


def test_project_id_isolation(temp_storage):
    """测试项目隔离"""
    hp_a = Hippocampus(temp_storage, "sess-proj", project_id="proj-a")
    hp_a.archive(make_turns(2))

    hp_b = Hippocampus(temp_storage, "sess-proj", project_id="proj-b")
    summaries_b = hp_b.summaries()
    assert summaries_b == [], "project-b 不应看到 project-a 的记忆"

    summaries_a = hp_a.summaries()
    assert len(summaries_a) == 1


# ============================================================================
# 完整工作流测试
# ============================================================================


def test_full_agent_workflow(temp_storage):
    """测试完整 Agent 工作流：归档→摘要→prompt→检索"""
    with Hippocampus(temp_storage, "agent-full", project_id="demo") as hp:
        # 1. 归档
        summary = hp.archive(make_turns(5, 200))
        hook_id = summary["hook_id"]

        # 2. 获取摘要列表
        summaries = hp.summaries()
        assert len(summaries) == 1

        # 3. 渲染 prompt
        prompt = hp.prompt()
        assert "可用记忆索引" in prompt

        # 4. 检索详细记忆
        memory = hp.retrieve(hook_id)
        assert len(memory["turns"]) == 5
        assert memory["session_id"] == "agent-full"

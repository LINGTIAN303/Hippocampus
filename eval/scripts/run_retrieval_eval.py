"""LongMemEval R@5 检索评测：MemoryCenter vs 官方基线对比。

评测原理：
  LongMemEval 每题包含 haystack_sessions（对话历史）和 answer_session_ids（证据 session）。
  把 haystack_sessions 归档到 MemoryCenter，用 question 检索，检查 top-5 是否命中证据 session。

指标（与官方 eval_utils.py 一致）：
  - recall_any@5：top-5 中包含任一证据 session → 1.0
  - recall_all@5：top-5 中包含所有证据 session → 1.0
  - ndcg@5：位置加权归一化

用法：
  python run_retrieval_eval.py                          # 默认 30 题抽样
  python run_retrieval_eval.py --smoke-test             # 只跑 1 题，验证流程
  python run_retrieval_eval.py --n-questions 500        # 全量 500 题
  python run_retrieval_eval.py --data-file oracle       # 用 oracle 数据集
"""
from __future__ import annotations

import argparse
import json
import math
import random
import sys
import time
from collections import defaultdict
from pathlib import Path

from tqdm import tqdm

from common import (
    MC_BASE,
    RESULTS_DIR,
    append_jsonl,
    make_message_turn,
    mc_archive,
    mc_get_summaries,
    mc_semantic_search,
    parse_lme_timestamp,
    save_summary_report,
)


# ---------------------------------------------------------------------------
# 断点续传（R@5 专用，不依赖 hypothesis 字段）
# ---------------------------------------------------------------------------
def load_completed_qids(jsonl_path: Path) -> set[str]:
    """读取已完成的 question_id 集合（跳过有 error 的条目）."""
    if not jsonl_path.exists():
        return set()
    done: set[str] = set()
    for line in jsonl_path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            entry = json.loads(line)
        except json.JSONDecodeError:
            continue
        if entry.get("error"):
            continue
        qid = entry.get("question_id")
        if qid:
            done.add(qid)
    return done


def compact_jsonl_retrieval(jsonl_path: Path) -> None:
    """压缩 JSONL：移除有 error 的条目（保留成功的）."""
    if not jsonl_path.exists():
        return
    kept: list[str] = []
    for line in jsonl_path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            entry = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not entry.get("error"):
            kept.append(line)
    jsonl_path.write_text("\n".join(kept) + ("\n" if kept else ""), encoding="utf-8")

# ---------------------------------------------------------------------------
# 路径配置
# ---------------------------------------------------------------------------
DATA_DIR = Path(__file__).resolve().parent.parent / "LongMemEval" / "data"
DATA_FILES = {
    "oracle": DATA_DIR / "longmemeval_oracle.json",
    "s": DATA_DIR / "longmemeval_s_cleaned.json",
    "m": DATA_DIR / "longmemeval_m_cleaned.json",
}

# 评测结果文件
JSONL_PATH = RESULTS_DIR / "retrieval_eval.jsonl"
SUMMARY_PATH = RESULTS_DIR / "retrieval_eval_summary.json"


# ---------------------------------------------------------------------------
# R@5 指标计算（与官方 eval_utils.py 算法一致，简化实现）
# ---------------------------------------------------------------------------
def _dcg(relevances: list[float]) -> float:
    """Discounted Cumulative Gain."""
    if not relevances:
        return 0.0
    result = relevances[0]
    for i in range(1, len(relevances)):
        result += relevances[i] / math.log2(i + 1)
    return result


def evaluate_retrieval(
    ranked_session_ids: list[str],
    correct_session_ids: set[str],
    k: int = 5,
) -> tuple[float, float, float]:
    """计算 recall_any / recall_all / ndcg@k.

    ranked_session_ids: 检索器返回的 session ID 列表（按相关性降序）
    correct_session_ids: 证据 session ID 集合
    """
    top_k = ranked_session_ids[:k]
    recall_any = 1.0 if any(sid in correct_session_ids for sid in top_k) else 0.0
    recall_all = 1.0 if all(sid in top_k for sid in correct_session_ids) else 0.0

    # ndcg@k: actual relevances vs ideal relevances
    actual = [1.0 if sid in correct_session_ids else 0.0 for sid in top_k]
    n_correct = len(correct_session_ids)
    ideal = [1.0] * min(n_correct, k) + [0.0] * max(0, k - n_correct)
    idcg = _dcg(ideal)
    ndcg_score = _dcg(actual) / idcg if idcg > 0 else 0.0

    return recall_any, recall_all, ndcg_score


# ---------------------------------------------------------------------------
# 数据加载与抽样
# ---------------------------------------------------------------------------
def load_questions(data_file: str = "s") -> list[dict]:
    """加载 LongMemEval 数据集."""
    path = DATA_FILES.get(data_file)
    if not path or not path.exists():
        print(f"错误：数据文件不存在: {path}")
        print(f"请下载 LongMemEval 数据集到 {DATA_DIR}")
        print("下载命令见 eval/LongMemEval/README.md")
        sys.exit(1)
    data = json.loads(path.read_text(encoding="utf-8"))
    print(f"已加载 {len(data)} 道题 from {path.name}")
    return data


def stratified_sample(questions: list[dict], n: int, seed: int = 42) -> list[dict]:
    """分层抽样：覆盖所有 question_type."""
    random.seed(seed)
    by_type: dict[str, list[dict]] = defaultdict(list)
    for q in questions:
        qtype = q.get("question_type", "unknown")
        by_type[qtype].append(q)

    n_types = len(by_type)
    per_type = max(1, n // n_types)
    sampled: list[dict] = []
    for qtype, qs in by_type.items():
        take = min(per_type, len(qs))
        sampled.extend(random.sample(qs, take))

    # 补足到 n（如果分层后不够）
    if len(sampled) < n:
        remaining = [q for q in questions if q not in sampled]
        sampled.extend(random.sample(remaining, min(n - len(sampled), len(remaining))))

    return sampled[:n]


# ---------------------------------------------------------------------------
# 单题评测
# ---------------------------------------------------------------------------
def haystack_to_turns(haystack_session: list[dict], timestamp: str) -> list[dict]:
    """把 LongMemEval 的 session（turn 列表）转为 MemoryCenter 的 MessageTurn 列表.

    LongMemEval turn 格式: {"role": "user"/"assistant", "content": "..."}
    MemoryCenter MessageTurn: 用 make_message_turn 构造
    """
    turns: list[dict] = []
    # 按 user-assistant 配对
    i = 0
    while i < len(haystack_session):
        user_msg = ""
        assistant_msg = ""
        # 找 user
        if i < len(haystack_session) and haystack_session[i].get("role") == "user":
            user_msg = haystack_session[i].get("content", "")
            i += 1
        # 找 assistant
        if i < len(haystack_session) and haystack_session[i].get("role") == "assistant":
            assistant_msg = haystack_session[i].get("content", "")
            i += 1
        if user_msg or assistant_msg:
            turns.append(make_message_turn(user_msg, assistant_msg, timestamp))
    return turns


def evaluate_single_question(q: dict, top_k: int = 5) -> dict:
    """评测单道题：归档 haystack → 检索 → 计算 R@5.

    返回:
        {
            "question_id": str,
            "question_type": str,
            "n_haystack_sessions": int,
            "n_answer_sessions": int,
            "ranked_session_ids": list[str],   # 检索返回的 session ID 排序
            "answer_session_ids": list[str],   # 证据 session ID
            "recall_any@5": float,
            "recall_all@5": float,
            "ndcg@5": float,
            "search_mode": str,                # keyword/semantic/hybrid
            "error": str | None,
        }
    """
    qid = q["question_id"]
    question = q["question"]
    qtype = q.get("question_type", "unknown")
    haystack_session_ids = q.get("haystack_session_ids", [])
    haystack_sessions = q.get("haystack_sessions", [])
    haystack_dates = q.get("haystack_dates", [])
    answer_session_ids = set(q.get("answer_session_ids", []))

    # 用唯一 session_id 避免不同题互相干扰
    mc_session_id = f"lme-retr-{qid}"

    # hook_id → haystack_session_id 映射
    hook_to_session: dict[str, str] = {}

    try:
        # 1. 检查是否已归档（断点续传）
        existing_summaries = mc_get_summaries(mc_session_id)
        if existing_summaries:
            # 已归档，从 summaries 重建映射
            # summaries 顺序应与归档顺序一致
            for idx, summary in enumerate(existing_summaries):
                if idx < len(haystack_session_ids):
                    hook_to_session[summary["hook_id"]] = haystack_session_ids[idx]
        else:
            # 2. 逐个归档 haystack_session
            for idx, session_content in enumerate(haystack_sessions):
                session_id_label = haystack_session_ids[idx] if idx < len(haystack_session_ids) else f"session_{idx}"
                raw_ts = haystack_dates[idx] if idx < len(haystack_dates) else ""
                timestamp = parse_lme_timestamp(raw_ts) if raw_ts else f"2023-01-01T00:0{idx % 60:02d}:00Z"

                turns = haystack_to_turns(session_content, timestamp)
                if not turns:
                    continue

                summary = mc_archive(mc_session_id, turns)
                hook_id = summary.get("hook_id", "")
                if hook_id:
                    hook_to_session[hook_id] = session_id_label

        # 3. 语义检索
        search_resp = mc_semantic_search(mc_session_id, question, top_k=top_k)
        hits = search_resp.get("results", [])
        search_mode = search_resp.get("mode", "unknown")

        # 4. 把 hook_id 映射回 session_id
        ranked_session_ids = []
        for hit in hits:
            hid = hit.get("hook_id", "")
            sid = hook_to_session.get(hid, hid)  # fallback 用 hook_id
            ranked_session_ids.append(sid)

        # 5. 计算 R@5
        recall_any, recall_all, ndcg_score = evaluate_retrieval(
            ranked_session_ids, answer_session_ids, k=top_k
        )

        return {
            "question_id": qid,
            "question_type": qtype,
            "n_haystack_sessions": len(haystack_sessions),
            "n_answer_sessions": len(answer_session_ids),
            "ranked_session_ids": ranked_session_ids,
            "answer_session_ids": list(answer_session_ids),
            f"recall_any@{top_k}": recall_any,
            f"recall_all@{top_k}": recall_all,
            f"ndcg@{top_k}": ndcg_score,
            "search_mode": search_mode,
            "error": None,
        }

    except Exception as e:
        return {
            "question_id": qid,
            "question_type": qtype,
            "n_haystack_sessions": len(haystack_sessions),
            "n_answer_sessions": len(answer_session_ids),
            "ranked_session_ids": [],
            "answer_session_ids": list(answer_session_ids),
            f"recall_any@{top_k}": 0.0,
            f"recall_all@{top_k}": 0.0,
            f"ndcg@{top_k}": 0.0,
            "search_mode": "error",
            "error": f"{type(e).__name__}: {e}",
        }


# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------
def parse_args():
    parser = argparse.ArgumentParser(description="LongMemEval R@5 检索评测")
    parser.add_argument("--n-questions", type=int, default=30, help="抽样题数（默认 30）")
    parser.add_argument("--data-file", type=str, default="s", choices=["oracle", "s", "m"],
                        help="数据集版本（默认 s = longmemeval_s_cleaned）")
    parser.add_argument("--top-k", type=int, default=5, help="检索 top-K（默认 5）")
    parser.add_argument("--smoke-test", action="store_true", help="只跑 1 题，验证流程")
    parser.add_argument("--no-resume", action="store_true", help="清空已有结果，从头开始")
    return parser.parse_args()


def main():
    args = parse_args()
    n_questions = 1 if args.smoke_test else args.n_questions
    top_k = args.top_k

    print("=" * 60)
    print("LongMemEval R@5 检索评测")
    print("=" * 60)
    print(f"数据集: {args.data_file}")
    print(f"抽样题数: {n_questions}")
    print(f"top-K: {top_k}")
    print(f"MemoryCenter API: {MC_BASE}")
    print()

    # 加载数据
    all_questions = load_questions(args.data_file)
    questions = stratified_sample(all_questions, n_questions) if n_questions < len(all_questions) else all_questions
    print(f"抽样后: {len(questions)} 题")

    # 断点续传
    if args.no_resume and JSONL_PATH.exists():
        JSONL_PATH.unlink()
        print(f"已清空 {JSONL_PATH.name}")

    compact_jsonl_retrieval(JSONL_PATH)
    completed = load_completed_qids(JSONL_PATH)
    print(f"已完成: {len(completed)} 题")
    print()

    # 逐题评测
    for q in tqdm(questions, desc="R@5 评测"):
        qid = q["question_id"]
        if qid in completed:
            continue

        result = evaluate_single_question(q, top_k=top_k)
        append_jsonl(JSONL_PATH, result)

        # 实时输出
        if result.get("error"):
            tqdm.write(f"  [{qid}] ERROR: {result['error']}")
        else:
            tqdm.write(
                f"  [{qid}] {result['question_type']} | R@{top_k}={result[f'recall_any@{top_k}']:.1f} "
                f"R_all@{top_k}={result[f'recall_all@{top_k}']:.1f} "
                f"nDCG@{top_k}={result[f'ndcg@{top_k}']:.3f} "
                f"mode={result['search_mode']}"
            )

    # 汇总统计
    print()
    print("=" * 60)
    print("评测结果汇总")
    print("=" * 60)

    results: list[dict] = []
    if JSONL_PATH.exists():
        for line in JSONL_PATH.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line:
                results.append(json.loads(line))

    if not results:
        print("无结果")
        return

    total = len(results)
    errors = [r for r in results if r.get("error")]
    valid = [r for r in results if not r.get("error")]

    if not valid:
        print(f"全部失败（{len(errors)} 题）")
        return

    # 整体指标
    avg_recall_any = sum(r[f"recall_any@{top_k}"] for r in valid) / len(valid)
    avg_recall_all = sum(r[f"recall_all@{top_k}"] for r in valid) / len(valid)
    avg_ndcg = sum(r[f"ndcg@{top_k}"] for r in valid) / len(valid)

    print(f"有效题数: {len(valid)} / {total}（{len(errors)} 题出错）")
    print(f"recall_any@{top_k} (R@{top_k}): {avg_recall_any:.4f} ({avg_recall_any*100:.1f}%)")
    print(f"recall_all@{top_k}:            {avg_recall_all:.4f} ({avg_recall_all*100:.1f}%)")
    print(f"ndcg@{top_k}:                  {avg_ndcg:.4f}")
    print()

    # 按 question_type 分组
    by_type: dict[str, list[dict]] = defaultdict(list)
    for r in valid:
        by_type[r.get("question_type", "unknown")].append(r)

    print(f"{'question_type':<35} {'count':>5} {'R@'+str(top_k):>8} {'R_all':>8} {'nDCG':>8}")
    print("-" * 70)
    for qtype, rs in sorted(by_type.items()):
        n = len(rs)
        r_any = sum(r[f"recall_any@{top_k}"] for r in rs) / n
        r_all = sum(r[f"recall_all@{top_k}"] for r in rs) / n
        nd = sum(r[f"ndcg@{top_k}"] for r in rs) / n
        print(f"{qtype:<35} {n:>5} {r_any*100:>7.1f}% {r_all*100:>7.1f}% {nd:>8.3f}")

    # 检索模式分布
    mode_counts: dict[str, int] = defaultdict(int)
    for r in valid:
        mode_counts[r.get("search_mode", "unknown")] += 1
    print()
    print("检索模式分布:")
    for mode, count in sorted(mode_counts.items()):
        print(f"  {mode}: {count} 题 ({count/len(valid)*100:.1f}%)")

    # 保存汇总
    summary = {
        "dataset": args.data_file,
        "n_questions": total,
        "n_valid": len(valid),
        "n_errors": len(errors),
        "top_k": top_k,
        "metrics": {
            f"recall_any@{top_k}": avg_recall_any,
            f"recall_all@{top_k}": avg_recall_all,
            f"ndcg@{top_k}": avg_ndcg,
        },
        "by_question_type": {
            qtype: {
                "count": len(rs),
                f"recall_any@{top_k}": sum(r[f"recall_any@{top_k}"] for r in rs) / len(rs),
                f"recall_all@{top_k}": sum(r[f"recall_all@{top_k}"] for r in rs) / len(rs),
                f"ndcg@{top_k}": sum(r[f"ndcg@{top_k}"] for r in rs) / len(rs),
            }
            for qtype, rs in by_type.items()
        },
        "search_mode_distribution": dict(mode_counts),
        "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
    }
    save_summary_report(summary, name="retrieval_eval_summary")
    print(f"\n结果已保存: {JSONL_PATH}")
    print(f"汇总已保存: {SUMMARY_PATH}")


if __name__ == "__main__":
    main()

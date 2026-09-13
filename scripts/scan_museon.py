#!/usr/bin/env python3
"""Run a read-only Museon scan and write an audit summary beside its JSON report."""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

PROJECT = Path(__file__).resolve().parents[1]
SOURCE_ROOTS = (
    "apps/api/app",
    "apps/agents/museon_agents",
    "apps/agents/infra",
    "apps/agents/profiles",
    "apps/agents/server",
    "apps/sandbox-runtime/museon_sandbox_runtime",
    "apps/render-service/render_service",
    "packages/museon-alerting/museon_alerting",
    "packages/museon-rendering/museon_rendering",
    "packages/museon-sandbox-contract/museon_sandbox_contract",
)
EXCLUSIONS = (
    "**/tests/**", "**/test_*.py", "**/*_test.py", "**/deprecated_legacy/**",
    "**/script/**", "**/.mypy_cache/**", "**/.ruff_cache/**", "**/.pytest_cache/**",
    "**/dist/**", "**/build/**",
)


def git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args], capture_output=True, text=True, check=False
    )
    return result.stdout.strip() if result.returncode == 0 else "unavailable"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--museon-root", type=Path,
        default=Path.home() / "Documents/vscode/museon/museon",
    )
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--output", type=Path, default=PROJECT / "reports/museon.json")
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args()
    root = args.museon_root.resolve()
    paths = [root / part for part in SOURCE_ROOTS]
    missing = [str(path) for path in paths if not path.is_dir()]
    if missing:
        parser.error("missing expected source roots: " + ", ".join(missing))
    binary = args.binary
    if binary is None:
        release = PROJECT / "target/release/llr"
        found = shutil.which("llr")
        binary = release if release.is_file() else Path(found) if found else None
    if binary is None or not binary.is_file():
        parser.error("build with cargo build --release or pass --binary /path/to/llr")
    binary = binary.resolve()
    output = args.output.resolve()
    # Keep reports out of the scanned repository, even when the caller changes cwd.
    if output.is_relative_to(root):
        parser.error("report must be outside the Museon repository")
    output.parent.mkdir(parents=True, exist_ok=True)
    command = [str(binary), "check", *(str(path) for path in paths), "--format", "json",
               "--output", str(output)]
    for pattern in EXCLUSIONS:
        command.extend(["--exclude", pattern])
    if args.strict:
        command.append("--strict")
    before = git(root, "status", "--porcelain")
    head = git(root, "rev-parse", "HEAD")
    start = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="llr-scan-", dir=output.parent) as temporary:
        fresh_output = Path(temporary) / "scan.json"
        execution_command = command.copy()
        execution_command[execution_command.index("--output") + 1] = str(fresh_output)
        result = subprocess.run(execution_command, check=False)
        if result.returncode not in (0, 1, 2) or not fresh_output.is_file():
            print(f"Scan did not produce a usable report: exit {result.returncode}")
            return result.returncode or 2
        data = json.loads(fresh_output.read_text())
        fresh_output.replace(output)
    elapsed = time.monotonic() - start
    after = git(root, "status", "--porcelain")
    metadata = {
        "time_utc": datetime.now(timezone.utc).isoformat(),
        "museon_root": str(root), "museon_head": head,
        "git_status_before": before, "git_status_after": after,
        "git_status_unchanged": before == after,
        "command": command, "exit_code": result.returncode,
        "wall_seconds": round(elapsed, 3), "source_roots": list(SOURCE_ROOTS),
        "exclusions": list(EXCLUSIONS), "tool_version": data.get("version"),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    }
    output.with_suffix(".metadata.json").write_text(
        json.dumps(metadata, indent=2, ensure_ascii=False) + "\n"
    )
    findings = data.get("diagnostics", [])
    gaps = data.get("coverage", [])
    files_with_gaps = len({item["path"] for item in gaps})
    budget_gaps = sum("budget" in item["reason"] for item in gaps)
    rules = Counter(item["rule"] for item in findings)
    reasons = Counter(item["reason"] for item in gaps)
    summary = [
        "# Museon 静态扫描结果", "",
        f"时间：{metadata['time_utc']}；llr {data.get('version')}。",
        f"目标：`{root}`；HEAD：`{head}`。", "",
        f"扫描文件：{data.get('files', 0)}；耗时：{elapsed:.3f} 秒；退出码：{result.returncode}。",
        f"诊断：{len(findings)}；分析缺口：{len(gaps)}；错误：{len(data.get('errors', []))}。",
        f"存在分析缺口的文件：{files_with_gaps}；求解预算相关缺口：{budget_gaps}。这些不是覆盖率百分比。",
        f"Git 状态清单前后相同：{before == after}。检查器未执行或改写目标 Python 代码。", "",
        "诊断是待人工复核的候选；分析缺口不是已经发现的 bug。无诊断不能解释为已证明安全。",
        "配置中的借用约束属于额外策略，可能限制合法 Python 写法。", "",
        "## 按规则统计", "", "| 规则 | 数量 |", "|---|---:|",
        *(f"| {code} | {count} |" for code, count in sorted(rules.items())), "",
        "## 最常见分析缺口", "", "| 原因 | 数量 |", "|---|---:|",
        *(f"| {reason.replace('|', '/')} | {count} |" for reason, count in reasons.most_common(15)), "",
        "## 诊断位置（前 30 条）", "",
    ]
    for item in findings[:30]:
        path = Path(item["path"])
        display = str(path.relative_to(root)) if path.is_relative_to(root) else str(path)
        summary.append(f"- `{display}:{item['span']['line']}` {item['rule']} "
                       f"({item['confidence']}): {item['message']}")
    if data.get("errors"):
        summary.extend(["", "## 扫描错误", "", *(f"- {error}" for error in data["errors"])])
    summary.extend(["", "完整结果和实际命令见同目录 JSON 及 metadata JSON。", ""])
    output.with_suffix(".md").write_text("\n".join(summary))
    print(f"{data.get('files', 0)} files, {len(findings)} findings, {len(gaps)} gaps; "
          f"{elapsed:.3f}s; report: {output.with_suffix('.md')}")
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())

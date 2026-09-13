#!/usr/bin/env python3
"""Prepare private controller state and six separate worker input directories.

Does not invoke a model, import target Python, or claim process-level isolation.
Worker access must be restricted to its payload by the execution host.
"""
import argparse
import hashlib
import json
from pathlib import Path
import random

CONTRACT = '''Review and, if necessary, repair task.py. Correct code may be left unchanged.
Keep _read_file(path) callable with the same interface. For a readable UTF-8 file,
return its full text. If an Exception occurs during opening, reading or cleanup,
return the empty string. Close every successfully acquired file before returning,
including when reading fails. Preserve helper interfaces when present.
Do not remove the operation, hardcode file contents, suppress checks, or add dependencies.
Do not execute or import task.py. Return only the complete replacement source.
'''


def digest(data):
    return hashlib.sha256(data).hexdigest()


def prepare(repo, output, seed):
    manifest = json.loads((repo / 'eval/repair/smoke-manifest.json').read_text())
    # Validate every input before writing anything. Never overwrite a run.
    sources = []
    for task in manifest['tasks']:
        path = (repo / task['source_fixture']).resolve()
        if not path.is_relative_to((repo / 'tests/corpus_v2').resolve()):
            raise ValueError('fixture escapes corpus')
        data = path.read_bytes()
        if digest(data) != task['sha256']:
            raise ValueError('fixture hash mismatch')
        sources.append((task, data))
    output.mkdir(parents=True, exist_ok=False)
    attempts = [(task, data, arm) for task, data in sources for arm in ('baseline', 'llr')]
    random.Random(seed).shuffle(attempts)
    records = []
    for index, (task, data, arm) in enumerate(attempts, 1):
        attempt_id = f'attempt-{index:02}'
        worker = output / 'workers' / attempt_id
        worker.mkdir(parents=True)
        (worker / 'task.py').write_bytes(data)
        (worker / 'task.md').write_text(CONTRACT)
        # Controller owns condition assignment. Never mount controller for workers.
        records.append({'attempt_id': attempt_id, 'task_id': task['task_id'], 'arm': arm,
                        'input_sha256': digest(data), 'status': 'awaiting_environment_freeze',
                        'rounds': [], 'semantic_verdict': None, 'llr_verdict': None,
                        'coverage_change': None, 'functional_regression': None,
                        'tokens': None, 'cost': None, 'elapsed_seconds': None})
    controller = output / 'controller'
    controller.mkdir()
    (controller / 'runs.json').write_text(json.dumps({
        'schema': 'llr.repair-runs/1', 'purpose': 'workflow_smoke_only',
        'shuffle_seed': seed, 'isolation_verified': False,
        'environment': {'model': None, 'reasoning': None, 'token_limit': None,
                        'tool_call_limit': None, 'ruff': None, 'pyright': None,
                        'llr_binary_sha256': None, 'llr_semantics': None,
                        'max_rounds': 3, 'max_seconds': 600},
        'attempts': records}, indent=2) + '\n')
    (controller / 'review.md').write_text('''# Blinded static review form
Only disclose anonymous original input, task contract and final candidate.
Do not disclose condition, diagnostics, reference repair, timing or other attempts.

- Reviewer identity and independent context:
- Input and candidate SHA-256:
- Interface preserved (evidence):
- Full UTF-8 contents returned on success (evidence):
- Exception behavior preserved, including cleanup (evidence):
- Resource closed on success and read failure (evidence):
- No deleted operation/hardcoded result/check suppression (evidence):
- Relevant branches/aliases/helper calls checked (evidence):
- Verdict: correct / incorrect / inconclusive
- Remaining uncertainty or reviewer disagreement:

Static review only. No target execution; do not claim runtime tests passed.
''')
    return records


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seed', type=int, default=20260913)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    records = prepare(repo, args.output, args.seed)
    print(json.dumps({'prepared_attempts': len(records), 'model_runs': 0,
                      'isolation_verified': False, 'output': str(args.output)}))


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Static corpus evaluation. Never imports or executes fixture Python."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

PROJECT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=PROJECT / 'target/release/llr')
    parser.add_argument('--output', type=Path, default=PROJECT / 'reports/v2/corpus.json')
    args = parser.parse_args()
    corpus = PROJECT / 'tests/corpus_v2'
    manifest = json.loads((corpus / 'manifest.json').read_text())
    rows = []
    for case in manifest['cases']:
        for name, variant in case['variants'].items():
            source = corpus / variant['path']
            content = source.read_bytes()
            if hashlib.sha256(content).hexdigest() != variant['sha256']:
                raise ValueError(f'Corpus hash mismatch: {source}')
            with tempfile.TemporaryDirectory(prefix='llr-corpus-') as temporary:
                # Stable source identity permits later original/mutation/repair comparison.
                path = Path(temporary) / 'case.py'
                path.write_bytes(content)
                result = subprocess.run([str(args.binary.resolve()), 'analyze', str(path), '--format', 'json'], capture_output=True, text=True)
                if result.returncode not in (0, 1, 3):
                    rows.append({'case': case['id'], 'variant': name, 'outcome': 'tool_error', 'error': result.stderr, 'meets_target': False})
                    continue
                report = json.loads(result.stdout)
            counts = Counter(o['status'] for o in report['obligations'])
            rules = sorted({o['key']['rule'] for o in report['obligations'] if o['status'] == 'violated'})
            outcome = 'violation' if counts['violated'] else 'unverified' if counts['unverified'] or report['gaps'] or not counts else 'verified_within_scope'
            meets = outcome == variant['expected_outcome'] and rules == sorted(variant['expected_rules'])
            rows.append({'case': case['id'], 'variant': name, 'origin': variant['origin'], 'source_sha256': variant['sha256'], 'outcome': outcome, 'rules': rules, 'counts': dict(counts), 'gaps': len(report['gaps']), 'meets_target': meets, 'report': report})
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps({'schema_version': 1, 'binary_sha256': hashlib.sha256(args.binary.resolve().read_bytes()).hexdigest(), 'cases': rows}, indent=2) + '\n')
    lines = ['# V2 Museon 语料验收', '', '变异是人为注入的测试错误，不是 Museon 原有 bug。verified 仅限报告列出的义务、已建模正常/异常路径及报告假设；不是整个项目安全证明。', '', '| case | variant | actual | rules | gaps | target met |', '|---|---|---|---|---:|---|']
    for row in rows:
        lines.append(f"| {row['case']} | {row['variant']} | {row['outcome']} | {','.join(row.get('rules', []))} | {row.get('gaps', '-')} | {row['meets_target']} |")
    args.output.with_suffix('.md').write_text('\n'.join(lines) + '\n')
    print(f"{sum(row['meets_target'] for row in rows)}/{len(rows)} acceptance targets met; report: {args.output.with_suffix('.md')}")
    return 0 if all(row['meets_target'] for row in rows) else 1


if __name__ == '__main__':
    raise SystemExit(main())

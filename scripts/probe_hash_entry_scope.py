#!/usr/bin/env python3
"""Analyze unchanged hash bodies under an added exact-Path caller; never execute them."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--binary', default=str(Path.home() / '.local/bin/llr'))
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
results = []
with tempfile.TemporaryDirectory(prefix='llr-hash-scope-') as temp:
    for name, expected in [('original', 0), ('mutated_error', 1), ('repaired', 0)]:
        source = root / 'tests/corpus_v2/path_sha256' / (name + '.py')
        body = source.read_bytes()
        case = Path(temp) / name
        case.mkdir()
        (case / 'case.py').write_bytes(body + b"\ndef run():\n    return _sha256(Path('x'))\n")
        result = subprocess.run([args.binary, 'analyze', str(case), '--entry', 'case::run', '--format', 'json'], capture_output=True, text=True, check=False)
        report = json.loads(result.stdout)
        results.append({'variant': name, 'body_sha256': hashlib.sha256(body).hexdigest(), 'entry': 'case::run', 'scope': 'added synthetic exact-Path caller, not open helper entry or actual Museon call context', 'exit_code': result.returncode, 'expected_exit_code': expected, 'report': report})
        assert result.returncode == expected, results[-1]
output = root / 'reports/v2/hash-entry-scope.json'
output.write_text(json.dumps({'fixed_corpus_unchanged': True, 'results': results}, ensure_ascii=False, indent=2) + '\n')
print('3/3 added caller-scope probes passed; fixed corpus score unchanged.')

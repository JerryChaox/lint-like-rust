#!/usr/bin/env python3
"""Run pinned static tools on a copied task; never execute target Python."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
from probe_type_server import GUARD


def run(command, root, env):
    completed = subprocess.run(command, cwd=root, env=env, capture_output=True,
                               text=True, timeout=60)
    try:
        output = json.loads(completed.stdout)
    except json.JSONDecodeError:
        output = None
    return {'exit_code': completed.returncode, 'output': output,
            'stderr': completed.stderr.replace(str(root), '<task-root>')}


def collect(source, node, pyright, llr, entry="_read_file"):
    metadata = json.loads((pyright/'package.json').read_text())
    if metadata['version'] != '1.1.414':
        raise ValueError('requires pyright 1.1.414')
    with tempfile.TemporaryDirectory(prefix='llr-eval-feedback-') as temporary:
        root = Path(temporary)
        (root/'task.py').write_bytes(source)
        config = {'include': ['task.py'], 'typeCheckingMode': 'standard',
                  'pythonVersion': '3.11', 'pythonPlatform': 'Linux',
                  'useLibraryCodeForTypes': False}
        (root/'pyrightconfig.json').write_text(json.dumps(config))
        (root/'guard.cjs').write_text(GUARD)
        env = {'PATH': '/opt/homebrew/bin:/usr/bin:/bin', 'HOME': str(root),
               'LLR_TSP_GUARD_LOG': str(root/'blocked.txt')}
        # uv's cache belongs to the controller; it never imports task.py.
        ruff = subprocess.run(['uv', 'tool', 'run', '--from', 'ruff==0.12.12',
                               'ruff', 'check', '--isolated', '--select', 'E4,E7,E9,F',
                               '--output-format', 'json', str(root/'task.py')],
                              capture_output=True, text=True, timeout=60)
        feedback = {'ruff': {'exit_code': ruff.returncode, 'output': json.loads(ruff.stdout),
                             'stderr': ruff.stderr},
                    'pyright': run([str(node), '--require', str(root/'guard.cjs'),
                                    str(pyright/'index.js'), '--outputjson',
                                    '--project', str(root/'pyrightconfig.json')], root, env),
                    'llr': run([str(llr), 'analyze', str(root), '--entry',
                                'task::'+entry, '--format', 'json'], root, env)}
        for tool, item in feedback.items():
            if item['output'] is None or item['exit_code'] not in ({0,1,3} if tool=='llr' else {0,1}):
                raise ValueError(f'{tool} tool failure: {item["exit_code"]} {item["stderr"]}')
        if not isinstance(feedback['ruff']['output'], list):
            raise ValueError('invalid Ruff diagnostics')
        summary = feedback['pyright']['output'].get('summary', {})
        if summary.get('filesAnalyzed') != 1:
            raise ValueError('Pyright did not analyze exactly one task')
        assert (root/'task.py').read_bytes() == source
        result = {'source_sha256': hashlib.sha256(source).hexdigest(),
                  'versions': {'ruff':'0.12.12','pyright':'1.1.414'},
                  'llr_binary_sha256': hashlib.sha256(llr.read_bytes()).hexdigest(),
                  'pyright_config': config, 'ruff_selection': 'E4,E7,E9,F',
                  'blocked_process_calls': (root/'blocked.txt').read_text().splitlines()
                      if (root/'blocked.txt').exists() else [],
                  'feedback': feedback}
        return json.loads(json.dumps(result).replace(str(root), '<task-root>'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--entry', default='_read_file')
    parser.add_argument('--node', type=Path, default=Path('/opt/homebrew/bin/node'))
    parser.add_argument('--pyright', type=Path, required=True)
    parser.add_argument('--llr', type=Path, required=True)
    args = parser.parse_args()
    result = collect(args.input.read_bytes(), args.node, args.pyright, args.llr.resolve(), args.entry)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps({tool: item['exit_code'] for tool,item in result['feedback'].items()}))


if __name__ == '__main__':
    main()

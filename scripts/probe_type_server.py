#!/usr/bin/env python3
"""Probe a pinned TSP server using only synthetic, isolated Python documents.

This is an opt-in developer probe, not an llr backend. Never loads target configs.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time

from type_facts import normalize, source_hash

GUARD = r"""
const cp = require('node:child_process');
const fs = require('node:fs');
for (const method of ['spawn','spawnSync','exec','execSync','execFile','execFileSync','fork']) {
  cp[method] = function () {
    fs.appendFileSync(process.env.LLR_TSP_GUARD_LOG, method + '\n');
    throw new Error('LLR probe blocks child process: ' + method);
  };
}
require('node:module').syncBuiltinESMExports();
"""

class Client:
    def __init__(self, proc):
        self.proc = proc
        self.messages = queue.Queue()
        self.stderr = []
        self.next_id = 0
        threading.Thread(target=self._read, daemon=True).start()
        threading.Thread(target=self._errors, daemon=True).start()

    def _errors(self):
        for line in self.proc.stderr:
            self.stderr.append(line.decode(errors='replace'))

    def _read(self):
        try:
            while True:
                headers = {}
                while True:
                    line = self.proc.stdout.readline()
                    if not line:
                        raise EOFError('type server closed stdout')
                    if line in (b'\r\n', b'\n'):
                        break
                    key, value = line.decode().split(':', 1)
                    headers[key.lower()] = value.strip()
                size = int(headers['content-length'])
                if not 0 <= size <= 16_000_000:
                    raise ValueError('oversized response')
                body = self.proc.stdout.read(size)
                self.messages.put(json.loads(body))
        except Exception as exc:
            self.messages.put(exc)

    def send(self, body):
        data = json.dumps({'jsonrpc': '2.0', **body}).encode()
        self.proc.stdin.write(f'Content-Length: {len(data)}\r\n\r\n'.encode() + data)
        self.proc.stdin.flush()

    def request(self, method, params=None):
        self.next_id += 1
        ident = self.next_id
        self.send({'id': ident, 'method': method, 'params': params})
        deadline = time.monotonic() + 25
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(method)
            message = self.messages.get(timeout=remaining)
            if isinstance(message, Exception):
                raise message
            if message.get('method') and 'id' in message:
                if message['method'] == 'workspace/configuration':
                    result = [{} for _ in message.get('params', {}).get('items', [])]
                else:
                    result = None
                self.send({'id': message['id'], 'result': result})
            elif message.get('id') == ident:
                return message

    def result(self, method, params=None):
        response = self.request(method, params)
        if 'error' in response:
            raise RuntimeError(f'{method}: {response["error"]}')
        return response.get('result')


def node_at(uri, source, marker):
    """TSP/LSP positions are UTF-16 units, not Python chars or UTF-8 bytes."""
    for line, text in enumerate(source.splitlines()):
        if marker in text:
            start = text.index(marker)
            offset = len(text[:start].encode('utf-16-le')) // 2
            end = offset + len(marker.encode('utf-16-le')) // 2
            return {'uri': uri, 'range': {'start': {'line': line, 'character': offset},
                                         'end': {'line': line, 'character': end}}}
    raise ValueError(marker)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--node', type=Path, required=True)
    parser.add_argument('--server-package', type=Path, required=True)
    parser.add_argument('--llr', type=Path, help='Run the complete live-provider to CLI bridge')
    parser.add_argument('--output', type=Path, default=Path('reports/v2/type-server-probe.json'))
    args = parser.parse_args()
    package = args.server_package.resolve()
    metadata = json.loads((package / 'package.json').read_text())
    if metadata['name'] != 'pyright-typeserver' or metadata['version'] != '1.1.414':
        raise ValueError('probe requires pyright-typeserver@1.1.414')
    entry = package / metadata['bin']['pyright-typeserver']
    provider_digest = hashlib.sha256()
    provider_files = sorted([package / 'package.json', entry, *(package / 'dist').rglob('*.js')])
    for part in provider_files:
        provider_digest.update(str(part.relative_to(package)).encode() + b'\0' + part.read_bytes())
    stub_digest = hashlib.sha256()
    stubs = sorted((package / 'dist' / 'typeshed-fallback').rglob('*.pyi'))
    assert stubs, 'missing pinned stubs'
    for stub in stubs:
        stub_digest.update(str(stub.relative_to(package)).encode() + b'\0' + stub.read_bytes())
    result = {'package': metadata['name'], 'version': metadata['version'],
              'entry_sha256': hashlib.sha256(entry.read_bytes()).hexdigest()}
    with tempfile.TemporaryDirectory(prefix='llr-tsp-probe-') as temporary:
        root = Path(temporary)
        guard = root / 'guard.cjs'
        guard.write_text(GUARD)
        guard_log = root / 'blocked-processes.txt'
        source = ("from pathlib import Path\nfrom helper import close_it\n"
                  "class Dialog:\n    def open(self) -> int:\n        return 1\n"
                  "p = Path('input')\nlabel = '中文😀'; f = p.open('rb')\n"
                  "other = Dialog().open()\nclose_it(f)\nunknown = mystery()\n"
                  "maybe = close_it if unknown_flag else mystery\nmaybe(f)\n"
                  "def replaced(f, replacement):\n    target = close_it\n    target = replacement\n    target(f)\n")
        helper = ("from pathlib import Path\nPath(__file__).with_suffix('.executed').write_text('unexpected execution')\n"
                  "def close_it(handle):\n    handle.close()\n")
        (root / 'helper.py').write_text(helper)
        (root / 'sitecustomize.py').write_text("from pathlib import Path\nPath(__file__).with_suffix('.executed').write_text('unexpected startup')\n")
        document = root / 'sample.py'
        document.write_text(source)
        env = {'PATH': str(args.node.resolve().parent), 'HOME': str(root),
               'LLR_TSP_GUARD_LOG': str(guard_log)}
        proc = subprocess.Popen([str(args.node.resolve()), '--require', str(guard), str(entry), '--stdio'],
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                cwd=root, env=env)
        client = Client(proc)
        try:
            result['initialize'] = client.result('initialize', {
                'processId': os.getpid(), 'rootUri': root.as_uri(),
                'workspaceFolders': [{'uri': root.as_uri(), 'name': 'synthetic'}],
                'capabilities': {'workspace': {'configuration': True}}})
            client.send({'method': 'initialized', 'params': {}})
            result['protocol'] = client.result('typeServer/getSupportedProtocolVersion')
            if result['protocol'] != '0.4.1':
                raise ValueError('unexpected protocol version')
            client.send({'method': 'textDocument/didOpen', 'params': {'textDocument': {
                'uri': document.as_uri(), 'languageId': 'python', 'version': 1, 'text': source}}})
            snapshot = client.result('typeServer/getSnapshot')
            result['snapshot'] = snapshot
            result['types'] = {}
            result['facts'] = {}
            for name, marker in [('path_open', 'p.open'), ('dialog_open', 'Dialog().open'),
                                 ('helper', 'close_it(f)'), ('unknown', 'mystery()'),
                                 ('mixed', 'maybe(f)'), ('replaced', 'target(f)')]:
                node = node_at(document.as_uri(), source, marker)
                if name in ('helper','mixed','replaced'):
                    node['range']['end']['character'] = node['range']['start']['character'] + len({'helper':'close_it','mixed':'maybe','replaced':'target'}[name])
                for attempt in range(6):
                    snapshot = client.result('typeServer/getSnapshot')
                    response = client.request('typeServer/getComputedType', {'arg': node, 'snapshot': snapshot})
                    if response.get('error', {}).get('code') != -32802:
                        break
                result['types'][name] = response
                binding = {'snapshot': snapshot, 'uri': document.as_uri(),
                           'document_sha256': source_hash(source),
                           'provider_sha256': provider_digest.hexdigest(),
                           'query_range': node['range'],
                           'stubs_sha256': stub_digest.hexdigest(),
                           'configuration_sha256': source_hash('isolated-probe-v1;configuration={};guard=' + GUARD),
                           'protocol': result['protocol']}
                result['facts'][name] = normalize(response, source=source, query=node,
                                                   binding=binding, expected_binding=binding)

            if args.llr:
                bundle = {'schema_version': 1,
                          'facts': [{'path': 'sample.py', 'normalized': fact} for fact in result['facts'].values()],
                          'documents': [{'uri': document.as_uri(), 'path': 'sample.py', 'document_sha256': source_hash(source)},
                                        {'uri': (root / 'helper.py').as_uri(), 'path': 'helper.py', 'document_sha256': source_hash(helper)}]}
                facts_file = root / 'type-facts.json'
                facts_file.write_text(json.dumps(bundle))
                def analyze_with_facts(use_facts=True):
                    command = [str(args.llr.resolve()), 'analyze', str(root), '--entry', 'sample::<module>', '--format', 'json']
                    if use_facts:
                        command += ['--type-evidence', str(facts_file)]
                    return subprocess.run(command, capture_output=True, text=True, timeout=25)
                baseline = analyze_with_facts(False)
                attached = analyze_with_facts()
                assert baseline.returncode == attached.returncode == 3, attached.stderr
                baseline_report = json.loads(baseline.stdout)
                attached_report = json.loads(attached.stdout)
                assert baseline_report['obligations'] == attached_report['obligations']
                assert baseline_report['gaps'] == attached_report['gaps']
                matches = attached_report['nominal_declaration_matches']
                names = list(result['facts'])
                helper_matches = [m for m in matches if m['fact_index'] == names.index('helper')]
                dialog_matches = [m for m in matches if m['fact_index'] == names.index('dialog_open')]
                assert any(m['symbol'] == 'helper::close_it' for m in helper_matches), helper_matches
                assert any(m['symbol'] == 'sample::Dialog.open' for m in dialog_matches), dialog_matches
                assert result['facts']['mixed']['status'] == result['facts']['replaced']['status'] == 'unknown'
                (root / 'helper.py').write_text(helper + '# edited target\n')
                stale_target = analyze_with_facts()
                assert stale_target.returncode == 2, stale_target.stdout
                (root / 'helper.py').write_text(helper)
                document.write_text(source + '# edited query document\n')
                stale_source = analyze_with_facts()
                assert stale_source.returncode == 2, stale_source.stdout
                document.write_text(source)
                result['cli_bridge'] = {'acceptance': 'live_provider_to_cli_passed', 'baseline_exit': baseline.returncode,
                                        'attached_exit': attached.returncode, 'proof_obligations_unchanged': True,
                                        'declaration_matches': matches,
                                        'mixed_status': result['facts']['mixed']['status'],
                                        'replaced_status': result['facts']['replaced']['status'],
                                        'stale_target_exit': stale_target.returncode, 'stale_source_exit': stale_source.returncode}
            result['snapshot'] = snapshot
            changed = source.replace("p.open('rb')", '1')
            client.send({'method': 'textDocument/didChange', 'params': {
                'textDocument': {'uri': document.as_uri(), 'version': 2}, 'contentChanges': [{'text': changed}]}})
            result['new_snapshot'] = client.result('typeServer/getSnapshot')
            result['stale_response'] = client.request('typeServer/getComputedType', {
                'arg': node_at(document.as_uri(), source, 'p.open'), 'snapshot': snapshot})
            types = {name: value.get('result') for name, value in result['types'].items()}
            overloads = (types['path_open'] or {}).get('overloads', [])
            assert overloads and all('/pathlib/' in item['declaration']['node']['uri'] for item in overloads)
            assert types['dialog_open']['declaration']['node']['uri'].endswith('/sample.py')
            assert types['helper']['declaration']['node']['uri'].endswith('/helper.py')
            assert types['unknown']['name'] == 'unknown'
            assert all(result['facts'][name]['status'] == 'candidates'
                       for name in ('path_open', 'dialog_open', 'helper'))
            assert result['facts']['unknown']['status'] == 'unknown'
            assert all(fact['dispatch'] == 'nominal_candidates_only' for fact in result['facts'].values())
            assert result['stale_response'].get('error', {}).get('code') == -32802
            assert result['new_snapshot'] != result['snapshot']
            assert not list(root.glob('*.executed')), 'synthetic target/startup code was executed'
            result['execution_canaries'] = 'not_triggered'
            result['acceptance'] = 'guarded_probe_passed'
            client.result('shutdown')
            client.send({'method': 'exit'})
        finally:
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=3)
            result['blocked_process_calls'] = guard_log.read_text().splitlines() if guard_log.exists() else []
        # Remove temporary absolute paths from durable evidence.
        serialized = json.dumps(result, indent=2).replace(str(root), '<synthetic-root>').replace(str(package), '<server-package>')
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(serialized + '\n')
        print(json.dumps({'acceptance': result.get('acceptance'), 'protocol': result.get('protocol'), 'snapshot': result.get('snapshot'),
                          'new_snapshot': result.get('new_snapshot'),
                          'type_queries': {k: ('error' if 'error' in v else 'result') for k, v in result.get('types', {}).items()},
                          'blocked_process_calls': result['blocked_process_calls']}))

if __name__ == '__main__':
    main()

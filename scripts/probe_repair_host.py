#!/usr/bin/env python3
"""Probe an ephemeral Codex CLI host with synthetic content only."""
import json
from pathlib import Path
import subprocess
import tempfile

DISABLED = ['shell_tool', 'unified_exec', 'multi_agent', 'multi_agent_v2', 'apps',
            'browser_use', 'browser_use_external', 'computer_use', 'in_app_browser',
            'code_mode', 'plugins', 'hooks', 'memories', 'skill_search',
            'image_generation', 'view_image', 'workspace_dependencies', 'goals', 'sleep_tool']


def command(root):
    cmd = ['/opt/homebrew/bin/codex', 'exec', '--ignore-user-config', '--ignore-rules',
           '--ephemeral', '--skip-git-repo-check', '--sandbox', 'read-only',
           '--cd', str(root), '--json', '-c', 'web_search="disabled"',
           '-c', 'project_doc_max_bytes=0', '-c', 'skills.bundled.enabled=false',
           '--enable', 'skip_host_skill_discovery']
    for feature in DISABLED:
        cmd += ['--disable', feature]
    return cmd


def main():
    with tempfile.TemporaryDirectory(prefix='llr-host-probe-') as temporary:
        root = Path(temporary)
        prompt = ('This is a synthetic host capability probe, not a repair task. '
                  'Do not call any tool. Return JSON with keys visible_tool_names '
                  '(all callable tool names currently exposed to you), '
                  'prior_task_context (whether you see a prior user task), '
                  'marker (copy H7K4). Do not guess unavailable context.')
        completed = subprocess.run(command(root)+[prompt], capture_output=True,
                                   text=True, timeout=90)
        events = [json.loads(line) for line in completed.stdout.splitlines() if line.startswith('{')]
        messages = [e['item']['text'] for e in events if e.get('item', {}).get('type') == 'agent_message']
        errors = [e['item']['message'] for e in events if e.get('item', {}).get('type') == 'error']
        capability = json.loads(messages[-1]) if messages else {}
        # Self-report alone cannot certify isolation. Any visible tools or host error
        # rejects this candidate configuration before real task content is sent.
        rejected = bool(completed.returncode or errors or capability.get('visible_tool_names')
                        or capability.get('prior_task_context') is not False
                        or capability.get('marker') != 'H7K4')
        output = Path('reports/v2/repair-host-probe.json')
        output.parent.mkdir(parents=True, exist_ok=True)
        # No target inputs, private files or credentials are supplied to the model.
        result = {'exit_code': completed.returncode, 'command': command(Path('<empty-root>')),
                  'stdout': completed.stdout, 'stderr': completed.stderr,
                  'kind': 'synthetic_capability_probe_not_isolation_proof',
                  'admission': 'rejected' if rejected else 'requires_independent_verification',
                  'capability_self_report': capability, 'host_errors': errors}
        output.write_text(json.dumps(result, indent=2)+'\n')
        print(completed.stdout)
        print('exit_code:', completed.returncode, 'admission:', result['admission'])
        if completed.returncode:
            print(completed.stderr[-2000:])


if __name__ == '__main__':
    main()

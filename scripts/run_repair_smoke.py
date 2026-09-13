#!/usr/bin/env python3
"""Exploratory fresh-context smoke with observed tool-event auditing.

Not a sandbox isolation proof or confirmatory efficacy experiment.
"""
import argparse
import json
import hashlib
from pathlib import Path
import subprocess
import tempfile
import time
from probe_repair_host import command
from prepare_repair_eval import CONTRACT

MODEL='gpt-6-astra'
EFFORT='medium'


def invoke(prompt, schema, output):
    with tempfile.TemporaryDirectory(prefix='llr-text-attempt-') as temporary:
        root=Path(temporary)
        schema_file=root/'schema.json';schema_file.write_text(json.dumps(schema))
        started=time.monotonic()
        cmd=command(root)+['-m',MODEL,'-c',f'model_reasoning_effort="{EFFORT}"',
                           '--output-schema',str(schema_file),'-']
        try:
            completed=subprocess.run(cmd,input=prompt,capture_output=True,text=True,timeout=180)
            events=[json.loads(line) for line in completed.stdout.splitlines() if line.startswith('{')]
            messages=[];violations=[];usage=None
            for event in events:
                kind=event.get('type');item=event.get('item',{})
                if kind=='turn.completed':usage=event.get('usage')
                if kind not in {'thread.started','turn.started','turn.completed','item.started','item.updated','item.completed'}:
                    violations.append(event)
                if item:
                    item_type=item.get('type')
                    if item_type=='agent_message' and kind=='item.completed':messages.append(item['text'])
                    elif item_type=='error':
                        if not item.get('message','').startswith('Under-development features enabled: skip_host_skill_discovery.'):
                            violations.append(event)
                    elif item_type not in {'agent_message','reasoning'}:
                        violations.append(event)
            final=json.loads(messages[-1]) if messages else None
            total=sum((usage or {}).get(k,0) for k in ['input_tokens','output_tokens'])
            valid=completed.returncode==0 and not violations and final is not None and usage is not None
            valid=valid and total<=60000 and len(messages[-1].encode())<=65536
            result={'model':MODEL,'reasoning':EFFORT,'status':'observed_text_only' if valid else 'invalid',
                    'enforced_isolation':False,'exit_code':completed.returncode,'usage':usage,
                    'elapsed_seconds':time.monotonic()-started,'cost':None,'events':events,
                    'violations':violations,'response':final,
                    'prompt_sha256':hashlib.sha256(prompt.encode()).hexdigest()}
        except (subprocess.TimeoutExpired,json.JSONDecodeError) as exc:
            result={'status':'invalid','reason':type(exc).__name__,'elapsed_seconds':time.monotonic()-started}
    output.parent.mkdir(parents=True,exist_ok=True)
    with output.open('x') as stream:json.dump(result,stream,indent=2)
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--attempt',required=True)
    parser.add_argument('--run-root',type=Path,required=True)
    parser.add_argument('--output-root',type=Path,required=True)
    args=parser.parse_args()
    repo=Path(__file__).resolve().parents[1]
    runs=json.loads((args.run_root/'controller/runs.json').read_text())
    attempt=next(r for r in runs['attempts'] if r['attempt_id']==args.attempt)
    worker=args.run_root/'workers'/args.attempt
    source=(worker/'task.py').read_text()
    assert hashlib.sha256(source.encode()).hexdigest()==attempt['input_sha256']
    number=attempt['task_id'].split('-')[-1]
    feedback=json.loads((repo/f'reports/v2/repair-smoke-feedback-{number}.json').read_text())
    assert feedback['source_sha256']==attempt['input_sha256']
    tools={k:v for k,v in feedback['feedback'].items() if k!='llr' or attempt['arm']=='llr'}
    prompt=('Solve using only the supplied text. Do not call any tools, read files, or delegate.\n'+CONTRACT+
            '\nSOURCE:\n'+source+'\nSTATIC TOOL FEEDBACK:\n'+json.dumps(tools)+
            '\nReturn JSON containing source: the full replacement Python source. No markdown fences.')
    schema={'type':'object','properties':{'source':{'type':'string'}},'required':['source'],'additionalProperties':False}
    result=invoke(prompt,schema,args.output_root/(args.attempt+'.json'))
    print(json.dumps({'attempt':args.attempt,'status':result['status'],'usage':result.get('usage')}),flush=True)


if __name__=='__main__':main()

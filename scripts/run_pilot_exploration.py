#!/usr/bin/env python3
"""Three source-backed drafts, exploratory only. No analyzer tuning."""
import argparse
import ast
import json
import hashlib
from pathlib import Path
from run_repair_smoke import invoke
from collect_repair_feedback import collect

REPO=Path(__file__).resolve().parents[1]
DRAFT=REPO/'eval/repair/pilot-draft'
RESULT=REPO/'reports/v2/pilot-exploration'
TASKS={'marker_validation':'_read_runtime_replacement_marker','json_lines':'save_as_jsonl','ready_payload':'_ready_payload'}


def admission(task):
    original=(DRAFT/'controller'/task/'reference.py').read_text()
    source=(DRAFT/task/'task.py').read_text()
    contract=(DRAFT/task/'task.md').read_text()
    schema={'type':'object','properties':{
        'admissible':{'type':'boolean'},
        'input_verdict':{'type':'string','enum':['correct','incorrect','inconclusive']},
        'reason':{'type':'string'},'assumptions':{'type':'string'}},
        'required':['admissible','input_verdict','reason','assumptions'],'additionalProperties':False}
    prompt=('Use only provided text, no tools/files/delegation. Static independent task admission review. '
            'Decide whether the contract faithfully captures the reference behavior under standard library semantics, '
            'and whether the input has an unambiguous defect or is correct. Do not demand executing the program. '
            'External logging is assumed to have its standard behavior. If the contract is contradictory or '
            'the task cannot be judged with explicit assumptions, set admissible false.\nCONTRACT:\n'+contract+
            '\nREFERENCE:\n'+original+'\nINPUT:\n'+source)
    result=invoke(prompt,schema,RESULT/(task+'-admission.json'))
    print(task,'admission',result['status'],result.get('response'),flush=True)


def feedback(task):
    admission_result=json.loads((RESULT/(task+'-admission.json')).read_text())
    if admission_result['status']!='observed_text_only' or not admission_result['response']['admissible']:
        raise ValueError('task not admitted')
    source=(DRAFT/task/'task.py').read_bytes()
    record=json.loads((DRAFT/'controller'/task/'record.json').read_text())
    if hashlib.sha256(source).hexdigest()!=record['input_sha256']:
        raise ValueError('draft input hash changed')
    expected='incorrect' if record['condition']=='error' else 'correct'
    if admission_result['response']['input_verdict']!=expected:
        raise ValueError('independent admission disagrees with controller label')
    result=collect(source,Path('/opt/homebrew/bin/node'),Path('/tmp/llr-eval-tools/node_modules/pyright'),Path('/Users/jiaweichen/.local/bin/llr'),TASKS[task])
    (RESULT/(task+'-feedback.json')).write_text(json.dumps(result,indent=2)+'\n')
    print(task,'feedback',{k:v['exit_code'] for k,v in result['feedback'].items()},flush=True)


def repair(task,arm):
    source=(DRAFT/task/'task.py').read_text();contract=(DRAFT/task/'task.md').read_text()
    all_feedback=json.loads((RESULT/(task+'-feedback.json')).read_text())
    tools={k:v for k,v in all_feedback['feedback'].items() if k!='llr' or arm=='llr'}
    schema={'type':'object','properties':{'source':{'type':'string'}},'required':['source'],'additionalProperties':False}
    prompt=('Use only supplied text. No tools, files or delegation. Preserve the provided library imports '
            'and dependency choices; missing third-party packages in the diagnostic environment do not authorize '
            'removing business behavior. Return full replacement Python source as JSON.\nCONTRACT:\n'+contract+
            '\nSOURCE:\n'+source+'\nSTATIC FEEDBACK:\n'+json.dumps(tools))
    result=invoke(prompt,schema,RESULT/(task+'-'+arm+'-repair.json'))
    print(task,arm,'repair',result['status'],flush=True)


def review(task,arm):
    r=json.loads((RESULT/(task+'-'+arm+'-repair.json')).read_text())
    if r['status']!='observed_text_only':raise ValueError('invalid repair')
    candidate=r['response']['source'];ast.parse(candidate)
    original=(DRAFT/task/'task.py').read_text();contract=(DRAFT/task/'task.md').read_text()
    schema={'type':'object','properties':{'verdict':{'type':'string','enum':['correct','incorrect','inconclusive']},
        'evidence':{'type':'string'},'limitations':{'type':'string'}},'required':['verdict','evidence','limitations'],'additionalProperties':False}
    prompt=('Static blinded review. Use only text, no tools/files/delegation. Judge candidate against contract; '
            'original input may contain a defect. Check successful results, error/cleanup paths, validation logic, '
            'ordering and retained actual read/write behavior. Do not treat disappeared lint as correctness. '
            'State assumptions and inconclusive when necessary.\nCONTRACT:\n'+contract+'\nORIGINAL INPUT:\n'+original+'\nCANDIDATE:\n'+candidate)
    result=invoke(prompt,schema,RESULT/(task+'-'+arm+'-review.json'))
    post=collect(candidate.encode(),Path('/opt/homebrew/bin/node'),Path('/tmp/llr-eval-tools/node_modules/pyright'),Path('/Users/jiaweichen/.local/bin/llr'),TASKS[task])
    (RESULT/(task+'-'+arm+'-postcheck.json')).write_text(json.dumps(post,indent=2)+'\n')
    print(task,arm,'review',result['status'],result.get('response'),flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage',choices=['admission','feedback','repair','review'])
    parser.add_argument('task',choices=TASKS)
    parser.add_argument('--arm',choices=['baseline','llr'])
    a=parser.parse_args()
    if a.stage in {'repair','review'}:
        if not a.arm:parser.error('--arm required')
        globals()[a.stage](a.task,a.arm)
    else:globals()[a.stage](a.task)

#!/usr/bin/env python3
"""Fresh-context static review. No condition, diagnostics or reference repair."""
import argparse
import ast
import hashlib
import json
from pathlib import Path
from run_repair_smoke import invoke
from prepare_repair_eval import CONTRACT


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--attempt',required=True)
    parser.add_argument('--run-root',type=Path,required=True)
    parser.add_argument('--output-root',type=Path,required=True)
    args=parser.parse_args()
    run=json.loads((args.output_root/(args.attempt+'.json')).read_text())
    if run['status']!='observed_text_only':
        raise ValueError('invalid repair attempt cannot be reviewed as valid')
    candidate=run['response']['source']
    ast.parse(candidate)  # Syntax only. Never imports or executes the candidate.
    original=(args.run_root/'workers'/args.attempt/'task.py').read_text()
    prompt=('You are a static correctness reviewer in a fresh context. Use only supplied text; '
            'do not use tools, files, or delegation. No program execution. Judge against the contract, '
            'not whether it resembles the original. The original may contain a defect. '
            'Review successful UTF-8 reads, read failure, open failure, cleanup failure, '
            'preservation of helper interfaces, and absence of hardcoded/deleted behavior. '
            'Return incorrect for a demonstrated violation, inconclusive when evidence is insufficient.\n'
            'CONTRACT:\n'+CONTRACT+'\nORIGINAL INPUT:\n'+original+'\nCANDIDATE:\n'+candidate)
    schema={'type':'object','properties':{
        'verdict':{'type':'string','enum':['correct','incorrect','inconclusive']},
        'evidence':{'type':'string'},'limitations':{'type':'string'}},
        'required':['verdict','evidence','limitations'],'additionalProperties':False}
    result=invoke(prompt,schema,args.output_root/(args.attempt+'-review.json'))
    print(json.dumps({'attempt':args.attempt,'review_status':result['status'],
                      'verdict':(result.get('response') or {}).get('verdict'),
                      'candidate_sha256':hashlib.sha256(candidate.encode()).hexdigest()}),flush=True)


if __name__=='__main__':main()

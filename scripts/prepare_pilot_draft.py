#!/usr/bin/env python3
"""Prepare source-backed draft tasks; static validation only, no llr selection."""
import ast
import hashlib
import json
from pathlib import Path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def build(source_root, repo, output):
    inventory=json.loads((repo/'eval/repair/candidate-inventory.json').read_text())
    specs=[
        ('marker_validation','_read_runtime_replacement_marker','error',
         'import json\nRUNTIME_RETIRED_MARKER_PATH = "input.json"\n\n',
         'Keep the no-argument API. Read the configured UTF-8 JSON marker; preserve all marker validation errors, legacy retired-state fallback, whitespace normalization, aborted-lease validation/deduplication and returned (state, lease_id, aborted_lease_ids) tuple. Propagate file/JSON/validation errors as before; close the file on success and errors.'),
        ('json_lines','save_as_jsonl','error',
         'import json\nfrom pathlib import Path\nfrom typing import List, Dict, Any\nfrom loguru import logger\n\n',
         'Preserve save_as_jsonl(comments, output_file). Create parents, overwrite the destination in UTF-8, write one ensure_ascii=False JSON object plus newline per comment in input order, including empty-list behavior. Preserve success/info logging and file-size reporting after successful writes. Propagate exceptions; close the output file on normal and exceptional exits.'),
        ('ready_payload','_ready_payload','control',
         'import json\nREADY_PATH = "input.json"\n\n',
         'Preserve the no-argument API. Read READY_PATH as UTF-8 JSON and return it only if it is a dictionary, otherwise None. Return None on any Exception from opening, JSON reading or cleanup. Close an acquired file on all paths.')]
    prepared=[]
    for task_id, name, condition, scaffold, contract in specs:
        candidate=next(c for c in inventory['candidates'] if c['function']==name)
        data=(source_root/candidate['path']).read_bytes()
        assert sha(data)==candidate['file_sha256'], 'source snapshot changed'
        excerpt=b''.join(data.splitlines(keepends=True)[candidate['line_start']-1:candidate['line_end']])
        assert sha(excerpt)==candidate['excerpt_sha256']
        original=scaffold+excerpt.decode()
        if task_id=='marker_validation':
            before='        marker = json.load(handle)'
            after='        pass\n    marker = json.load(handle)'
        elif task_id=='json_lines':
            before='        for comment in comments:\n            f.write(json.dumps(comment, ensure_ascii=False) + "\\n")'
            after='        pass\n    for comment in comments:\n        f.write(json.dumps(comment, ensure_ascii=False) + "\\n")'
        else:
            before=after=None
        if before:
            assert original.count(before)==1
            task=original.replace(before,after,1)
        else:
            task=original
        ast.parse(original);ast.parse(task)
        prepared.append((task_id,original,task,contract,{
            'task_id':task_id,'condition':condition,'status':'draft_requires_independent_review',
            'source':candidate,'scaffolding':scaffold,'input_sha256':sha(task.encode()),
            'reference_sha256':sha(original.encode()),'mutation':{'before':before,'after':after},
            'origin':'synthetic_mutation' if before else 'source_control_with_scaffolding',
            'is_existing_museon_bug':False,'mechanism':'file_context_lifetime',
            'subgroup':'json_write_loop' if task_id=='json_lines' else 'json_read',
            'independent_verdict':None}))
    output.mkdir(parents=True,exist_ok=False)
    for task_id,original,task,contract,record in prepared:
        directory=output/task_id;directory.mkdir()
        (directory/'task.py').write_text(task)
        (directory/'task.md').write_text('Review and repair if needed. Correct code may remain unchanged.\n'+contract+'\nDo not execute or import target code. Return complete replacement source.\n')
        # Reference is controller-only, never part of worker packet.
        controller=output/'controller'/task_id;controller.mkdir(parents=True)
        (controller/'reference.py').write_text(original)
        (controller/'record.json').write_text(json.dumps(record,indent=2)+'\n')
    return [record for *_,record in prepared]


if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-root',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    records=build(args.source_root,Path(__file__).resolve().parents[1],args.output)
    print(json.dumps({'drafts':len(records),'admitted':0,'model_runs':0}))

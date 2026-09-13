#!/usr/bin/env python3
import ast
import json
from pathlib import Path
from run_pilot_exploration import TASKS,RESULT,DRAFT

rows=[]
for task in TASKS:
    original=(DRAFT/task/'task.py').read_text()
    before=json.loads((RESULT/(task+'-feedback.json')).read_text())
    for arm in ['baseline','llr']:
        repair=json.loads((RESULT/(task+'-'+arm+'-repair.json')).read_text())
        review=json.loads((RESULT/(task+'-'+arm+'-review.json')).read_text())
        post=json.loads((RESULT/(task+'-'+arm+'-postcheck.json')).read_text())
        assert repair['status']==review['status']=='observed_text_only'
        assert before['llr_binary_sha256']==post['llr_binary_sha256']
        u=repair['usage']
        rows.append({'task':task,'arm':arm,'review':review['response']['verdict'],
                     'llr_before':before['feedback']['llr']['exit_code'],
                     'llr_after':post['feedback']['llr']['exit_code'],
                     'tokens':u['input_tokens']+u['output_tokens'],
                     'ast_changed':ast.dump(ast.parse(original))!=ast.dump(ast.parse(repair['response']['source'])),
                     'candidate_sha256':post['source_sha256'],
                     'pyright_after':post['feedback']['pyright']['exit_code']})
summary={'kind':'exploratory_trace_audited','independent_admissions':3,'repair_attempts':6,'blind_reviews':6,'enforced_isolation':False,'rows':rows}
(RESULT/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(rows,indent=2))

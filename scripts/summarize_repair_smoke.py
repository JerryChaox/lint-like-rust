#!/usr/bin/env python3
"""Join frozen arm assignment, observed repair, blinded review and postcheck."""
import argparse
import json
from pathlib import Path


def summarize(run_root, result_root):
    assignments=json.loads((run_root/'controller/runs.json').read_text())['attempts']
    rows=[]
    for assignment in assignments:
        ident=assignment['attempt_id']
        repair=json.loads((result_root/(ident+'.json')).read_text())
        review=json.loads((result_root/(ident+'-review.json')).read_text())
        check=json.loads((result_root/(ident+'-postcheck.json')).read_text())
        valid=repair['status']==review['status']=='observed_text_only'
        usage=repair.get('usage',{})
        rows.append({'attempt':ident,'task':assignment['task_id'],'arm':assignment['arm'],
                     'valid_observed_trace':valid,'verdict':review['response']['verdict'] if valid else 'invalid',
                     'review_evidence':review.get('response'),
                     'llr_exit':check['feedback']['llr']['exit_code'],
                     'tokens':usage.get('input_tokens',0)+usage.get('output_tokens',0),
                     'seconds':round(repair['elapsed_seconds'],2),
                     'candidate_sha256':check['source_sha256'],'rounds':1})
    return {'kind':'exploratory_trace_audited_smoke','enforced_isolation':False,
            'model':'gpt-6-astra','reasoning':'medium','repair_attempts':len(rows),
            'blind_review_attempts':len(rows),'cost':None,'rows':rows}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-root',type=Path,required=True)
    parser.add_argument('--result-root',type=Path,required=True)
    args=parser.parse_args()
    result=summarize(args.run_root,args.result_root)
    (args.result_root/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
    for row in result['rows']:print({k:v for k,v in row.items() if k!='review_evidence'})


if __name__=='__main__':main()

#!/usr/bin/env python3
"""Static controlled-caller acceptance for frozen log bodies; not corpus coverage."""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

PROJECT=Path(__file__).resolve().parents[1]
CALLER="""
def run():
    date='2026-09-13'
    record={'ts':'2026-09-13T12:00:00Z','decision':'sleep_window_wait','nested':[1,True,None,{'key':'value'}]}
    _append_brain_log(Path('logs'),date,record)
"""
UNKNOWN="""
def run():
    date='2026-09-13'
    record=external()
    _append_brain_log(Path('logs'),date,record)
"""
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',type=Path,default=Path.home()/'.local/bin/llr');p.add_argument('--source-root',type=Path,default=Path('/Users/jiaweichen/Documents/vscode/museon/museon'));args=p.parse_args()
    manifest=json.loads((PROJECT/'tests/corpus_v2/manifest.json').read_text());case=next(c for c in manifest['cases'] if c['id']=='append_brain_log')
    output=PROJECT/'reports/v2/log-data-scope';output.mkdir(parents=True,exist_ok=True)
    rows=[]
    cases=[(name,v,CALLER,1 if v['expected_rules'] else 0) for name,v in case['variants'].items()]
    cases.append(('unknown',case['variants']['repaired'],UNKNOWN,3))
    for name,variant,caller,expected in cases:
        body=(PROJECT/'tests/corpus_v2'/variant['path']).read_bytes();digest=hashlib.sha256(body).hexdigest();assert digest==variant['sha256']
        with tempfile.TemporaryDirectory(prefix='llr-log-data-') as td:
            path=Path(td)/'case.py';path.write_bytes(body+caller.encode())
            target=output/(name+'.json')
            r=subprocess.run([str(args.binary.resolve()),'analyze',str(path),'--entry','case::run','--format','json','--output',str(target)],capture_output=True,text=True)
            assert r.returncode==expected,r.stdout+r.stderr
            report=json.loads(target.read_text())
            rows.append({'name':name,'frozen_body_sha256':digest,'caller_is_synthetic':True,'source_body_unchanged':True,'exit_code':r.returncode,'gaps':len(report['gaps']),'counts_as_fixed_corpus_coverage':False})
    for name,expected,kind in [('repaired',0,'resolved'),('unknown',3,'became_unverified')]:
        target=output/('compare-'+name+'.json')
        r=subprocess.run([str(args.binary.resolve()),'compare',str(output/'mutated_error.json'),str(output/(name+'.json')),'--output',str(target)],capture_output=True,text=True)
        delta=json.loads(target.read_text());assert r.returncode==expected and delta['issues'] and all(i['kind']==kind for i in delta['issues']),delta
    live_path=args.source_root/case['source']['path'];live=live_path.read_bytes()
    assert hashlib.sha256(live).hexdigest()==case['source']['file_sha256']
    tree=ast.parse(live);calls=[]
    for node in ast.walk(tree):
        if isinstance(node,ast.Call) and isinstance(node.func,ast.Name) and node.func.id=='_append_brain_log':
            record=node.args[2]
            fields=[]
            if isinstance(record,ast.Dict):
                for key,value in zip(record.keys,record.values):
                    fields.append({'key':key.value if isinstance(key,ast.Constant) else None,'value_syntax':ast.unparse(value),'ast_kind':type(value).__name__})
            calls.append({'line':node.lineno,'record_fields':fields,'actual_callsite_verified':False})
    (output/'summary.json').write_text(json.dumps({'scope':'controlled caller development only','fixtures':rows,'actual_museon_source_sha256':hashlib.sha256(live).hexdigest(),'actual_museon_calls':calls,'actual_callers_unresolved':'Timestamp datetime protocol and state/phase input provenance remain unproved; dictionary syntax alone does not certify its values.'},indent=2)+'\n')
    print(f'Controlled log callers:0/1/0/3; compare resolved/became_unverified. Audited {len(calls)} actual calls, not claimed verified; fixed corpus unchanged.')

if __name__=='__main__':main()

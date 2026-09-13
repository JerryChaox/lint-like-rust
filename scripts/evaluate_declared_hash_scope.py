#!/usr/bin/env python3
"""Evaluate frozen hash inputs under their already-declared standard-Path assumption.

No source edits, target imports or target execution. Default acceptance is separate.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

PROJECT = Path(__file__).resolve().parents[1]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=Path.home()/'.local/bin/llr')
    parser.add_argument('--output',type=Path,default=PROJECT/'reports/v2/declared-hash-scope')
    args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    manifest_path=PROJECT/'tests/corpus_v2/manifest.json'
    manifest=json.loads(manifest_path.read_text())
    case=next(c for c in manifest['cases'] if c['id']=='path_sha256')
    rows=[]
    for name,variant in case['variants'].items():
        # This assertion is frozen in the original benchmark, not a newly
        # inferred type or an extra assumption invented to improve its score.
        assertion='path is a standard pathlib.Path instance as annotated'
        assert assertion in variant['scope']['assumptions']
        assert 'custom Path subclasses' in variant['scope']['not_proven']
        content=(PROJECT/'tests/corpus_v2'/variant['path']).read_bytes()
        digest=hashlib.sha256(content).hexdigest();assert digest==variant['sha256']
        with tempfile.TemporaryDirectory(prefix='llr-entry-contract-') as td:
            path=Path(td)/'case.py';path.write_bytes(content)
            bundle={'schema_version':1,'entries':[{'path':'case.py','source_sha256':digest,'symbol':'case::_sha256','parameters':[{'name':'path','kind':'exact_stdlib_path'}]}]}
            contract=Path(td)/'entry.json';contract.write_text(json.dumps(bundle))
            output=args.output/(name+'.json')
            command=[str(args.binary.resolve()),'analyze',str(path),'--format','json','--entry-contract',str(contract),'--output',str(output)]
            result=subprocess.run(command,capture_output=True,text=True)
            expected=1 if variant['expected_rules'] else 0
            assert result.returncode==expected,result.stdout+result.stderr
            report=json.loads(output.read_text())
            assert not report['gaps'],report
            rules=sorted({o['key']['rule'] for o in report['obligations'] if o['status']=='violated'})
            assert rules==sorted(variant['expected_rules'])
            assert report['verification_basis']=='conditional_on_explicit_caller_assumptions'
            default=subprocess.run([str(args.binary.resolve()),'analyze',str(path),'--format','json'],capture_output=True,text=True)
            assert default.returncode==3,default.stdout+default.stderr
        rows.append({'variant':name,'source_sha256':digest,'manifest_assumption':assertion,'caller_asserted_not_inferred':True,'exit_code':result.returncode,'rules':rules,'default_exit_code':3,'report':str(output.relative_to(args.output))})
    compare=args.output/'compare-repaired.json'
    result=subprocess.run([str(args.binary.resolve()),'compare',str(args.output/'mutated_error.json'),str(args.output/'repaired.json'),'--output',str(compare)],capture_output=True,text=True)
    delta=json.loads(compare.read_text());assert result.returncode==0 and delta['issues'] and all(i['kind']=='resolved' for i in delta['issues']),delta
    summary={'schema_version':1,'manifest_sha256':hashlib.sha256(manifest_path.read_bytes()).hexdigest(),'scoped_hash_targets_met':3,'scoped_hash_targets_total':3,'default_hash_targets_met':0,'original_manifest_and_sources_unchanged':True,'rows':rows}
    (args.output/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    print('Frozen hash under its declared exact-Path assumption:3/3; CLI0/1/0 and compare resolved. Default hash remains0/3.')

if __name__=='__main__':main()

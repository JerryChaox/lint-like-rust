#!/usr/bin/env python3
"""Static CLI acceptance; never executes the Python fixtures."""
import argparse
import json
from pathlib import Path
import subprocess
p=argparse.ArgumentParser();p.add_argument('--binary',default=str(Path.home()/'.local/bin/llr'));args=p.parse_args()
out=Path('reports/v2/ownership-transfer');out.mkdir(parents=True,exist_ok=True)
for name,expected in [('before',1),('after',0),('unknown',3)]:
    target=out/(name+'.json')
    r=subprocess.run([args.binary,'analyze','examples/v2/ownership-transfer/'+name,'--entry','case::run','--format','json','--output',str(target)],capture_output=True,text=True)
    assert r.returncode==expected,r.stderr+r.stdout
    if name=='before':
        j=json.loads(target.read_text());errors=[o for o in j['obligations'] if o['status']=='violated']
        assert len(errors)==1 and errors[0]['key']['rule']=='OWN001'
        assert errors[0]['evidence']['class']=='safety_policy'
for name,expected,kind in [('after',0,'resolved'),('unknown',3,'became_unverified')]:
    target=out/('compare-'+name+'.json')
    r=subprocess.run([args.binary,'compare',str(out/'before.json'),str(out/(name+'.json')),'--output',str(target)],capture_output=True,text=True)
    j=json.loads(target.read_text());assert r.returncode==expected and j['issues'][0]['kind']==kind,j
print('OWN001 policy before/repair/unknown:1/0/3; compare resolved/became_unverified.')

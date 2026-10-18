"""Retain and replace the initial LP-reference adapter's unused MIP-bound field."""
import argparse,json
from pathlib import Path
from run import ROOT,BASE,run_one
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--file',type=Path,required=True);a=p.parse_args();data=json.loads(a.file.read_text());corrections=[]
for i,r in enumerate(data['records']):
 if r.get('solver')=='highs'and r.get('issues')==['bound on wrong side']:
  path=next(ROOT.glob('docs/*/corpus/**/'+r['case']+'.json'));spec=json.loads(path.read_text())['model'];assert all(k=='continuous'for k in spec['kinds'])
  replacement=run_one('highs',path,data['time_limit'],'default',a.binary)
  assert not replacement.get('issues');corrections.append(dict(reason='LP has no MIP dual bound; use independently checked LP row-dual bound',original=r,replacement=replacement));data['records'][i]=replacement
if corrections:
 a.file.with_name('reference-adapter-corrections.json').write_text(json.dumps(corrections,indent=2)+'\n');a.file.write_text(json.dumps(data,indent=2)+'\n')
print('LP reference corrections',len(corrections))

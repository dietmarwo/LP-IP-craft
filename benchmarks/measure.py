import json,subprocess,statistics,sys,hashlib
from pathlib import Path
b=Path(__file__).parent
iteration=int(sys.argv[1]); previous='baseline' if iteration==1 else f'iteration-{iteration-1}'
cases=[('selection',12),('selection',32),('selection',64),('packing',1024),('packing',4096),('irregular',1024),('network',2048),('box-lp',0),('knapsack',0),('nodecopy',0)]
rows=[]
for run in range(3):
    for name,size in cases:
        for version in ([previous,f'iteration-{iteration}'] if run%2==0 else [f'iteration-{iteration}',previous]):
            proc=subprocess.run([str(b/'binaries'/version),name,str(size),'warm','1','1'],capture_output=True,text=True,timeout=90)
            assert proc.returncode==0,(version,name,proc.stderr)
            row=json.loads(proc.stdout);row.update(version=version,round=run);rows.append(row)
            assert all(x['accepted'] for x in row['runs']),(version,name,row)
summary=[]
for name,size in cases:
    d={'fixture':name,'size':size}
    for v in [previous,f'iteration-{iteration}']:
        vals=[x for r in rows if r['version']==v and r['fixture']==name and r['size']==size for x in r['runs'] if x['run']>0]
        d[v]={'ms':1000*statistics.median(x['seconds'] for x in vals),'nodes':statistics.median(x['nodes'] for x in vals),'iterations':statistics.median(x['iterations'] for x in vals)}
    d['speedup']=d[previous]['ms']/d[f'iteration-{iteration}']['ms'];summary.append(d)
(b/f'iteration-{iteration}-bench.json').write_text(json.dumps({'summary':summary,'records':rows},indent=2))
for d in summary: print(d,flush=True)

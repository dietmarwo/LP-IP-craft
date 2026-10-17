import json,subprocess
from pathlib import Path
b=Path(__file__).parent
rows=[]
for label,binary in [('baseline','baseline'),('highs','highs'),('native','iteration-10')]:
 p=subprocess.run([str(b/'binaries'/binary),'irregular','4096','warm','1','1'],capture_output=True,text=True,timeout=100)
 r=json.loads(p.stdout) if p.returncode==0 else {'process_error':p.stderr,'returncode':p.returncode}
 r['backend']=label;rows.append(r)
 (b/'large-limit-probe.json').write_text(json.dumps({'protocol':'One warmup and one measured solve per backend; exploratory, not a five-sample median. Same 30 s limit.','records':rows},indent=2))
 print(label,[(s['seconds'],s['status'],s['accepted']) for s in r.get('runs',[])],flush=True)

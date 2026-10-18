"""Original plain-LP references for root-gap tables, outside production timing."""
import argparse,json,sys,subprocess,math
from pathlib import Path
import lp_ip_craft as lp
from run import DEV,ROOT,BASE
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--seconds',type=float,default=10);a=p.parse_args();out=[]
for name in DEV:
 path=next(ROOT.glob('docs/*/corpus/**/'+name+'.json'));payload=json.loads(path.read_text());spec=payload['model'];relaxed=dict(spec);relaxed['kinds']=['continuous']*len(spec['objective'])
 temp=a.output.parent/(name+'-lp-input.json');temp.parent.mkdir(parents=True,exist_ok=True);temp.write_text(json.dumps(dict(model=relaxed))+'\n')
 result=subprocess.run([sys.executable,str(BASE/'worker.py'),str(temp),str(a.seconds),'highs'],capture_output=True,text=True,check=True)
 r=json.loads(result.stdout.splitlines()[-1]);point=r['result']['candidate'];clean=dict(relaxed)
 for k in ['lower','upper','row_lower','row_upper']:clean[k]=[(-math.inf if k in ['lower','row_lower']else math.inf)if v is None else v for v in clean[k]]
 if point is not None:assert lp.evaluate(lp.Model(**clean),point).feasible_candidate
 r['case']=name;out.append(r)
a.output.write_text(json.dumps(dict(purpose='untimed original LP relaxation reference',records=out),indent=2)+'\n')

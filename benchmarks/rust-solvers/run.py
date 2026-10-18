"""Serial fresh-process comparisons with common-array and original-model checks."""
import argparse,hashlib,json,math,os,subprocess,sys
from pathlib import Path
import numpy as np
import lp_ip_craft as lp
BASE=Path(__file__).resolve().parent;ROOT=BASE.parent.parent
DEV=['avgas','bell5','egout','flugpl','gt2','lseu','p0548','p01','rgn']

def check(spec,result):
    issues=[];point=result.get('candidate');value=None
    if point is not None and len(point)==len(spec['objective']):
        clean=dict(spec)
        for key in ['lower','upper','row_lower','row_upper']:
            clean[key]=[(-math.inf if key in ['lower','row_lower']else math.inf)if v is None else v for v in spec[key]]
        e=lp.evaluate(lp.Model(**clean),point);value=e.objective
        if not e.feasible_candidate:issues.append('original candidate rejected')
        if result.get('objective')is not None and abs(value-result['objective'])>1e-6*(1+abs(value)):issues.append('objective mismatch')
    if result.get('status','').lower()=='optimal' and value is None:issues.append('optimal without complete candidate')
    return value,issues

def run_one(solver,path,seconds,policy,binary,baseline=None,off=None):
    payload=json.loads(path.read_text());spec=payload['model'];env=os.environ.copy();env.pop('RS_OFF',None)
    if off:env['RS_OFF']=off
    if solver=='baseline':
        env['PYTHONPATH']=str(baseline)
        command=[sys.executable,str(BASE/'worker.py'),str(path),str(seconds),'baseline']
    elif solver=='highs':command=[sys.executable,str(BASE/'worker.py'),str(path),str(seconds),'highs']
    else:command=[str(binary),solver,str(path),str(seconds),policy]
    try:
        r=subprocess.run(command,env=env,capture_output=True,text=True,timeout=seconds+30)
        if r.returncode:return dict(solver=solver,policy=policy,case=path.stem,status='process_error',error=r.stderr[-3000:])
        record=json.loads(r.stdout.splitlines()[-1])
    except subprocess.TimeoutExpired:return dict(solver=solver,policy=policy,case=path.stem,status='watchdog',seconds=seconds+30)
    result=record['result'];objective,issues=check(spec,result);expected=payload.get('expected');sense=spec['sense'];status=result['status'].lower().replace(' ','_')
    if expected is not None:
        b=result.get('bound')
        if b is not None and (b>expected+1e-6*(1+abs(expected)) if sense=='minimize' else b<expected-1e-6*(1+abs(expected))):issues.append('bound on wrong side')
        if status=='optimal'and(objective is None or abs(objective-expected)>1e-6*(1+abs(expected))):issues.append('wrong optimality')
    record.update(case=path.stem,group='heldout'if path.parent.name=='heldout'else'development',input_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),model_sha256=hashlib.sha256(json.dumps(spec,sort_keys=True,separators=(',',':')).encode()).hexdigest(),issues=issues,checked_objective=objective,reference=expected,rs_off=off)
    return record

def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--baseline-python',type=Path);p.add_argument('--seconds',type=float,default=10);p.add_argument('--output',type=Path,required=True);p.add_argument('--heldout',action='store_true');p.add_argument('--families',choices=['development','heldout']);p.add_argument('--cases',nargs='+');p.add_argument('--policies',nargs='+',default=['default','presolve','gmi','mir','combined','heuristics','branch-product']);p.add_argument('--solvers',nargs='+',default=['baseline','native','ripsolve','highs']);p.add_argument('--resume',action='store_true');a=p.parse_args()
    paths=sorted((BASE/'heldout').glob('*.json'))if a.heldout else[next(ROOT.glob('docs/*/corpus/**/'+name+'.json'))for name in(a.cases or DEV)]
    if a.families:paths=sorted((BASE/'families').glob(a.families+'-*.json'))
    paths=[p for p in paths if p.name!='manifest.json'and(not a.cases or p.stem in a.cases)]
    records=json.loads(a.output.read_text())['records']if a.resume and a.output.exists()else[]
    completed={(r['case'],r['solver'],r.get('policy','default'))for r in records}
    for path in paths:
        for solver in a.solvers:
            policies=a.policies if solver=='native'else(['serial']if solver=='ripsolve'and a.policies==['serial']else['default'])
            for policy in policies:
                if(path.stem,solver,policy)in completed:continue
                record=run_one(solver,path,a.seconds,policy,a.binary,a.baseline_python);records.append(record)
                a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps(dict(protocol='fresh processes, identical frozen arrays, serial runs after builds; native/HiGHS one thread; ripsolve threads=1 may add a CDCL thread unless policy=serial',time_limit=a.seconds,binary_sha256=hashlib.sha256(a.binary.read_bytes()).hexdigest(),records=records),indent=2,allow_nan=False)+'\n')
                r=record.get('result',record);print(path.stem,solver,policy,r.get('status'),r.get('objective'),round(record.get('seconds',0),3),record.get('issues'),flush=True)
if __name__=='__main__':main()

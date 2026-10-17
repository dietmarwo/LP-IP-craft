"""Convert a deliberately small, explicit subset of standard free/fixed MPS.

Unsupported sections/bound types fail closed. Independent SciPy LP/MIP solves
provide objectives; original MPS is retained alongside SHA256 provenance.
"""
import argparse,hashlib,json,math,shutil
from pathlib import Path
import numpy as np
from scipy.optimize import linprog,milp,Bounds,LinearConstraint
from scipy.sparse import csr_matrix

def parse(path):
    section=None;row_types={};obj=None;cols={};cost={};rhs={};ranges={};bounds={};integer=set();marker_integer=set();marked=False
    def number(v):return float(v.replace('D','E'))
    for line in path.read_text().splitlines():
        if not line.strip() or line.startswith('*'):continue
        t=line.split()
        if not line[0].isspace():
            section=t[0]
            if section in ['NAME','ENDATA']:continue
            assert section in ['ROWS','COLUMNS','RHS','RANGES','BOUNDS'],(path,section)
            continue
        if section=='ROWS':
            kind,name=t;assert kind in ['N','E','L','G']
            if kind=='N':assert obj is None;obj=name
            else:row_types[name]=kind
        elif section=='COLUMNS':
            if "'MARKER'" in t:
                marked="'INTORG'" in t;continue
            name=t[0];cols.setdefault(name,{})
            if marked:integer.add(name);marker_integer.add(name)
            assert len(t) in [3,5],t
            for i in range(1,len(t),2):
                r,v=t[i],number(t[i+1])
                if r==obj:cost[name]=cost.get(name,0.)+v
                else:assert r in row_types;cols[name][r]=cols[name].get(r,0.)+v
        elif section in ['RHS','RANGES']:
            target=rhs if section=='RHS' else ranges
            assert len(t) in [3,5],t
            for i in range(1,len(t),2):target[t[i]]=number(t[i+1])
        elif section=='BOUNDS':
            kind,_,name=t[:3];v=number(t[3]) if len(t)>3 else 0.
            lo,hi=bounds.get(name,(0.,math.inf))
            if kind in ['LO','LI']:lo=v
            elif kind in ['UP','UI']:hi=v
            elif kind=='FX':lo=hi=v
            elif kind=='FR':lo,hi=-math.inf,math.inf
            elif kind=='MI':lo=-math.inf
            elif kind=='PL':hi=math.inf
            elif kind=='BV':lo,hi=0.,1.
            else:raise ValueError((path,'unsupported bound',kind))
            if kind in ['LI','UI','BV']:integer.add(name)
            bounds[name]=(lo,hi)
    for name in marker_integer:
        bounds.setdefault(name,(0.,1.))
    names=list(cols);rn=list(row_types);rmap={r:i for i,r in enumerate(rn)}
    rows=[{} for _ in rn]
    for j,n in enumerate(names):
        for r,v in cols[n].items():rows[rmap[r]][j]=v
    rl=[];ru=[]
    for r in rn:
        v=rhs.get(r,0.);k=row_types[r]
        l,u=(v,v) if k=='E' else (-math.inf,v) if k=='L' else (v,math.inf)
        if r in ranges:
            width=ranges[r]
            if k=='L' or (k=='E' and width<0):l=u-abs(width)
            else:u=l+abs(width)
        rl.append(l);ru.append(u)
    ptr=[0];ix=[];values=[]
    for row in rows:
        for j,v in sorted(row.items()):
            if v:ix.append(j);values.append(v)
        ptr.append(len(ix))
    return dict(sense='minimize',objective=[cost.get(n,0.) for n in names],objective_offset=-rhs.get(obj,0.),lower=[bounds.get(n,(0.,math.inf))[0] for n in names],upper=[bounds.get(n,(0.,math.inf))[1] for n in names],row_lower=rl,row_upper=ru,kinds=['integer' if n in integer else 'continuous' for n in names],indptr=ptr,indices=ix,values=values)

def main():
    p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
    records=[]
    for name in ['afiro','adlittle','israel','e226','etamacro','25fv47','egout','avgas','flugpl','bell5']:
        src=a.source/(name+'.mps');m=parse(src);dest=a.output/src.name;shutil.copyfile(src,dest)
        A=csr_matrix((m['values'],m['indices'],m['indptr']),shape=(len(m['row_lower']),len(m['objective'])))
        integral=np.array([k!='continuous' for k in m['kinds']],int)
        if any(integral):r=milp(m['objective'],integrality=integral,bounds=Bounds(m['lower'],m['upper']),constraints=LinearConstraint(A,m['row_lower'],m['row_upper']),options={'time_limit':30.})
        else:
            rl=np.array(m['row_lower']);ru=np.array(m['row_upper']);eq=rl==ru
            upper=np.isfinite(ru)&~eq;lower=np.isfinite(rl)&~eq
            from scipy.sparse import vstack
            r=linprog(m['objective'],A_ub=vstack([A[upper],-A[lower]]),b_ub=np.r_[ru[upper],-rl[lower]],A_eq=A[eq],b_eq=rl[eq],bounds=list(zip(m['lower'],m['upper'])),method='highs',options={'time_limit':30.})
        assert r.success,(name,r.message)
        reference=float(r.fun+m['objective_offset'])
        # JSON null encodes the appropriate infinite bound, never a new finite bound.
        clean={k:[v if not isinstance(v,float) or math.isfinite(v) else None for v in x] if isinstance(x,list) else x for k,x in m.items()}
        payload=dict(model=clean,expected=reference,provenance=dict(name=name,source='HiGHS/check/instances/'+src.name,source_sha256=hashlib.sha256(src.read_bytes()).hexdigest(),transform='MPS to identical CSR, no finite-bound substitution',oracle='SciPy '+__import__('scipy').__version__+' HiGHS; separate untimed solve',oracle_status=int(r.status)))
        output=a.output/(name+'.json');output.write_text(json.dumps(payload,indent=2)+'\n')
        records.append(dict(n=len(m['objective']),m=len(m['row_lower']),nnz=len(m['values']),mip=bool(any(integral)),requires_infinite_bounds=bool(any(integral)) and any(not math.isfinite(v) for side in ['lower','upper'] for v in m[side]),expected=reference,json_sha256=hashlib.sha256(output.read_bytes()).hexdigest(),**payload['provenance']))
        print(name,records[-1],flush=True)
    (a.output/'manifest.json').write_text(json.dumps(records,indent=2)+'\n')
if __name__=='__main__':main()

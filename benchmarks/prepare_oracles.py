"""Freeze the checked-in random/exact/component test generators before timing."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import numpy as np

p=argparse.ArgumentParser();p.add_argument('repo');p.add_argument('output');a=p.parse_args()
repo=Path(a.repo);sys.path.insert(0,str(repo/'examples'))
import validate_native as exact
import validate_random as random

records=[]
def specification(matrix,c,lower,upper,rl,ru,integer,sense='minimize',offset=0.):
    ptr=[0];indices=[];values=[]
    for row in matrix:
        nz=np.flatnonzero(row)
        indices.extend(int(j) for j in nz);values.extend(float(row[j]) for j in nz);ptr.append(len(indices))
    return dict(objective=np.asarray(c).tolist(),objective_offset=offset,sense=sense,
                lower=np.asarray(lower).tolist(),upper=np.asarray(upper).tolist(),
                row_lower=np.asarray(rl).tolist(),row_upper=np.asarray(ru).tolist(),
                kinds=['integer' if k else 'continuous' for k in integer],
                indptr=ptr,indices=indices,values=values)

for seed in range(1,10):
    rng=np.random.default_rng(seed)
    for mip,count in [(False,400),(True,150)]:
        for case in range(count):
            data=random.draw(rng,mip)
            status,reference=random.oracle(*data)
            records.append(dict(case_id=f'random-{seed}-{"mip" if mip else "lp"}-{case}',suite='random',seed=seed,
                                model=specification(*data),expected_status=status,reference=reference,oracle='checked-in SciPy feasibility-first LP/MIP oracle'))
    print('prepared random seed',seed,flush=True)

for seed in [59103,91027]:
    rng=np.random.default_rng(seed)
    for domain in ['lp','ip','mip']:
        for case in range(200):
            n=5;rows=4
            matrix=rng.integers(-6,7,(rows,n)).astype(float);c=rng.integers(-8,9,n).astype(float)
            lower=np.zeros(n);upper=np.full(n,3.);witness=rng.integers(0,4,n).astype(float)
            activity=matrix@witness;rl=activity-rng.integers(0,5,rows);ru=activity+rng.integers(0,5,rows)
            integer=np.array([True]*5 if domain=='ip' else [True,False,True,False,True] if domain=='mip' else [False]*5)
            if domain=='ip':reference=exact.exact_box_ip(c,matrix,rl,ru)
            elif domain=='mip':reference=exact.exact_mixed(c,matrix,rl,ru)
            else:
                result=random.linprog(c,A_ub=np.vstack([matrix,-matrix]),b_ub=np.r_[ru,-rl],bounds=[(0,3)]*n,method='highs')
                assert result.success;reference=result.fun
            records.append(dict(case_id=f'exact-{seed}-{domain}-{case}',suite='exact',domain=domain,seed=seed,
                                model=specification(matrix,c,lower,upper,rl,ru,integer),expected_status='optimal',reference=float(reference),
                                oracle='SciPy LP' if domain=='lp' else 'exhaustive integer enumeration' if domain=='ip' else 'exact rational vertex enumeration'))
    print('prepared exact seed',seed,flush=True)

rng=np.random.default_rng(946237)
for domain in ['lp','ip','mip']:
    for case in range(60):
        matrices=[];costs=[];lowers=[];uppers=[];reference=1.25
        for _ in range(2):
            matrix=rng.integers(-6,7,(4,5)).astype(float);c=rng.integers(-8,9,5).astype(float);w=rng.integers(0,4,5)
            activity=matrix@w;lo=activity-rng.integers(0,5,4);up=activity+rng.integers(0,5,4)
            if domain=='ip':value=exact.exact_box_ip(c,matrix,lo,up)
            elif domain=='mip':value=exact.exact_mixed(c,matrix,lo,up)
            else:
                result=random.linprog(c,A_ub=np.vstack([matrix,-matrix]),b_ub=np.r_[up,-lo],bounds=[(0,3)]*5,method='highs');assert result.success;value=result.fun
            reference+=float(value);matrices.append(matrix);costs.extend(c);lowers.extend(lo);uppers.extend(up)
        matrix=np.zeros((8,10));matrix[:4,:5]=matrices[0];matrix[4:,5:]=matrices[1]
        integer=np.array(([False]*5 if domain=='lp' else [True]*5 if domain=='ip' else [True,False,True,False,True])*2)
        records.append(dict(case_id=f'component-{domain}-{case}',suite='components',domain=domain,seed=946237,
                            model=specification(matrix,costs,[0.]*10,[3.]*10,lowers,uppers,integer,offset=1.25),expected_status='optimal',reference=reference,oracle='sum of two independently checked component optima'))

payload=dict(records=records,numpy=np.__version__,generators={name:hashlib.sha256((repo/'examples'/name).read_bytes()).hexdigest() for name in ['validate_native.py','validate_random.py','validate_components.py']})
output=Path(a.output);output.write_text(json.dumps(payload,separators=(',',':'))+'\n')
print('wrote',len(records),'models',output,flush=True)

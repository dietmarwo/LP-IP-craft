import hashlib,json,random
from pathlib import Path
base=Path(__file__).resolve().parent/'families';base.mkdir(exist_ok=True)
records=[]
for group,seeds in [('development',range(10,15)),('heldout',range(110,115))]:
 for seed in seeds:
  for family in ['bigm','knapsack']:
   rng=random.Random(seed);n=12;A=[];lower=[0.]*n;upper=[1.]*n;kinds=['binary']*n
   if family=='bigm':
    kinds=['continuous']*6+['binary']*6;upper=[10.]*6+[1.]*6
    for j in range(6):row=[0.]*n;row[j]=1.;row[j+6]=-float(rng.randint(10,100));A.append(row)
    for _ in range(3):row=[float(rng.randint(1,5))for _ in range(6)]+[0.]*6;A.append([-v for v in row])
    ru=[0.]*6+[-float(rng.randint(5,20))for _ in range(3)];c=[float(rng.randint(1,3))for _ in range(6)]+[float(rng.randint(3,10))for _ in range(6)];sense='minimize'
   else:
    A=[[float(rng.randint(1,20))for _ in range(n)]for _ in range(3)];ru=[sum(row)*.4 for row in A];c=[float(rng.randint(1,30))for _ in range(n)];sense='maximize'
   indptr=[0];indices=[];values=[]
   for row in A:
    for j,v in enumerate(row):
     if v:indices.append(j);values.append(v)
    indptr.append(len(indices))
   payload=dict(model=dict(objective=c,objective_offset=0.,lower=lower,upper=upper,kinds=kinds,sense=sense,row_lower=[None]*len(A),row_upper=ru,indptr=indptr,indices=indices,values=values),expected=None,provenance=dict(family=family,seed=seed,group=group))
   path=base/f'{group}-{family}-{seed}.json';path.write_text(json.dumps(payload,indent=2)+'\n');records.append(dict(file=path.name,group=group,sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
(base/'manifest.json').write_text(json.dumps(records,indent=2)+'\n')

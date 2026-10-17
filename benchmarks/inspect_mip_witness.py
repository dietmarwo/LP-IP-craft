import json
from pathlib import Path
from fractions import Fraction as F
import lp_ip_craft as lp
root=Path(__file__).resolve().parent.parent;out=[]
for name in ['flugpl','egout']:
 p=next(root.glob('docs/*/corpus/**/'+name+'.json'));d=json.loads(p.read_text())['model']
 for key,sign in [('lower',-1),('upper',1),('row_lower',-1),('row_upper',1)]:d[key]=[v if v is not None else sign*float('inf') for v in d[key]]
 m=lp.Model(**d);r=lp.solve(m,backend='native',backend_options=lp.NativeOptions(decompose=False,time_limit_seconds=10.));x=r.candidate;z=list(map(F,x));free=[]
 for j,v in enumerate(x):
  l,u=d['lower'][j],d['upper'][j]
  if d['kinds'][j]!='continuous':z[j]=F(round(v))
  elif abs(l)!=float('inf') and abs(v-l)<=1e-8*(1+abs(l)):z[j]=F(l)
  elif abs(u)!=float('inf') and abs(v-u)<=1e-8*(1+abs(u)):z[j]=F(u)
  else:free.append(j)
 print(name,'free',len(free),free,flush=True)
 n=len(free);pos={j:p for p,j in enumerate(free)};basis={};eq=[]
 for i in range(len(d['row_lower'])):
  rr=range(d['indptr'][i],d['indptr'][i+1]);act=sum(d['values'][k]*x[d['indices'][k]]for k in rr);l,u=d['row_lower'][i],d['row_upper'][i]
  side=l if l==u or abs(act-l)<=1e-6*(1+abs(act)) else u if abs(act-u)<=1e-6*(1+abs(act))else None
  if side is not None:eq.append((l!=u,i,side))
 for _,i,b in sorted(eq):
  if len(basis)==n:break
  row=[F(0)for _ in range(n)]+[F(b)]
  for k in range(d['indptr'][i],d['indptr'][i+1]):
   j=d['indices'][k];c=F(d['values'][k])
   if j in pos:row[pos[j]]=c
   else:row[-1]-=c*z[j]
  for p in range(n):
   if not row[p]:continue
   if p in basis:
    v=row[p];row=[a-v*b for a,b in zip(row,basis[p])]
   else:
    v=row[p];basis[p]=[a/v for a in row];break
 for p in reversed(range(n)):
  if p in basis:z[free[p]]=basis[p][-1]-sum(basis[p][k]*z[free[k]]for k in range(p+1,n))
 failures=[]
 for j,v in enumerate(z):
  if (d['lower'][j]!=-float('inf')and v<F(d['lower'][j]))or(d['upper'][j]!=float('inf')and v>F(d['upper'][j])):failures.append(('bound',j,float(v),d['lower'][j],d['upper'][j]))
 for i in range(len(d['row_lower'])):
  act=sum(F(d['values'][k])*z[d['indices'][k]]for k in range(d['indptr'][i],d['indptr'][i+1]));l,u=d['row_lower'][i],d['row_upper'][i]
  if(l!=-float('inf')and act<F(l))or(u!=float('inf')and act>F(u)):failures.append(('row',i,float(act),l,u,float(min(abs(act-F(b))for b in [l,u]if abs(b)!=float('inf')))))
 print(name,'rank',len(basis),'failures',failures[:12],flush=True);out.append(dict(name=name,free=free,failures=failures,candidate=x))
(root/'docs/mip-search-024/data/witness-inspection.json').write_text(json.dumps(out,indent=2))

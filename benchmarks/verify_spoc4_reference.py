"""Verify the prior reference bound from its saved dyadic prices, without a solver."""
import json
from pathlib import Path
import numpy as np
from spoc4_matching import load
root=Path(__file__).resolve().parent
reference=root.parent/'docs/spoc4-025/data/reference-ipm'
r=json.loads((reference/'i-highs-lp-reference.json').read_text())
_,w,A,_=load(root/'spoc4-025/data/matching-i.txt')
y=np.load(reference/'i-highs-lp-reference.dual.npy')
assert len(y)==A.shape[0]and y.dtype==np.int64 and np.all(y>=0)
cert=r['exact_packing_upper_bound'];denom=cert['denominator']
a=np.zeros(len(w),dtype=np.int64)
np.add.at(a,A.indices,np.repeat(y,np.diff(A.indptr)))
numerator=sum(map(int,y))+sum(map(int,np.maximum(w*denom-a,0)))
assert numerator==cert['numerator']
assert numerator//denom==cert['objective_upper_milli']==34120529
print('Exact dyadic price bound verified: integer optimum <= 34120.529')

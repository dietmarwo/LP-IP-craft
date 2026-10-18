"""Freeze unseen parser-supported small check-instance MIPs before policy runs."""
import argparse,hashlib,json,math,sys
from pathlib import Path
import shutil,subprocess
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from prepare_public import parse
p=argparse.ArgumentParser();p.add_argument('--highs',type=Path,required=True);a=p.parse_args()
base=Path(__file__).resolve().parent
existing={p.stem for p in (base.parent.parent/'docs').glob('*/corpus/**/*.json')}
records=[];excluded=[]
for source in sorted((a.highs/'check/instances').glob('*.mps')):
 if source.stem in existing:continue
 try:m=parse(source)
 except Exception as e:excluded.append(dict(name=source.name,reason='parser: '+str(e)));continue
 if not any(k!='continuous'for k in m['kinds']):continue
 if len(m['objective'])>1000 or len(m['row_lower'])>1000:excluded.append(dict(name=source.name,reason='predeclared dimension cap'));continue
 if len(records)>=20:excluded.append(dict(name=source.name,reason='after first twenty eligible names'));continue
 destination=base/'heldout';destination.mkdir(parents=True,exist_ok=True);shutil.copyfile(source,destination/source.name)
 clean={k:[v if not isinstance(v,float)or math.isfinite(v)else None for v in x]if isinstance(x,list)else x for k,x in m.items()}
 payload=dict(model=clean,expected=None,provenance=dict(source='HiGHS/check/instances/'+source.name,sha256=hashlib.sha256(source.read_bytes()).hexdigest(),native_bounds_infinite=any(v is None for side in ['lower','upper']for v in clean[side])))
 path=destination/(source.stem+'.json');path.write_text(json.dumps(payload,indent=2)+'\n')
 records.append(dict(name=source.stem,n=len(m['objective']),m=len(m['row_lower']),input_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),**payload['provenance']))
manifest=dict(highs_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=a.highs,text=True).strip(),selection='first twenty sorted unseen, parser-supported MIPs with n,m<=1000; no performance filtering',instances=records,excluded=excluded)
(base/'heldout/manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('heldout',len(records),[(r['name'],r['n'],r['m'])for r in records])

"""Build a 0.19/0.20 baseline and old HiGHS against the current coverage harness.

Supply separate source checkouts; no production source is modified. To obtain
HiGHS sources, first run reference/bootstrap_sources.py. The 0.19 source is
available from the previous ZIP/history, or checkout the recorded baseline SHA.
"""
import argparse,hashlib,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--baseline-repo',type=Path,required=True);p.add_argument('--highs-repo',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--only-highs',action='store_true');a=p.parse_args()
base=Path(__file__).resolve().parent;a.output.mkdir(parents=True,exist_ok=True)
baseline19 = 'version = "0.19.0"' in (a.baseline_repo/'Cargo.toml').read_text()
baseline22 = 'version = "0.22.0"' in (a.baseline_repo/'Cargo.toml').read_text()
assert baseline19 or any(f'version = "{v}"' in (a.baseline_repo/'Cargo.toml').read_text() for v in ["0.20.0","0.21.0","0.22.0"])
root=a.output.resolve();(root/'shared').mkdir(exist_ok=True)
hashes={}
for f in ['main.rs','fixtures.rs','extended.rs','imports.rs']:
    data=(base/'shared'/f).read_text();(root/'shared'/f).write_text(data);hashes[f]=hashlib.sha256(data.encode()).hexdigest()
# The 0.19 API lacks these two controls. For 0.20 preserve them, including
# its default enabled certified fixing and disabled optional root probes.
main=(root/'shared/main.rs').read_text()
(root/'shared/baseline-main.rs').write_text('\n'.join(l for l in main.splitlines() if not baseline19 or ('strong_branching:' not in l and 'reduced_cost_fixing:' not in l))+'\n')
(root/'Cargo.toml').write_text('[workspace]\nmembers=["baseline","highs"]\nresolver="3"\n[profile.release]\ncodegen-units=1\n')
for label,repo,backend,entry in [('baseline',a.baseline_repo,'native','baseline-main.rs'),('highs',a.highs_repo,'highs','main.rs')]:
    dest=root/label;dest.mkdir(exist_ok=True)
    (dest/'Cargo.toml').write_text(f'''[package]
name="packing-{label}"
version="0.1.0"
edition="2024"
[features]
default=["backend_{backend}"{',"evolved"' if baseline22 and label == 'baseline' else ''}]
backend_native=[]
backend_highs=[]
kernel-profile=[]
evolved=[]
search-diagnostics=[]
[dependencies]
lp-ip-craft={{path={json.dumps(str(repo.resolve()/'crates/core'))}}}
lp-ip-craft-{backend}={{path={json.dumps(str(repo.resolve()/('crates/'+backend)))}}}
serde_json="1"
[[bin]]
name="packing-{label}"
path="../shared/{entry}"
''')
for label in ['baseline','highs']:
    if a.only_highs and label != 'highs':continue
    subprocess.run(['cargo','build','--release','--manifest-path',str(root/'Cargo.toml'),'-p','packing-'+label],check=True)
(root/'harness-sha256.json').write_text(json.dumps(hashes,indent=2)+'\n')

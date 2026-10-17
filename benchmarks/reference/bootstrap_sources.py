"""Restore pinned public sources and the delivered 0.4 adapter patch."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

base=Path(__file__).resolve().parent
p=argparse.ArgumentParser()
p.add_argument('--repository',default='https://github.com/dietmarwo/LP-IP-craft.git')
a=p.parse_args()

def git(path,*args):
    return subprocess.check_output(['git','-C',str(path),*args],text=True).strip()

native=base/'native-0.16.0'
old=base/'highs-0.4.0/LP-IP-craft'
for path,commit in [(native,'76ca1c9a3c22f0076f282826bb1c1c6a3d5692a5'),
                    (old,'a7a04074747a99048810204efe0421b4093c9fb4')]:
    if not path.exists():
        path.parent.mkdir(parents=True,exist_ok=True)
        subprocess.run(['git','clone','--quiet','--no-checkout',a.repository,str(path)],check=True)
        subprocess.run(['git','-C',str(path),'checkout','--quiet','--detach',commit],check=True)
    if git(path,'rev-parse','HEAD')!=commit:
        raise RuntimeError(f'{path}: existing checkout is not the pinned commit')

patch=base/'highs-adapter-0.4.0.patch'
expected_patch_hash=hashlib.sha256(patch.read_bytes()).hexdigest()
marker=base/'highs-0.4.0/.comparison-patch-sha256'
if not marker.exists():
    subprocess.run(['git','-C',str(old),'apply','--check',str(patch)],check=True)
    subprocess.run(['git','-C',str(old),'apply',str(patch)],check=True)
    marker.write_text(expected_patch_hash+'\n')
elif marker.read_text().strip()!=expected_patch_hash:
    raise RuntimeError('Existing baseline uses a different patch')
hashes=json.loads((base/'highs-source-sha256.json').read_text())
for relative,expected in hashes.items():
    path=old/relative
    if hashlib.sha256(path.read_bytes()).hexdigest()!=expected:
        raise RuntimeError(f'Baseline source checksum mismatch: {relative}')
print('Pinned native source and all',len(hashes),'delivered baseline code hashes verified.')

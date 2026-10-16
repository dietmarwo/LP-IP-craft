"""Compare already-built iteration benchmark executables without compilation.

python scripts/compare_native_iterations.py --binaries /path/bench-1 ... /path/bench-5
Each binary is the release `crates/native/examples/benchmark.rs` from its
iteration checkpoint. No process runs in parallel with another benchmark.
"""
import argparse
import hashlib
import json
import platform
import statistics
import subprocess
from pathlib import Path

def main():
    p=argparse.ArgumentParser()
    p.add_argument("--binaries",nargs=5,required=True)
    p.add_argument("--rounds",type=int,default=7)
    p.add_argument("--output",default="docs/native/comparison.json")
    args=p.parse_args()
    records=[];objectives={}
    for round_number in range(args.rounds):
        order=list(range(5));shift=round_number%5
        order=order[shift:]+order[:shift]
        if round_number%2:order.reverse()
        for i in order:
            batch=json.loads(subprocess.check_output([args.binaries[i],"3"],text=True))
            for row in batch:
                name=row["fixture"]
                if name in objectives:assert abs(row["objective"]-objectives[name])<1e-7
                else:objectives[name]=row["objective"]
                records.append({"iteration":i+1,"round":round_number+1,**row})
    hashes=[hashlib.sha256(Path(b).read_bytes()).hexdigest() for b in args.binaries]
    result={"platform":platform.platform(),"rounds":args.rounds,
            "runs_per_fixture_and_iteration":args.rounds*3,
            "binary_sha256":hashes,"records":records}
    Path(args.output).write_text(json.dumps(result,indent=2)+"\n")
    for name in objectives:
        values=[statistics.median(row["seconds"] for row in records if row["fixture"]==name and row["iteration"]==i) for i in range(1,6)]
        print(name, [round(v*1000,4) for v in values],"ms",round(values[0]/values[-1],2),"x baseline/final")

if __name__=="__main__":main()

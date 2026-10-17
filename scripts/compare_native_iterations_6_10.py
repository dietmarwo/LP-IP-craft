"""Compare already-built versions 5–10, then final-version worker caps.

No builds or solver processes run concurrently with another measurement.
Version 5 uses the extended fixture source introduced in iteration 6, with
its numerical code unchanged. See docs/native/performance-6-10.md.
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
    p.add_argument("--binaries",nargs=6,required=True)
    p.add_argument("--rounds",type=int,default=7)
    p.add_argument("--output",default="docs/native/comparison-6-10.json")
    args=p.parse_args();records=[];reference={}
    for number in range(args.rounds):
        order=list(range(6));shift=number%6;order=order[shift:]+order[:shift]
        if number%2:order.reverse()
        for i in order:
            rows=json.loads(subprocess.check_output([args.binaries[i],"3"],text=True))
            for row in rows:
                name=row["fixture"]
                if name in reference:assert abs(row["objective"]-reference[name])<1e-7
                else:reference[name]=row["objective"]
                records.append({"iteration":i+5,"round":number+1,**row})
    parallel=[]
    for number in range(args.rounds):
        for workers in ([1,16] if number%2==0 else [16,1]):
            rows=json.loads(subprocess.check_output([args.binaries[-1],"3",str(workers)],text=True))
            for row in rows:
                assert abs(row["objective"]-reference[row["fixture"]])<1e-7
                parallel.append({"worker_cap":workers,"round":number+1,**row})
    data={"platform":platform.platform(),"rounds":args.rounds,"measured_runs_per_fixture":args.rounds*3,
          "binary_sha256":[hashlib.sha256(Path(b).read_bytes()).hexdigest() for b in args.binaries],
          "records":records,"parallel_comparison":parallel}
    Path(args.output).write_text(json.dumps(data,indent=2)+"\n")
    for name in reference:
        medians=[statistics.median(r["seconds"] for r in records if r["fixture"]==name and r["iteration"]==i) for i in range(5,11)]
        print(name,[round(v*1000,4) for v in medians],"ms",round(medians[0]/medians[-1],2),"x version 5/final")
    for workers in [1,16]:
        rows=[r for r in parallel if r["fixture"]=="blocks" and r["worker_cap"]==workers]
        print("final blocks",workers,"worker cap",statistics.median(r["seconds"] for r in rows)*1000,"ms","effective",rows[0]["workers"])

if __name__=="__main__":main()

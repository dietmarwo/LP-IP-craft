"""Render the retained 0.23 observations; never fabricates missing timings."""
import json
from pathlib import Path

root = Path(__file__).resolve().parent.parent
folder = root / "docs/packing-ftran-mip-diagnostics"
data = folder / "data"


def read(name):
    return json.loads((data / name).read_text())


def num(value, places=3):
    return "—" if value is None else f"{value:,.{places}f}"


def key(fixture):
    return Path(fixture[5:]).stem if fixture.startswith("json=") else fixture


def table(headers, rows):
    return "| " + " | ".join(headers) + " |\n|" + "|".join(["---"]*len(headers)) + "|\n" + "\n".join("| " + " | ".join(map(str, row)) + " |" for row in rows) + "\n"


audit = read("audit.json")
text = ["# LP-IP-craft 0.23 — executed packing/FTRAN and MIP diagnosis\n",
        "Only continuation items 1 and 2 were implemented. Production remains owned safe Rust, with no external numerical engine. The accepted 0.22 source is the baseline. Search algorithms, defaults, tolerances, frozen adjudications and budgets are preserved; the new trace is opt-in.\n",
        "## Changes\n",
        "1. For FT forward solves without trailing conventional etas, reuse the captured output index list directly instead of copying it into another list, marking it, then filtering/exporting it again. FT-plus-eta and dense-tail fallbacks retain the old support-union path.\n"
        "2. Seed captured FT forward reach from the already recorded spike. This removes duplicate dense RHS discovery without changing seed order, traversal thresholds, cooldowns or numerical arithmetic.\n"
        "3. Separate profile costs for support, symbolic reach, workspace clearing, upper arithmetic and output transfer. Normal builds compile out these kernel clocks/counters.\n"
        "4. Add bounded search observations in existing backend metadata: raw root LP objective versus conservative bound, original-model candidate acceptance versus incumbent certification, frontier progress, LP work, prune reasons and exact integer-propagation coverage. See [DIAGNOSTICS.md](DIAGNOSTICS.md).\n"
        "5. Add six original public MIPs and six independently planted fresh packing holdouts, plus fresh generated MIP seeds. Retain original MPS and conversion/reference provenance.\n",
        "No Feasibility Jump, reliability branching, new cut separator, mixed-node propagation or parallel search was added.\n",
        "## Measurement protocol\n",
        "Each ordinary comparison uses one discarded fresh-process round and three measured rounds, rotated/reversed order, one warmup per process, matched input fingerprints and independent original-model checks. One numerical thread; component controls are separately labelled. Public/default timing permits existing decomposition; the trace diagnosis explicitly disables decomposition. HiGHS is the pinned isolated 1.15.0 reference; LP controls request serial dual simplex, with normal presolve. Whole-call budgets are 30 seconds, 100000 MIP relaxations/probes and 1000000 native simplex pivots. Builds and correctness workloads are not concurrent with these measurements. This is a shared host, not exclusive hardware. Different table batches are not interchangeable ablations.\n",
        "`accepted` in the harness means independently checked optimality against the reference; a limited run may retain a feasible candidate while accepted=false. Unsupported cases are capability failures, not zero-time solutions.\n",
        "## Matched LP timing\n"]
rows = []
selection = {"extended": {("packing",1024),("packing",2048),("packing",4096),("packing-wide",256),("packing-wide",1024),("packing-permuted",1024),("irregular",1024)},
             "standard": {("network",2048)}, "aspect": {("aspect-wide",512),("aspect-tall",512)}, "large": {("irregular",4096)}}
for batch, selected in selection.items():
    summary = read(f"final-{batch}.json")["summary"]
    if batch == "large":
        summary = [dict(r, fixture="irregular", size=4096, median_ms=1000*r["median_seconds"]) for r in summary]
    for baseline in summary:
        if baseline["backend"] != "baseline" or (baseline["fixture"], baseline["size"]) not in selected:
            continue
        matches = {(r["backend"]): r for r in summary if (r["fixture"],r["size"]) == (baseline["fixture"],baseline["size"])}
        native, highs = matches["native"], matches["highs"]
        rows.append([batch, baseline["fixture"]+" "+str(baseline["size"]), num(baseline.get("median_ms")), num(native.get("median_ms")),
                     num(100*(native["median_ms"]/baseline["median_ms"]-1),1)+"%", num(highs.get("median_ms"))])
text.append(table(["Batch", "Fixture", "0.22 ms", "0.23 ms", "Time change", "HiGHS ms"], rows))
text.append("Negative time change is faster. Inspect retained ranges and all smaller cases in the raw data. Gains are workload-dependent; there are retained regressions.\n")
if (data / "final-network-repeat.json").exists():
    text.append("### Focused network repeat\n\nThe standard-suite network result warranted an additional nine-measured-round rotated comparison; the original result above is retained.\n")
    text.append(table(["Backend", "Fixture", "Median ms", "Range ms"], [[r["backend"],r["fixture"]+" "+str(r["size"]),num(r.get("median_ms"))," / ".join(num(v) for v in r.get("range_ms",[]))] for r in read("final-network-repeat.json")["summary"]]))
text.append("### Fresh packing holdouts\n")
summary = read("final-fresh-packing.json")["summary"]
rows = []
for baseline in [r for r in summary if r["backend"] == "baseline"]:
    native = next(r for r in summary if r["backend"] == "native" and r["fixture"] == baseline["fixture"])
    rows.append([key(baseline["fixture"]),num(baseline["median_ms"]),num(native["median_ms"]),num(100*(native["median_ms"]/baseline["median_ms"]-1),1)+"%"])
text.append(table(["Fresh fixture", "0.22 ms", "0.23 ms", "Time change"], rows))
if (data / "final-fresh-repeat.json").exists():
    text.append("### Fresh-holdout follow-up under default decomposition\n\nThe initial three-round results above are retained. Their regressions prompted this independent five-round rerun with the original default decomposition settings. Do not pool medians from the two batches.\n")
    summary = read("final-fresh-repeat.json")["summary"]
    rows = []
    for baseline in [r for r in summary if r["backend"] == "baseline"]:
        native = next(r for r in summary if r["backend"] == "native" and r["fixture"] == baseline["fixture"])
        rows.append([key(baseline["fixture"]),num(baseline["median_ms"]),num(native["median_ms"]),num(100*(native["median_ms"]/baseline["median_ms"]-1),1)+"%"])
    text.append(table(["Fresh fixture", "0.22 repeat ms", "0.23 repeat ms", "Time change"], rows))
ablation = read("final-ftran-ablation.json")
text.append("### Five-way FTRAN ablation and rejected third experiment\n\nFive measured rounds after a discarded round, decomposition explicitly off for every version. `export` and `seeded` are the early 0.22-based experiment binaries; candidate 2 is the accepted 0.23 numerical source. Candidate 3 additionally used a known-seed early rejection, but did not improve whole-solve times consistently and was reverted. The patch is retained in `experiments/experiment3-rejected.patch`. The degree-nine JSON model is two-component under default settings, so these monolithic times must not be compared directly to the earlier default-decomposition table.\n")
rows = []
for baseline in [r for r in ablation["summary"] if r["backend"] == "baseline"]:
    group = {r["backend"]: r for r in ablation["summary"] if (r["fixture"],r["size"]) == (baseline["fixture"],baseline["size"])}
    rows.append([key(baseline["fixture"])+(" "+str(baseline["size"]) if baseline["size"] else ""), *[num(group[label]["median_ms"]) for label in ["baseline","export","seeded","candidate2","candidate3"]]])
text.append(table(["Fixture", "Baseline", "Export", "Seeded", "Candidate 2", "Rejected 3"], rows))
text.append("### Forward-solve attribution\n\nBoth profile binaries use the detailed exclusive timing categories; these instrumented costs are diagnostic and are not the uninstrumented headline timings.\n")
profiles = [r for r in read("final-profiles.json")["summary"] if r["fixture"] == "packing" and r["size"] == 4096]
labels = {r["backend"]: r["exclusive_median_ms"] for r in profiles}
text.append(table(["Packing4096 exclusive category", "0.22 profile ms", "0.23 profile ms"],
                  [[category,num(labels["before"].get(category,0)),num(labels["after"].get(category,0))] for category in
                   ["ftran_support","ftran_support_export","reach_ft_ftran","spike_capture","ftran_ft_upper","ftran_output","ftran_workspace_clear"]]))
text.append("The accepted implementation still has full spike-capture scans, output copies/clears, dense tails and fallback loops. It does not make every forward solve hyper-sparse. Frozen generic-basis replay does not exercise every captured FT update path; dedicated update tests and the full oracle matrix cover those changes.\n")
text.append("## Public models and retained failures\n")
rows = []
for batch in ["public", "new-public"]:
    summary = read(f"final-{batch}.json")["summary"]
    for baseline in [r for r in summary if r["backend"] == "baseline"]:
        group = {r["backend"]: r for r in summary if r["fixture"] == baseline["fixture"]}
        def cell(r):
            statuses = sorted(set(r.get("statuses", [])))
            if statuses == ["error"] or not statuses:
                return "error / Unsupported"
            return num(r.get("median_ms")) + " / " + num(r.get("nodes"),0) + " / " + ",".join(statuses)
        rows.append([key(baseline["fixture"]),cell(group["baseline"]),cell(group["native"]),cell(group["highs"])])
text.append(table(["Model", "0.22 ms / nodes / status", "0.23 ms / nodes / status", "HiGHS ms / nodes / status"], rows))
text.append("## Search diagnosis\n")
text.append("The main finding is that tolerance-feasible candidates do not always become usable pruning certificates. Egout and flugpl obtain no certified incumbent bounds and do no incumbent-bound pruning in the observed runs, despite accepted candidates. This changes the next-work priority for those mixed models: investigate certification before assuming more incumbent heuristics or branching alone will close the gap. See [DIAGNOSIS.md](DIAGNOSIS.md) for evidence and limits.\n")
observations = read("mip-diagnostics.json")
records = [r for r in observations["records"] if r["round"] == 1 and r["trace_enabled"]]
rows = []
for record in records:
    run = record["result"].get("runs", [{}])[-1]
    d = run.get("search_diagnostics")
    if not d:
        rows.append([key(record["fixture"]),run.get("status","process_error"),"—","—","—","—","—"])
        continue
    first = d["first_certified_incumbent"]
    p = d["propagation"]
    rows.append([key(record["fixture"]),run["status"],num(d["root"]["lp_objective"],6),num(d["root"]["conservative_bound"],6),
                 d["root"]["fractional_integers"],"—" if first is None else str(first["nodes"]),f"{p['eligible_rows']}/{d['search_rows']}"])
text.append(table(["Model", "Status", "Root LP objective", "Root conservative bound", "Fractional integers", "First certificate: nodes", "Exact propagation rows"], rows))
text.append("These are serial, decomposition-off observations, not an attribution of HiGHS's entire advantage. Root inference and integer-lattice tightening can strengthen the conservative root bound. Original-model acceptance and certification milestones are separate. Ineligible propagation rows include mixed rows; no extra inference was added. Full event traces, terminal gaps, visit/depth/LP work, truncation and prune statistics remain in `data/mip-diagnostics.json`.\n")
text.append("### Trace overhead and work equivalence\n")
text.append(table(["Model", "Off ms", "On ms", "Overhead", "Same statuses/nodes/pivots"],
                  [[key(r["fixture"]),num(r["off"]["median_ms"]),num(r["on"]["median_ms"]),num(r.get("overhead_percent"),1)+"%",r["work_equal"]] for r in observations["summary"]]))
text.append("Overhead is measured with three fresh-process rounds after a discarded round. Tiny jobs have noisy percentages; enabled clocks, event capture and original-candidate rechecks count against wall budgets. Work equality is not expected at a wall-time cutoff.\n")
text.append("### Existing-policy probes\n")
rows = []
for record in observations["ablations"]:
    run = record["result"].get("runs", [{}])[-1]
    d = run.get("search_diagnostics", {})
    first = d.get("first_certified_incumbent")
    rows.append([key(record["fixture"]),record["mode"],run.get("status","process_error"),run.get("nodes","—"),run.get("iterations","—"),"—" if first is None else first["nodes"]])
text.append(table(["Model", "Existing control", "Status", "Nodes", "Pivots", "First certified nodes"], rows))
text.append("Each probe is one fresh process with warmup and one retained observation, not a repeated timing comparison. `presolve-off` disables several reductions and can lose finite-bound admission, so it does not isolate node propagation. These probes do not justify changing any default by themselves.\n")
text.append("## Verification\n")
text.append(f"- {audit['rust_checks']} Rust checks and {audit['python_checks']} Python tests pass; formatting and strict all-target/all-feature Clippy pass.\n"
            f"- {audit['oracle_cases']:,} frozen oracle runs pass under unchanged adjudications. The original 19 raw disputed expectations remain visible in each full policy pass.\n"
            f"- 400-model exact bound audit: {audit['unsafe_finite_bounds']} unsafe finite bounds.\n"
            f"- {audit['basis_replay_records']} frozen basis replay observations; maximum backward error {audit['max_backward_error']:.3g}. These replay generic solves on frozen bases; captured FT support changes also have dedicated update/zero/cancellation/eta-fallback tests.\n"
            "- All six added public conversions match HiGHS's own original MPS reader, independently of the solver comparison on converted CSR.\n"
            "- The diagnostics tests cover enabled/disabled work equality, original objective offsets and senses, cancellation, limits, capped traces, uncertified starts, mixed-row coverage, components and root preprocessing.\n")
text.append("## Reproduction and provenance\n\nSee [REPRODUCE.md](REPRODUCE.md), the pre-implementation [PLAN.md](PLAN.md), the build manifest, unchanged input/adjudication hashes, fixture manifests and raw logs. `audit_packing_diagnostics.py` validates retained evidence; running the audit alone is not a new solve or timing measurement. All historical 0.22 evidence remains unchanged.\n")
(folder / "RESULTS.md").write_text("\n".join(text))
print(folder / "RESULTS.md")

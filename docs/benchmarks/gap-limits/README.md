# MIP gap stop evidence

The 0.30.0 source preparation is based on LP-IP-craft `e29a4b0a688b4d59724992f969b83e0e6d0c9881` (0.29.0). No default gap tolerance is introduced. Independent candidate, bound and objective checks retain their existing tolerances.

The panel covers eight public MIP-search cases, two existing continuous packing fixtures, three LP reference cases, and three generated binary packing cases with independent HiGHS optima. Generated cases include their exact JSON and source seed; existing corpus inputs are reused by path and recorded SHA-256. All failed and time/node-limited outcomes remain in the records.

See [tables](tables.md) and `summary.json`. Gap stops return `gap_limit` with a feasible incumbent, rather than an optimality claim. At a zero objective the relative gap is infinite unless closed, so an absolute limit is needed to request a nonzero gap. Specialized packing and parallel tree controls reject positive gap tolerances; the packing models in this panel use the general search. Separable models run whole, and `effective_options` records that decision.

## Files and commands

- `results.jsonl`: full, 1e-2 and 1e-4 gap runs, 10-second solver budgets.
- `baseline-0.29.0.jsonl`: the same full configurations on the baseline.
- `baseline-fixed-nodes.jsonl`, `default-fixed-nodes.jsonl`: deterministic default comparison at 8,192 nodes with no cooperative wall limit.
- `corpus/`: new small binary packing models, generator provenance and independent optima.

```sh
python benchmarks/generate_gap_packing.py
python benchmarks/gap_limits.py --output docs/benchmarks/gap-limits/results.jsonl
BASELINE_PYTHON=/absolute/baseline-0.29.0/.venv/bin/python
python benchmarks/gap_limits.py --python "$BASELINE_PYTHON" --variants full \
  --output docs/benchmarks/gap-limits/baseline-0.29.0.jsonl
python benchmarks/gap_limits.py --variants full --node-limit 8192 \
  --output docs/benchmarks/gap-limits/default-fixed-nodes.jsonl
python benchmarks/gap_limits.py --python "$BASELINE_PYTHON" --variants full --node-limit 8192 \
  --output docs/benchmarks/gap-limits/baseline-fixed-nodes.jsonl
python benchmarks/report_gap_limits.py
```

Host: AMD EPYC 9V74, nine exposed CPUs, Linux x86_64, CPython 3.12.14, Rust 1.100.0. Benchmarks ran alongside other validation; use node counts for the reproducible comparison and do not treat these times as isolated throughput measurements. The process watchdog prevents a stuck solve from blocking the campaign.

"""Recompute complete final tables, compact metrics and an optional scientific figure."""

from pathlib import Path
import argparse, csv, json, statistics, hashlib

ROOT = Path(__file__).resolve().parents[1]


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--root", type=Path, default=ROOT / "docs/parallel-028")
    p.add_argument("--plot", action="store_true")
    a = p.parse_args()
    d = a.root / "data"
    names = [
        "final-matrix",
        "common-upstream",
        "heldout",
        "factor-scaling",
        "large-solver-scaling",
        "mixed-batches",
        "scheduling-confirmation",
        "scalar-isolation-production",
    ]
    # The raw journals are not tracked (`/docs/*/data/` in .gitignore); they
    # ship in the delivery archive, see docs/EVIDENCE.md.
    required = [n + ".json" for n in names] + [
        "applications-public.json",
        "applications-spoc.json",
        "audit-full/audit.json",
    ]
    missing = [n for n in required if not (d / n).is_file()]
    if missing:
        p.error(
            f"raw data missing under {d}: {', '.join(missing)}. The journals ship "
            "in the 0.28 delivery archive (docs/EVIDENCE.md); pass its "
            "docs/parallel-028 directory with --root."
        )
    datasets = {n: json.loads((d / (n + ".json")).read_text()) for n in names}
    for name, x in datasets.items():
        assert all(r["cpu_seconds"] is not None for r in x["summary"]), (
            name + ": the tables need child CPU time, recorded only where "
            "getrusage exists (POSIX)"
        )
        assert x["status"] == "passed" and len(x["records"]) == (
            x["repeats"] + 1
        ) * len(x["cases"]) * len(x["builds"]), name
        assert all(
            r.get("returncode") == 0 and not r.get("error") for r in x["records"]
        ), name
        assert all(r["samples"] == x["repeats"] for r in x["summary"]), name
        assert all(
            not row["payload"].get("kernel_profile", False) for row in x["records"]
        ), "profiled binary in final timing"
        keys = [(r["build"], tuple(r["case"]), r["repetition"]) for r in x["records"]]
        assert len(keys) == len(set(keys)), name

    def find(name, case, build="final"):
        return next(
            r
            for r in datasets[name]["summary"]
            if r["case"] == case and r["build"] == build
        )

    def pair(name, one, eight):
        x = find(name, one)
        y = find(name, eight)
        return dict(
            one_ms=x["seconds"] * 1000,
            eight_ms=y["seconds"] * 1000,
            speedup=x["seconds"] / y["seconds"],
            one_cpu_ms=x["cpu_seconds"] * 1000,
            eight_cpu_ms=y["cpu_seconds"] * 1000,
        )

    def table(headers, rows):
        return "\n".join(
            [
                "| " + " | ".join(headers) + " |",
                "|" + "|".join(["---"] * len(headers)) + "|",
            ]
            + ["| " + " | ".join(map(str, r)) + " |" for r in rows]
        )

    sections = [
        "# Recomputed final measurements",
        "",
        "Medians; all measured samples and warmups pass the same original-model/known-answer checks. API/kernel time, full process CPU and workflow have distinct scopes. Larger one/eight ratios are faster. Small and slower observations are retained.",
        "",
    ]
    csvrows = []
    for name, data in datasets.items():
        rows = []
        for r in data["summary"]:
            f, n, m, i, o, j = r["case"]
            rows.append(
                [
                    r["build"],
                    f,
                    n,
                    m,
                    i,
                    o,
                    j,
                    f"{r['seconds']*1000:.3f}",
                    f"{r['cpu_seconds']*1000:.3f}",
                    r["samples"],
                ]
            )
            csvrows.append(
                [
                    name,
                    r["build"],
                    f,
                    n,
                    m,
                    i,
                    o,
                    j,
                    r["seconds"] * 1000,
                    r["cpu_seconds"] * 1000,
                    r["samples"],
                ]
            )
        sections += [
            "## " + name,
            "",
            table(
                [
                    "Build",
                    "Family",
                    "Size",
                    "Mode",
                    "Inner",
                    "Outer",
                    "Jobs",
                    "API/kernel ms",
                    "Process CPU ms",
                    "Repeats",
                ],
                rows,
            ),
            "",
        ]
    # Numeric factors must match across every width and fresh-matrix repetition.
    for n in [384, 512, 768, 1024]:
        checks = {
            r["payload"]["checksum"]
            for r in datasets["factor-scaling"]["records"]
            if r["case"][1] == n
        }
        assert len(checks) == 1, (n, checks)
    metrics = {
        "factor": {
            str(n): pair(
                "factor-scaling",
                ["factor", n, "single", 1, 1, 3],
                ["factor", n, "single", 8, 1, 3],
            )
            for n in [384, 512, 768, 1024]
        },
        "large_lp": {
            str(n): pair(
                "large-solver-scaling",
                ["dense", n, "single", 1, 1, 1],
                ["dense", n, "single", 8, 1, 1],
            )
            for n in [768, 1024]
        },
        "batch128_jobs64": pair(
            "final-matrix",
            ["packing", 128, "batch", 1, 1, 64],
            ["packing", 128, "batch", 1, 8, 64],
        ),
        "component_skew128": pair(
            "final-matrix",
            ["packing", 128, "skewcomponents", 1, 1, 1],
            ["packing", 128, "skewcomponents", 1, 8, 1],
        ),
        "same_eight_upstream": [],
    }
    for c in datasets["scheduling-confirmation"]["cases"]:
        b = find("scheduling-confirmation", c, "baseline")
        f = find("scheduling-confirmation", c)
        metrics["same_eight_upstream"].append(
            dict(
                case=c,
                baseline_ms=b["seconds"] * 1000,
                final_ms=f["seconds"] * 1000,
                speedup=b["seconds"] / f["seconds"],
                baseline_cpu_ms=b["cpu_seconds"] * 1000,
                final_cpu_ms=f["cpu_seconds"] * 1000,
            )
        )
    applications = {}
    for kind in ["public", "spoc"]:
        x = json.loads((d / ("applications-" + kind + ".json")).read_text())
        assert x["status"] == "passed"
        assert len(x["records"]) == len(x["configurations"]) * (x["repeats"] + 1)
        assert all(
            r["returncode"] == 0 and not r["error"] and not r["payload"]["issues"]
            for r in x["records"]
        )
        for r in x["records"]:
            q = r["payload"]["result"]
            if q.get("point_file"):
                point = d / q["point_file"]
                assert (
                    point.stat().st_size == q["point_length"] * 8
                    and hashlib.sha256(point.read_bytes()).hexdigest()
                    == q["point_sha256"]
                )
        applications[kind] = x
        rows = []
        for r in x["summary"]:
            case = r["case"]
            label = Path(case[1]).stem if kind == "public" else case[1] + "/" + case[2]
            rows.append(
                [
                    label,
                    r["build"],
                    r["inner"],
                    r["outer"],
                    f"{r['seconds']*1000:.3f}",
                    f"{r['workflow_seconds']*1000:.3f}",
                    f"{r['relaxation_seconds']*1000:.3f}",
                    ",".join(r["statuses"]),
                    ",".join(map(str, r["objectives"])),
                ]
            )
        sections += [
            "## applications-" + kind,
            "",
            table(
                [
                    "Case",
                    "Build",
                    "Inner",
                    "Outer",
                    "API ms",
                    "Workflow ms",
                    "Root phase ms",
                    "Statuses",
                    "Objectives",
                ],
                rows,
            ),
            "",
        ]

    def app(instance, scenario, build, inner=1, outer=1):
        return next(
            r
            for r in applications["spoc"]["summary"]
            if r["case"] == ["spoc", instance, scenario]
            and r["build"] == build
            and r["inner"] == inner
            and r["outer"] == outer
        )

    metrics["scalar_loop_isolation"] = []
    for case in datasets["scalar-isolation-production"]["cases"]:
        base = find("scalar-isolation-production", case, "baseline")
        before = find("scalar-isolation-production", case, "before")
        final = find("scalar-isolation-production", case, "final")
        metrics["scalar_loop_isolation"].append(
            dict(
                case=case,
                baseline_ms=base["seconds"] * 1000,
                before_ms=before["seconds"] * 1000,
                final_ms=final["seconds"] * 1000,
                before_to_final=before["seconds"] / final["seconds"],
            )
        )
    metrics["spoc"] = {}
    for instance in ["i", "ii"]:
        b = app(instance, "root", "baseline")
        s = app(instance, "root", "final")
        e = app(instance, "root", "final", 8)
        metrics["spoc"][instance] = dict(
            upstream_api_ms=b["seconds"] * 1000,
            serial_api_ms=s["seconds"] * 1000,
            eight_api_ms=e["seconds"] * 1000,
            upstream_root_ms=b["relaxation_seconds"] * 1000,
            serial_root_ms=s["relaxation_seconds"] * 1000,
            eight_root_ms=e["relaxation_seconds"] * 1000,
            upstream_workflow_ms=b["workflow_seconds"] * 1000,
            eight_workflow_ms=e["workflow_seconds"] * 1000,
            api_speedup_vs_upstream=b["seconds"] / e["seconds"],
            root_speedup_vs_upstream=b["relaxation_seconds"] / e["relaxation_seconds"],
            root_speedup_vs_current1=s["relaxation_seconds"] / e["relaxation_seconds"],
        )
    metrics["public"] = {}
    for build, inner in [("baseline", 1), ("final", 1), ("final", 8)]:
        rows = [
            r
            for r in applications["public"]["summary"]
            if r["build"] == build and r["inner"] == inner
        ]
        metrics["public"][build + str(inner)] = dict(
            cases=len(rows),
            all_repeats_optimal=sum(
                all(s == "optimal" for s in r["statuses"]) for r in rows
            ),
            statuses={str(Path(r["case"][1]).stem): r["statuses"] for r in rows},
        )
    audit = json.loads((d / "audit-full/audit.json").read_text())
    assert (
        not audit["issues"]
        and not audit["status_or_acceptance_changes"]
        and audit["versions"]
        == {"baseline": "0.27.0", "serial": "0.28.0", "parallel": "0.28.0"}
    )
    metrics["audit"] = audit
    (a.root / "TABLES.md").write_text("\n".join(sections))
    with (d / "final-scaling.csv").open("w", newline="") as out:
        w = csv.writer(out, lineterminator="\n")
        w.writerow(
            [
                "dataset",
                "build",
                "family",
                "size",
                "mode",
                "inner",
                "outer",
                "jobs",
                "api_or_kernel_ms",
                "process_cpu_ms",
                "samples",
            ]
        )
        w.writerows(csvrows)
    (d / "metrics.json").write_text(json.dumps(metrics, indent=2) + "\n")
    if a.plot:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt

        colors = ["#087e8b", "#c56b1b", "#6e7681"]
        fig, axes = plt.subplots(2, 2, figsize=(13.2, 8.6))
        threads = [1, 2, 4, 8]
        for n, c in zip([768, 1024], colors):
            base = find("factor-scaling", ["factor", n, "single", 1, 1, 3])["seconds"]
            axes[0, 0].plot(
                threads,
                [
                    base
                    / find("factor-scaling", ["factor", n, "single", t, 1, 3])[
                        "seconds"
                    ]
                    for t in threads
                ],
                "-o",
                label=f"{n} variables",
                color=c,
            )
            base = find("large-solver-scaling", ["dense", n, "single", 1, 1, 1])[
                "seconds"
            ]
            axes[0, 1].plot(
                threads,
                [
                    base
                    / find("large-solver-scaling", ["dense", n, "single", t, 1, 1])[
                        "seconds"
                    ]
                    for t in threads
                ],
                "-o",
                label=f"{n} variables",
                color=c,
            )
        for ax, title in [
            (axes[0, 0], "Standalone dense LU: three fresh factors + solves"),
            (axes[0, 1], "Complete connected LP solve"),
        ]:
            ax.set_title(title)
            ax.set_xlabel("Numerical worker maximum")
            ax.set_ylabel("One-worker / requested-worker time")
            ax.set_xscale("log", base=2)
            ax.set_xticks(threads, threads)
            ax.legend()
            ax.grid(alpha=0.2)
        ax = axes[1, 0]
        positions = [0, 1]
        width = 0.25
        for offset, (build, inner, label, c) in enumerate(
            [
                ("baseline", 1, "Upstream 1", colors[2]),
                ("final", 1, "0.28 / 1", colors[1]),
                ("final", 8, "0.28 / max 8", colors[0]),
            ]
        ):
            ax.bar(
                [x + (offset - 1) * width for x in positions],
                [
                    app(i, "root", build, inner)["relaxation_seconds"] * 1000
                    for i in ["i", "ii"]
                ],
                width,
                label=label,
                color=c,
            )
        ax.set_xticks(positions, ["SpOC4 I (scalar fallback)", "SpOC4 II"])
        ax.set_title("Real packing root: equal 64 steps")
        ax.set_ylabel("Root-phase wall time (ms)")
        ax.legend(fontsize=9)
        ax.grid(axis="y", alpha=0.2)
        ax = axes[1, 1]
        base = find("final-matrix", ["packing", 128, "batch", 1, 1, 64])["seconds"]
        ax.plot(
            threads,
            [
                base
                / find("final-matrix", ["packing", 128, "batch", 1, t, 64])["seconds"]
                for t in threads
            ],
            "-o",
            color=colors[0],
        )
        ax.set_xscale("log", base=2)
        ax.set_xticks(threads, threads)
        ax.set_title("64 independent packing 128 solves")
        ax.set_xlabel("Outer workers, inner 1")
        ax.set_ylabel("Throughput speedup")
        ax.grid(alpha=0.2)
        fig.suptitle(
            "LP-IP-craft 0.28: measured eight-CPU scaling\nOne warmup; medians: 3 solver/application / 7 factor samples; original checks unchanged",
            fontsize=14,
        )
        fig.tight_layout(rect=[0, 0, 1, 0.93])
        assets = a.root / "assets"
        assets.mkdir(exist_ok=True)
        fig.savefig(assets / "scaling.png", dpi=180)
        fig.savefig(assets / "scaling.svg")
        p = assets / "scaling.svg"
        p.write_text(
            "\n".join(line.rstrip() for line in p.read_text().splitlines()) + "\n"
        )
    print(json.dumps(metrics, indent=2))


if __name__ == "__main__":
    main()

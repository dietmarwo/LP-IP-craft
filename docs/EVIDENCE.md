# Raw evidence for the native reports

The 0.27 ripsolve delivery includes `docs/ripsolve-027/data/`, the two frozen
oracle input/adjudication files below, comparison source and original held-out
inputs. It does not duplicate all historical raw report directories. Its root
`MANIFEST.sha256` covers the delivered source/evidence and baseline patch.
See [the 0.27 results](ripsolve-027/RESULTS.md) and
[reproduction instructions](ripsolve-027/REPRODUCE.md).

## Historical 0.24 evidence archive

The reports under `docs/*/RESULTS.md` were computed from raw observations
(JSON records, logs and frozen inputs) kept in each report's `data/`
directory. Those directories hold 427 files and 426.6 MB, almost all
machine-generated JSON, so they are not committed. They ship in the
delivery archive:

- `LP-IP-craft-0_24_0-mip-search.zip`,
  SHA-256 `c1d5a38c2d2845a72f29734ca419184f264729b24c874eea01781d19869fa8f1`

[`evidence-0.24.0.sha256`](evidence-0.24.0.sha256) lists the SHA-256 of
every data file as it appears in that archive. To check a copy, unzip it and
run, from the extracted `LP-IP-craft-0.24.0/` directory:

```sh
sha256sum -c /path/to/checkout/docs/evidence-0.24.0.sha256
```

To audit or regenerate a report, copy the `data/` directories into a
checkout (`docs/*/data/` is git-ignored). For example,
`python benchmarks/audit_mip_search_024.py` followed by
`python benchmarks/report_mip_search_024.py` reproduces
`docs/mip-search-024/RESULTS.md` byte for byte from the frozen data.

| Report directory | Files | MB |
| --- | ---: | ---: |
| basis-updates | 27 | 17.6 |
| irregular-lu | 45 | 28.1 |
| performance-ten | 54 | 69.1 |
| packing-mip | 43 | 51.3 |
| robustness-wide-cert | 45 | 28.6 |
| highs-lessons-implementation | 62 | 49.8 |
| packing-ftran-mip-diagnostics | 89 | 113.9 |
| mip-search-024 | 62 | 68.3 |
| **Total** | **427** | **426.6** |

`performance-ten/data/oracle-inputs.json` and
`performance-ten/data/oracle-adjudication.json` are the frozen 6,330-case
oracle inputs and adjudications that every later correctness audit checks by
hash.

## Archive files not imported

- `SHA256SUMS` is left over from an earlier delivery. It covers 169 of the
  427 data files (the four report directories up to 0.20, all of which
  still match) and none of the 0.21–0.24 data; 31 of its 445 entries no longer
  match the delivered files. `evidence-0.24.0.sha256` replaces it for data;
  git covers the source.
- `DELIVERY.md` still describes 0.21.0.
- `RELEASE.json` records commit ids of the authoring repository
  (`30929bf…`, production `4f1680d…`) that do not exist here. Its counts
  (139 Rust checks, 209 Python checks, 83,670 oracle runs) match the
  delivered report.
- `history/` holds 30 patches from `76ca1c9` (0.16) through 0.20. They were
  replayed as individual commits with `git am --exclude='docs/*/data/*'`, so
  this history keeps their authorship and messages. The archive has no
  per-release patches for 0.21–0.24; those arrive as one import commit. The
  directory also contains `.0010-…patch.lX2GNP`, a truncated 15 MB temporary
  copy of patch 0010 from packaging, which was ignored.

## SpOC4 evidence (0.25 and 0.26)

The raw SpOC4 records are not committed either. Both releases' records ship
in the 0.26 delivery archive, whose `MANIFEST.sha256` is complete and valid:

- `LP-IP-craft-0.26.0-spoc4-repair.zip`,
  SHA-256 `7cc10abfe9b061dd8850ef54bf1ae5a5926b9fca9034e5863d93869fd91488c0`

[`evidence-0.26.0.sha256`](evidence-0.26.0.sha256) lists those 186 files,
checked the same way from the extracted `LP-IP-craft-0.26.0/` directory.

| Directory | Files | MB | Contents |
| --- | ---: | ---: | --- |
| benchmarks/spoc4-025/results | 44 | 0.96 | 0.25 full-table records, solution vectors, kernel profiles, audit |
| docs/spoc4-025/data | 13 | 0.37 | 0.25 validation logs and baseline records |
| benchmarks/spoc4-026/results | 60 | 1.48 | 0.26 suite records and original-order solution vectors |
| benchmarks/spoc4-026/results-adapter | 48 | 1.08 | exploratory runs from the adapter start (with zero-weight picks) |
| benchmarks/spoc4-026/reference | 6 | 0.99 | HiGHS IPM reference for matching-II, prices and logs |
| docs/spoc4-026/data | 15 | 0.04 | 0.26 validation logs |

The ESA input tables and their GPL-3.0 provenance are committed under
`benchmarks/spoc4-025/data/`. With the records copied back,
`python benchmarks/summarize_spoc4_025.py` rechecks the 0.25 vectors and
`python benchmarks/audit_spoc4_026.py` regenerates
`benchmarks/spoc4-026/AUDIT.json`.

The 0.25.0 source itself was never delivered separately here: its release
commit `c2eb282` exists only in the authoring repository. The 0.25 import
commit is the 0.26 tree with the archive's own 0.26 patch
(`patches/LP-IP-craft-0.26.0.patch`, based on `c2eb282`) applied in reverse.

## Native 0.27 evidence

The 0.27 delivery archive arrived truncated:

- `LP-IP-craft-0.27.0-ripsolve.zip`, 10,926,426 bytes,
  SHA-256 `a1dc91c72922a1abadb85a750d3013bad3a26093c4045c1beeefbbd3aaecd5ab`

It has no central directory and ends partway through
`docs/ripsolve-027/data/pilot.json`; 458 of its 546 `MANIFEST.sha256`
entries were recovered intact from the local file headers. The source
is complete regardless: the archive's `PATCH-0.27.0.diff` matches the
SHA-256 recorded in its `SOURCE-REVISION`
(`cba04ee4…f27f`), and applied to `825ee42` it reproduces all 482
repository files of the manifest byte for byte. That is the import
commit.

[`evidence-0.27.0.sha256`](evidence-0.27.0.sha256) lists the manifest's
60 raw files under `docs/ripsolve-027/data/`. 21 were recovered (the
oracle, bound, held-out, family and branching records and the frozen LP
inputs). The other 39 were not in the upload: `pilot.json`,
`plain-lp.json`, `public.json`, `public-60s.json`,
`reference-adapter-corrections.json`, `rgn-lp-input.json`,
`ripsolve-ablations.json`, `ripsolve-serial.json`,
`ripsolve-status-reproduction.json`, the SpOC4 records and solution
vectors (`spoc4-*`) and all 21 logs under `validation/`. The review
re-measured instead of relying on them; see
[ripsolve-027/REVIEW.md](ripsolve-027/REVIEW.md).

## Eight-worker 0.28 evidence

The 0.28 archive contains all tracked source, 30 checkpoint plans/results, raw `docs/parallel-028/data/` journals and failed/excluded campaigns, independent audit vectors/certificates, SHA-checked application points and validation logs. `data/validation-summary.json` identifies numerical checkpoint, baseline, executable/extension hashes, frozen inputs, each iteration test count and final campaigns. [Results](parallel-028/RESULTS.md) distinguish within-solver latency, existing outer throughput and fixed-work root progress. An exact base-to-release patch and complete git bundle preserve development/reversions. The new root `MANIFEST.sha256` covers every delivered file other than itself. This does not recreate historical raw evidence already missing from upstream.

## LocalMIP pilot evidence

The raw records of the [LocalMIP pilot](local-mip-pilot/RESULTS.md) are not
committed. They ship in the delivery archive:

- `LP-IP-craft-LocalMIP-review-v2.zip`, 1,104,801 bytes,
  SHA-256 `1225736595597d29d9b90c93a965c10fb5c763214ba9552895c1ec93c54b77bb`

[`evidence-local-mip-pilot.sha256`](evidence-local-mip-pilot.sha256) is the
archive's own `SHA256SUMS` and lists its 289 other files; check a copy with
`sha256sum -c` from the extracted directory. All entries matched at import.

| Directory | Files | Contents |
| --- | ---: | --- |
| evidence/campaign | 175 | 174 run records (A/B on 19 models, C/D on the 10 external ones, three seeds) and the frozen schedule |
| evidence/smoke | 31 | 30 calibration records and the schedule |
| evidence/repeats | 25 | 24 repeats of `bell5`, `gt2` and `50v-10` and the schedule |
| evidence/parity, evidence/parity-stock | 28 | 13 effort-zero runs each of the patched and the stock driver |
| evidence/validation | 18 | build, test and oracle logs, the enumeration check and the FPR replay |
| evidence (top level) | 5 | scored summaries, `source.json` and the input manifest |
| history | 3 | the plan, the first edition's log and its comparisons |

The archive's full patch (`LP-IP-craft-LocalMIP-v2.patch`, applied to
`2468f09`, tree `64ea895`) is the first commit of #52. The plan, the delivered report, the
first edition's log and `input-manifest.json` are committed under
`docs/local-mip-pilot/`. `python benchmarks/local-mip/analyze.py
/path/to/evidence/campaign` reproduces the values of `evidence/scored.json`.

## Gurobi-gap evidence

The raw records of the [comparison with Gurobi](gurobi-gap/RESULTS.md) are
not committed. They ship in the delivery archive:

- `LP-IP-craft-Gurobi-gap-v2-patch.zip`, 744,447 bytes,
  SHA-256 `040d18fc1d60db32f858dc782cbdcd09d2fbb51d1a4c8288d568b7b60aec37e6`

[`evidence-gurobi-gap.sha256`](evidence-gurobi-gap.sha256) is the archive's
own `SHA256SUMS` and lists its 171 other files; check a copy with
`sha256sum -c` from the extracted directory. All entries matched at import.

| Directory | Files | Contents |
| --- | ---: | --- |
| evidence/input | 70 | the 69 frozen models (35 blind, 11 regression, 8 development, 8 relaxation, 7 LP) and their manifest |
| evidence/transfer | 25 | the 24 fresh generator cases and their manifest |
| evidence/public | 7 | the six public MIPLIB cases and the selection manifest |
| evidence/equality-input | 7 | the six signed equality chains and their manifest |
| evidence (top level) | 58 | journals, audits and scores of the comparisons, the `noswot` diagnosis, the SpOC4 records, build and test logs |
| top level | 4 | the report, the full patch, the patch from the preceding revision and the README |

The archive's full patch (`LP-IP-craft-Gurobi-gap-v2.patch`, applied to
`bcc4af3`, tree `c289160`) is the first commit of #53. The plan and the
delivered report are committed under `docs/gurobi-gap/`. With the models of
`evidence/input` and `evidence/transfer`,
`python benchmarks/gurobi-gap/audit.py MANIFEST JOURNAL --verified-journal OUT`
and `summarize.py OUT --group GROUP --baseline stock:adaptive+lattice
--no-static --output SCORE.json` reproduce `delivery-roster-score.json`
(group `blind`) and `delivery-transfer-score.json` (group `transfer`).
`delivery-default.jsonl` compares the preceding patch with revision 2, not
the unpatched solver; the [review](gurobi-gap/REVIEW.md) has the comparison
with the unpatched solver.

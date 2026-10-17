# Raw evidence for the native reports

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

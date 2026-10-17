# Public coverage inputs

Original MPS files are copied byte-for-byte from the HiGHS source distribution
bundled with `highs-sys 1.15.0`, `HiGHS/check/instances/`. SHA256 hashes and
conversion/oracle details are in manifest.json. These are a small coverage
sample, not the complete Netlib or MIPLIB suites. Netlib collection description:
https://www.netlib.org/lp/data/readme . The saved readme records its published
objective conventions; our JSON explicitly retains MPS objective constants
(e226 includes +7.113), so compare with each JSON's reference, not blindly with
an old table which may omit constants. Some distributed models differ in data
precision from historical copies.

`prepare_public.py` retains sense, objective constant, row ranges, variable
bounds, integral markers, and duplicate-term summation. It does not invent
finite bounds. JSON null denotes the appropriate infinite bound. CSR data is
checked by a separate untimed SciPy solve; solve comparisons independently
check original-model feasibility, objective and LP dual residuals. The small
MPS converter is benchmark tooling, not a production MPS parser.

Seven LPs: afiro, adlittle, avgas, israel, e226, etamacro, 25fv47. Three MIPs:
egout, flugpl, bell5. All three MIPs retain infinite variable bounds and are
unsupported by both native releases. 25fv47 already fails numerically in 0.19;
HiGHS solves it. Every case remains in comparisons and acceptance totals.

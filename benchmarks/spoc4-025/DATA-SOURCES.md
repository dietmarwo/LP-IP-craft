# ESA input provenance

Original repository: https://github.com/esa/SpOC4/tree/e4276399b51852e6f1c08c2d0c133bd13f8f967a/Challenge%201%20Luna%20Tomato%20Logistics

The unmodified `matching-i.txt` and `matching-ii.txt` tables are copied from that revision. The repository's GPL-3.0 text is preserved as `data/ESA-GPL-3.0.txt`; these data retain that provenance/license and are separate from the project's MIT Rust/Python code. No ESA challenge implementation code is imported.

SHA-256:

- matching-i.txt: `3f125328969d305b81fcc65ea1906bead3ee0909ea3cce72c31b288930042c5e`
- matching-ii.txt: `37fff9d2ec5f423d80f7a7f42608b16e3a1a3780567b6310d54b78fe3990e1c8`

The independent adapter parses the original decimal strings to integer thousandths, maintains disjoint E/L/D namespaces and validates candidates with original resource IDs. The copied adapter comes from the preceding assessment, not ESA's implementation. External solver imports in that optional adapter are for reference experiments only; the new benchmark suite calls the owned native solver.

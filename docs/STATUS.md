# Status

Snapshot of what exists and what comes next. Updated with each release.

## Working paper

The v1.1 preprint — *Prove, Don't Disclose: Verifiable CBAM Compliance Across
Industrial Supply Chains Using Rules as Code and Zero-Knowledge Proofs*,
Kaimeter Working Paper 2026-01 — is finalized in `paper/` and awaiting
publication on Zenodo (DOI
[10.5281/zenodo.22740284](https://doi.org/10.5281/zenodo.22740284)).

The paper workflow builds the PDF from the sources with the pinned
`pandoc/latex:3.11` image and asserts link health, the Appendix B arithmetic
and version/DOI consistency.

## Reference implementation

No crate has been released yet. `kaimeter-rules` v0.1 is targeted for October
2026 with the aluminium bundle and the Appendix B vector passing at full
precision; the planned releases are in §10 of the working paper and tracked in
the root README.

Rust quality gates and mutation testing are configured and activate when
`crates/` appears. The comment policy is enforced from the first commit that
carries `xtask/`.

## Repository

| Area | State |
| --- | --- |
| Working paper sources | Complete for v1.1 in `paper/` |
| Paper PDF build | Reproducible, pinned `pandoc/latex:3.11` image |
| Comment policy | `cargo xtask check-comments`, wired into the pre-commit hook |
| Rust quality gates | Path-filtered to `crates/**` until v0.1 lands |
| Citation metadata | `CITATION.cff` and `.zenodo.json` for the v1.1 release |
| Conformance vectors | Appendix B aluminium vector checked in verbatim |

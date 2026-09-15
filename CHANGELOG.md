# Changelog

Notable changes to the Kaimeter repository. The working paper is versioned as
`paper-vX.Y`; the reference implementation follows semantic versions from
v0.1.

## Unreleased

### Added

- `kaimeter-rules` 0.1.0 (rule bundle `2026.2.0`): reporting-period and
  sector-dependent mark-up rules, default-value selection with the
  other-countries and Annex IV fallbacks, Art. 14 precursor averaging, the
  Annex I aluminium scope table, the primary route with slope-method PFCs,
  and sector and cross-sector stubs.
- Canonical bundle serialisation and SHA-256 identity with the `bundle-hash`
  binary.
- Parameter tables with row-level legal, source and retrieval provenance, and
  the stdlib-only workbook extraction tool.
- Appendix B conformance vector, checked in verbatim and passing at full
  fixed-point precision, and `CITATIONS.md` mapping every rule to its
  provision.
- Workspace-wide Rust quality gates and `xtask check-comments` enforcement.

## paper-v1.1 - 2026-09-15

Working paper published on Zenodo: version DOI
[10.5281/zenodo.22740284](https://doi.org/10.5281/zenodo.22740284), concept DOI
[10.5281/zenodo.22740283](https://doi.org/10.5281/zenodo.22740283).

### Added

- Repository conventions, commit protocol and contribution guide.
- Pinned Rust toolchain, workspace lints, rustfmt and clippy configuration.
- `xtask check-comments` implementing the comment policy, wired into the
  pre-commit hook.
- Rust CI quality gates, path-filtered to `crates/**` until v0.1: format,
  clippy, tests, coverage, cargo-deny, rustdoc and mutation testing.
- Working paper build: `paper/metadata.yaml`, LaTeX template, Pandoc filter,
  build script and the Appendix B conformance vector.
- Paper CI: PDF build in the pinned `pandoc/latex:3.11` image, markdown lint,
  link check, arithmetic and metadata consistency assertions.
- CC BY 4.0 license text and the paper README.
- Root README, citation metadata, Zenodo deposit metadata and status page.
- Publishing and contribution workflow documentation, pull request template.

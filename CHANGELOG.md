# Changelog

Notable changes to the Kaimeter repository. The working paper is versioned as
`paper-vX.Y`; the reference implementation will follow semantic versions from
v0.1.

## Unreleased

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
- Resolved the DOI and contact placeholders in the working paper.
- Root README, citation metadata, Zenodo deposit metadata and status page.

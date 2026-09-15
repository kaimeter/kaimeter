# Kaimeter Working Paper 2026-01

*Prove, Don't Disclose: Verifiable CBAM Compliance Across Industrial Supply
Chains Using Rules as Code and Zero-Knowledge Proofs* — version 1.1, preprint.

- DOI: <https://doi.org/10.5281/zenodo.22740284>
- Repository: <https://github.com/kaimeter/kaimeter>

## Build

The PDF is generated from `whitepaper.md` with Pandoc and XeLaTeX:

```sh
pwsh paper/build.ps1
```

The script requires `pandoc` and `xelatex` on the PATH. It writes
`paper/dist/kaimeter-working-paper-2026-01-v1.1.pdf` plus the intermediate
`.tex`; that directory is not tracked by git. CI builds the same way inside
the pinned `pandoc/latex:3.11` image, with the timestamp and trailer ID
derived from the sources so repeated builds are byte-identical.

`metadata.yaml` is the single source of truth for the title, author, series,
version, date, DOI and keywords. `templates/paper.tex` formats the PDF,
`filters/paper.lua` prepares the Markdown for it, and
`vectors/worked-example-aluminium.json` is the Appendix B conformance vector
for `kaimeter-rules`.

## Cite

Ma, Yiu Ming Patrick (2026). *Prove, Don't Disclose: Verifiable CBAM
Compliance Across Industrial Supply Chains Using Rules as Code and
Zero-Knowledge Proofs.* Kaimeter Working Paper 2026-01, preprint. DOI
[10.5281/zenodo.22740284](https://doi.org/10.5281/zenodo.22740284).

## License

The paper text is licensed under [Creative Commons Attribution 4.0
International](LICENSE-CC-BY-4.0). The reference implementation's code is
licensed under the Apache License 2.0; see the repository root `LICENSE`.

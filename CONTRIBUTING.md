# Contributing to Kaimeter

We encourage contributions. This file documents the rules that keep the
repository professional-grade: licensing hygiene, provenance, and review.

## Licence headers

Every human-authored source file begins with a two-line header:

```
// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors
```

The comment syntax adapts to the file (`#` for Python and YAML, `<!-- -->`
for HTML/XML, `--` for SQL, and so on). Files that do not carry headers:
`LICENSE`, `NOTICE`, generated artifacts (lockfiles, build output), and data
files — data is licensed separately under CC BY 4.0.

A pre-commit check fails any source file missing its SPDX identifier, so
headers are stamped by the pipeline, not remembered by humans.

## Provenance and AI assistance

Every contribution carries provenance. Pull requests must disclose
AI-assisted contributions. AI may draft; a human must review, edit, and take
authorship before anything ships. Files are merged only after human review,
and the commit history is the audit trail.

Commits must be GPG-signed. See GitHub's guide to
[signing commits](https://docs.github.com/en/authentication/managing-commit-signature-verification/signing-commits).

## Locale files

Locale strings live in `locales/` as flat key-value JSON dictionaries.

- `en.json` is canonical: every other locale must carry exactly the same
  keys. The loader refuses to start with a locale that is missing a key, so
  a half-translated locale can never ship.
- Keys are dot-namespaced by surface — `app.*` for interface strings,
  `core.error.*` for domain errors. Segments are lowercase snake_case.
- `termbase.json` holds locked compliance terminology (for example,
  embedded emissions, carbon border adjustment mechanism, goods covered).
  A locked term's renderings live there, one per locale; locked terms are
  not paraphrased inside locale files.
- Locale codes are BCP-47 (`en`, `zh-CN`, `de`).
- Adding a locale means adding one file and registering the code in
  `I18n::load` (`src/i18n.rs`).

## Workflow

1. Open an issue to discuss a change before writing code.
2. Create a branch from `main` and keep pull requests small and reviewable.
3. Certify each commit with the Developer Certificate of Origin
   (`git commit -s`; see https://developercertificate.org).
4. Wait for review; maintainers merge.

## Code of conduct

Everyone interacting in Kaimeter's codebases, issue trackers, and chat
channels is expected to treat others with respect.

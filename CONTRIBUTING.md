<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 Keldrion, LLC and contributors -->

# Contributing to Kaimeter

We encourage contributions. This file documents the rules that keep the
repository professional-grade: licensing hygiene, provenance, and review.

## Licence headers

Every human-authored source file begins with a two-line header:

```
// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors
```

The comment syntax adapts to the file (`#` for Python, YAML, TOML and
Dockerfiles, `<!-- -->` for HTML/XML and Markdown, `--` for SQL, and so on).

Some files deliberately carry no header. `scripts/check-spdx.sh` holds the
list, with a reason per entry: `LICENSE` and `NOTICE` (the licence texts),
`Cargo.lock` (generated), `locales/*.json` (JSON has no comment syntax),
`samples/*` (parser fixtures that tests `include_str!`, so a header would change
the fixture), `*.png`, and the tool-configuration dotfiles.

`scripts/check-spdx.sh` fails any other tracked file missing either line. It
runs in CI as the `spdx-headers` job, and locally:

```sh
./scripts/check-spdx.sh
```

## Provenance and AI assistance

Every contribution carries provenance. Pull requests must disclose
AI-assisted contributions. AI may draft; a human must review, edit, and take
authorship before anything ships. Files are merged only after human review,
and the commit history is the audit trail.

Commits must be GPG-signed. See GitHub's guide to
[signing commits](https://docs.github.com/en/authentication/managing-commit-signature-verification/signing-commits).

### Signing from WSL against a Windows keyring

If your private key lives on Windows and you work in WSL, you do not need to
export it. WSL can drive the Windows GnuPG through interop, so the key never
leaves the Windows keyring. Two things are required, and both are non-obvious:

- **Path translation.** Git hands gpg a temporary file for each signature.
  Windows `gpg.exe` cannot read WSL paths, so a wrapper must translate them.
- **`TMPDIR` on the repository mount.** WSL `/tmp` is not reachable from Windows
  over `\\wsl.localhost` — verified: signing still succeeds, but the resulting
  signature cannot be verified afterwards, which is worse than an outright
  failure.

`bin/git` and `.githooks/gpg-windows` implement this. Use them via:

```sh
export PATH="$PWD/bin:$PATH"
```

Always confirm a signature rather than assuming it:

```sh
git verify-commit HEAD
git verify-tag <tag>
```

`git log --format=%G?` reports the _tag's_ status for a tagged commit, not the
commit's, so it can read `N` on a correctly signed commit. The two commands above
are authoritative.

## Locale files

Locale strings live in `locales/` as flat key-value JSON dictionaries — the
single authoring source for every translation: backend messages and the
wizard UI alike.

- `en.json` is canonical: every other locale must carry exactly the same
  keys. The loader refuses to start with a locale that is missing a key, so
  a half-translated locale can never ship.
- Keys are dot-namespaced by surface — `core.error.*` for domain errors,
  `calendar.*` for regulatory deadlines, `ui.*` for wizard UI strings
  (the prefix is stripped at render time; reference-catalog labels nest
  under it, e.g. `ui.country.US`, `ui.cn.73181500`). Segments are lowercase
  snake_case, except the wizard's legacy camelCase message keys
  (`ui.step1Title`), which are kept as-is.
- `termbase.json` holds locked compliance terminology (for example,
  embedded emissions, carbon border adjustment mechanism, goods covered).
  A locked term's renderings live there, one per locale; locked terms are
  not paraphrased inside locale files.
- Locale codes are BCP-47 (`en`, `zh-CN`, `de`).
- Adding a locale means adding one file and registering the code in
  `I18n::load` (`src/i18n.rs`).
- The wizard (`web/wizard.html`) embeds its dictionaries between
  `/*kaimeter-locales-start*/ … /*kaimeter-locales-end*/` markers so it
  stays a single file that runs from `file://`. That block is **generated**
  from the locale files — after editing any `ui.*` key, run
  `cargo run --bin regen-wizard` and commit the result; a test fails the
  build when the block is stale. Never hand-edit the block.

## Workflow

1. Open an issue to discuss a change before writing code.
2. Create a branch from `main` and keep pull requests small and reviewable.
3. Certify each commit with the Developer Certificate of Origin
   (`git commit -s`; see https://developercertificate.org).
4. Wait for review; maintainers merge.

## Code of conduct

Everyone interacting in Kaimeter's codebases, issue trackers, and chat
channels is expected to treat others with respect.

# Contributing to Kaimeter

Thank you for your interest in contributing to Kaimeter! This document explains how to set up your development environment, submit changes, and what to expect during review.

## Code of Conduct

By participating in this project, you agree to uphold a respectful, harassment-free environment. Be kind, be constructive, and assume good faith.

## Reporting Bugs & Requesting Features

Open a [GitHub issue](https://github.com/kaimeter/kaimeter/issues) and include:

- A short, descriptive title
- Steps to reproduce (for bugs) or a use-case description (for features)
- Expected vs. actual behavior
- Your OS, Rust toolchain version (`rustc --version`), and any relevant logs

Please search existing issues first to avoid duplicates.

## Development Setup

Kaimeter is written in Rust. To get started:

1. Install [rustup](https://rustup.rs)
2. Clone the repository:

   ```sh
   git clone https://github.com/kaimeter/kaimeter.git
   cd kaimeter
   ```

3. Verify your toolchain:

   ```sh
   cargo build
   cargo test --workspace
   ```

## Submitting Changes

1. Fork the repository and create a topic branch from `main`:

   ```sh
   git checkout -b feat/my-change
   ```

2. Make your changes, keeping pull requests small and focused on a single concern.
3. Make sure the following pass locally:

   ```sh
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

4. Push your branch and open a pull request with a clear description of the change and the motivation.

## Commit Messages

- Use the imperative mood: "add retry logic", not "added retry logic"
- Keep the first line under ~72 characters; add details in the body
- Reference issues with `Fixes #123` or `Closes #123` where applicable

## Developer Certificate of Origin (DCO)

All contributions must be signed off, certifying that you have the right to submit the work under the project's license — the same process used by the Linux kernel.

To sign off a commit, add a `Signed-off-by` line matching your commit author email:

```sh
git commit -s -m "Your commit message"
```

This can be added automatically for every commit in your clone with:

```sh
git config --local user.name "Your Name"
git config --local user.email "you@example.com"
```

Each commit in a pull request must contain a trailer of the form:

```
Signed-off-by: Your Name <you@example.com>
```

The full text of the DCO that you certify by signing off:

```
Developer Certificate of Origin
Version 1.1

Copyright (C) 2004, 2006 The Linux Foundation and its contributors.
1 Letterman Drive
Suite D4700
San Francisco, CA, 94129

Everyone is permitted to copy and distribute verbatim copies of this
license document, but changing it is not allowed.

Developer's Certificate of Origin 1.1

By making a contribution to this project, I certify that:

(a) The contribution was created in whole or in part by me and I
    have the right to submit it under the open source license
    indicated in the file; or

(b) The contribution is based upon previous work that, to the best
    of my knowledge, is covered under an appropriate open source
    license and I have the right under that license to submit that
    work with modifications, whether created in whole or in part
    by me, under the same open source license (unless I am
    permitted to submit under a different license), as indicated
    in the file; or

(c) The contribution was provided directly to me by some other
    person who certified (a), (b) or (c) and I have not modified
    it.

(d) I understand and agree that this project and the contribution
    are public and that a record of the contribution (including all
    personal information I submit with it, including my sign-off) is
    maintained indefinitely and may be redistributed consistent with
    this project or the open source license(s) involved.
```

## License

By contributing to Kaimeter, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).

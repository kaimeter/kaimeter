# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Keldrion, LLC and contributors

#!/bin/sh
# Fail any tracked source file that is missing its SPDX header.
#
# CONTRIBUTING.md promises "a pre-commit check fails any source file missing its
# SPDX identifier". Until this script existed that promise was false: there was
# no hook, and the only `spdx` strings in CI were SBOM generation. This makes the
# claim true.
#
# Usage: scripts/check-spdx.sh
# Exits non-zero and lists every offending file.

set -eu

cd "$(git rev-parse --show-toplevel)"

# Each entry is a reason a file cannot or should not carry a header. Keep this
# list explicit: a broad pattern here silently hides new gaps.
is_allowed() {
    case "$1" in
        LICENSE | NOTICE) return 0 ;;                 # the licence texts
        Cargo.lock) return 0 ;;                       # generated
        package.json | package-lock.json | */package.json | */package-lock.json)
            return 0 ;;                               # npm requires strict JSON; a comment is a parse error (verified)
        locales/en.json | locales/zh-CN.json | locales/termbase.json)
            return 0 ;;                               # JSON has no comments
        samples/*) return 0 ;;                        # fixtures include_str!-ed by tests
        *.png) return 0 ;;                            # binary
        .gitignore | .prettierignore | .prettierrc.json)
            return 0 ;;                               # tool configuration
        web/index.html)
            return 0 ;;                               # Vite entry: Vite rewrites the doctype, so a header here would not survive; the artifact carries one from web/index.html
        *) return 1 ;;
    esac
}

fail=0
checked=0

for file in $(git ls-files); do
    if is_allowed "$file"; then
        continue
    fi
    checked=$((checked + 1))
    # Headers sit at the top; a small window tolerates a shebang.
    header=$(head -n 5 "$file" 2>/dev/null || true)
    if ! printf '%s\n' "$header" | grep -q 'SPDX-License-Identifier'; then
        printf 'missing SPDX header:  %s\n' "$file"
        fail=1
    fi
    if ! printf '%s\n' "$header" | grep -q 'Copyright .* Keldrion'; then
        printf 'missing copyright:    %s\n' "$file"
        fail=1
    fi
done

if [ "$fail" -ne 0 ]; then
    printf '\n%d file(s) checked. Add the header, or allowlist the path in\n' "$checked" >&2
    printf 'scripts/check-spdx.sh with a reason.\n' >&2
    exit 1
fi

printf 'SPDX headers OK (%d files checked)\n' "$checked"

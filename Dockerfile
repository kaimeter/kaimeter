# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Keldrion, LLC and contributors

# Kaimeter core server — minimal container image.
# Multi-stage: build the frontend with Node, build the static release binary with
# Rust, then copy ONLY that binary into an empty scratch image. Nothing else ships.

# ---- Stage 1: frontend -------------------------------------------------------
# The UI is built from source here rather than copied from the repository, so an
# image can never ship a wizard that drifted from web/src.
FROM node:22-slim AS web
WORKDIR /src

# Dependencies first, for layer caching: this layer moves only with the lockfile.
COPY web/package.json web/package-lock.json ./web/
RUN npm --prefix web ci

# The bundled fallback dictionaries are generated from locales/*.json, so the
# locale files are frontend build inputs too.
COPY locales ./locales
COPY web ./web
RUN npm --prefix web run build

# ---- Stage 2: build ----------------------------------------------------------
FROM rust:1.83-slim AS build
WORKDIR /build

# Manifest first for layer caching.
COPY Cargo.toml ./
COPY build.rs ./
COPY src ./src
COPY migrations ./migrations
# Locale JSON assets are embedded into the binary at compile time (include_str!),
# so they must be present at build time — but they do not ship in the runtime image.
COPY locales ./locales
# The finished single-file wizard from the stage above. build.rs embeds it as-is
# instead of invoking Node, which this stage deliberately does not have.
COPY --from=web /src/web/wizard.html ./web/wizard.html
ENV KAIMETER_SKIP_FRONTEND=1

RUN cargo build --release

# ---- Stage 3: runtime --------------------------------------------------------
FROM scratch
# The binary is the product. Copy the exact same release binary.
COPY --from=build /build/target/release/kaimeter /kaimeter

# No shell, no package manager, nothing else.
ENTRYPOINT ["/kaimeter"]

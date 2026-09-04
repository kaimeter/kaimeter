# Kaimeter core server — minimal container image.
# Multi-stage: build the static release binary, then copy ONLY that binary
# into an empty scratch image. Nothing else ships.

# ---- Stage 1: build ----------------------------------------------------------
FROM rust:1.83-slim AS build
WORKDIR /build

# Manifest first for layer caching.
COPY Cargo.toml ./
COPY src ./src
COPY migrations ./migrations
# Locale JSON assets are embedded into the binary at compile time
# (include_str!), so they must be present at build time — but they do
# not ship in the runtime image.
COPY locales ./locales

RUN cargo build --release

# ---- Stage 2: runtime --------------------------------------------------------
FROM scratch
# The binary is the product. Copy the exact same release binary.
COPY --from=build /build/target/release/kaimeter /kaimeter

# No shell, no package manager, nothing else.
ENTRYPOINT ["/kaimeter"]

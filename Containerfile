# syntax=docker/dockerfile:1

# Image for the service-backed scale profile ONLY (Postgres, optional Valkey).
# The default memory/sqlite profile is a pure local binary — run it natively with
# `make run`; it needs no image. Built with podman by default
# (`podman build -t miku -f Containerfile .`); `docker build -f Containerfile .`
# also works.

# ---- Build stage ----
FROM rust:1-slim-bookworm AS builder

# ring/cc (via sqlx rustls) need a C toolchain; pkg-config for build scripts.
RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy the whole workspace: the binary depends on the crates/* path members, and
# miku-app embeds crates/miku-index-postgres/migrations at compile time via
# sqlx::migrate!.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY src ./src
COPY static ./static

# Scale image: build only the service-backed features (Postgres + Valkey), so
# one image serves both the `postgres` and `postgres-valkey` runtimes. The
# default sqlite/memory backends are intentionally left out.
RUN cargo build --locked --bin miku --no-default-features --features postgres,valkey

# ---- Runtime stage ----
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system miku \
    && useradd --system --gid miku miku

WORKDIR /app

# Binary + the assets the app loads from CWD at runtime (templates via minijinja
# path_loader, static via ServeDir). The miku_docs/ content dir is provided at
# runtime as a bind mount. The binary lives on PATH so it does not collide with
# the /app/miku_docs content directory.
COPY --from=builder /build/target/debug/miku /usr/local/bin/miku
COPY src/templates /app/src/templates
COPY static /app/static

RUN mkdir -p /app/miku_docs && chown -R miku:miku /app
USER miku

EXPOSE 3000
CMD ["miku"]

---
id: ADR-0013
title: crates.io release surface
slug: crates-io-release-surface
status: Accepted
date-proposed: 2026-07-14
date-accepted: 2026-07-14
deciders: [haru]
mirror: asobi:miku:decision:crates-io-release-surface
supersedes: []
superseded-by:
relates-to: [ADR-0010]
impacts: [Cargo.toml, crates]
config-keys: []
tags: [rust, cargo, release]
---

# ADR-0013 — crates.io release surface

## Decision

The workspace is not published wholesale. Application and deployment crates are private by default (`publish = false`). The first public crates, if there is a real consumer, are the stable reusable
layers:

- `miku-domain`;
- `miku-markdown`;
- optionally `miku-index-sqlite` and `miku-index-postgres` after API review.

The Miku application is released primarily as a binary/container/Git tag. A future `cargo install miku` package may be published after all public path dependencies have registry versions and the CLI
contract is intentional.

Every published crate uses SemVer, complete metadata, README/rustdoc, license, repository links, package inspection, and `cargo publish --dry-run` before an explicit dependency-ordered publish.

## Why

crates.io is a permanent public API commitment. Publishing internal adapters before their boundaries stabilize would turn implementation details into support obligations.

## Trade-offs / Rejected

- Rejected publishing every workspace member automatically.
- Rejected treating `cargo install` as the only release channel.
- Deferred public Valkey crate until its operational and API surface is proven.

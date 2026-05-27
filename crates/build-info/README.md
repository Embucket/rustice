# build-info

Compile-time build and git metadata for Embucket binaries, with zero runtime cost.

## What it does

`build.rs` runs git commands at compile time (`rev-parse HEAD`, `describe --tags`,
`diff-index`, `ls-files`) and emits the results as environment variables. `src/lib.rs`
reads them back with `env!()`, so the values are baked into the binary as `&'static str`
— no dependencies, no runtime work.

## Usage

```rust
use build_info::BuildInfo;

println!("{}", BuildInfo::full_version());
// 0.1.0 (7b92aa23) on main built 2025-05-26
// dirty tree: 0.1.0 (7b92aa23-dirty) on main built 2025-05-26

if BuildInfo::is_dirty() {
    eprintln!("warning: built from a dirty working tree");
}
```

## API

`BuildInfo` exposes these compile-time constants:

| Const | Source | Example |
|-------|--------|---------|
| `VERSION` | `CARGO_PKG_VERSION` | `0.1.0` |
| `GIT_SHA` | `git rev-parse HEAD` | `7b92aa2347…` |
| `GIT_SHA_SHORT` | short SHA | `7b92aa23` |
| `GIT_BRANCH` | current branch | `main` |
| `GIT_DESCRIBE` | `git describe --tags` | `v0.1.0-5-g7b92aa23` |
| `GIT_DIRTY` | uncommitted changes | `"true"` / `"false"` |
| `BUILD_TIMESTAMP` | RFC 3339 build time | `2025-05-26T…` |

Helpers: `full_version() -> String`, `is_dirty() -> bool`.

## Consumers

`embucketd` and `embucket-lambda` use it to report version info on startup and over the API.

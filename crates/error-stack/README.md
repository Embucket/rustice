# error-stack

Traits for building and rendering hierarchical (stacked) error chains across Embucket crates.

This is a pure trait crate with no dependencies. Domain error enums implement `StackError`
— in practice via the [`error-stack-trace`](../error-stack-trace) `#[error_stack_trace::debug]`
proc-macro applied alongside `#[derive(snafu::Snafu)]` — and the blanket-impl extension
traits give every error consistent, layered formatting.

## Exports

| Item | Kind | Purpose |
|------|------|---------|
| `StackError` | trait | Layered error contract: `debug_fmt(layer, buf)`, `next()`, `last()`, `transparent()`. Implemented by domain error types (supports `Arc<T>` / `Box<T>` wrapping). |
| `ErrorExt` | trait (blanket) | `output_msg()` — formats an error plus its debug output into a single message. |
| `ErrorChainExt` | trait (blanket) | `error_chain()` — walks `std::error::Error::source()` and returns the chain as numbered pieces. |

## Layout

- `src/error.rs` — `StackError` trait and `ErrorExt` impl.
- `src/error_chain.rs` — `ErrorChainExt` impl; the `get_error_chain!` macro walks the source chain.

## Usage

```rust
use error_stack::{ErrorExt, ErrorChainExt};

// `err` is any domain error implementing StackError
eprintln!("{}", err.output_msg());     // message + layered debug
for line in err.error_chain() {        // numbered source chain
    eprintln!("{line}");
}
```

## Consumers

`executor`, `catalog`, `catalog-metastore`, `functions`, `queries`, `state-store`,
`api-snowflake-rest`, and `api-snowflake-rest-sessions`.

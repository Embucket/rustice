# error-stack-trace

Procedural macro that auto-implements [`error-stack`](../error-stack)'s `StackError`
(and a matching `Debug`) for error enums, so layered error chains come for free.

## What it provides

A single attribute macro:

```rust
#[proc_macro_attribute]
pub fn debug(args: TokenStream, input: TokenStream) -> TokenStream
```

Applied as `#[error_stack_trace::debug]`, it parses the enum's variants (including snafu
`#[source]` / nested error fields) and generates the `debug_fmt`, `next`, and `transparent`
match arms required by `StackError`, plus a custom `Debug` impl that renders the full chain.

Built on `syn` / `quote` / `proc-macro2`; the parsing/codegen lives in `src/stack.rs`.

## Usage

Used together with `#[derive(snafu::Snafu)]` on a domain error enum:

```rust
#[derive(snafu::Snafu)]
#[snafu(visibility(pub))]
#[error_stack_trace::debug]
pub enum Error {
    #[snafu(display("Query execution exceeded timeout"))]
    QueryTimeout {
        #[snafu(implicit)]
        location: snafu::Location,
    },
    #[snafu(display("Cannot register UDF functions"))]
    RegisterUDF {
        #[snafu(source(from(DataFusionError, Box::new)))]
        error: Box<DataFusionError>,
        #[snafu(implicit)]
        location: snafu::Location,
    },
}
```

Canonical example: `crates/executor/src/error.rs:16`.

## Consumers

`executor`, `catalog`, `catalog-metastore`, `functions`, `queries`, `state-store`,
`api-snowflake-rest`, and `api-snowflake-rest-sessions`.

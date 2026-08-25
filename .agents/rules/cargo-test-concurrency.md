# Cargo Test Concurrency Rule

- **Do NOT run more than one `cargo test` at a time**.
- Compiling Rust code and running tests consumes significant CPU and memory resources.
- Always wait for an ongoing `cargo test` command to complete before launching another `cargo test`.

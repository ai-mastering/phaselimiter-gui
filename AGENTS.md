# Development Style

- Design the overall structure carefully, with ease of change in mind.
- Keep module implementations simple, readable, and correct. Avoid unnecessary
  abstraction or polish.
- Consider broader design changes when solving a problem. Discuss them with the
  user before proceeding.
- Test important behavior and module contracts without tying tests too closely
  to implementation details.

# Efficient Rust Development

- During iteration, use `cargo check --locked` for type checking. Run relevant
  tests when behavior changes; defer the full test suite and Clippy until a
  coherent change is ready for validation.
- Before finishing Rust changes, run `cargo fmt --check`, `cargo test --locked`,
  and `cargo clippy --locked --all-targets -- -D warnings`. Repeat checks only
  when subsequent changes or failures justify it. Documentation-only changes
  do not require Rust builds or tests.
- Run `cargo build --locked` when an executable is needed for manual or GUI
  verification. Do not routinely run check, test, Clippy, and build after every
  small edit. Use release and cross-platform builds when the task specifically
  needs them, such as packaging, performance, or platform-specific validation.
- This project is a single binary crate. Filtering `cargo test` reduces test
  execution, but still compiles the test binary, including GUI code. A successful
  `cargo check` does not replace behavior tests or executable verification.
- Preserve Cargo's incremental cache. Do not run `cargo clean`, disable
  incremental compilation, or change build flags merely to retry a failed check.
  Keep the target directory and build settings consistent throughout a task.
- In the Linux devcontainer, `/workspace` is a host bind mount. If it uses a
  shared filesystem such as `fuse.grpcfuse`, put build artifacts on the container
  filesystem instead. Honor an existing `CARGO_TARGET_DIR`; otherwise use
  `/root/.cache/phaselimiter-target` for this root-user devcontainer. Set
  `CARGO_TARGET_DIR` on every Cargo invocation (or in a persistent environment);
  an `export` in one tool shell may not persist into the next. Do not apply this
  container-specific path on native hosts or CI. The first build in a new target
  directory must populate its cache.
- When using an external target directory, launch executables from that directory
  instead of assuming `target/debug/`. Pass the environment to scripts that call
  Cargo, and check scripts for hard-coded artifact paths before using them.
- Do not run Cargo checks concurrently against the same target directory: they
  contend for Cargo's build lock. Report validation failures and missing checks
  accurately rather than repeatedly running unrelated checks.

# Repository Map

`ir` is a Rust CLI that runs self-describing R scripts and Quarto documents. It
extracts dependency metadata, resolves packages through an embedded R driver,
materializes a cached library, and launches the requested R or Quarto workload.

## Layout

- `src/main.rs`: binary entrypoint and top-level subcommand routing.
- `src/cli.rs`: clap command definitions plus hand-scanned argument parsing for
  `run`, `tool run`, `tool install`, and the hidden `rx` path.
- `src/script.rs`: script source selection and YAML frontmatter parsing.
- `src/runtime.rs`: Rscript selection, dependency resolution, cache directory
  helpers, resolver temp files, and user script execution.
- `src/tool.rs`: package executable discovery, `ir tool run`, `ir tool install`,
  generated launcher contents, and launcher quoting.
- `src/cache.rs`: `ir cache` subcommand behavior.
- `src/resolve_cache.rs`: resolution marker keys and warm-cache reads.
- `src/rig.rs`: R version selection through `rig`.
- `src/quarto.rs`: Quarto document detection, rendering, and render-specific
  Rscript argument handling.
- `src/bin/rx.rs`: standalone `rx` shim that forwards into `ir tool rx`.
- `driver/resolve.R`: embedded R resolver used by `runtime.rs`.
- `tests/cli.rs`: end-to-end CLI coverage.
- `tests/fixtures/`: R and Quarto inputs used by integration tests.
- `tests/snapshots/`: expected help output snapshots.
- `docs/`: Quarto documentation site.
- `examples/`: small runnable examples.
- `scripts/`: install scripts for release artifacts.

## Conventions

- Keep `main.rs` small. New command behavior should live in the focused module
  that owns the command or runtime behavior.
- CLI shape and argument scanning belong in `src/cli.rs`; command execution
  belongs in `src/runtime.rs`, `src/tool.rs`, or `src/cache.rs`.
- Frontmatter concerns belong in `src/script.rs`. Package ref normalization for
  resolver input belongs in `src/runtime.rs`.
- Code that shells out to R, `rig`, or Quarto should produce direct, actionable
  errors and should not silently fall back between unrelated behaviors.
- Public behavior is covered through CLI integration tests in `tests/cli.rs`.
  Update help snapshots when command help changes.

# ir

A [uv](https://docs.astral.sh/uv/)-style front-end to R. Write a self-contained
R script that declares its own dependencies in a comment header, and `ir` will
resolve them, build a dedicated package library, and run the script against it.

```r
#!/usr/bin/env -S ir run
# dependencies:
#   dplyr>=1.0
#   tidyr
# R: ">= 4.0"

library(dplyr)
library(tidyr)

1 + 1
```

```console
$ ir run script.R
# or, if the script is executable:
$ ./script.R
```

## How it works

`ir run script.R` runs in two phases:

1. **Resolve + materialise** (a private, throw-away R session).
   - The YAML frontmatter is parsed with the **yaml12** package.
   - The declared dependencies are resolved into concrete package versions
     with **pak** (`pak::pkg_deps`), including the full transitive closure.
   - The resolved set is hashed (together with the R version and platform) into
     a content-addressed library path under the cache directory.
   - **renv** (`renv::use`) installs the packages into renv's package cache and
     materialises that path as a light-weight library of **symlinks** into the
     cache. The library lives in our cache, not R's temp dir, so it persists.
2. **Run** (an ordinary R session).
   - The script runs as `Rscript script.R`, so it sees the user's normal R
     environment — `.Renviron`, `.Rprofile` and site files are all read.
   - The materialised library is injected via `R_LIBS`, which **prepends** it to
     `.libPaths()`: the resolved dependencies take precedence, while the user's
     other libraries remain available as a fallback.

Libraries are content-addressed: two scripts that resolve to the same set of
package versions share one materialised library, and the individual packages
are shared system-wide through renv's cache.

## Frontmatter format

The header is a block of leading `#` comments parsed as YAML (after a single
optional `#!` shebang line). Because it is parsed as real YAML by the `yaml12`
package, two YAML rules apply:

- The `R:` constraint must be **quoted** — `R: ">= 4.0"` — because a bare value
  starting with `>` is not valid YAML.
- Either list style works for `dependencies`: one entry per indented line, or
  explicit YAML list items with `-`. (Entries are split on whitespace, and
  package refs never contain spaces.)

```r
# dependencies:
#   dplyr>=1.0        # lower bound
#   tidyr             # latest
#   cli@3.6.6         # exact version
# R: ">= 4.0"         # optional; soft-checked against the running R
```

Supported dependency specs in this prototype:

| Spec         | Meaning                                          |
| ------------ | ------------------------------------------------ |
| `pkg`        | latest available                                 |
| `pkg>=1.0`   | at least version 1.0                             |
| `pkg@1.2`    | exactly version 1.2                              |
| `pkg@>=1.0`  | pak's native lower-bound ref                     |

`pkg>=1.0` is only a shorthand for pak's native `pkg@>=1.0` ref. Other pak ref
forms, such as GitHub refs and URL refs, are passed through unchanged. Version
operators that are not pak package refs, including `pkg<=1.2`, are not resolved
by `ir`.

## Requirements

- A Rust toolchain (to build `ir`).
- `R` / `Rscript` on `PATH` (this prototype uses whatever R it finds).
- The R packages `yaml12`, `pak`, `renv`, and `secretbase` installed in that R.

## Build & install

```console
$ cargo build --release
$ cp target/release/ir ~/.local/bin/   # or anywhere on PATH
```

## Testing

```console
$ cargo test
```

`cargo test` runs the Rust CLI tests (`tests/cli.rs`) and, when an R toolchain
with `testthat` and `yaml12` is available, the R resolution suite
(`tests/test-resolve.R`) — which covers pak ref normalisation, unsupported
version-operator pass-through, exotic-ref pass-through, frontmatter parsing, and
R-version checks. The R suite can also be run on its own:

```console
$ Rscript -e 'testthat::test_file("tests/test-resolve.R", stop_on_failure = TRUE)'
```

## Configuration

| Variable        | Default                                              |
| --------------- | ---------------------------------------------------- |
| `IR_CACHE_DIR`  | `~/Library/Caches/ir` (macOS), `~/.cache/ir` (Linux) |
| `IR_RSCRIPT`    | `Rscript` (resolved via `PATH`)                      |

## Limitations (prototype)

- Uses the `R`/`Rscript` already on `PATH`; the `R:` constraint is only a soft
  warning, not a version selector.
- Dependency specs support pak package refs. `pkg>=1.0` is accepted as a
  shorthand for pak's `pkg@>=1.0`; upper-bound syntax such as `pkg<=1.2` is not
  resolved by `ir`.
- Repositories default to CRAN (`https://cran.r-project.org`).

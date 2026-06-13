---
title: 'ir 0.1.0: self-describing R scripts and Quarto documents'
description: >
  `ir` is a new command-line tool for running standalone R scripts and
  rendering Quarto documents whose package requirements, and optional R
  version, live inside the file itself.
topics: [Best Practices, Publishing]
tags/software: [R, Quarto, Packages, Reproducibility, CLI]
---

Today we are announcing the first public release of `ir`, a small
command-line tool for running standalone R scripts and rendering Quarto
documents that declare their runtime requirements in the file itself.

`ir` is for the one-file workflows that do not quite need a project, but
still need to be understandable, repeatable, and easy to run later. Put
the package requirements, and optionally the R version, next to the
code, then run the file. `ir` resolves the requirements, prepares a
cached package library, and launches R or Quarto with that library ready
to use.

## Why `ir`?

R scripts often begin as small, local utilities: a report refresh, a
data pull, a model check, a quick diagnostic, or an example shared with
a colleague. Over time, the script can become important, but the setup
still lives somewhere else: in a README, in a shell history, in a
project library, or in the author's current R installation.

`ir` makes the runtime part of the source file. That means a script can
say, directly:

- which packages it needs
- which installed R version should run it, when that needs to be
  explicit
- whether user libraries should be visible
- whether CRAN packages should be resolved as of a specific date

The goal is not to replace project-level tools like `renv`. For
projects, use a project. `ir` is designed for the smaller shape: a
script or document that should explain how to run itself.

## A self-describing R script

Here is a complete script:

```r
#!/usr/bin/env -S ir run
#| packages:
#|   - dplyr>=1.0
#|   - tidyr
#| isolated: true
#| exclude-newer: "2024-01-15"

library(dplyr)
library(tidyr)

1 + 1
```

Run it with:

```console
$ ir run script.R
```

Or, on macOS and Linux, make it executable and run it directly:

```console
$ ./script.R
```

The metadata block is YAML written in `#|` comments after an optional
shebang. `ir` reads that metadata, resolves the declared packages with
`pak`, materializes a package library with `renv`, and starts R with the
resolved library at the front of `.libPaths()`.

By default, user libraries remain visible as a fallback. Add
`isolated: true` in the file, or use `--isolated` at the command line,
to run without the user library.

## Cached by design

The first run of a new dependency set does the normal work of resolving
and installing packages. Later runs reuse cached resolutions and
content-addressed package libraries when the same requirements are seen
again.

That makes `ir` useful for short-lived and repeated command-line work.
You can run a script, run an inline expression, render a report, or
launch a package-provided executable without creating a project
directory just to hold the dependency state.

```console
$ ir run --with cli -e 'cli::cli_alert_success("works")'
$ ir run --r-version 4.5 script.R
$ ir render report.qmd --to html
```

`ir` also bootstraps its own resolver tooling on first use, so you do
not need to pre-install `pak` or `renv`.

## Reproducibility controls for small files

For scripts and reports that need more explicit reproducibility,
`exclude-newer` is usually the first thing to reach for:

```yaml
exclude-newer: 2024-01-15
```

This resolves packages from the Posit Package Manager snapshot for that
date. It also uses the R release that was current on that date, so if
you were writing with the current release of R and current CRAN
packages, you rarely need to set a separate `r-version`.

Use `r-version` when the script really needs a specific installed R
version or version range:

```yaml
r-version: "4.3"
```

Together, those options let a file carry the important parts of its
runtime contract without requiring a surrounding project, but most files
should only need the date.

## Quarto documents too

`ir` uses the same metadata model for Quarto documents. Put package
metadata under an `ir:` key in the document YAML:

```yaml
---
title: My report
ir:
  packages:
    - dplyr>=1.0
    - gt@1.0
  isolated: true
  exclude-newer: 2024-01-15
---
```

Then render with:

```console
$ ir render report.qmd
$ ir render report.qmd --to pdf
```

When an R version is selected, `ir` pins Quarto's knitr R to that
selection. `ir` also seeds `rmarkdown` automatically for knitr-based
renders unless you declare it yourself.

## Running R package tools with `rx`

The `ir` release also includes `rx`, a short alias for `ir tool run`
that runs executables provided by R packages:

```console
$ rx btw

# same as:
$ ir tool run btw
```

Package authors can put executable scripts in a package's `exec/`
directory. Those scripts can be regular `Rscript` files or
[`Rapp`](https://github.com/r-lib/Rapp) apps. `rx` resolves the package,
finds the requested executable, and runs it in an isolated library. For
tools you use regularly, `ir tool install` writes persistent launchers:

```console
$ ir tool install btw
```

This gives package-provided CLIs a shared command namespace: run them on
demand with `rx`, or promote the ones you use often to persistent
launchers with `ir tool install`.

## Install `ir`

Install a pre-built binary on Linux or macOS:

```console
$ curl -fsSL https://raw.githubusercontent.com/t-kalinowski/ir/main/scripts/install.sh | sh
```

Install on Windows PowerShell:

```console
> irm https://raw.githubusercontent.com/t-kalinowski/ir/main/scripts/install.ps1 | iex
```

The installers download the latest release and install both `ir` and
`rx`. You will also need `R` / `Rscript`; `rig` is required when
selecting R versions, and Quarto is required when rendering Quarto
sources.

## Learn more

The project is open source under the MIT license. To get started:

- Read the documentation: https://t-kalinowski.github.io/ir/
- Browse the source: https://github.com/t-kalinowski/ir
- Open an issue: https://github.com/t-kalinowski/ir/issues

This is a first public release, and feedback is especially useful now.
If you try `ir` on your own scripts or Quarto documents, we would like
to hear which workflows feel natural, where the metadata model needs
more room, and which command-line edges still need smoothing.

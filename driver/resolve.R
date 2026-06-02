# ir resolve driver
#
# Run by the `ir` Rust binary in a private, throw-away R session.
#
#   Rscript --vanilla resolve.R <script_path> <cache_dir> <out_file>
#
# Responsibilities (steps 1-4 of the `ir` pipeline):
#   1. Parse the commented YAML frontmatter of <script_path> with yaml12.
#   2. Resolve the declared dependencies into concrete versions with pak.
#   3. Hash the resolved set to derive a content-addressed library path
#      under <cache_dir>.
#   4. Materialise that path as a light-weight library of symlinks into
#      renv's package cache via renv::use().
#
# The resulting library path is written to <out_file> for the Rust process
# to pick up. This session then exits; the Rust process launches the user's
# script in a fresh, isolated R session pointed at the library.

`%||%` <- function(x, y) if (is.null(x)) y else x

local({

  args <- commandArgs(trailingOnly = TRUE)
  if (length(args) < 3L)
    stop("usage: resolve.R <script_path> <cache_dir> <out_file>", call. = FALSE)
  script_path <- args[[1L]]
  cache_dir   <- args[[2L]]
  out_file    <- args[[3L]]

  # We run with --vanilla, so no repository is configured. Fall back to a
  # canonical CRAN mirror when the session has none.
  repos <- getOption("repos")
  cran  <- if (!is.null(repos)) repos[["CRAN"]] else NULL
  if (is.null(cran) || is.na(cran) || !nzchar(cran) || identical(cran, "@CRAN@")) {
    repos <- c(CRAN = "https://cran.r-project.org")
    options(repos = repos)
  }

  ## 1. Parse frontmatter -----------------------------------------------------

  lines <- readLines(script_path, warn = FALSE)

  # Drop a leading shebang line (e.g. `#!/usr/bin/env -S ir run`).
  if (length(lines) && grepl("^#!", lines[[1L]]))
    lines <- lines[-1L]

  # The frontmatter is the leading contiguous block of comment lines.
  is_comment <- grepl("^[[:space:]]*#", lines)
  stop_at <- which(!is_comment)
  block <- if (length(stop_at)) lines[seq_len(stop_at[[1L]] - 1L)] else lines

  # Strip the leading '#' and one optional space to recover the YAML text.
  yaml_text <- paste(sub("^[[:space:]]*#[[:space:]]?", "", block), collapse = "\n")
  spec <- tryCatch(
    if (nzchar(yaml_text)) yaml12::parse_yaml(yaml_text) else list(),
    error = function(e)
      stop(sprintf("could not parse script frontmatter as YAML: %s",
                   conditionMessage(e)), call. = FALSE)
  )
  # A leading comment that isn't a YAML mapping (e.g. prose) is simply not
  # frontmatter; treat it as an absent header rather than an error.
  if (!is.list(spec)) spec <- list()

  # Accept both a YAML list (`- dplyr`) and a whitespace-separated scalar
  # (`dplyr>=1.0 tidyr`); package refs never contain spaces.
  deps <- as.character(spec$dependencies %||% character())
  deps <- unlist(strsplit(trimws(deps), "[[:space:]]+"))
  deps <- deps[nzchar(deps)]

  # Optional R version constraint: soft-check only for this prototype.
  if (!is.null(spec$R)) {
    req <- trimws(as.character(spec$R)[[1L]])
    m <- regmatches(req, regexec("^(>=|>|<=|<|==)?[[:space:]]*([0-9][0-9.-]*)$", req))[[1L]]
    if (length(m) == 3L) {
      op <- if (nzchar(m[[2L]])) m[[2L]] else ">="
      ok <- do.call(op, list(getRversion(), numeric_version(m[[3L]])))
      if (!isTRUE(ok))
        warning(sprintf("script requests R %s but running R %s",
                        req, getRversion()), call. = FALSE, immediate. = TRUE)
    }
  }

  ## 2. Resolve with pak ------------------------------------------------------
  # A script may legitimately declare no dependencies; it then gets an empty
  # but still isolated library (base R only), so undeclared library() calls
  # fail loudly instead of silently borrowing the user's packages.

  if (length(deps)) {

    # Resolve an exact pin (`pkg==1.2`) to a concrete pak ref `pkg@<version>`.
    # pak's `@<version>` requires a *literal* published version string, so a
    # partial spec like `1.2` would fail even though `1.2.0` exists. We match
    # the request against published versions numerically -- like pip/uv, where
    # `==1.2` selects the version equal to 1.2 (i.e. 1.2.0), not 1.2.x.
    exact_ref <- function(pkg, ver) {
      target <- tryCatch(numeric_version(ver), error = function(e) NULL)
      hist <- tryCatch(pak::pkg_history(pkg), error = function(e) NULL)
      if (is.null(target) || is.null(hist) || !nrow(hist))
        return(sprintf("%s@%s", pkg, ver))  # fall back; let pak validate
      avail <- as.character(hist$Version)
      hit <- avail[numeric_version(avail) == target]
      if (!length(hit)) {
        near <- tail(avail, 6L)
        stop(sprintf("no version of '%s' equal to %s (available: %s)",
                     pkg, ver, paste(near, collapse = ", ")), call. = FALSE)
      }
      # Prefer the literal string if published, else any numeric match.
      sprintf("%s@%s", pkg, if (ver %in% hit) ver else hit[[length(hit)]])
    }

    # Translate dependency specs into pak package references:
    #   `pkg`         -> `pkg`         (latest)
    #   `pkg>=1.0`    -> `pkg@>=1.0`   (lower bound)
    #   `pkg==1.2`    -> `pkg@1.2.0`   (exact; version normalised)
    #   `pkg 1.0`     -> `pkg@1.0.0`   (exact; version normalised)
    to_ref <- function(d) {
      d <- trimws(d)
      m <- regmatches(d, regexec(
        "^([A-Za-z][A-Za-z0-9.]*[A-Za-z0-9])[[:space:]]*(>=|==|=)?[[:space:]]*([0-9][0-9.-]*)?$",
        d
      ))[[1L]]
      if (length(m) != 4L) return(d)  # leave anything exotic untouched (e.g. github refs)
      pkg <- m[[2L]]; op <- m[[3L]]; ver <- m[[4L]]
      if (!nzchar(ver)) return(pkg)
      if (identical(op, ">=")) sprintf("%s@>=%s", pkg, ver) else exact_ref(pkg, ver)
    }
    refs_in <- vapply(deps, to_ref, character(1L), USE.NAMES = FALSE)

    res <- pak::pkg_deps(refs_in, dependencies = NA, upgrade = TRUE)

    failed <- res[res$status != "OK", , drop = FALSE]
    if (nrow(failed))
      stop("pak could not resolve: ",
           paste(failed$ref, collapse = ", "), call. = FALSE)

    # Drop base / recommended packages: those are supplied by R itself.
    keep <- is.na(res$priority) | !(res$priority %in% c("base", "recommended"))
    res <- res[keep, , drop = FALSE]

    pkgs     <- res$package
    resolved <- sort(unique(sprintf("%s@%s", res$package, res$version)))

  } else {
    pkgs     <- character()
    resolved <- character()
  }

  ## 3. Hash the resolved set -> content-addressed library path ---------------
  # Bind the hash to the R version and platform: the symlinks point into the
  # renv cache, whose layout is itself keyed by R version and platform.
  key <- paste(c(resolved,
                 as.character(getRversion()),
                 R.version$platform),
               collapse = "\n")
  tf <- tempfile(); writeLines(key, tf)
  hash <- unname(tools::md5sum(tf))
  unlink(tf)

  library_path <- file.path(cache_dir, "libraries", hash)

  ## 4. Materialise the symlinked library via renv::use() ---------------------
  # Skip when the library already holds every resolved package: repeat runs of
  # an unchanged script then cost nothing beyond resolution.
  dir.create(library_path, recursive = TRUE, showWarnings = FALSE)
  have <- list.files(library_path)
  if (length(pkgs) && !all(pkgs %in% have)) {
    # renv::use() installs into the renv cache and links the packages into
    # `library` as symlinks. Because `library` lives in our cache (not the R
    # temp dir), renv leaves it in place when the session ends.
    do.call(renv::use, c(
      as.list(resolved),
      list(
        library = library_path,
        repos   = repos,
        attach  = FALSE,
        sandbox = FALSE,
        isolate = TRUE,
        verbose = TRUE
      )
    ))
  }

  writeLines(library_path, out_file)
  invisible()
})

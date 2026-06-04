# ir resolve driver
#
# Run by the `ir` Rust binary in a private, throw-away R session.
#
#   IR_RESOLVE_RESULT_FILE=<result_file> Rscript resolve.R
#
# Responsibilities (steps 1-4 of the `ir` pipeline):
#   1. Consume package dependency specs from stdin, one dependency per line.
#   2. Resolve the declared dependencies into concrete versions with pak.
#   3. Hash the resolved set to derive a content-addressed library path
#      under <cache_dir>.
#   4. Materialise that path as a light-weight library of symlinks into
#      renv's package cache via renv::use().
#
# The resulting library path is written to the temp result file named by
# IR_RESOLVE_RESULT_FILE. stdout/stderr stay available for pak progress.
# This session then exits; the Rust process launches the user's script in a
# fresh, isolated R session pointed at the library.
#
# The helpers below are pure and side-effect free so they can be unit tested
# (see tests/test-resolve.R). The pipeline runs only when this file is executed
# as a script -- `sys.nframe() == 0L` is false when the file is sourced.

## --- resolver input ---------------------------------------------------------

ir_env_optional <- function(name) {
  value <- Sys.getenv(name, unset = NA_character_)
  if (is.na(value) || !nzchar(value)) NULL else value
}

# Optional date-bounded resolution. `exclude-newer` is a YAML mapping key whose
# value is an ISO date; resolution then uses that day's Posit Package Manager
# CRAN snapshot instead of the latest CRAN repository.
ir_exclude_newer <- function(value) {
  if (is.null(value)) return(NULL)

  value <- trimws(as.character(value)[[1L]])
  if (!grepl("^[0-9]{4}-[0-9]{2}-[0-9]{2}$", value))
    stop("`exclude-newer` must be a date string in YYYY-MM-DD format",
         call. = FALSE)

  value
}

# The hard dependency types, always resolved for a package unless an explicit
# policy overrides them.
ir_dep_hard <- function() c("depends", "imports", "linkingto")

# Expand one requirement's dependency-type token into the set of types to follow
# for that package. Rust sends a canonical, lower-cased, comma-separated set of
# concrete DESCRIPTION types (or omits it). `NA` is the default (hard
# dependencies); an empty string follows no dependencies (pak `dependencies =
# FALSE`); otherwise it is the comma-separated set verbatim.
ir_dep_policy <- function(token) {
  if (is.na(token)) return(ir_dep_hard())
  token <- trimws(token)
  if (!nzchar(token)) return(character())
  parts <- trimws(strsplit(token, ",", fixed = TRUE)[[1L]])
  tolower(parts[nzchar(parts)])
}

## --- pak ref normalisation --------------------------------------------------

# Translate one dependency spec into a pak package reference:
#   `pkg`         -> `pkg`         (latest)
#   `pkg>=1.0`    -> `pkg@>=1.0`   (lower bound; solver picks)
#   `pkg==1.0`    -> `pkg@1.0`     (exact version)
# Native pak refs, GitHub refs, and URL refs are passed through untouched.
# Unsupported version operators such as `pkg<=1.2` are also passed to pak
# unchanged, so pak remains the source of truth for supported refs.
ir_to_ref <- function(d) {
  d <- trimws(d)
  m <- regmatches(d, regexec(
    "^([A-Za-z][A-Za-z0-9.]*[A-Za-z0-9])[[:space:]]*(>=|==)[[:space:]]*([0-9][0-9.-]*)$",
    d
  ))[[1L]]
  if (length(m) != 4L) return(d)
  if (m[[3L]] == ">=") sprintf("%s@>=%s", m[[2L]], m[[4L]])
  else sprintf("%s@%s", m[[2L]], m[[4L]])
}

# Prune a resolved set to the packages reachable under each package's dependency
# policy. `direct_policies` is a named list mapping each directly-requested
# package to the dependency types to follow from it; every other package follows
# its hard dependencies. Starting from the direct packages, we walk the `deps`
# edge tables pak attaches to each resolved row, following only edges whose type
# is allowed by the originating package's policy. This realises per-package
# dependency selection -- including reductions such as `dependencies = FALSE` --
# from the single superset solve, with soft dependencies followed only at the
# direct level (transitive packages always use hard deps).
ir_prune_to_policy <- function(res, direct_policies) {
  hard <- ir_dep_hard()
  policy_of <- function(pkg) {
    pol <- direct_policies[[pkg]]
    if (is.null(pol)) hard else pol
  }

  keep  <- names(direct_policies)
  queue <- keep
  while (length(queue)) {
    pkg   <- queue[[1L]]
    queue <- queue[-1L]
    idx <- which(res$package == pkg)
    if (!length(idx)) next
    deps <- res$deps[[idx[[1L]]]]
    if (is.null(deps) || !nrow(deps)) next
    follow <- deps$package[tolower(deps$type) %in% policy_of(pkg)]
    follow <- unique(follow[follow %in% res$package])
    new <- setdiff(follow, keep)
    keep  <- c(keep, new)
    queue <- c(queue, new)
  }
  unique(keep)
}

## --- cache location ---------------------------------------------------------

# The cache root: the standard per-package user cache directory, overridable
# with IR_CACHE_DIR. Holds `libraries/` (materialised libraries) and
# `resolutions/` (the resolution request cache).
ir_cache_dir <- function() {
  env <- Sys.getenv("IR_CACHE_DIR")
  if (nzchar(env)) env else tools::R_user_dir("ir", "cache")
}

## --- repositories -----------------------------------------------------------

ir_ppm_snapshot_url <- function(exclude_newer) {
  sprintf("https://packagemanager.posit.co/cran/%s", exclude_newer)
}

ir_repos <- function(exclude_newer = NULL, repos = getOption("repos")) {
  if (!is.null(exclude_newer))
    return(c(CRAN = ir_ppm_snapshot_url(exclude_newer)))

  cran <- if (!is.null(repos)) repos[["CRAN"]] else NULL
  if (is.null(cran) || is.na(cran) || !nzchar(cran) || identical(cran, "@CRAN@"))
    c(CRAN = "https://cran.r-project.org")
  else
    repos
}

## --- resolution cache -------------------------------------------------------

# Key identifying a resolution request: the declared package specs (order
# independent), the resolution source, and the R version / platform. Each spec is
# the requirement line as received -- a bare ref, or `ref<TAB>types` when a
# per-package dependency policy is attached -- so a policy change yields a
# distinct key while a plain-ref request hashes exactly as before. Latest
# resolution includes the current day so newly published versions are picked up
# at most once per day. Dated PPM snapshot resolution uses only the snapshot date
# because that repository state is immutable. Order independent so reordering
# specs doesn't bust the cache.
ir_input_key <- function(specs,
                         date          = Sys.Date(),
                         rversion      = getRversion(),
                         platform      = R.version$platform,
                         exclude_newer = NULL) {
  source_key <- if (is.null(exclude_newer))
    as.character(date)
  else
    sprintf("exclude-newer: %s", exclude_newer)

  secretbase::sha256(paste(c(sort(specs),
                             source_key,
                             as.character(rversion),
                             platform),
                           collapse = "\n"))
}

## --- pipeline ---------------------------------------------------------------

ir_resolve_main <- function() {

  specs       <- readLines(file("stdin"), warn = FALSE)
  result_file <- ir_env_optional("IR_RESOLVE_RESULT_FILE")
  package_result_file <- ir_env_optional("IR_RESOLVE_PACKAGE_RESULT_FILE")
  stopifnot(!is.null(result_file))
  cache_dir   <- ir_cache_dir()

  ## 1. Consume inputs parsed by Rust from script frontmatter and the command
  ## line. Each requirement line is a bare ref, or `ref<TAB>types` when a
  ## per-package dependency policy is attached. The token holds a canonical
  ## comma-separated set of dependency types ("" means resolve no dependencies);
  ## its absence (no tab) is the default of the package's hard dependencies.
  exclude_newer <- ir_exclude_newer(ir_env_optional("IR_EXCLUDE_NEWER"))
  repos <- ir_repos(exclude_newer)
  options(repos = repos)

  tab      <- regexpr("\t", specs, fixed = TRUE)
  refs_raw <- ifelse(tab > 0L, substr(specs, 1L, tab - 1L), specs)
  tokens   <- ifelse(tab > 0L, substr(specs, tab + 1L, nchar(specs)), NA_character_)

  ## 1b. Resolution cache: if this exact request was resolved already and its
  ## library still exists, reuse it and skip pak entirely. The marker is written
  ## only after a successful materialise (below), so its presence implies a
  ## complete library.
  primary_ref <- if (length(refs_raw)) ir_to_ref(refs_raw[[1L]]) else NULL
  marker <- file.path(cache_dir, "resolutions",
                      ir_input_key(specs, exclude_newer = exclude_newer))
  package_marker <- if (!is.null(primary_ref)) {
    file.path(cache_dir, "resolutions",
              paste0(basename(marker), "-primary-", secretbase::sha256(primary_ref)))
  } else {
    NULL
  }
  if (file.exists(marker)) {
    cached <- readLines(marker, n = 1L, warn = FALSE)
    if (length(cached) && nzchar(cached) && dir.exists(cached)) {
      if (!is.null(package_result_file) &&
          (is.null(package_marker) || !file.exists(package_marker))) {
        # The library is reusable, but this caller needs primary-package
        # metadata that older cache entries did not record.
      } else {
        writeLines(cached, result_file)
        if (!is.null(package_result_file)) {
          package <- readLines(package_marker, n = 1L, warn = FALSE)
          writeLines(package, package_result_file)
        }
        return(invisible())
      }
    }
  }

  ## 2. Resolve with pak
  # A script may legitimately declare no dependencies; it then gets an empty
  # but still isolated library (base R only), so undeclared library() calls
  # fail loudly instead of silently borrowing the user's packages.
  primary_package <- NULL
  if (length(refs_raw)) {
    refs_in  <- vapply(refs_raw, ir_to_ref, character(1L), USE.NAMES = FALSE)
    policies <- lapply(tokens, ir_dep_policy)

    # Per-package dependency selection from a single consistent solve. We solve a
    # superset whose direct refs follow the union of every requested policy and
    # whose transitive deps follow only hard deps, then prune (below) to what each
    # package's own policy reaches. With no custom policy this is exactly
    # `dependencies = NA`, so the common case resolves identically to before.
    any_custom <- any(!is.na(tokens))
    dependencies <- if (any_custom) {
      caps <- c(depends = "Depends", imports = "Imports", linkingto = "LinkingTo",
                suggests = "Suggests", enhances = "Enhances")
      direct_union <- unique(unlist(policies, use.names = FALSE))
      list(direct   = unname(caps[direct_union]),
           indirect = c("Depends", "Imports", "LinkingTo"))
    } else {
      NA
    }

    res <- pak::pkg_deps(refs_in, dependencies = dependencies, upgrade = TRUE)

    failed <- res[res$status != "OK", , drop = FALSE]
    if (nrow(failed))
      stop("pak could not resolve: ",
           paste(failed$ref, collapse = ", "), call. = FALSE)

    if (!is.null(package_result_file)) {
      primary <- unique(res$package[res$direct & res$ref == refs_in[[1L]]])
      if (length(primary) != 1L)
        stop("package ref must resolve to exactly one R package: ",
             refs_raw[[1L]], call. = FALSE)
      primary_package <- primary[[1L]]
    }

    # Prune the superset to each package's policy. Map every directly-requested
    # package to the dependency types to follow from it (unioning policies if a
    # package is requested more than once), then walk the closure keeping only
    # reachable packages.
    if (any_custom) {
      direct_policies <- list()
      for (i in seq_along(refs_in)) {
        for (p in unique(res$package[res$direct & res$ref == refs_in[[i]]])) {
          existing <- direct_policies[[p]]
          if (is.null(existing)) existing <- character()
          direct_policies[[p]] <- union(existing, policies[[i]])
        }
      }
      res <- res[res$package %in% ir_prune_to_policy(res, direct_policies), ,
                 drop = FALSE]
    }

    # Drop base / recommended packages: those are supplied by R itself.
    keep <- is.na(res$priority) | !(res$priority %in% c("base", "recommended"))
    res <- res[keep, , drop = FALSE]

    pkgs     <- res$package
    resolved <- sort(unique(sprintf("%s@%s", res$package, res$version)))
  } else {
    pkgs     <- character()
    resolved <- character()
    if (!is.null(package_result_file))
      stop("cannot resolve a primary package without dependencies",
           call. = FALSE)
  }

  ## 3. Hash the resolved set -> content-addressed library path
  # Bind the hash to the R version and platform: the symlinks point into the
  # renv cache, whose layout is itself keyed by R version and platform.
  key <- paste(c(resolved,
                 as.character(getRversion()),
                 R.version$platform),
               collapse = "\n")
  library_path <- file.path(cache_dir, "libraries", secretbase::sha256(key))

  ## 4. Materialise the symlinked library via renv::use()
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

  ## 4b. Record the resolution so an identical request skips pak.
  dir.create(dirname(marker), recursive = TRUE, showWarnings = FALSE)
  writeLines(library_path, marker)
  if (!is.null(primary_package)) {
    writeLines(primary_package, package_marker)
  }

  writeLines(library_path, result_file)
  if (!is.null(package_result_file)) {
    writeLines(primary_package, package_result_file)
  }
  invisible()
}

if (sys.nframe() == 0L) ir_resolve_main()

ir_count_files <- function(path) {
  if (!file.exists(path)) {
    return(0L)
  }

  info <- file.info(path)
  if (!isTRUE(info$isdir)) {
    return(1L)
  }

  length(list.files(
    path,
    all.files = TRUE,
    no.. = TRUE,
    recursive = TRUE,
    full.names = TRUE,
    include.dirs = FALSE
  ))
}

ir_clear_cache <- function(label, paths) {
  paths <- unique(paths[nzchar(paths)])

  for (path in paths) {
    if (!file.exists(path)) {
      cat("No ", label, " found at: ", path, "\n", sep = "")
      next
    }

    files <- ir_count_files(path)
    cat("Clearing ", label, " at: ", path, "\n", sep = "")
    unlink(path, recursive = TRUE, force = TRUE)
    if (file.exists(path)) {
      stop("failed to remove ", label, " `", path, "`", call. = FALSE)
    }
    cat(
      "Removed ", files, " ",
      if (identical(files, 1L)) "file" else "files",
      " from ", label, "\n",
      sep = ""
    )
  }
}

ir_split_paths <- function(paths) {
  if (!nzchar(paths)) {
    return(character())
  }

  pattern <- if (.Platform$OS.type == "windows") ";" else "[;:]"
  parts <- strsplit(paths, pattern)[[1L]]
  parts[nzchar(parts)]
}

ir_r_user_cache_dir <- function(package) {
  tools::R_user_dir(package, "cache")
}

ir_pkgcache_cache_dir <- function() {
  r_pkg_cache_dir <- Sys.getenv("R_PKG_CACHE_DIR", "")
  if (nzchar(r_pkg_cache_dir)) {
    return(file.path(r_pkg_cache_dir, "R", "pkgcache"))
  }

  ir_r_user_cache_dir("pkgcache")
}

ir_pak_cache_dir <- function() {
  r_pkg_cache_dir <- Sys.getenv("R_PKG_CACHE_DIR", "")
  if (nzchar(r_pkg_cache_dir)) {
    return(file.path(r_pkg_cache_dir, "lib"))
  }

  ir_r_user_cache_dir("pak")
}

ir_renv_cache_dirs <- function() {
  cache <- Sys.getenv("RENV_PATHS_CACHE", "")
  if (nzchar(cache)) {
    return(ir_split_paths(cache))
  }

  if (requireNamespace("renv", quietly = TRUE)) {
    return(renv::paths$root("cache"))
  }

  root <- Sys.getenv("RENV_PATHS_ROOT", "")
  if (nzchar(root)) {
    return(file.path(root, "cache"))
  }

  file.path(ir_r_user_cache_dir("renv"), "cache")
}

ir_reticulate_legacy_cache_dir <- function() {
  if (requireNamespace("rappdirs", quietly = TRUE)) {
    return(rappdirs::user_cache_dir("r-reticulate", NULL))
  }

  r_user_cache_dir <- Sys.getenv("R_USER_CACHE_DIR", "")
  if (nzchar(r_user_cache_dir)) {
    return(file.path(r_user_cache_dir, "r-reticulate"))
  }

  if (.Platform$OS.type == "windows") {
    local_app_data <- Sys.getenv("LOCALAPPDATA", "")
    if (nzchar(local_app_data)) {
      return(file.path(local_app_data, "r-reticulate", "Cache"))
    }

    return(file.path(
      Sys.getenv("USERPROFILE"),
      "Local Settings",
      "Application Data",
      "r-reticulate",
      "Cache"
    ))
  }

  if (identical(Sys.info()[["sysname"]], "Darwin")) {
    return(path.expand("~/Library/Caches/r-reticulate"))
  }

  file.path(Sys.getenv("XDG_CACHE_HOME", path.expand("~/.cache")), "r-reticulate")
}

ir_clear_cache("pak package cache", ir_pkgcache_cache_dir())
ir_clear_cache("pak cache", ir_pak_cache_dir())
ir_clear_cache("renv cache", ir_renv_cache_dirs())
ir_clear_cache("reticulate cache", ir_r_user_cache_dir("reticulate"))
ir_clear_cache("reticulate legacy cache", ir_reticulate_legacy_cache_dir())

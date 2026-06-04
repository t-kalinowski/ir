#!/usr/bin/env -S ir run
#| r-version: "4.4.3"
#| dependencies:
#|   - jsonlite
#| exclude-newer: "2026-06-01"

stopifnot(requireNamespace("jsonlite", quietly = TRUE))

lib <- normalizePath(.libPaths()[[1]], winslash = "/", mustWork = TRUE)
expected <- normalizePath(Sys.getenv("IR_EXPECT_CACHE_DIR"), winslash = "/", mustWork = FALSE)

cat("ir.fixture=r-version-frontmatter\n")
cat("version.r_version=", as.character(getRversion()), "\n", sep = "")
cat("version.lib_in_cache=", tolower(startsWith(lib, file.path(expected, "libraries"))), "\n", sep = "")

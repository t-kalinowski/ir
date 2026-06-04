#!/usr/bin/env -S ir run
#| dependencies:
#|   - glue
#| ---
#| dependencies:
#|   - definitelynotapackageir

library(glue)

lib <- normalizePath(.libPaths()[[1]], winslash = "/", mustWork = TRUE)
stopifnot(file.exists(file.path(lib, "glue", "DESCRIPTION")))

cat("ir.fixture=multi-doc\n")
cat(
  "multi.packages=glue:",
  tolower(requireNamespace("glue", quietly = TRUE)),
  "\n",
  sep = ""
)
cat(
  "multi.ignored_package=",
  tolower(requireNamespace("definitelynotapackageir", quietly = TRUE)),
  "\n",
  sep = ""
)
cat(glue::glue("multi.result={2 + 3}\n"))

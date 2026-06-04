#!/usr/bin/env -S ir run
#| dependencies:
#|   - dplyr>=1.0
#|   - tidyr
#|   - glue
#|   - jsonlite

pkgs <- c("dplyr", "tidyr", "glue", "jsonlite")
available <- vapply(pkgs, requireNamespace, logical(1), quietly = TRUE)
stopifnot(all(available))

lib <- normalizePath(.libPaths()[[1]], winslash = "/", mustWork = TRUE)
expected <- normalizePath(Sys.getenv("IR_EXPECT_CACHE_DIR"), winslash = "/", mustWork = FALSE)
stopifnot(all(file.exists(file.path(lib, pkgs, "DESCRIPTION"))))

data <- dplyr::tibble(group = c("a", "b", "a"), value = c(1L, 2L, 3L)) |>
  dplyr::group_by(group) |>
  dplyr::summarise(total = sum(value), .groups = "drop") |>
  tidyr::pivot_wider(names_from = group, values_from = total)

cat("ir.fixture=run-script\n")
cat("script.args=", paste(commandArgs(TRUE), collapse = "|"), "\n", sep = "")
cat("script.lib_in_cache=", tolower(startsWith(lib, file.path(expected, "libraries"))), "\n", sep = "")
cat("script.user_library=", Sys.getenv("R_LIBS_USER", unset = "<unset>"), "\n", sep = "")
cat("script.packages=", paste(names(available), tolower(available), sep = ":", collapse = ","), "\n", sep = "")
cat(glue::glue("script.result=a:{data$a},b:{data$b}"), "\n", sep = "")
cat("script.json=", jsonlite::toJSON(list(ok = TRUE, rows = nrow(data)), auto_unbox = TRUE), "\n", sep = "")

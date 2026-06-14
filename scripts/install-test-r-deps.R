packages <- c(
  "renv",
  "secretbase",
  "cli",
  "glue",
  "jsonlite",
  "dplyr",
  "tidyr",
  "reticulate",
  "fansi",
  "htmltools",
  "knitr",
  "rmarkdown",
  "quarto",
  "btw",
  "Rapp",
  "docopt",
  "pkgsearch",
  "prettyunits"
)

is_missing <- function(package) {
  !requireNamespace(package, quietly = TRUE)
}

missing <- packages[vapply(packages, is_missing, logical(1))]
if (length(missing)) {
  pak::pkg_install(packages)
}

still_missing <- packages[vapply(packages, is_missing, logical(1))]
if (length(still_missing)) {
  stop(
    "missing R packages after install: ",
    paste(still_missing, collapse = ", "),
    call. = FALSE
  )
}

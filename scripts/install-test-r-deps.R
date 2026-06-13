packages <- c(
  "pak",
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

repos <- c(
  CRAN = Sys.getenv(
    "IR_TEST_R_REPOS",
    "https://packagemanager.posit.co/cran/latest"
  )
)

is_missing <- function(package) {
  !requireNamespace(package, quietly = TRUE)
}

missing <- packages[vapply(packages, is_missing, logical(1))]
if (length(missing)) {
  install.packages(missing, repos = repos, Ncpus = 2)
}

still_missing <- packages[vapply(packages, is_missing, logical(1))]
if (length(still_missing)) {
  stop(
    "missing R packages after install: ",
    paste(still_missing, collapse = ", "),
    call. = FALSE
  )
}

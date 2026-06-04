#!/usr/bin/env -S ir run
#| dependencies:
#|   - dplyr==1.0.10
#|   - tidyr==1.2.1
#| exclude-newer: "2022-12-31"

library(dplyr)
library(tidyr)

stopifnot(packageVersion("dplyr") == package_version("1.0.10"))
stopifnot(packageVersion("tidyr") == package_version("1.2.1"))

orders <- dplyr::tibble(
  account_region = c("atlas|north", "atlas|south", "beacon|north", "beacon|south", "cedar|west"),
  product = c("compute", "storage", "compute", "support", "storage"),
  tickets = c(4L, 2L, 7L, 1L, 3L),
  revenue = c(1200, 800, 1800, 300, 950)
)

summary <- orders %>%
  tidyr::separate(account_region, into = c("account", "region"), sep = "\\|") %>%
  dplyr::group_by(account, region) %>%
  dplyr::summarise(
    tickets = sum(tickets),
    revenue = sum(revenue),
    .groups = "drop"
  ) %>%
  tidyr::pivot_wider(
    names_from = region,
    values_from = tickets,
    values_fill = 0
  )

cat("dialect=legacy tidyverse\n")
cat("exclude_newer=2022-12-31\n")
cat("dplyr=", as.character(packageVersion("dplyr")), "\n", sep = "")
cat("tidyr=", as.character(packageVersion("tidyr")), "\n", sep = "")
cat("library=", .libPaths()[[1]], "\n", sep = "")
print(summary)

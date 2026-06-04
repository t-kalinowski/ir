#| dependencies:
#|   - jsonlite
#| exclude-newer: "2026-06-01"

# Rendered by the `ir` integration tests through a real R install. Records facts
# about the R session so the test can prove `ir` injected the resolved library
# (via R_LIBS) and the selected R all the way into the running session. If `ir`
# failed to inject the resolved library, `jsonlite` would not be on .libPaths()
# and this script would error.
facts <- list(
  r_version = as.character(getRversion()),
  libpaths = .libPaths(),
  jsonlite_version = as.character(packageVersion("jsonlite"))
)

out <- Sys.getenv("IR_E2E_FACTS")
if (!nzchar(out)) stop("IR_E2E_FACTS is not set")
jsonlite::write_json(facts, out, auto_unbox = TRUE, pretty = TRUE)

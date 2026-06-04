# ir live demo

Run these commands from the repository root.

```console
ir run examples/live-demo/01-legacy-tidyverse.R
ir run examples/live-demo/02-modern-tidyverse.R
```

The first script resolves dplyr and tidyr from the 2022-12-31 Posit Package
Manager snapshot. The second resolves dplyr and tidyr from 2024-06-01. Both
scripts live in the same project and solve the same small data task, but they
use different tidyverse dialects.

Render the matching Quarto reports the same way:

```console
ir run examples/live-demo/03-legacy-tidyverse-report.qmd
ir run examples/live-demo/04-modern-tidyverse-report.qmd
```

Render the presentation:

```console
quarto render examples/live-demo/ir-live-demo.qmd
```

After the simple date-based flow, show explicit pak refs:

```console
ir run examples/live-demo/05-explicit-pak-refs.R
```

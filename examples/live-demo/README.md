# ir live demo

Run these commands from the repository root.

```console
ir run examples/live-demo/01-legacy-tidyverse.R
ir run examples/live-demo/02-modern-tidyverse.R
```

The first script resolves dplyr 1.0.10 and tidyr 1.2.1 from the 2022-12-31
Posit Package Manager snapshot. The second resolves dplyr 1.1.4 and tidyr 1.3.1
from 2024-06-01. Both scripts live in the same project and solve the same small
data task, but they use different tidyverse dialects.

Render the matching Quarto reports the same way:

```console
ir run examples/live-demo/03-legacy-tidyverse-report.qmd
ir run examples/live-demo/04-modern-tidyverse-report.qmd
```

Render the presentation:

```console
quarto render examples/live-demo/ir-live-demo.qmd
```

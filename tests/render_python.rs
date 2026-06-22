//! Python render integration tests for the public `ir` CLI.

mod support;

use support::*;

use std::fs;
#[cfg(unix)]
use std::path::Path;

#[cfg(unix)]
#[test]
fn render_quarto_ir_python_frontmatter_sets_quarto_python() {
    let cache_dir = temp_dir("ir-render-python-cache");
    let bin_dir = temp_dir("ir-render-python-bin");
    let doc = temp_path("ir-render-python", "qmd");
    let fake_python = bin_dir.join("python");
    let rscript = bin_dir.join("Rscript");
    let quarto = bin_dir.join("quarto");
    let r_deps = temp_path("ir-render-python-r-deps", "txt");
    let r_driver = temp_path("ir-render-python-r-driver", "txt");
    let py_driver = temp_path("ir-render-python-py-driver", "txt");
    let python_packages = temp_path("ir-render-python-packages", "txt");
    let python_env = temp_path("ir-render-python-env", "txt");

    fs::write(
        &doc,
        r#"---
title: python render
format: html
jupyter: python3
ir:
  python-packages:
    - pandas
  python-version: "3.11"
  exclude-newer: "2026-06-01"
---

```{python}
print("ok")
```
"#,
    )
    .unwrap();
    write_executable(&fake_python, "#!/bin/sh\nexit 0\n");
    write_executable(
        &rscript,
        &format!(
            "#!/bin/sh\n\
if [ -n \"${{IR_RESOLVE_RESULT_FILE:-}}\" ]; then\n\
  printf '%s\\n' \"$1\" > {}\n\
  printf 'exclude_newer=%s\\n' \"${{IR_EXCLUDE_NEWER:-}}\" > {}\n\
  cat >> {}\n\
  mkdir -p \"$IR_CACHE_DIR/fake-library\"\n\
  printf '%s\\n' \"$IR_CACHE_DIR/fake-library\" > \"$IR_RESOLVE_RESULT_FILE\"\n\
  exit 0\n\
fi\n\
if [ -n \"${{IR_PYTHON_RESULT_FILE:-}}\" ]; then\n\
  printf '%s\\n' \"$1\" > {}\n\
  if grep -q 'ir_ensure_python_pak' \"$1\"; then\n\
    echo python resolver should use shared tooling bootstrap >&2\n\
    exit 1\n\
  fi\n\
  if grep -q 'ir_ensure_python_tooling' \"$1\"; then\n\
    echo python resolver should add reticulate through shared tooling >&2\n\
    exit 1\n\
  fi\n\
  if ! grep -q 'ir_ensure_tooling' \"$1\"; then\n\
    echo python resolver should include shared tooling bootstrap >&2\n\
    exit 1\n\
  fi\n\
  cat > {}\n\
  printf 'python_version=%s\\n' \"${{IR_PYTHON_VERSION:-}}\" > {}\n\
  printf 'exclude_newer=%s\\n' \"${{IR_PYTHON_EXCLUDE_NEWER:-}}\" >> {}\n\
  printf '%s\\n' {} > \"$IR_PYTHON_RESULT_FILE\"\n\
  exit 0\n\
fi\n\
echo unexpected Rscript invocation >&2\n\
exit 1\n",
            r_driver.display(),
            r_deps.display(),
            r_deps.display(),
            py_driver.display(),
            python_packages.display(),
            python_env.display(),
            python_env.display(),
            fake_python.display()
        ),
    );
    write_executable(
        &quarto,
        "#!/bin/sh\nprintf 'quarto_python=%s\\n' \"$QUARTO_PYTHON\"\nprintf 'reticulate_python=%s\\n' \"$RETICULATE_PYTHON\"\n",
    );
    let expected_driver_dir = cache_dir.join("drivers");
    let stale_r_driver = expected_driver_dir.join("resolve.R");
    let stale_py_driver = expected_driver_dir.join("resolve-python.R");
    fs::create_dir_all(&expected_driver_dir).unwrap();
    fs::write(&stale_r_driver, "stale\n").unwrap();
    fs::write(&stale_py_driver, "stale\n").unwrap();
    let mut permissions = fs::metadata(&stale_r_driver).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&stale_r_driver, permissions).unwrap();
    let mut permissions = fs::metadata(&stale_py_driver).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&stale_py_driver, permissions).unwrap();

    let out = ir()
        .env("IR_CACHE_DIR", &cache_dir)
        .env("IR_QUARTO", &quarto)
        .args(["render", "--rscript"])
        .arg(&rscript)
        .arg(&doc)
        .output()
        .unwrap();

    assert_success(&out);
    assert_stdout_contains(&out, &format!("quarto_python={}", fake_python.display()));
    assert_stdout_contains(
        &out,
        &format!("reticulate_python={}", fake_python.display()),
    );

    let deps = fs::read_to_string(&r_deps).unwrap();
    assert!(deps.contains("exclude_newer=2026-06-01"), "{deps}");
    assert!(
        !deps.lines().any(|line| line == "reticulate"),
        "Python-only frontmatter should not inject user-library reticulate\n{deps}"
    );
    let r_driver_path = Path::new(fs::read_to_string(&r_driver).unwrap().trim()).to_path_buf();
    let py_driver_path = Path::new(fs::read_to_string(&py_driver).unwrap().trim()).to_path_buf();
    assert!(r_driver_path.starts_with(&expected_driver_dir));
    assert!(py_driver_path.starts_with(&expected_driver_dir));
    assert_ne!(r_driver_path, stale_r_driver);
    assert_ne!(py_driver_path, stale_py_driver);
    let r_driver_file = r_driver_path.file_name().unwrap().to_string_lossy();
    let py_driver_file = py_driver_path.file_name().unwrap().to_string_lossy();
    assert!(
        r_driver_file.starts_with("resolve-") && r_driver_file.ends_with(".R"),
        "{r_driver_file}"
    );
    assert!(
        py_driver_file.starts_with("resolve-python-") && py_driver_file.ends_with(".R"),
        "{py_driver_file}"
    );
    assert!(fs::read_to_string(&r_driver_path)
        .unwrap()
        .contains("ir_ensure_tooling"));
    assert!(fs::read_to_string(&py_driver_path)
        .unwrap()
        .contains("ir_ensure_tooling"));
    assert!(fs::metadata(&r_driver_path)
        .unwrap()
        .permissions()
        .readonly());
    assert!(fs::metadata(&py_driver_path)
        .unwrap()
        .permissions()
        .readonly());

    let packages = fs::read_to_string(&python_packages).unwrap();
    assert!(packages.contains("pandas"), "{packages}");
    assert!(packages.contains("jupyter"), "{packages}");

    let env = fs::read_to_string(&python_env).unwrap();
    assert!(env.contains("python_version=3.11"), "{env}");
    assert!(env.contains("exclude_newer=2026-06-01"), "{env}");
}

#[cfg(unix)]
#[test]
fn render_quarto_mixed_r_python_frontmatter_sets_python_env_vars() {
    let cache_dir = temp_dir("ir-render-mixed-python-cache");
    let bin_dir = temp_dir("ir-render-mixed-python-bin");
    let doc = temp_path("ir-render-mixed-python", "qmd");
    let fake_python = bin_dir.join("python");
    let rscript = bin_dir.join("Rscript");
    let quarto = bin_dir.join("quarto");
    let r_deps = temp_path("ir-render-mixed-python-r-deps", "txt");
    let python_env = temp_path("ir-render-mixed-python-env", "txt");

    fs::write(
        &doc,
        r#"---
title: mixed python render
format: html
ir:
  packages:
    - reticulate
  python-packages:
    - pandas
  python-version: "3.11"
  exclude-newer: "2026-06-01"
---

```{r}
reticulate::py_config()
```
"#,
    )
    .unwrap();
    write_executable(&fake_python, "#!/bin/sh\nexit 0\n");
    write_executable(
        &rscript,
        &format!(
            "#!/bin/sh\n\
if [ -n \"${{IR_RESOLVE_RESULT_FILE:-}}\" ]; then\n\
  printf 'exclude_newer=%s\\n' \"${{IR_EXCLUDE_NEWER:-}}\" > {}\n\
  cat >> {}\n\
  mkdir -p \"$IR_CACHE_DIR/fake-library\"\n\
  printf '%s\\n' \"$IR_CACHE_DIR/fake-library\" > \"$IR_RESOLVE_RESULT_FILE\"\n\
  exit 0\n\
fi\n\
if [ -n \"${{IR_PYTHON_RESULT_FILE:-}}\" ]; then\n\
  cat > /dev/null\n\
  printf 'python_version=%s\\n' \"${{IR_PYTHON_VERSION:-}}\" > {}\n\
  printf 'exclude_newer=%s\\n' \"${{IR_PYTHON_EXCLUDE_NEWER:-}}\" >> {}\n\
  printf '%s\\n' {} > \"$IR_PYTHON_RESULT_FILE\"\n\
  exit 0\n\
fi\n\
echo unexpected Rscript invocation >&2\n\
exit 1\n",
            r_deps.display(),
            r_deps.display(),
            python_env.display(),
            python_env.display(),
            fake_python.display()
        ),
    );
    write_executable(
        &quarto,
        "#!/bin/sh\nprintf 'quarto_python=%s\\n' \"${QUARTO_PYTHON:-}\"\nprintf 'reticulate_python=%s\\n' \"${RETICULATE_PYTHON:-}\"\n",
    );

    let out = ir()
        .env("IR_CACHE_DIR", &cache_dir)
        .env("IR_QUARTO", &quarto)
        .args(["render", "--rscript"])
        .arg(&rscript)
        .arg(&doc)
        .output()
        .unwrap();

    assert_success(&out);
    assert_stdout_contains(&out, &format!("quarto_python={}", fake_python.display()));
    assert_stdout_contains(
        &out,
        &format!("reticulate_python={}", fake_python.display()),
    );

    let deps = fs::read_to_string(&r_deps).unwrap();
    assert!(deps.contains("exclude_newer=2026-06-01"), "{deps}");
    assert!(deps.lines().any(|line| line == "reticulate"), "{deps}");

    let env = fs::read_to_string(&python_env).unwrap();
    assert!(env.contains("python_version=3.11"), "{env}");
    assert!(env.contains("exclude_newer=2026-06-01"), "{env}");
}

#[cfg(unix)]
#[test]
fn render_quarto_ir_python_frontmatter_passes_exclude_newer_override_raw() {
    let cache_dir = temp_dir("ir-render-python-env-cache");
    let bin_dir = temp_dir("ir-render-python-env-bin");
    let doc = temp_path("ir-render-python-env", "qmd");
    let fake_python = bin_dir.join("python");
    let rscript = bin_dir.join("Rscript");
    let quarto = bin_dir.join("quarto");
    let python_env = temp_path("ir-render-python-env-observed", "txt");

    fs::write(
        &doc,
        r#"---
title: python render env
format: html
jupyter: python3
ir:
  python-packages:
    - pandas
  exclude-newer: "2026-06-01"
---
"#,
    )
    .unwrap();
    write_executable(&fake_python, "#!/bin/sh\nexit 0\n");
    write_executable(
        &rscript,
        &format!(
            "#!/bin/sh\n\
if [ -n \"${{IR_RESOLVE_RESULT_FILE:-}}\" ]; then\n\
  if [ -n \"${{IR_EXCLUDE_NEWER:-}}\" ]; then\n\
    echo \"unexpected R exclude-newer: $IR_EXCLUDE_NEWER\" >&2\n\
    exit 1\n\
  fi\n\
  cat > /dev/null\n\
  mkdir -p \"$IR_CACHE_DIR/fake-library\"\n\
  printf '%s\\n' \"$IR_CACHE_DIR/fake-library\" > \"$IR_RESOLVE_RESULT_FILE\"\n\
  exit 0\n\
fi\n\
if [ -n \"${{IR_PYTHON_RESULT_FILE:-}}\" ]; then\n\
  cat > /dev/null\n\
  printf 'python_version=%s\\n' \"${{IR_PYTHON_VERSION:-}}\" > {}\n\
  printf 'exclude_newer=%s\\n' \"${{IR_PYTHON_EXCLUDE_NEWER:-}}\" >> {}\n\
  printf '%s\\n' {} > \"$IR_PYTHON_RESULT_FILE\"\n\
  exit 0\n\
fi\n\
echo unexpected Rscript invocation >&2\n\
exit 1\n",
            python_env.display(),
            python_env.display(),
            fake_python.display()
        ),
    );
    write_executable(&quarto, "#!/bin/sh\nexit 0\n");

    let out = ir()
        .env("IR_CACHE_DIR", &cache_dir)
        .env("IR_QUARTO", &quarto)
        .env("IR_PYTHON_VERSION", "9.99")
        .env("IR_PYTHON_EXCLUDE_NEWER", "1999-01-01")
        .env("IR_EXCLUDE_NEWER", " \t ")
        .args(["render", "--rscript"])
        .arg(&rscript)
        .arg(&doc)
        .output()
        .unwrap();

    assert_success(&out);
    let env = fs::read_to_string(&python_env).unwrap();
    assert!(env.contains("python_version=\n"), "{env}");
    assert!(env.contains("exclude_newer= \t \n"), "{env}");
}

#[cfg(unix)]
#[test]
fn render_quarto_ignores_legacy_top_level_uv_frontmatter() {
    let cache_dir = temp_dir("ir-render-legacy-uv-cache");
    let bin_dir = temp_dir("ir-render-legacy-uv-bin");
    let doc = temp_path("ir-render-legacy-uv", "qmd");
    let rscript = bin_dir.join("Rscript");
    let quarto = bin_dir.join("quarto");

    fs::write(
        &doc,
        r#"---
title: legacy uv render
format: html
jupyter: python3
uv:
  packages:
    - pandas
---
"#,
    )
    .unwrap();
    write_executable(
        &rscript,
        "#!/bin/sh\n\
if [ -n \"${IR_RESOLVE_RESULT_FILE:-}\" ]; then\n\
  cat > /dev/null\n\
  mkdir -p \"$IR_CACHE_DIR/fake-library\"\n\
  printf '%s\\n' \"$IR_CACHE_DIR/fake-library\" > \"$IR_RESOLVE_RESULT_FILE\"\n\
  exit 0\n\
fi\n\
if [ -n \"${IR_PYTHON_RESULT_FILE:-}\" ]; then\n\
  echo legacy uv frontmatter should not trigger Python resolution >&2\n\
  exit 1\n\
fi\n\
echo unexpected Rscript invocation >&2\n\
exit 1\n",
    );
    write_executable(
        &quarto,
        "#!/bin/sh\nprintf 'quarto_python=%s\\n' \"${QUARTO_PYTHON:-}\"\n",
    );

    let out = ir()
        .env("IR_CACHE_DIR", &cache_dir)
        .env("IR_QUARTO", &quarto)
        .args(["render", "--rscript"])
        .arg(&rscript)
        .arg(&doc)
        .output()
        .unwrap();

    assert_success(&out);
    assert_stdout_contains(&out, "quarto_python=\n");
}

#[cfg(unix)]
#[test]
fn render_quarto_accepts_r_and_python_version_frontmatter() {
    let cache_dir = temp_dir("ir-render-r-python-version-cache");
    let bin_dir = temp_dir("ir-render-r-python-version-bin");
    let doc = temp_path("ir-render-r-python-version", "qmd");
    let fake_python = bin_dir.join("python");
    let rscript = bin_dir.join("Rscript");
    let quarto = bin_dir.join("quarto");
    let python_env = temp_path("ir-render-r-python-version-env", "txt");

    fs::write(
        &doc,
        r#"---
title: version pins
ir:
  r-version: "4.4"
  python-packages:
    - pandas
  python-version: "3.11"
---
"#,
    )
    .unwrap();
    write_executable(&fake_python, "#!/bin/sh\nexit 0\n");
    write_executable(
        &rscript,
        &format!(
            "#!/bin/sh\n\
if [ -n \"${{IR_RESOLVE_RESULT_FILE:-}}\" ]; then\n\
  cat > /dev/null\n\
  mkdir -p \"$IR_CACHE_DIR/fake-library\"\n\
  printf '%s\\n' \"$IR_CACHE_DIR/fake-library\" > \"$IR_RESOLVE_RESULT_FILE\"\n\
  exit 0\n\
fi\n\
if [ -n \"${{IR_PYTHON_RESULT_FILE:-}}\" ]; then\n\
  cat > /dev/null\n\
  printf 'python_version=%s\\n' \"${{IR_PYTHON_VERSION:-}}\" > {}\n\
  printf '%s\\n' {} > \"$IR_PYTHON_RESULT_FILE\"\n\
  exit 0\n\
fi\n\
echo unexpected Rscript invocation >&2\n\
exit 1\n",
            python_env.display(),
            fake_python.display()
        ),
    );
    write_executable(&quarto, "#!/bin/sh\nexit 0\n");

    let out = ir()
        .env("IR_CACHE_DIR", &cache_dir)
        .env("IR_QUARTO", &quarto)
        .args(["render", "--rscript"])
        .arg(&rscript)
        .arg(&doc)
        .output()
        .unwrap();

    assert_success(&out);
    let env = fs::read_to_string(&python_env).unwrap();
    assert!(env.contains("python_version=3.11"), "{env}");
}

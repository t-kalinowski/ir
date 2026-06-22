#![cfg(unix)]

//! Resolver tooling integration tests for the public `ir` CLI.

mod support;

use support::*;

use std::fs;
use std::path::Path;

fn resolver_tooling_fixture_source() -> String {
    format!("source({})", r_string(&fixture("resolver-tooling.R")))
}

fn assert_pak_installed_resolver_tooling(install_marker: &Path, pak_marker: &Path) {
    let install_packages = fs::read_to_string(install_marker).unwrap();
    assert_eq!(install_packages.trim(), "pak");

    let pak_refs = fs::read_to_string(pak_marker).unwrap();
    assert!(pak_refs.lines().any(|line| line == "renv"), "{pak_refs}");
    assert!(
        pak_refs.lines().any(|line| line == "secretbase"),
        "{pak_refs}"
    );
    assert!(!pak_refs.lines().any(|line| line == "pak"), "{pak_refs}");
}

#[test]
fn resolver_tooling_uses_compatible_user_library_packages() {
    let cache_dir = temp_dir("ir-compatible-tooling-cache");
    let user_library = temp_dir("ir-compatible-tooling-user-library");
    let fake_load_marker = temp_path("ir-compatible-secretbase-loaded", "txt");
    let profile = temp_path("ir-compatible-tooling-profile", "R");

    fs::write(
        &profile,
        format!(
            r#"
{}
ir_test_write_secretbase(Sys.getenv("R_LIBS_USER"), marker = {})
ir_test_write_pak(Sys.getenv("R_LIBS_USER"))
ir_test_write_renv(Sys.getenv("R_LIBS_USER"))

utils::assignInNamespace("install.packages", function(...) {{
  stop("resolver should use compatible R_LIBS_USER tooling", call. = FALSE)
}}, ns = "utils")
"#,
            resolver_tooling_fixture_source(),
            r_string(&fake_load_marker)
        ),
    )
    .unwrap();

    let out = ir()
        .env("IR_CACHE_DIR", &cache_dir)
        .env("R_LIBS_USER", &user_library)
        .env("R_PROFILE_USER", &profile)
        .args([
            "run",
            "--isolated",
            "--with",
            "cli",
            "--vanilla",
            "-e",
            "cat('ir.fixture=compatible-tooling\\n')",
        ])
        .output()
        .unwrap();

    assert_success(&out);
    assert_stdout_contains(&out, "ir.fixture=compatible-tooling");
    assert!(
        fake_load_marker.exists(),
        "resolver should load compatible secretbase from R_LIBS_USER"
    );
}

#[test]
fn resolver_tooling_bootstraps_only_pak_with_install_packages() {
    let cache_dir = temp_dir("ir-pak-tooling-cache");
    let empty_library = temp_dir("ir-pak-tooling-empty-library");
    let install_marker = temp_path("ir-pak-tooling-install", "txt");
    let pak_marker = temp_path("ir-pak-tooling-pak", "txt");
    let profile = temp_path("ir-pak-tooling-profile", "R");

    fs::write(
        &profile,
        format!(
            r#"
{}
.libPaths(Sys.getenv("IR_TEST_EMPTY_LIB"))

utils::assignInNamespace("install.packages", function(pkgs, lib, repos, ...) {{
  writeLines(as.character(pkgs), {})
  if (!identical(as.character(pkgs), "pak"))
    stop("resolver should bootstrap only pak with install.packages",
         call. = FALSE)
  ir_test_write_pak(
    lib,
    namespace = "export(pkg_deps)\nexport(pkg_install)",
    code = ir_test_fake_pak_code(install_marker = {})
  )
}}, ns = "utils")
"#,
            resolver_tooling_fixture_source(),
            r_string(&install_marker),
            r_string(&pak_marker)
        ),
    )
    .unwrap();

    let out = ir()
        .env("IR_CACHE_DIR", &cache_dir)
        .env("IR_TEST_EMPTY_LIB", &empty_library)
        .env("R_LIBS_SITE", &empty_library)
        .env("R_LIBS_USER", &empty_library)
        .env("R_PROFILE_USER", &profile)
        .args([
            "run",
            "--isolated",
            "--with",
            "cli",
            "--vanilla",
            "-e",
            "cat('ir.fixture=pak-tooling\\n')",
        ])
        .output()
        .unwrap();

    assert_success(&out);
    assert_stdout_contains(&out, "ir.fixture=pak-tooling");
    assert_pak_installed_resolver_tooling(&install_marker, &pak_marker);
}

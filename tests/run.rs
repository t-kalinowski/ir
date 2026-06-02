use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn run_sets_resolved_library_on_r_libs_and_forwards_script_args() {
    let temp = unique_temp_dir("ir-run-test");
    fs::create_dir_all(&temp).unwrap();

    let bin_dir = temp.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();

    let script = temp.join("script.R");
    fs::write(
        &script,
        "#!/usr/bin/env -S ir run\n# dependencies:\n#   - cli\n# R: >= 4.0\n\ncat('ok')\n",
    )
    .unwrap();

    let log = temp.join("rscript.log");
    let resolved_library = temp.join("resolved-library");
    let fake_rscript = bin_dir.join("Rscript");
    fs::write(&fake_rscript, fake_rscript_source(&log, &resolved_library)).unwrap();
    let mut permissions = fs::metadata(&fake_rscript).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_rscript, permissions).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ir"))
        .arg("run")
        .arg(&script)
        .arg("--flag")
        .arg("value")
        .env("PATH", prepend_path(&bin_dir))
        .env("IR_CACHE_DIR", temp.join("cache"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let log = fs::read_to_string(&log).unwrap();
    assert!(log.contains(&format!("R_LIBS={}", resolved_library.display())));
    assert!(log.contains(&format!("SCRIPT={}", script.display())));
    assert!(log.contains("ARGS=--flag value"));

    let _ = fs::remove_dir_all(temp);
}

fn fake_rscript_source(log: &Path, resolved_library: &Path) -> String {
    format!(
        r#"#!/bin/sh
set -eu
case "$(basename "$2")" in
  ir-resolve-*.R)
    printf 'IR_LIBRARY_PATH={resolved_library}
'
    exit 0
    ;;
esac
printf 'R_LIBS=%s
' "$R_LIBS" > "{log}"
printf 'SCRIPT=%s
' "$2" >> "{log}"
shift 2
printf 'ARGS=%s
' "$*" >> "{log}"
exit 0
"#,
        log = log.display(),
        resolved_library = resolved_library.display()
    )
}

fn prepend_path(dir: &Path) -> OsString {
    let old = env::var_os("PATH").unwrap_or_default();
    let mut path = OsString::from(dir);
    path.push(":");
    path.push(old);
    path
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}

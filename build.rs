use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=IR_RIG_AVAILABLE_REFRESH");

    let output = Command::new("rig")
        .args(["available", "--all", "--json"])
        .output()
        .unwrap_or_else(|e| panic!("failed to launch `rig available --all --json`: {e}"));

    if !output.status.success() {
        panic!(
            "`rig available --all --json` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let json = String::from_utf8(output.stdout)
        .expect("`rig available --all --json` returned non-UTF-8 output");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    fs::write(out_dir.join("rig_available_all.json"), json)
        .expect("failed to write embedded rig availability JSON");

    println!(
        "cargo:rustc-env=IR_RIG_AVAILABLE_BUILD_DATE={}",
        current_utc_date()
    );
}

fn current_utc_date() -> String {
    let output = Command::new("Rscript")
        .args(["--vanilla", "-e", "cat(as.character(Sys.Date()))"])
        .output()
        .unwrap_or_else(|e| panic!("failed to launch `Rscript` for build date: {e}"));

    if !output.status.success() {
        panic!(
            "`Rscript --vanilla -e cat(as.character(Sys.Date()))` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let date = String::from_utf8(output.stdout)
        .expect("`Rscript --vanilla -e cat(as.character(Sys.Date()))` returned non-UTF-8 output")
        .trim()
        .to_string();
    if !is_iso_date(&date) {
        panic!("`Rscript --vanilla -e cat(as.character(Sys.Date()))` returned invalid build date `{date}`");
    }

    date
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && [0, 1, 2, 3, 5, 6, 8, 9]
            .into_iter()
            .all(|i| bytes[i].is_ascii_digit())
        && (1..=12).contains(&two_digit_number(&bytes[5..7]))
        && (1..=31).contains(&two_digit_number(&bytes[8..10]))
}

fn two_digit_number(bytes: &[u8]) -> u8 {
    (bytes[0] - b'0') * 10 + bytes[1] - b'0'
}

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_secs()
        / 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    (year, month as u32, day as u32)
}

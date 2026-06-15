use std::error::Error;
use std::io;
use std::process::{Command, Stdio};

use super::model::{normalize_available_release_dates, AvailableR, InstalledR, InstalledRRecord};

pub(super) fn rig_available() -> Result<Vec<AvailableR>, Box<dyn Error>> {
    let output = rig_output(&["available", "--all", "--json"])?;
    let json = clean_rig_json_output(&output)?;
    parse_rig_available_json(&json)
}

pub(super) fn rig_list() -> Result<Vec<InstalledR>, Box<dyn Error>> {
    let output = rig_output(&["list", "--json"])?;
    let json = clean_rig_json_output(&output)?;
    let versions: Vec<InstalledRRecord> = serde_json::from_str(&json)
        .map_err(|e| format!("failed to parse `rig list --json` JSON: {e}"))?;
    Ok(versions
        .into_iter()
        .filter_map(InstalledR::from_record)
        .collect())
}

fn parse_rig_available_json(json: &str) -> Result<Vec<AvailableR>, Box<dyn Error>> {
    let mut versions: Vec<AvailableR> = serde_json::from_str(json)
        .map_err(|e| format!("failed to parse `rig available --json` JSON: {e}"))?;

    normalize_available_release_dates(&mut versions)?;
    Ok(versions)
}

fn clean_rig_json_output(output: &[u8]) -> Result<String, Box<dyn Error>> {
    let output = String::from_utf8(output.to_vec())
        .map_err(|e| format!("`rig --json` returned non-UTF-8 output: {e}"))?;
    Ok(output
        .lines()
        .filter(|line| !line.starts_with("[INFO]"))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn rig_output(args: &[&str]) -> Result<Vec<u8>, Box<dyn Error>> {
    let output = Command::new("rig")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                "could not find `rig` on PATH. Install rig to use `r-version` or `exclude-newer`."
                    .to_string()
            } else {
                format!("failed to launch `rig`: {e}")
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("`rig {}` failed: {stderr}", args.join(" ")).into());
    }

    Ok(output.stdout)
}

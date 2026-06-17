use std::error::Error;
use std::fs;
use std::process::Command;

use super::r_selection::{self, AvailableCandidate, VersionRequirement};
use super::release_metadata::{parse_minor_releases_json, MinorRelease};
use super::rig_client::{self, AvailableR};

const R_VERSIONS_URL: &str = "https://api.r-hub.io/rversions/r-versions";

include!(concat!(env!("OUT_DIR"), "/r_version_minor_releases.rs"));

pub(crate) fn required_available_version(
    req: &str,
    requirement: &VersionRequirement,
    exclude_newer: Option<&str>,
) -> Result<AvailableR, Box<dyn Error>> {
    if let Some(exclude_newer) = exclude_newer {
        if exclude_newer <= EMBEDDED_AVAILABLE_METADATA_DATE {
            return required_available_version_from_candidates(
                req,
                requirement,
                Some(exclude_newer),
                EMBEDDED_MINOR_RELEASES.iter().map(AvailableCandidate::from),
            );
        }

        let available = cached_minor_releases()?;
        return required_available_version_from_candidates(
            req,
            requirement,
            Some(exclude_newer),
            available.iter().map(AvailableCandidate::from),
        );
    }

    let available = rig_client::available()?;
    required_available_version_from_candidates(
        req,
        requirement,
        None,
        available.iter().map(AvailableCandidate::from),
    )
}

fn required_available_version_from_candidates<'a>(
    req: &str,
    requirement: &VersionRequirement,
    exclude_newer: Option<&str>,
    candidates: impl IntoIterator<Item = AvailableCandidate<'a>>,
) -> Result<AvailableR, Box<dyn Error>> {
    r_selection::select_available_candidate(req, requirement, exclude_newer, candidates)
        .map(AvailableR::from)
}

fn cached_minor_releases() -> Result<Vec<MinorRelease<'static>>, Box<dyn Error>> {
    let path = crate::runtime::ir_cache_dir()?
        .join("r-versions")
        .join("available.json");
    if path.exists() {
        let json = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read `{}`: {e}", path.display()))?;
        return parse_minor_releases_json(&json, "R version availability metadata cache")
            .map_err(|e| -> Box<dyn Error> { e.into() });
    }

    let json = download_available_json()?;
    let available = parse_minor_releases_json(&json, "R version availability metadata")
        .map_err(|e| -> Box<dyn Error> { e.into() })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create `{}`: {e}", parent.display()))?;
    }
    fs::write(&path, json).map_err(|e| format!("failed to write `{}`: {e}", path.display()))?;
    Ok(available)
}

fn download_available_json() -> Result<String, Box<dyn Error>> {
    let output = Command::new("Rscript")
        .args([
            "--vanilla",
            "-e",
            "cat(readLines(commandArgs(TRUE)[[1]], warn = FALSE), sep = \"\\n\")",
            R_VERSIONS_URL,
        ])
        .output()
        .map_err(|e| {
            format!("failed to launch `Rscript` for R version availability metadata: {e}")
        })?;

    if !output.status.success() {
        return Err(format!(
            "`Rscript --vanilla -e <read R version availability metadata>` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let json = String::from_utf8(output.stdout)
        .map_err(|e| format!("R version availability metadata response was not UTF-8: {e}"))?;
    if json.trim().is_empty() {
        return Err("R version availability metadata response was empty".into());
    }

    Ok(json)
}

impl<'a> From<&'a AvailableR> for AvailableCandidate<'a> {
    fn from(value: &'a AvailableR) -> Self {
        Self {
            name: &value.name,
            version: &value.version,
            date: value.date.as_deref(),
        }
    }
}

impl<'a, 'b> From<&'a MinorRelease<'b>> for AvailableCandidate<'a> {
    fn from(value: &'a MinorRelease<'b>) -> Self {
        Self {
            name: value.version.as_ref(),
            version: value.version.as_ref(),
            date: Some(value.date.as_ref()),
        }
    }
}

impl From<AvailableCandidate<'_>> for AvailableR {
    fn from(value: AvailableCandidate<'_>) -> Self {
        Self {
            name: value.name.to_string(),
            version: value.version.to_string(),
            date: value.date.map(str::to_string),
        }
    }
}

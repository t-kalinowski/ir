use std::error::Error;
use std::fs;

use super::r_selection::{self, AvailableCandidate, VersionRequirement};
use super::release_metadata::{parse_release_metadata_json, ReleaseMetadata};
use super::rig_client::{self, AvailableR};

include!(concat!(env!("OUT_DIR"), "/r_version_releases.rs"));

pub(crate) fn required_available_version(
    req: &str,
    requirement: &VersionRequirement,
    exclude_newer: Option<&str>,
) -> Result<AvailableR, Box<dyn Error>> {
    if let Some(exclude_newer) = exclude_newer {
        if exclude_newer <= EMBEDDED_AVAILABLE_METADATA_DATE
            && embedded_release_metadata_matches_host()
        {
            return required_available_version_from_candidates(
                req,
                requirement,
                Some(exclude_newer),
                EMBEDDED_R_RELEASES.iter().map(AvailableCandidate::from),
            );
        }

        let available = cached_release_metadata()?;
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

fn cached_release_metadata() -> Result<Vec<ReleaseMetadata<'static>>, Box<dyn Error>> {
    let path = crate::runtime::ir_cache_dir()?
        .join("r-versions")
        .join("available.json");
    if path.exists() {
        let json = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read `{}`: {e}", path.display()))?;
        return parse_release_metadata_json(&json, "R version availability metadata cache")
            .map_err(|e| -> Box<dyn Error> { e.into() });
    }

    let json = download_available_json()?;
    let available = parse_release_metadata_json(&json, "R version availability metadata")
        .map_err(|e| -> Box<dyn Error> { e.into() })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create `{}`: {e}", parent.display()))?;
    }
    fs::write(&path, json).map_err(|e| format!("failed to write `{}`: {e}", path.display()))?;
    Ok(available)
}

fn download_available_json() -> Result<String, Box<dyn Error>> {
    let json = String::from_utf8(rig_client::output(&["available", "--json", "--all"])?)
        .map_err(|e| format!("`rig available --json --all` returned non-UTF-8 output: {e}"))?;
    if json.trim().is_empty() {
        return Err("`rig available --json --all` returned empty output".into());
    }

    Ok(json)
}

fn embedded_release_metadata_matches_host() -> bool {
    EMBEDDED_AVAILABLE_METADATA_TARGET
        .map(|target| current_rig_target().as_deref() == Some(target))
        .unwrap_or(false)
}

fn current_rig_target() -> Option<String> {
    let platform = match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        "linux" => return current_linux_rig_target(),
        _ => return None,
    }
    .to_string();
    let arch = match (platform.as_str(), std::env::consts::ARCH) {
        ("macos", "aarch64" | "arm64") => "arm64",
        (_, "arm64") => "aarch64",
        (_, arch) => arch,
    };

    Some(format!("{platform}/{arch}"))
}

fn current_linux_rig_target() -> Option<String> {
    let os_release = fs::read_to_string("/etc/os-release").ok()?;
    let id = os_release_field(&os_release, "ID")?;
    let version = os_release_field(&os_release, "VERSION_ID")?;
    let arch = match std::env::consts::ARCH {
        "arm64" => "aarch64",
        arch => arch,
    };

    Some(format!("linux-{id}-{version}/{arch}"))
}

fn os_release_field(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (field, value) = line.split_once('=')?;
        if field == key {
            Some(value.trim_matches('"').to_string())
        } else {
            None
        }
    })
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

impl<'a, 'b> From<&'a ReleaseMetadata<'b>> for AvailableCandidate<'a> {
    fn from(value: &'a ReleaseMetadata<'b>) -> Self {
        Self {
            name: value.name.as_ref(),
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

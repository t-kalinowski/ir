use std::error::Error;
use std::fs;

use super::r_selection::{self, AvailableCandidate, VersionRequirement};
use super::release_metadata::{parse_release_metadata_json, ReleaseMetadata};
use super::rig_client::{self, AvailableR};

const EMBEDDED_AVAILABLE_METADATA_DATE: &str = include_str!("r-versions-fetched-at.txt");
const EMBEDDED_R_RELEASES: &str = include_str!("r-versions.json");

pub(crate) fn required_available_version(
    req: &str,
    requirement: &VersionRequirement,
    exclude_newer: Option<&str>,
) -> Result<AvailableR, Box<dyn Error>> {
    if let Some(exclude_newer) = exclude_newer {
        if requirement_uses_symbolic_name(requirement) {
            return required_available_version_from_host(req, requirement, Some(exclude_newer));
        }
        if exclude_newer <= embedded_available_metadata_date()? {
            let available = embedded_release_metadata()?;
            let embedded = required_available_version_from_candidates(
                req,
                requirement,
                Some(exclude_newer),
                available.iter().map(AvailableCandidate::from),
            );
            embedded?;
        }

        return required_available_version_from_host(req, requirement, Some(exclude_newer));
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

fn required_available_version_from_host(
    req: &str,
    requirement: &VersionRequirement,
    exclude_newer: Option<&str>,
) -> Result<AvailableR, Box<dyn Error>> {
    if requirement_uses_symbolic_name(requirement) {
        let available = rig_client::available()?;
        return required_available_version_from_candidates(
            req,
            requirement,
            exclude_newer,
            available.iter().map(AvailableCandidate::from),
        );
    }

    let available = cached_release_metadata()?;
    required_available_version_from_candidates(
        req,
        requirement,
        exclude_newer,
        available.iter().map(AvailableCandidate::from),
    )
}

fn embedded_available_metadata_date() -> Result<&'static str, Box<dyn Error>> {
    let date = EMBEDDED_AVAILABLE_METADATA_DATE.trim();
    if r_selection::iso_date_prefix(date) != Some(date) {
        return Err(
            "embedded R version availability metadata date must be in YYYY-MM-DD format".into(),
        );
    }
    Ok(date)
}

fn embedded_release_metadata() -> Result<Vec<ReleaseMetadata>, Box<dyn Error>> {
    parse_release_metadata_json(
        EMBEDDED_R_RELEASES,
        "embedded R version availability metadata",
    )
    .map_err(|e| -> Box<dyn Error> { e.into() })
}

fn cached_release_metadata() -> Result<Vec<ReleaseMetadata>, Box<dyn Error>> {
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

fn requirement_uses_symbolic_name(requirement: &VersionRequirement) -> bool {
    matches!(requirement, VersionRequirement::Bare(req) if r_selection::parse_version(req).is_none())
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

impl<'a> From<&'a ReleaseMetadata> for AvailableCandidate<'a> {
    fn from(value: &'a ReleaseMetadata) -> Self {
        Self {
            name: &value.name,
            version: &value.version,
            date: Some(&value.date),
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

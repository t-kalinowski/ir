use std::error::Error;
use std::fs;

use super::r_selection::{self, AvailableCandidate, VersionRequirement};
use super::rig_client::{self, AvailableR};

const EMBEDDED_AVAILABLE_BUILD_DATE: &str = env!("IR_RIG_AVAILABLE_BUILD_DATE");
const EMBEDDED_AVAILABLE_JSON: &str =
    include_str!(concat!(env!("OUT_DIR"), "/rig_available_all.json"));

pub(crate) fn required_available_version(
    req: &str,
    requirement: &VersionRequirement,
    exclude_newer: Option<&str>,
) -> Result<AvailableR, Box<dyn Error>> {
    if let Some(exclude_newer) = exclude_newer {
        let available = available_before_or_on(exclude_newer)?;
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

pub(crate) fn available_before_or_on(
    exclude_newer: &str,
) -> Result<Vec<AvailableR>, Box<dyn Error>> {
    let mut available = if exclude_newer <= EMBEDDED_AVAILABLE_BUILD_DATE {
        embedded_available()?
    } else {
        cached_available_all()?
    };

    retain_released_before_or_on(&mut available, exclude_newer);
    Ok(available)
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

fn embedded_available() -> Result<Vec<AvailableR>, Box<dyn Error>> {
    let mut available = parse_available_json(
        EMBEDDED_AVAILABLE_JSON,
        "embedded `rig available --all --json`",
    )?;
    retain_released_before_or_on(&mut available, EMBEDDED_AVAILABLE_BUILD_DATE);
    Ok(available)
}

fn retain_released_before_or_on(available: &mut Vec<AvailableR>, exclude_newer: &str) {
    available.retain(|version| {
        matches!(
            version.date.as_deref(),
            Some(date) if date <= exclude_newer
        )
    });
}

fn cached_available_all() -> Result<Vec<AvailableR>, Box<dyn Error>> {
    let path = crate::runtime::ir_cache_dir()?
        .join("rig")
        .join("available-all.json");
    if path.exists() {
        let json = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read `{}`: {e}", path.display()))?;
        return parse_available_json(&json, "`rig available --all --json` cache");
    }

    let json = String::from_utf8(rig_client::output(&["available", "--all", "--json"])?)
        .map_err(|e| format!("`rig available --all --json` returned non-UTF-8 output: {e}"))?;
    let available = parse_available_json(&json, "`rig available --all --json`")?;
    let json = serde_json::to_string_pretty(&available)
        .map_err(|e| format!("failed to serialize cached rig available --all JSON: {e}"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create `{}`: {e}", parent.display()))?;
    }
    fs::write(&path, json).map_err(|e| format!("failed to write `{}`: {e}", path.display()))?;
    Ok(available)
}

fn parse_available_json(json: &str, source: &str) -> Result<Vec<AvailableR>, Box<dyn Error>> {
    let mut versions: Vec<AvailableR> =
        serde_json::from_str(json).map_err(|e| format!("failed to parse {source} JSON: {e}"))?;

    for version in &mut versions {
        if let Some(date) = version.date.as_deref() {
            version.date = Some(
                r_selection::iso_date_prefix(date)
                    .ok_or_else(|| {
                        format!(
                            "rig available returned invalid release date `{}` for R {}",
                            date, version.version
                        )
                    })?
                    .to_string(),
            );
        }
    }

    Ok(versions)
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

impl From<AvailableCandidate<'_>> for AvailableR {
    fn from(value: AvailableCandidate<'_>) -> Self {
        Self {
            name: value.name.to_string(),
            version: value.version.to_string(),
            date: value.date.map(str::to_string),
        }
    }
}

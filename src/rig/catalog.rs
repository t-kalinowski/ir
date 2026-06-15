use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::command;
use super::model::{
    latest_available_release_date, normalize_available_release_dates, parse_iso_date_field,
    AvailableCandidate, AvailableR,
};

pub(super) const EMBEDDED_AVAILABLE_BUILD_DATE: &str = "2026-06-03";
const RIG_AVAILABLE_MAX_AGE_SECONDS: u64 = 24 * 60 * 60;

const EMBEDDED_AVAILABLE: &[AvailableCandidate<'static>] = &[
    AvailableCandidate {
        name: "3.6.0",
        version: "3.6.0",
        date: Some("2019-04-26"),
    },
    AvailableCandidate {
        name: "3.6.1",
        version: "3.6.1",
        date: Some("2019-07-05"),
    },
    AvailableCandidate {
        name: "3.6.2",
        version: "3.6.2",
        date: Some("2019-12-12"),
    },
    AvailableCandidate {
        name: "3.6.3",
        version: "3.6.3",
        date: Some("2020-02-29"),
    },
    AvailableCandidate {
        name: "4.0.0",
        version: "4.0.0",
        date: Some("2020-04-24"),
    },
    AvailableCandidate {
        name: "4.0.1",
        version: "4.0.1",
        date: Some("2020-06-06"),
    },
    AvailableCandidate {
        name: "4.0.2",
        version: "4.0.2",
        date: Some("2020-06-22"),
    },
    AvailableCandidate {
        name: "4.0.3",
        version: "4.0.3",
        date: Some("2020-10-10"),
    },
    AvailableCandidate {
        name: "4.0.4",
        version: "4.0.4",
        date: Some("2021-02-15"),
    },
    AvailableCandidate {
        name: "4.0.5",
        version: "4.0.5",
        date: Some("2021-03-31"),
    },
    AvailableCandidate {
        name: "4.1.0",
        version: "4.1.0",
        date: Some("2021-05-18"),
    },
    AvailableCandidate {
        name: "4.1.1",
        version: "4.1.1",
        date: Some("2021-08-10"),
    },
    AvailableCandidate {
        name: "4.1.2",
        version: "4.1.2",
        date: Some("2021-11-01"),
    },
    AvailableCandidate {
        name: "4.1.3",
        version: "4.1.3",
        date: Some("2022-03-10"),
    },
    AvailableCandidate {
        name: "4.2.0",
        version: "4.2.0",
        date: Some("2022-04-22"),
    },
    AvailableCandidate {
        name: "4.2.1",
        version: "4.2.1",
        date: Some("2022-06-23"),
    },
    AvailableCandidate {
        name: "4.2.2",
        version: "4.2.2",
        date: Some("2022-10-31"),
    },
    AvailableCandidate {
        name: "4.2.3",
        version: "4.2.3",
        date: Some("2023-03-15"),
    },
    AvailableCandidate {
        name: "4.3.0",
        version: "4.3.0",
        date: Some("2023-04-21"),
    },
    AvailableCandidate {
        name: "4.3.1",
        version: "4.3.1",
        date: Some("2023-06-16"),
    },
    AvailableCandidate {
        name: "4.3.2",
        version: "4.3.2",
        date: Some("2023-10-31"),
    },
    AvailableCandidate {
        name: "4.3.3",
        version: "4.3.3",
        date: Some("2024-02-29"),
    },
    AvailableCandidate {
        name: "4.4.0",
        version: "4.4.0",
        date: Some("2024-04-24"),
    },
    AvailableCandidate {
        name: "4.4.1",
        version: "4.4.1",
        date: Some("2024-06-14"),
    },
    AvailableCandidate {
        name: "4.4.2",
        version: "4.4.2",
        date: Some("2024-10-31"),
    },
    AvailableCandidate {
        name: "4.4.3",
        version: "4.4.3",
        date: Some("2025-02-28"),
    },
    AvailableCandidate {
        name: "4.5.0",
        version: "4.5.0",
        date: Some("2025-04-11"),
    },
    AvailableCandidate {
        name: "4.5.1",
        version: "4.5.1",
        date: Some("2025-06-13"),
    },
    AvailableCandidate {
        name: "4.5.2",
        version: "4.5.2",
        date: Some("2025-10-31"),
    },
    AvailableCandidate {
        name: "4.5.3",
        version: "4.5.3",
        date: Some("2026-03-11"),
    },
    AvailableCandidate {
        name: "4.6.0",
        version: "4.6.0",
        date: Some("2026-04-24"),
    },
];

#[derive(Debug)]
pub(super) struct ReleaseCatalog {
    versions: Vec<AvailableR>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct AvailableRCache {
    known_through: String,
    checked_at: u64,
    versions: Vec<AvailableR>,
}

impl ReleaseCatalog {
    pub(super) fn embedded() -> Self {
        Self {
            versions: EMBEDDED_AVAILABLE
                .iter()
                .copied()
                .map(AvailableR::from)
                .collect(),
        }
    }

    pub(super) fn from_versions(versions: Vec<AvailableR>) -> Self {
        Self { versions }
    }

    pub(super) fn candidates(&self) -> impl Iterator<Item = AvailableCandidate<'_>> {
        self.versions.iter().map(AvailableCandidate::from)
    }
}

pub(super) fn for_exclude_newer(exclude_newer: &str) -> Result<ReleaseCatalog, Box<dyn Error>> {
    if exclude_newer <= EMBEDDED_AVAILABLE_BUILD_DATE {
        return Ok(ReleaseCatalog::embedded());
    }

    cached_or_live(exclude_newer)
}

pub(super) fn for_install_hint(
    exclude_newer: Option<&str>,
) -> Result<ReleaseCatalog, Box<dyn Error>> {
    match exclude_newer {
        Some(exclude_newer) => for_exclude_newer(exclude_newer),
        None => Ok(ReleaseCatalog::from_versions(command::rig_available()?)),
    }
}

fn cached_or_live(exclude_newer: &str) -> Result<ReleaseCatalog, Box<dyn Error>> {
    if let Some(catalog) = read_cached_rig_available(exclude_newer)? {
        return Ok(catalog);
    }

    let versions = command::rig_available()?;
    write_cached_rig_available(&versions)?;
    Ok(ReleaseCatalog::from_versions(versions))
}

fn read_cached_rig_available(
    exclude_newer: &str,
) -> Result<Option<ReleaseCatalog>, Box<dyn Error>> {
    let path = rig_available_cache_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let json = fs::read_to_string(&path)
        .map_err(|e| format!("failed to read `{}`: {e}", path.display()))?;
    let mut cache: AvailableRCache = serde_json::from_str(&json)
        .map_err(|e| format!("failed to parse `{}`: {e}", path.display()))?;
    let stored_known_through =
        parse_iso_date_field("rig available cache known_through", &cache.known_through)?;
    normalize_available_release_dates(&mut cache.versions)?;
    let release_known_through =
        latest_available_release_date(&cache.versions).unwrap_or(EMBEDDED_AVAILABLE_BUILD_DATE);
    let known_through = stored_known_through.as_str().min(release_known_through);

    if known_through < exclude_newer && !checked_recently(cache.checked_at)? {
        return Ok(None);
    }

    Ok(Some(ReleaseCatalog::from_versions(cache.versions)))
}

fn write_cached_rig_available(versions: &[AvailableR]) -> Result<(), Box<dyn Error>> {
    let path = rig_available_cache_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create `{}`: {e}", parent.display()))?;
    }
    let cache = AvailableRCache {
        known_through: latest_available_release_date(versions)
            .unwrap_or(EMBEDDED_AVAILABLE_BUILD_DATE)
            .to_string(),
        checked_at: current_utc_seconds()?,
        versions: versions.to_vec(),
    };
    let json = serde_json::to_string_pretty(&cache)
        .map_err(|e| format!("failed to serialize cached rig available JSON: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("failed to write `{}`: {e}", path.display()))?;
    Ok(())
}

fn rig_available_cache_path() -> Result<PathBuf, Box<dyn Error>> {
    Ok(crate::runtime::ir_cache_dir()?
        .join("rig")
        .join("available.json"))
}

fn checked_recently(checked_at: u64) -> Result<bool, Box<dyn Error>> {
    let now = current_utc_seconds()?;
    if checked_at > now {
        return Ok(false);
    }

    Ok(now - checked_at <= RIG_AVAILABLE_MAX_AGE_SECONDS)
}

fn current_utc_seconds() -> Result<u64, Box<dyn Error>> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system clock is before the Unix epoch: {e}"))?
        .as_secs())
}

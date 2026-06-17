use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::Path;

use super::r_selection::{self, AvailableCandidate, VersionRequirement};
use super::rig_client::{self, AvailableR, InstalledR};

const EMBEDDED_AVAILABLE_BUILD_DATE: &str = "2026-06-03";
const MINIMUM_AUTOMATIC_R_VERSION: &str = "4.0.0";
const MINIMUM_AUTOMATIC_R_RELEASE_DATE: &str = "2020-04-24";

const EMBEDDED_AVAILABLE: &[AvailableCandidate<'static>] = &[
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

pub(crate) fn required_available_version(
    req: &str,
    requirement: &VersionRequirement,
    exclude_newer: Option<&str>,
) -> Result<AvailableR, Box<dyn Error>> {
    if let Some(exclude_newer) = exclude_newer {
        if exclude_newer <= EMBEDDED_AVAILABLE_BUILD_DATE {
            return required_available_version_from_candidates(
                req,
                requirement,
                Some(exclude_newer),
                EMBEDDED_AVAILABLE.iter().copied(),
            );
        }

        let available = cached_available_all()?;
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

pub(crate) fn resolve_rscript_for_exclude_newer(
    exclude_newer: &str,
    installed: &[InstalledR],
) -> Result<OsString, Box<dyn Error>> {
    if exclude_newer < MINIMUM_AUTOMATIC_R_RELEASE_DATE {
        return Err(format!(
            "exclude-newer {} is before R {}, the oldest R version supported by ir's resolver. Use an exclude-newer date on or after {}.",
            exclude_newer, MINIMUM_AUTOMATIC_R_VERSION, MINIMUM_AUTOMATIC_R_RELEASE_DATE
        )
        .into());
    }

    let available = available_for_exclude_newer(exclude_newer, installed)?;

    if let Some(installed) = relevant_installed_releases(installed)
        .filter(|version| installed_minor_released_before_or_on(version, &available, exclude_newer))
        .max_by(|a, b| r_selection::compare_versions(&a.version, &b.version))
    {
        return installed.rscript();
    }

    let required = latest_available_before_or_on(&available, exclude_newer)?;
    Err(format!(
        "No installed R is available for exclude-newer {}. Run `rig install {}` to install R {}.",
        exclude_newer, required.name, required.version
    )
    .into())
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

fn available_for_exclude_newer(
    exclude_newer: &str,
    installed: &[InstalledR],
) -> Result<Vec<AvailableR>, Box<dyn Error>> {
    let embedded = embedded_available();
    if exclude_newer <= EMBEDDED_AVAILABLE_BUILD_DATE
        || available_covers_installed_releases(&embedded, installed)
    {
        return Ok(embedded);
    }

    cached_available_all_refreshing_for_installed(installed)
}

fn embedded_available() -> Vec<AvailableR> {
    EMBEDDED_AVAILABLE
        .iter()
        .copied()
        .map(AvailableR::from)
        .collect()
}

fn installed_minor_released_before_or_on(
    installed: &InstalledR,
    available: &[AvailableR],
    exclude_newer: &str,
) -> bool {
    let Some(installed_minor) = r_selection::minor_version(&installed.version) else {
        return false;
    };

    available.iter().any(|version| {
        let Some(available_minor) = r_selection::minor_version(&version.version) else {
            return false;
        };
        available_minor == installed_minor
            && version
                .date
                .as_deref()
                .and_then(r_selection::iso_date_prefix)
                .map(|date| date <= exclude_newer)
                .unwrap_or(false)
    })
}

fn available_matches_installed(available: &AvailableR, installed: &InstalledR) -> bool {
    available.version == installed.version
        || available.name == installed.name
        || installed
            .aliases
            .iter()
            .any(|alias| alias == &available.name)
        || matches!(
            (
                r_selection::minor_version(&available.version),
                r_selection::minor_version(&installed.version)
            ),
            (Some(available_minor), Some(installed_minor)) if available_minor == installed_minor
        )
}

fn latest_available_before_or_on(
    available: &[AvailableR],
    exclude_newer: &str,
) -> Result<AvailableR, Box<dyn Error>> {
    available
        .iter()
        .map(AvailableCandidate::from)
        .filter(|version| r_selection::released_before_or_on(version, Some(exclude_newer)))
        .filter(|version| version_supported_by_resolver(version.version))
        .max_by(|a, b| r_selection::compare_versions(a.version, b.version))
        .map(AvailableR::from)
        .ok_or_else(|| {
            format!("could not resolve an R version before or on {exclude_newer}").into()
        })
}

fn installed_is_symbolic_prerelease(installed: &InstalledR) -> bool {
    symbolic_prerelease_name(&installed.name)
        || installed
            .aliases
            .iter()
            .any(|alias| symbolic_prerelease_name(alias))
}

fn symbolic_prerelease_name(value: &str) -> bool {
    matches!(value, "devel" | "next")
}

fn installed_version_supported_by_resolver(installed: &InstalledR) -> bool {
    version_supported_by_resolver(&installed.version)
}

fn version_supported_by_resolver(version: &str) -> bool {
    r_selection::parse_version(version)
        .map(|_| r_selection::compare_versions(version, MINIMUM_AUTOMATIC_R_VERSION).is_ge())
        .unwrap_or(false)
}

fn cached_available_all() -> Result<Vec<AvailableR>, Box<dyn Error>> {
    cached_available_all_refreshing_if(|_| false)
}

fn cached_available_all_refreshing_for_installed(
    installed: &[InstalledR],
) -> Result<Vec<AvailableR>, Box<dyn Error>> {
    cached_available_all_refreshing_if(|available| {
        relevant_installed_releases(installed)
            .any(|installed| !installed_release_covered_by_available(available, installed))
    })
}

fn cached_available_all_refreshing_if(
    refresh_cached: impl FnOnce(&[AvailableR]) -> bool,
) -> Result<Vec<AvailableR>, Box<dyn Error>> {
    let path = crate::runtime::ir_cache_dir()?
        .join("rig")
        .join("available-all.json");
    if path.exists() {
        let json = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read `{}`: {e}", path.display()))?;
        let available = parse_available_json(&json)?;
        if !refresh_cached(&available) {
            return Ok(available);
        }
    }

    refresh_cached_available_all(&path)
}

fn available_covers_installed_releases(available: &[AvailableR], installed: &[InstalledR]) -> bool {
    let mut has_installed_release = false;
    for installed in relevant_installed_releases(installed) {
        has_installed_release = true;
        if !installed_release_covered_by_available(available, installed) {
            return false;
        }
    }

    has_installed_release
}

fn relevant_installed_releases(installed: &[InstalledR]) -> impl Iterator<Item = &InstalledR> {
    installed
        .iter()
        .filter(|version| !installed_is_symbolic_prerelease(version))
        .filter(|version| installed_version_supported_by_resolver(version))
}

fn installed_release_covered_by_available(
    available: &[AvailableR],
    installed: &InstalledR,
) -> bool {
    available.iter().any(|available| {
        available.date.is_some() && available_matches_installed(available, installed)
    })
}

fn refresh_cached_available_all(path: &Path) -> Result<Vec<AvailableR>, Box<dyn Error>> {
    let json = String::from_utf8(rig_client::output(&["available", "--all", "--json"])?)
        .map_err(|e| format!("`rig available --all --json` returned non-UTF-8 output: {e}"))?;
    let available = parse_available_json(&json)?;
    let json = serde_json::to_string_pretty(&available)
        .map_err(|e| format!("failed to serialize cached rig available JSON: {e}"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create `{}`: {e}", parent.display()))?;
    }
    fs::write(path, json).map_err(|e| format!("failed to write `{}`: {e}", path.display()))?;
    Ok(available)
}

fn parse_available_json(json: &str) -> Result<Vec<AvailableR>, Box<dyn Error>> {
    let mut versions: Vec<AvailableR> = serde_json::from_str(json)
        .map_err(|e| format!("failed to parse `rig available --all --json` JSON: {e}"))?;

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

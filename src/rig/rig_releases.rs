use std::error::Error;
use std::fs;
use std::process::Command;

use super::r_selection::{self, AvailableCandidate, VersionRequirement};
use super::rig_client::{self, AvailableR};

const R_VERSIONS_URL: &str = "https://api.r-hub.io/rversions/r-versions";
const EMBEDDED_AVAILABLE_METADATA_DATE: &str = "2026-06-17";
const EMBEDDED_MINOR_RELEASES: &[MinorRelease] = &[
    MinorRelease {
        version: "0.0",
        date: "1995-06-20",
    },
    MinorRelease {
        version: "0.1",
        date: "1996-02-12",
    },
    MinorRelease {
        version: "0.2",
        date: "1996-03-14",
    },
    MinorRelease {
        version: "0.3",
        date: "1996-03-22",
    },
    MinorRelease {
        version: "0.4",
        date: "1996-04-01",
    },
    MinorRelease {
        version: "0.5",
        date: "1996-05-13",
    },
    MinorRelease {
        version: "0.6",
        date: "1996-05-17",
    },
    MinorRelease {
        version: "0.7",
        date: "1996-05-28",
    },
    MinorRelease {
        version: "0.8",
        date: "1996-05-31",
    },
    MinorRelease {
        version: "0.9",
        date: "1996-06-07",
    },
    MinorRelease {
        version: "0.10",
        date: "1996-08-27",
    },
    MinorRelease {
        version: "0.11",
        date: "1996-09-09",
    },
    MinorRelease {
        version: "0.12",
        date: "1996-09-20",
    },
    MinorRelease {
        version: "0.13",
        date: "1996-11-07",
    },
    MinorRelease {
        version: "0.14",
        date: "1996-11-28",
    },
    MinorRelease {
        version: "0.15",
        date: "1996-12-19",
    },
    MinorRelease {
        version: "0.16",
        date: "1997-02-07",
    },
    MinorRelease {
        version: "0.49",
        date: "1997-04-23",
    },
    MinorRelease {
        version: "0.60",
        date: "1997-12-04",
    },
    MinorRelease {
        version: "0.61",
        date: "1997-12-21",
    },
    MinorRelease {
        version: "0.62",
        date: "1998-06-14",
    },
    MinorRelease {
        version: "0.63",
        date: "1998-11-13",
    },
    MinorRelease {
        version: "0.64",
        date: "1999-04-07",
    },
    MinorRelease {
        version: "0.65",
        date: "1999-08-27",
    },
    MinorRelease {
        version: "0.90",
        date: "1999-11-22",
    },
    MinorRelease {
        version: "0.99",
        date: "2000-02-07",
    },
    MinorRelease {
        version: "1.0",
        date: "2000-02-29",
    },
    MinorRelease {
        version: "1.1",
        date: "2000-06-15",
    },
    MinorRelease {
        version: "1.2",
        date: "2000-12-15",
    },
    MinorRelease {
        version: "1.3",
        date: "2001-06-22",
    },
    MinorRelease {
        version: "1.4",
        date: "2001-12-19",
    },
    MinorRelease {
        version: "1.5",
        date: "2002-04-29",
    },
    MinorRelease {
        version: "1.6",
        date: "2002-10-01",
    },
    MinorRelease {
        version: "1.7",
        date: "2003-04-16",
    },
    MinorRelease {
        version: "1.8",
        date: "2003-10-08",
    },
    MinorRelease {
        version: "1.9",
        date: "2004-04-12",
    },
    MinorRelease {
        version: "2.0",
        date: "2004-10-04",
    },
    MinorRelease {
        version: "2.1",
        date: "2005-04-18",
    },
    MinorRelease {
        version: "2.2",
        date: "2005-10-06",
    },
    MinorRelease {
        version: "2.3",
        date: "2006-04-24",
    },
    MinorRelease {
        version: "2.4",
        date: "2006-10-03",
    },
    MinorRelease {
        version: "2.5",
        date: "2007-04-24",
    },
    MinorRelease {
        version: "2.6",
        date: "2007-10-03",
    },
    MinorRelease {
        version: "2.7",
        date: "2008-04-22",
    },
    MinorRelease {
        version: "2.8",
        date: "2008-10-20",
    },
    MinorRelease {
        version: "2.9",
        date: "2009-04-17",
    },
    MinorRelease {
        version: "2.10",
        date: "2009-10-26",
    },
    MinorRelease {
        version: "2.11",
        date: "2010-04-22",
    },
    MinorRelease {
        version: "2.12",
        date: "2010-10-15",
    },
    MinorRelease {
        version: "2.13",
        date: "2011-04-13",
    },
    MinorRelease {
        version: "2.14",
        date: "2011-10-31",
    },
    MinorRelease {
        version: "2.15",
        date: "2012-03-30",
    },
    MinorRelease {
        version: "3.0",
        date: "2013-04-03",
    },
    MinorRelease {
        version: "3.1",
        date: "2014-04-10",
    },
    MinorRelease {
        version: "3.2",
        date: "2015-04-16",
    },
    MinorRelease {
        version: "3.3",
        date: "2016-05-03",
    },
    MinorRelease {
        version: "3.4",
        date: "2017-04-21",
    },
    MinorRelease {
        version: "3.5",
        date: "2018-04-23",
    },
    MinorRelease {
        version: "3.6",
        date: "2019-04-26",
    },
    MinorRelease {
        version: "4.0",
        date: "2020-04-24",
    },
    MinorRelease {
        version: "4.1",
        date: "2021-05-18",
    },
    MinorRelease {
        version: "4.2",
        date: "2022-04-22",
    },
    MinorRelease {
        version: "4.3",
        date: "2023-04-21",
    },
    MinorRelease {
        version: "4.4",
        date: "2024-04-24",
    },
    MinorRelease {
        version: "4.5",
        date: "2025-04-11",
    },
    MinorRelease {
        version: "4.6",
        date: "2026-04-24",
    },
];

#[derive(serde::Deserialize)]
struct RVersionRelease {
    semver: String,
    date: String,
}

struct MinorRelease {
    version: &'static str,
    date: &'static str,
}

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

fn cached_minor_releases() -> Result<Vec<AvailableR>, Box<dyn Error>> {
    let path = crate::runtime::ir_cache_dir()?
        .join("r-versions")
        .join("available.json");
    if path.exists() {
        let json = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read `{}`: {e}", path.display()))?;
        return parse_available_json(&json, "R version availability metadata cache");
    }

    let json = download_available_json()?;
    let available = parse_available_json(&json, "R version availability metadata")?;
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

fn parse_available_json(json: &str, source: &str) -> Result<Vec<AvailableR>, Box<dyn Error>> {
    let versions: Vec<RVersionRelease> =
        serde_json::from_str(json).map_err(|e| format!("failed to parse {source} JSON: {e}"))?;
    let mut releases = Vec::new();

    for version in versions {
        let Some(minor) = minor_release(&version.semver) else {
            continue;
        };
        let date = r_selection::iso_date_prefix(&version.date)
            .ok_or_else(|| {
                format!(
                    "R version availability metadata returned invalid release date `{}` for R {}",
                    version.date, version.semver
                )
            })?
            .to_string();
        releases.push(AvailableR {
            name: minor.clone(),
            version: minor,
            date: Some(date),
        });
    }

    Ok(releases)
}

fn minor_release(semver: &str) -> Option<String> {
    let mut parts = semver.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    let patch = parts.next()?;
    if parts.next().is_some() || patch != "0" {
        return None;
    }
    for part in [major, minor, patch] {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
    }

    Some(format!("{major}.{minor}"))
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

impl<'a> From<&'a MinorRelease> for AvailableCandidate<'a> {
    fn from(value: &'a MinorRelease) -> Self {
        Self {
            name: value.version,
            version: value.version,
            date: Some(value.date),
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

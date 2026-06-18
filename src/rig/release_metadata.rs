use std::collections::BTreeMap;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RMinorReleaseMetadata {
    pub(crate) major: u64,
    pub(crate) minor: u64,
    pub(crate) date: String,
}

#[derive(serde::Deserialize)]
struct RVersionRelease {
    #[serde(default)]
    name: Option<String>,
    version: String,
    #[serde(default)]
    semver: Option<String>,
    #[serde(default)]
    date: Option<String>,
}

pub(crate) fn parse_release_metadata_json(
    json: &str,
    source: &str,
) -> Result<Vec<RMinorReleaseMetadata>, String> {
    let versions: Vec<RVersionRelease> =
        serde_json::from_str(json).map_err(|e| format!("failed to parse {source} JSON: {e}"))?;
    let mut releases = BTreeMap::new();

    for version in versions {
        let name = version.name.as_deref().unwrap_or(&version.version);
        if matches!(name, "devel" | "next") {
            continue;
        };
        let raw_version = version.semver.as_deref().unwrap_or(&version.version);
        let Some((major, minor, _patch)) = version_parts(raw_version) else {
            continue;
        };
        let Some(date) = version.date.as_deref() else {
            continue;
        };
        let date = iso_date_prefix(date)
            .ok_or_else(|| {
                format!(
                    "R version availability metadata returned invalid release date `{}` for R {}",
                    date, raw_version
                )
            })?
            .to_string();
        let key = (major, minor);
        if releases
            .get(&key)
            .map(|previous: &RMinorReleaseMetadata| date < previous.date)
            .unwrap_or(true)
        {
            releases.insert(key, RMinorReleaseMetadata { major, minor, date });
        }
    }

    if releases.is_empty() {
        return Err(format!(
            "R version availability metadata in {source} did not contain any R minor releases"
        ));
    }

    Ok(releases.into_values().collect())
}

fn version_parts(semver: &str) -> Option<(u64, u64, u64)> {
    let mut parts = semver.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn iso_date_prefix(value: &str) -> Option<&str> {
    let date = value.get(..10)?;
    if is_iso_date(date) {
        Some(date)
    } else {
        None
    }
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4] == b'-'
        && bytes[5].is_ascii_digit()
        && bytes[6].is_ascii_digit()
        && bytes[7] == b'-'
        && bytes[8].is_ascii_digit()
        && bytes[9].is_ascii_digit()
}

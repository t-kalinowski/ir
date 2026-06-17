use super::r_selection;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ReleaseMetadata {
    pub(crate) name: String,
    pub(crate) version: String,
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
) -> Result<Vec<ReleaseMetadata>, String> {
    let versions: Vec<RVersionRelease> =
        serde_json::from_str(json).map_err(|e| format!("failed to parse {source} JSON: {e}"))?;
    let mut releases = Vec::new();

    for version in versions {
        let name = version.name.as_deref().unwrap_or(&version.version);
        if matches!(name, "devel" | "next") {
            continue;
        };
        let raw_version = version.semver.as_deref().unwrap_or(&version.version);
        let Some(release_version) = release_version(raw_version) else {
            continue;
        };
        let Some(date) = version.date.as_deref() else {
            continue;
        };
        let date = r_selection::iso_date_prefix(date)
            .ok_or_else(|| {
                format!(
                    "R version availability metadata returned invalid release date `{}` for R {}",
                    date, release_version
                )
            })?
            .to_string();
        releases.push(ReleaseMetadata {
            name: name.to_string(),
            version: release_version,
            date,
        });
    }

    if releases.is_empty() {
        return Err(format!(
            "R version availability metadata in {source} did not contain any R releases"
        ));
    }

    Ok(releases)
}

fn release_version(semver: &str) -> Option<String> {
    if semver.split('.').count() == 3 && r_selection::parse_version(semver).is_some() {
        Some(semver.to_string())
    } else {
        None
    }
}

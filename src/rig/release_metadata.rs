use std::borrow::Cow;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct MinorRelease<'a> {
    pub(crate) version: Cow<'a, str>,
    pub(crate) date: Cow<'a, str>,
}

#[derive(serde::Deserialize)]
struct RVersionRelease {
    semver: String,
    date: String,
}

pub(crate) fn parse_minor_releases_json(
    json: &str,
    source: &str,
) -> Result<Vec<MinorRelease<'static>>, String> {
    let versions: Vec<RVersionRelease> =
        serde_json::from_str(json).map_err(|e| format!("failed to parse {source} JSON: {e}"))?;
    let mut releases = Vec::new();

    for version in versions {
        let Some(minor) = minor_release(&version.semver) else {
            continue;
        };
        let date = iso_date_prefix(&version.date)
            .ok_or_else(|| {
                format!(
                    "R version availability metadata returned invalid release date `{}` for R {}",
                    version.date, version.semver
                )
            })?
            .to_string();
        releases.push(MinorRelease {
            version: Cow::Owned(minor),
            date: Cow::Owned(date),
        });
    }

    if releases.is_empty() {
        return Err(format!(
            "R version availability metadata in {source} did not contain any minor releases"
        ));
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

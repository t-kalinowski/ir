use std::error::Error;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub(super) struct AvailableR {
    pub(super) name: String,
    pub(super) version: String,
    pub(super) date: Option<String>,
}

#[derive(Debug)]
pub(super) struct InstalledR {
    pub(super) name: String,
    pub(super) version: String,
    pub(super) aliases: Vec<String>,
    pub(super) default: bool,
    binary: PathBuf,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct InstalledRRecord {
    name: Option<String>,
    version: Option<String>,
    aliases: Option<Vec<String>>,
    #[serde(default)]
    default: bool,
    binary: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AvailableCandidate<'a> {
    pub(super) name: &'a str,
    pub(super) version: &'a str,
    pub(super) date: Option<&'a str>,
}

#[derive(Debug)]
pub(super) enum VersionRequirement {
    Bare(String),
    Comparison {
        op: VersionOp,
        version: Vec<u64>,
        raw: String,
    },
}

#[derive(Debug)]
pub(super) enum VersionOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

impl InstalledR {
    pub(super) fn from_record(record: InstalledRRecord) -> Option<Self> {
        Some(Self {
            name: record.name?,
            version: record.version?,
            aliases: record.aliases.unwrap_or_default(),
            default: record.default,
            binary: record.binary?,
        })
    }

    pub(super) fn rscript(&self) -> Result<OsString, Box<dyn Error>> {
        let rscript = rscript_from_r_binary(&self.binary);
        if !rscript.exists() {
            return Err(format!(
                "rig reported R {} at `{}`, but `{}` does not exist",
                self.version,
                self.binary.display(),
                rscript.display()
            )
            .into());
        }

        Ok(rscript.into_os_string())
    }
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

pub(super) fn parse_iso_date_field(key: &str, value: &str) -> Result<String, Box<dyn Error>> {
    let value = value.trim();
    if !is_iso_date(value) {
        return Err(format!("`{key}` must be a date string in YYYY-MM-DD format").into());
    }
    Ok(value.to_string())
}

pub(super) fn is_iso_date(value: &str) -> bool {
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

pub(super) fn normalize_available_release_dates(
    versions: &mut [AvailableR],
) -> Result<(), Box<dyn Error>> {
    for version in versions {
        if let Some(date) = version.date.as_deref() {
            version.date = Some(
                iso_date_prefix(date)
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

    Ok(())
}

pub(super) fn latest_available_release_date(versions: &[AvailableR]) -> Option<&str> {
    versions
        .iter()
        .map(AvailableCandidate::from)
        .filter(stable_release_candidate)
        .filter_map(|version| version.date)
        .max()
}

pub(super) fn released_before_or_on(
    version: &AvailableCandidate<'_>,
    exclude_newer: Option<&str>,
) -> bool {
    let Some(exclude_newer) = exclude_newer else {
        return true;
    };
    let Some(date) = version.date.and_then(iso_date_prefix) else {
        return false;
    };
    date <= exclude_newer
}

pub(super) fn stable_release_candidate(version: &AvailableCandidate<'_>) -> bool {
    concrete_rig_name(version.name) && parse_version(version.version).is_some()
}

pub(super) fn stable_installed_release_candidate(version: &InstalledR) -> bool {
    concrete_rig_name(&version.name)
        && !version
            .aliases
            .iter()
            .any(|alias| alias == "devel" || alias == "next")
        && parse_version(&version.version).is_some()
}

pub(super) fn matches_available_candidate(
    installed: &InstalledR,
    available: &AvailableCandidate<'_>,
) -> bool {
    installed.version == available.version
        || installed.name == available.name
        || installed
            .aliases
            .iter()
            .any(|alias| alias == available.name)
}

pub(super) fn parse_version_requirement(req: &str) -> Result<VersionRequirement, Box<dyn Error>> {
    let req = req.trim();
    for (prefix, op) in [
        (">=", VersionOp::Gte),
        ("<=", VersionOp::Lte),
        ("==", VersionOp::Eq),
        (">", VersionOp::Gt),
        ("<", VersionOp::Lt),
    ] {
        if let Some(version) = req.strip_prefix(prefix) {
            let raw = version.trim().to_string();
            let version = parse_version(&raw)
                .ok_or_else(|| format!("`r-version` has an unsupported version spec `{req}`"))?;
            return Ok(VersionRequirement::Comparison { op, version, raw });
        }
    }

    if req.is_empty() {
        return Err("`r-version` must not be empty".into());
    }
    Ok(VersionRequirement::Bare(req.to_string()))
}

impl VersionRequirement {
    pub(super) fn matches_installed(&self, installed: &InstalledR) -> bool {
        self.matches_candidate(&installed.name, &installed.version, &installed.aliases)
    }

    pub(super) fn matches_requested_symbolic_candidate(
        &self,
        version: &AvailableCandidate<'_>,
    ) -> bool {
        matches!(
            self,
            VersionRequirement::Bare(req)
                if !concrete_rig_name(version.name) && req == version.name
        )
    }

    pub(super) fn matches_requested_installed_candidate(&self, version: &InstalledR) -> bool {
        matches!(
            self,
            VersionRequirement::Bare(req)
                if version.name == req.as_str() || version.aliases.iter().any(|alias| alias == req)
        )
    }

    pub(super) fn matches_candidate(
        &self,
        name: &str,
        candidate_version: &str,
        aliases: &[String],
    ) -> bool {
        match self {
            VersionRequirement::Bare(req) => {
                name == req
                    || candidate_version == req
                    || aliases.iter().any(|alias| alias == req)
                    || parse_version(req)
                        .map(|_| candidate_version.starts_with(&format!("{req}.")))
                        .unwrap_or(false)
            }
            VersionRequirement::Comparison {
                op,
                version: required_version,
                raw,
            } => {
                let Some(candidate) = parse_version(candidate_version) else {
                    return false;
                };
                if matches!(op, VersionOp::Eq)
                    && (name == raw || aliases.iter().any(|alias| alias == raw))
                {
                    return true;
                }
                match op {
                    VersionOp::Gt => compare_version_parts(&candidate, required_version).is_gt(),
                    VersionOp::Gte => compare_version_parts(&candidate, required_version).is_ge(),
                    VersionOp::Lt => compare_version_parts(&candidate, required_version).is_lt(),
                    VersionOp::Lte => compare_version_parts(&candidate, required_version).is_le(),
                    VersionOp::Eq => compare_version_parts(&candidate, required_version).is_eq(),
                }
            }
        }
    }
}

pub(super) fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    match (parse_version(a), parse_version(b)) {
        (Some(a), Some(b)) => compare_version_parts(&a, &b),
        _ => a.cmp(b),
    }
}

fn iso_date_prefix(value: &str) -> Option<&str> {
    let date = value.get(..10)?;
    if is_iso_date(date) {
        Some(date)
    } else {
        None
    }
}

fn concrete_rig_name(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_digit())
}

fn parse_version(value: &str) -> Option<Vec<u64>> {
    let mut parts = Vec::new();
    for part in value.split('.') {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        parts.push(part.parse().ok()?);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

fn compare_version_parts(a: &[u64], b: &[u64]) -> std::cmp::Ordering {
    let len = a.len().max(b.len());
    for idx in 0..len {
        let left = a.get(idx).copied().unwrap_or(0);
        let right = b.get(idx).copied().unwrap_or(0);
        match left.cmp(&right) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

fn rscript_from_r_binary(binary: &Path) -> PathBuf {
    binary.with_file_name(if cfg!(windows) {
        "Rscript.exe"
    } else {
        "Rscript"
    })
}

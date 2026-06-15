use std::error::Error;
use std::ffi::OsString;

mod catalog;
mod command;
mod model;
mod selection;

use model::{parse_iso_date_field, parse_version_requirement};

pub fn resolve_rscript(req: &str, exclude_newer: Option<&str>) -> Result<OsString, Box<dyn Error>> {
    let exclude_newer = exclude_newer
        .map(|value| parse_iso_date_field("exclude-newer", value))
        .transpose()?;
    let requirement = parse_version_requirement(req)?;
    let installed = command::rig_list()?;

    if let Some(installed) = selection::select_installed_for_requirement(&installed, &requirement) {
        return installed.rscript();
    }

    let catalog = catalog::for_install_hint(exclude_newer.as_deref())?;
    let required = selection::required_available_version(
        req,
        &requirement,
        &catalog,
        exclude_newer.as_deref(),
    )?;
    Err(format!(
        "R {} is required but is not installed. Run `rig install {}`.",
        required.version, required.name
    )
    .into())
}

pub fn resolve_rscript_for_exclude_newer(exclude_newer: &str) -> Result<OsString, Box<dyn Error>> {
    let exclude_newer = parse_iso_date_field("exclude-newer", exclude_newer)?;
    let installed = command::rig_list()?;
    let catalog = catalog::for_exclude_newer(&exclude_newer)?;

    if let Some(r) = selection::select_installed_for_date(&installed, &catalog, &exclude_newer) {
        return r.rscript();
    }

    let required = selection::required_available_version_for_date(&catalog, &exclude_newer)?;
    Err(format!(
        "No installed R is available for exclude-newer {}. Run `rig install {}` to install R {}.",
        exclude_newer, required.name, required.version
    )
    .into())
}

/// Rscript of rig's default R install (`"default": true` in `rig list --json`),
/// or `None` when rig is absent, has no default, or the binary is missing.
///
/// Best-effort: the caller falls back to a bare `"Rscript"` on `None`, so any
/// failure here (rig not on PATH, unparseable output) resolves to `None` rather
/// than aborting the run. On rig-managed Windows the only `Rscript` on PATH is a
/// `.bat` shim that `std::process::Command` cannot spawn, so resolving the real
/// `Rscript.exe` from the default install's `binary` is what makes the
/// no-`--r-version` path work there.
pub fn default_rscript() -> Option<OsString> {
    command::rig_list()
        .ok()?
        .into_iter()
        .find(|r| r.default)?
        .rscript()
        .ok()
}

use std::error::Error;

use super::catalog::ReleaseCatalog;
use super::model::{
    compare_versions, matches_available_candidate, released_before_or_on,
    stable_installed_release_candidate, stable_release_candidate, AvailableR, InstalledR,
    VersionRequirement,
};

pub(super) fn select_installed_for_requirement<'a>(
    installed: &'a [InstalledR],
    requirement: &VersionRequirement,
) -> Option<&'a InstalledR> {
    installed
        .iter()
        .filter(|version| {
            stable_installed_release_candidate(version)
                || requirement.matches_requested_installed_candidate(version)
        })
        .filter(|version| requirement.matches_installed(version))
        .max_by(|a, b| compare_versions(&a.version, &b.version))
}

pub(super) fn select_installed_for_date<'a>(
    installed: &'a [InstalledR],
    catalog: &ReleaseCatalog,
    exclude_newer: &str,
) -> Option<&'a InstalledR> {
    installed
        .iter()
        .filter(|version| stable_installed_release_candidate(version))
        .filter(|version| installed_released_before_or_on(version, catalog, exclude_newer))
        .max_by(|a, b| compare_versions(&a.version, &b.version))
}

pub(super) fn has_stable_installed_outside_catalog(
    installed: &[InstalledR],
    catalog: &ReleaseCatalog,
) -> bool {
    installed
        .iter()
        .filter(|version| stable_installed_release_candidate(version))
        .any(|installed| {
            !catalog.candidates().any(|candidate| {
                stable_release_candidate(&candidate)
                    && matches_available_candidate(installed, &candidate)
            })
        })
}

pub(super) fn required_available_version(
    req: &str,
    requirement: &VersionRequirement,
    catalog: &ReleaseCatalog,
    exclude_newer: Option<&str>,
) -> Result<AvailableR, Box<dyn Error>> {
    catalog
        .candidates()
        .filter(|version| released_before_or_on(version, exclude_newer))
        .filter(|version| {
            stable_release_candidate(version)
                || requirement.matches_requested_symbolic_candidate(version)
        })
        .filter(|version| requirement.matches_candidate(version.name, version.version, &[]))
        .max_by(|a, b| compare_versions(a.version, b.version))
        .map(AvailableR::from)
        .ok_or_else(|| {
            let suffix = exclude_newer
                .map(|date| format!(" before or on {date}"))
                .unwrap_or_default();
            format!("could not resolve R version `{req}` with available R versions{suffix}").into()
        })
}

pub(super) fn required_available_version_for_date(
    catalog: &ReleaseCatalog,
    exclude_newer: &str,
) -> Result<AvailableR, Box<dyn Error>> {
    catalog
        .candidates()
        .filter(|version| released_before_or_on(version, Some(exclude_newer)))
        .filter(stable_release_candidate)
        .max_by(|a, b| compare_versions(a.version, b.version))
        .map(AvailableR::from)
        .ok_or_else(|| {
            format!("could not resolve an R version available before or on {exclude_newer}").into()
        })
}

fn installed_released_before_or_on(
    installed: &InstalledR,
    catalog: &ReleaseCatalog,
    exclude_newer: &str,
) -> bool {
    catalog.candidates().any(|version| {
        stable_release_candidate(&version)
            && matches_available_candidate(installed, &version)
            && released_before_or_on(&version, Some(exclude_newer))
    })
}

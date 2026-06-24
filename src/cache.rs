use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use clap::ArgMatches;

use crate::runtime::{count_files, ir_cache_dir, nonempty_env, r_user_cache_dir};

pub(crate) fn cmd_cache(matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    match matches.subcommand() {
        Some(("clean", matches)) => {
            cmd_cache_clean(matches.get_flag("force"), matches.get_flag("all"))
        }
        Some(("dir", _)) => cmd_cache_dir(),
        _ => unreachable!("clap requires a cache subcommand"),
    }
}

pub(crate) fn cmd_cache_clean(_force: bool, all: bool) -> Result<(), Box<dyn Error>> {
    let cache_dir = ir_cache_dir()?;
    if all {
        return cmd_cache_clean_all(cache_dir);
    }

    if !cache_dir.exists() {
        println!("No cache found at: {}", cache_dir.display());
        return Ok(());
    }

    let files = count_files(&cache_dir)?;
    println!("Clearing cache at: {}", cache_dir.display());
    fs::remove_dir_all(&cache_dir)
        .map_err(|e| format!("failed to remove cache `{}`: {e}", cache_dir.display()))?;
    println!(
        "Removed {files} {}",
        if files == 1 { "file" } else { "files" }
    );
    Ok(())
}

fn cmd_cache_clean_all(cache_dir: PathBuf) -> Result<(), Box<dyn Error>> {
    for target in cache_clean_all_targets(cache_dir)? {
        clear_labeled_cache(&target.label, &target.path)?;
    }

    Ok(())
}

fn clear_labeled_cache(label: &str, path: &Path) -> Result<(), Box<dyn Error>> {
    if !path.exists() {
        println!("No {label} found at: {}", path.display());
        return Ok(());
    }

    let files = count_files(path)?;
    println!("Clearing {label} at: {}", path.display());
    fs::remove_dir_all(path)
        .map_err(|e| format!("failed to remove {label} `{}`: {e}", path.display()))?;
    println!(
        "Removed {files} {} from {label}",
        if files == 1 { "file" } else { "files" }
    );
    Ok(())
}

struct CacheCleanTarget {
    label: String,
    path: PathBuf,
}

impl CacheCleanTarget {
    fn new(label: impl Into<String>, path: PathBuf) -> Self {
        Self {
            label: label.into(),
            path,
        }
    }
}

fn cache_clean_all_targets(cache_dir: PathBuf) -> Result<Vec<CacheCleanTarget>, Box<dyn Error>> {
    let mut targets = vec![
        CacheCleanTarget::new("ir cache", cache_dir),
        CacheCleanTarget::new("pak package cache", pak_package_cache_dir()?),
        CacheCleanTarget::new("pak cache", pak_cache_dir()?),
    ];

    targets.extend(
        renv_cache_dirs()?
            .into_iter()
            .map(|path| CacheCleanTarget::new("renv cache", path)),
    );

    targets.push(CacheCleanTarget::new(
        "reticulate cache",
        r_cache_package_dir("reticulate")?,
    ));
    targets.push(CacheCleanTarget::new(
        "reticulate legacy cache",
        reticulate_legacy_cache_dir()?,
    ));

    Ok(targets)
}

fn r_cache_package_dir(package: &str) -> Result<PathBuf, Box<dyn Error>> {
    Ok(r_user_cache_dir()?.join("R").join(package))
}

fn pak_cache_dir() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = nonempty_env("R_PKG_CACHE_DIR") {
        return Ok(PathBuf::from(path));
    }

    if let Some(path) = nonempty_env("R_USER_CACHE_DIR") {
        return Ok(PathBuf::from(path).join("R").join("pak"));
    }

    #[cfg(windows)]
    {
        if let Some(root) = nonempty_env("LOCALAPPDATA") {
            return Ok(PathBuf::from(root).join("R").join("Cache").join("pak"));
        }
        if let Some(root) = nonempty_env("USERPROFILE") {
            return Ok(PathBuf::from(root)
                .join("AppData")
                .join("Local")
                .join("R")
                .join("Cache")
                .join("pak"));
        }
        Err("cannot determine Windows pak cache directory; set R_PKG_CACHE_DIR, R_USER_CACHE_DIR, LOCALAPPDATA, or USERPROFILE".into())
    }

    #[cfg(target_os = "macos")]
    {
        return Ok(home_dir()?
            .join("Library")
            .join("Caches")
            .join("org.R-project.R")
            .join("R")
            .join("pak"));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(root) = nonempty_env("XDG_CACHE_HOME") {
            return Ok(PathBuf::from(root).join("R").join("pak"));
        }
        Ok(home_dir()?.join(".cache").join("R").join("pak"))
    }
}

fn pak_package_cache_dir() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = nonempty_env("R_PKG_CACHE_DIR") {
        return Ok(PathBuf::from(path).join("R").join("pkgcache"));
    }

    r_cache_package_dir("pkgcache")
}

fn renv_cache_dirs() -> Result<Vec<PathBuf>, Box<dyn Error>> {
    if let Some(paths) = nonempty_env("RENV_PATHS_CACHE") {
        return split_renv_cache_dirs(paths);
    }

    if let Some(root) = nonempty_env("RENV_PATHS_ROOT") {
        return Ok(vec![PathBuf::from(root).join("cache")]);
    }

    Ok(vec![r_cache_package_dir("renv")?.join("cache")])
}

fn split_renv_cache_dirs(paths: std::ffi::OsString) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let paths = paths
        .into_string()
        .map_err(|_| "`RENV_PATHS_CACHE` must be valid UTF-8")?;
    let separators: &[char] = if cfg!(windows) { &[';'] } else { &[';', ':'] };

    Ok(paths
        .split(separators)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect())
}

fn reticulate_legacy_cache_dir() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(root) = nonempty_env("R_USER_CACHE_DIR") {
        return Ok(PathBuf::from(root).join("r-reticulate"));
    }

    #[cfg(windows)]
    {
        if let Some(root) = nonempty_env("LOCALAPPDATA") {
            return Ok(PathBuf::from(root).join("r-reticulate").join("Cache"));
        }
        if let Some(root) = nonempty_env("USERPROFILE") {
            return Ok(PathBuf::from(root)
                .join("Local Settings")
                .join("Application Data")
                .join("r-reticulate")
                .join("Cache"));
        }
        Err("cannot determine Windows reticulate legacy cache directory; set R_USER_CACHE_DIR, LOCALAPPDATA, or USERPROFILE".into())
    }

    #[cfg(target_os = "macos")]
    {
        return Ok(home_dir()?
            .join("Library")
            .join("Caches")
            .join("r-reticulate"));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(root) = nonempty_env("XDG_CACHE_HOME") {
            return Ok(PathBuf::from(root).join("r-reticulate"));
        }
        Ok(home_dir()?.join(".cache").join("r-reticulate"))
    }
}

#[cfg(unix)]
fn home_dir() -> Result<PathBuf, Box<dyn Error>> {
    let home = nonempty_env("HOME").ok_or("cannot determine home directory")?;
    Ok(PathBuf::from(home))
}

pub(crate) fn cmd_cache_dir() -> Result<(), Box<dyn Error>> {
    println!("{}", ir_cache_dir()?.display());
    Ok(())
}

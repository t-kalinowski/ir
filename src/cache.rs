use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::ArgMatches;

use crate::runtime::{count_files, ir_cache_dir, rscript_command, spawn_error};

const TOOL_CACHE_CLEANER: &str = include_str!("../driver/cache-clean.R");

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
    clear_labeled_cache("ir cache", &cache_dir)?;
    clean_tool_caches_with_r()
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

fn clean_tool_caches_with_r() -> Result<(), Box<dyn Error>> {
    let rscript = rscript_command();
    let script = TempScript::write("ir-cache-clean", "R", TOOL_CACHE_CLEANER)?;
    let status = Command::new(&rscript)
        .arg(script.path())
        .status()
        .map_err(|e| spawn_error(&rscript, e))?;

    if !status.success() {
        return Err(format!("R cache cleaner failed with status {status}").into());
    }

    Ok(())
}

struct TempScript {
    path: PathBuf,
}

impl TempScript {
    fn write(prefix: &str, ext: &str, contents: &str) -> Result<Self, Box<dyn Error>> {
        let path = unique_temp_path(prefix, ext);
        fs::write(&path, contents)
            .map_err(|e| format!("failed to write cache cleaner `{}`: {e}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempScript {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn unique_temp_path(prefix: &str, ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut path = env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
    path.set_extension(ext);
    path
}

pub(crate) fn cmd_cache_dir() -> Result<(), Box<dyn Error>> {
    println!("{}", ir_cache_dir()?.display());
    Ok(())
}

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const RESOLVE_FILE: &str = env!("IR_RESOLVE_DRIVER_FILE");
pub(crate) const PYTHON_RESOLVE_FILE: &str = env!("IR_PYTHON_RESOLVE_DRIVER_FILE");

pub(crate) fn cached_path(
    cache_dir: &Path,
    file_name: &str,
    contents: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    let dir = cache_dir.join("drivers");
    let path = dir.join(file_name);
    if path.exists() {
        return Ok(path);
    }

    write_path(&path, contents)
}

fn write_path(path: &Path, contents: &str) -> Result<PathBuf, Box<dyn Error>> {
    fs::create_dir_all(path.parent().ok_or("driver cache path has no parent")?)?;
    fs::write(&path, contents)?;
    let mut permissions = fs::metadata(&path)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions)?;
    Ok(path.to_path_buf())
}

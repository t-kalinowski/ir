//! `ir` — self-describing R scripts.
//!
//! Runs a standalone R script whose dependencies are declared in YAML
//! frontmatter at the top of the file:
//!
//! ```r
//! #!/usr/bin/env -S ir run
//! #| dependencies:
//! #|   - dplyr>=1.0
//! #|   - tidyr
//! #| R: ">= 4.0"
//! #| exclude after: "2024-01-15"
//!
//! library(dplyr)
//! 1 + 1
//! ```
//!
//! The pipeline has two phases:
//!
//!   1. Rust extracts the leading `#| ` YAML frontmatter block. A private R
//!      session (`driver/resolve.R`) parses that YAML frontmatter, resolves the
//!      dependencies with pak, hashes the resolved set into a content-addressed
//!      library path under the cache directory, and materialises that path as a
//!      light-weight library of symlinks into renv's package cache. The path is
//!      reported back to us.
//!
//!   2. We launch the user's script in a fresh, isolated R session whose
//!      library path is exactly that library plus base R.

use std::env;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// The R resolution driver, embedded at compile time so `ir` ships as one
/// self-contained binary while the source stays editable as real R.
const RESOLVE_DRIVER: &str = include_str!("../driver/resolve.R");

fn main() {
    if let Err(err) = try_main() {
        eprintln!("ir: {err}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("run") => {
            let args: Vec<String> = args.collect();
            if matches!(args.as_slice(), [arg] if arg == "--help" || arg == "-h") {
                print_run_help();
                return Ok(());
            }

            let run = parse_run_args(args)?;
            cmd_run(run)
        }
        Some("cache") => cmd_cache(args.collect()),
        Some("--version" | "-V") => {
            println!("ir {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--help" | "-h") | None => {
            print_help();
            Ok(())
        }
        Some(other) => Err(format!("unknown command `{other}` (try `ir run script.R`)").into()),
    }
}

struct RunArgs {
    rscript_args: Vec<String>,
    target: RunTarget,
    with: Vec<String>,
    target_args: Vec<String>,
}

enum RunTarget {
    Script(String),
    PackageExec(PackageExecTarget),
}

struct PackageExecTarget {
    package_ref: String,
    package_name: Option<String>,
    executable: String,
}

fn parse_run_args(args: Vec<String>) -> Result<RunArgs, Box<dyn Error>> {
    let mut rscript_args = Vec::new();
    let mut with = Vec::new();
    let mut from = None;
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        if arg == "--with" {
            i += 1;
            let dep = args
                .get(i)
                .ok_or("`--with` requires a package dependency spec")?;
            with.push(dep.clone());
            i += 1;
        } else if let Some(dep) = arg.strip_prefix("--with=") {
            if dep.is_empty() {
                return Err("`--with` requires a package dependency spec".into());
            }
            with.push(dep.to_string());
            i += 1;
        } else if arg == "--from" {
            i += 1;
            let package_ref = args.get(i).ok_or("`--from` requires a package ref")?;
            from = Some(package_ref.clone());
            i += 1;
        } else if let Some(package_ref) = arg.strip_prefix("--from=") {
            if package_ref.is_empty() {
                return Err("`--from` requires a package ref".into());
            }
            from = Some(package_ref.to_string());
            i += 1;
        } else if arg.starts_with('-') {
            rscript_args.push(arg.clone());
            i += 1;
        } else {
            let target = parse_run_target(arg, from.as_deref())?;
            let target_args = args[i + 1..].to_vec();
            return Ok(RunArgs {
                rscript_args,
                target,
                with,
                target_args,
            });
        }
    }

    if from.is_some() {
        return Err("`--from` requires a command to run".into());
    }

    Err("`ir run` requires a script path or package executable (try `ir run script.R`)".into())
}

fn parse_run_target(target: &str, from: Option<&str>) -> Result<RunTarget, Box<dyn Error>> {
    if let Some(package_ref) = from {
        if !is_package_executable_name(target) {
            return Err("`--from` requires a command name, not a path".into());
        }
        return Ok(RunTarget::PackageExec(PackageExecTarget {
            package_ref: package_ref.to_string(),
            package_name: infer_self_named_executable(package_ref),
            executable: target.to_string(),
        }));
    }

    let path = Path::new(target);
    if path.exists() || target.ends_with(".R") || target.contains('/') || target.contains('\\') {
        return Ok(RunTarget::Script(target.to_string()));
    }

    if let Some(executable) = infer_self_named_executable(target) {
        return Ok(RunTarget::PackageExec(PackageExecTarget {
            package_ref: target.to_string(),
            package_name: Some(executable.clone()),
            executable,
        }));
    }

    Ok(RunTarget::Script(target.to_string()))
}

fn infer_self_named_executable(package_ref: &str) -> Option<String> {
    let end = package_ref
        .find(|c: char| matches!(c, '@' | '<' | '>' | '=' | '!' | ' '))
        .unwrap_or(package_ref.len());
    let name = &package_ref[..end];
    if is_r_package_name(name) {
        Some(name.to_string())
    } else {
        None
    }
}

fn is_r_package_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && !name.ends_with('.')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '.')
}

fn is_package_executable_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && !name.contains(':')
}

fn cmd_cache(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    match args.first().map(String::as_str) {
        Some("clean") => cmd_cache_clean(&args[1..]),
        Some("dir") => cmd_cache_dir(&args[1..]),
        Some("--help" | "-h") => {
            print_cache_help();
            Ok(())
        }
        None => {
            print_cache_help();
            Err("`ir cache` requires a subcommand".into())
        }
        Some(other) => Err(format!("unrecognized subcommand `{other}`").into()),
    }
}

fn cmd_cache_clean(args: &[String]) -> Result<(), Box<dyn Error>> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_cache_clean_help();
        return Ok(());
    }

    for arg in args {
        match arg.as_str() {
            "--force" => {}
            other => return Err(format!("unexpected argument `{other}`").into()),
        }
    }

    let cache_dir = ir_cache_dir()?;
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

fn cmd_cache_dir(args: &[String]) -> Result<(), Box<dyn Error>> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_cache_dir_help();
        return Ok(());
    }
    if let Some(arg) = args.first() {
        return Err(format!("unexpected argument `{arg}`").into());
    }

    println!("{}", ir_cache_dir()?.display());
    Ok(())
}

fn print_help() {
    println!(
        concat!(
            "ir {} — self-describing R scripts\n",
            "\n",
            "USAGE:\n",
            "    ir run [--with <pkg-ref>]... [Rscript-options...] <script.R> [args...]\n",
            "    ir run [--with <pkg-ref>]... --from <pkg-ref> <command> [args...]\n",
            "    ir run [--with <pkg-ref>]... <pkg-ref> [args...]\n",
            "    ir cache <command>\n",
            "\n",
            "`ir run` reads the YAML frontmatter from <script.R>, resolves its\n",
            "dependencies, builds a dedicated package library, and runs the script\n",
            "against it. It can also resolve a package ref and run an executable\n",
            "from that package's exec directory. Leading Rscript options are passed\n",
            "to Rscript for script targets; trailing args are passed through to the\n",
            "script or package executable.\n",
            "`ir cache` manages the dependency resolution and materialised library\n",
            "cache.\n",
            "\n",
            "ENVIRONMENT:\n",
            "    IR_CACHE_DIR   override the cache dir (default: tools::R_user_dir(\"ir\", \"cache\"))\n",
            "    IR_RSCRIPT     path to the Rscript executable (default: Rscript on PATH)"
        ),
        env!("CARGO_PKG_VERSION")
    );
}

fn print_run_help() {
    println!(concat!(
        "Run an R script\n",
        "\n",
        "USAGE:\n",
        "    ir run [--with <pkg-ref>]... [Rscript-options...] <script.R> [args...]\n",
        "    ir run [--with <pkg-ref>]... --from <pkg-ref> <command> [args...]\n",
        "    ir run [--with <pkg-ref>]... <pkg-ref> [args...]\n",
        "\n",
        "`ir run` reads the YAML frontmatter from <script.R>, resolves its\n",
        "dependencies, builds a dedicated package library, and runs the script\n",
        "against it. It can also resolve <pkg-ref>, find\n",
        "<library>/<package>/exec/<command> or <command>.R, and launch it\n",
        "through its shebang. A bare package ref such as `btw` is treated as\n",
        "`--from btw btw` when it is not an existing path. `--with` adds explicit\n",
        "dependencies to the resolved library.\n",
        "\n",
        "ENVIRONMENT:\n",
        "    IR_CACHE_DIR   override the cache dir (default: tools::R_user_dir(\"ir\", \"cache\"))\n",
        "    IR_RSCRIPT     path to the Rscript executable (default: Rscript on PATH)"
    ));
}

fn print_cache_help() {
    println!(concat!(
        "Manage ir's cache\n",
        "\n",
        "USAGE:\n",
        "    ir cache <COMMAND>\n",
        "\n",
        "COMMANDS:\n",
        "    clean  Clear the cache, removing all entries\n",
        "    dir    Show the cache directory\n",
        "\n",
        "ENVIRONMENT:\n",
        "    IR_CACHE_DIR   override the cache dir (default: tools::R_user_dir(\"ir\", \"cache\"))"
    ));
}

fn print_cache_clean_help() {
    println!(concat!(
        "Clear the cache, removing all entries\n",
        "\n",
        "USAGE:\n",
        "    ir cache clean [OPTIONS]\n",
        "\n",
        "OPTIONS:\n",
        "    --force  Force removal of the cache"
    ));
}

fn print_cache_dir_help() {
    println!(concat!(
        "Show the cache directory\n",
        "\n",
        "USAGE:\n",
        "    ir cache dir"
    ));
}

/// Resolve dependencies for the run target, then run it against the resulting
/// library. Exits the process with the target's own exit code.
fn cmd_run(run: RunArgs) -> Result<(), Box<dyn Error>> {
    match run.target {
        RunTarget::Script(script) => {
            cmd_run_script(&script, &run.rscript_args, &run.with, &run.target_args)
        }
        RunTarget::PackageExec(target) => {
            if !run.rscript_args.is_empty() {
                return Err("Rscript options are only supported for script targets".into());
            }
            cmd_run_package_exec(&target, &run.with, &run.target_args)
        }
    }
}

fn cmd_run_script(
    script: &str,
    rscript_args: &[String],
    with: &[String],
    script_args: &[String],
) -> Result<(), Box<dyn Error>> {
    let script_path =
        fs::canonicalize(script).map_err(|e| format!("cannot read script `{script}`: {e}"))?;

    let rscript = rscript_command();

    // Phase 1: private R session resolves deps and materialises the library.
    // Rust sends the extracted YAML frontmatter on stdin and receives the library path.
    let resolution = resolve_script_library(&rscript, &script_path, with)?;

    // Phase 2: run the user's script in an isolated R session.
    let code = run_script(
        &rscript,
        resolution.library.as_deref(),
        &script_path,
        rscript_args,
        script_args,
    )?;
    std::process::exit(code);
}

fn cmd_run_package_exec(
    target: &PackageExecTarget,
    with: &[String],
    target_args: &[String],
) -> Result<(), Box<dyn Error>> {
    if target.package_ref.is_empty() || target.executable.is_empty() {
        return Err("package executable targets must have a package ref and command".into());
    }

    let rscript = rscript_command();
    let resolution = resolve_package_exec_library(&rscript, &target.package_ref, with)?;
    let library = resolution
        .library
        .ok_or("dependency resolver did not return a library path")?;
    let executable =
        find_package_executable(&library, target.package_name.as_deref(), &target.executable)?;
    let code = run_package_executable(&rscript, &library, &executable, target_args)?;
    std::process::exit(code);
}

struct Resolution {
    library: Option<PathBuf>,
}

fn resolve_script_library(
    rscript: &OsStr,
    script: &Path,
    extra_deps: &[String],
) -> Result<Resolution, Box<dyn Error>> {
    let frontmatter = read_op_frontmatter_to_string(script)?;
    resolve_library(rscript, &frontmatter, None, extra_deps)
}

fn resolve_package_exec_library(
    rscript: &OsStr,
    package_ref: &str,
    extra_deps: &[String],
) -> Result<Resolution, Box<dyn Error>> {
    resolve_library(rscript, "", Some(package_ref), extra_deps)
}

/// Phase 1 — run the embedded driver in a private R session and return the
/// path to the materialised library.
fn resolve_library(
    rscript: &OsStr,
    frontmatter: &str,
    from_dep: Option<&str>,
    extra_deps: &[String],
) -> Result<Resolution, Box<dyn Error>> {
    let tmp = env::temp_dir();
    let driver = unique_path(&tmp, "ir-resolve", "R");
    let out = unique_path(&tmp, "ir-libpath", "txt");
    fs::write(&driver, RESOLVE_DRIVER)?;

    let mut cmd = Command::new(rscript);
    cmd.arg(&driver).arg(&out);
    if let Some(from_dep) = from_dep {
        cmd.arg("--from").arg(from_dep);
    }
    for dep in extra_deps {
        cmd.arg("--with").arg(dep);
    }

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        // pak suppresses progress in noninteractive Rscript unless this is set.
        // Resolution cache hits return before pak, so this adds no cache-hit pak output.
        .env("R_PKG_SHOW_PROGRESS", "true")
        .spawn()
        .map_err(|e| spawn_error(rscript, e))?;

    let write_result = child
        .stdin
        .take()
        .ok_or("failed to open resolver stdin")?
        .write_all(frontmatter.as_bytes());
    let status = child
        .wait()
        .map_err(|e| format!("failed to wait for dependency resolver: {e}"))?;
    write_result?;

    let _ = fs::remove_file(&driver);
    let result = fs::read_to_string(&out).unwrap_or_default();
    let _ = fs::remove_file(&out);

    if !status.success() {
        return Err("dependency resolution failed".into());
    }

    let library = result.trim();

    Ok(Resolution {
        library: if library.is_empty() {
            None
        } else {
            Some(PathBuf::from(library))
        },
    })
}

fn find_package_executable(
    library: &Path,
    package: Option<&str>,
    executable: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(package) = package {
        let exec_dir = library.join(package).join("exec");
        return find_package_executable_in_dir(&exec_dir, executable).ok_or_else(|| {
            if !exec_dir.is_dir() {
                format!(
                    "package `{package}` does not have an exec directory in `{}`",
                    library.display()
                )
                .into()
            } else {
                format!(
                    "could not find executable `{executable}` or `{executable}.R` in `{}`",
                    exec_dir.display()
                )
                .into()
            }
        });
    }

    let mut candidates = Vec::new();
    for entry in fs::read_dir(library)
        .map_err(|e| format!("cannot read resolved library `{}`: {e}", library.display()))?
    {
        let exec_dir = entry?.path().join("exec");
        if let Some(path) = find_package_executable_in_dir(&exec_dir, executable) {
            candidates.push(path);
        }
    }
    candidates.sort();

    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => Err(format!(
            "could not find executable `{executable}` or `{executable}.R` in any package under `{}`",
            library.display()
        )
        .into()),
        _ => Err(format!(
            "found multiple executables named `{executable}` in `{}`; use a package ref whose installed package name can be inferred",
            library.display()
        )
        .into()),
    }
}

fn find_package_executable_in_dir(exec_dir: &Path, executable: &str) -> Option<PathBuf> {
    let candidates = [
        exec_dir.join(executable),
        exec_dir.join(format!("{executable}.R")),
    ];

    for candidate in candidates {
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

fn resolved_runtime_path(library: &Path, rscript: &OsStr) -> Result<OsString, Box<dyn Error>> {
    let mut entries = Vec::new();

    let rscript_path = Path::new(rscript);
    if let Some(parent) = rscript_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        entries.push(parent.to_path_buf());
    }

    for entry in fs::read_dir(library)
        .map_err(|e| format!("cannot read resolved library `{}`: {e}", library.display()))?
    {
        let exec = entry?.path().join("exec");
        if exec.is_dir() {
            entries.push(exec);
        }
    }

    let current_path = env::var_os("PATH").unwrap_or_default();
    entries.extend(env::split_paths(&current_path));
    Ok(env::join_paths(entries)?)
}

fn run_package_executable(
    rscript: &OsStr,
    library: &Path,
    executable: &Path,
    args: &[String],
) -> Result<i32, Box<dyn Error>> {
    let mut cmd = Command::new(executable);
    cmd.args(args)
        .env("R_LIBS", library)
        .env("PATH", resolved_runtime_path(library, rscript)?);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(program_spawn_error(executable.as_os_str(), cmd.exec()).into())
    }

    #[cfg(not(unix))]
    {
        let status = cmd
            .status()
            .map_err(|e| program_spawn_error(executable.as_os_str(), e))?;
        Ok(status.code().unwrap_or(1))
    }
}

fn program_spawn_error(program: &OsStr, err: io::Error) -> String {
    if err.kind() == io::ErrorKind::NotFound {
        format!("could not find `{}`", program.to_string_lossy())
    } else {
        format!("failed to launch `{}`: {err}", program.to_string_lossy())
    }
}

/// Phase 2 — run `script` in an ordinary R session pointed at `library`.
///
/// The script runs as an ordinary `Rscript [Rscript-options...] script.R` — its
/// `.Renviron`, `.Rprofile` and site files are read unless the forwarded
/// Rscript options disable them. The resolved library is injected via `R_LIBS`,
/// which is *prepended* to `.libPaths()`: resolved dependencies take precedence,
/// while the user's other libraries remain available. (`R_LIBS` is used rather
/// than `R_LIBS_USER`, since a user `.Renviron` setting `R_LIBS_USER` would
/// override the latter.)
///
/// As `ir`'s final step, on Unix we `exec` into Rscript so R takes over this
/// process — inheriting our PID, stdio and signals, and propagating its exit
/// status (signal deaths included) verbatim. `exec` returns only on launch
/// failure. Without `exec` (Windows), R runs as a child and we return its code.
fn run_script(
    rscript: &OsStr,
    library: Option<&Path>,
    script: &Path,
    rscript_args: &[String],
    script_args: &[String],
) -> Result<i32, Box<dyn Error>> {
    let mut cmd = Command::new(rscript);
    cmd.args(rscript_args).arg(script).args(script_args);

    if let Some(lib) = library {
        cmd.env("R_LIBS", lib);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Replace ir with R; returns only if the exec fails.
        Err(spawn_error(rscript, cmd.exec()).into())
    }

    #[cfg(not(unix))]
    {
        let status = cmd.status().map_err(|e| spawn_error(rscript, e))?;
        Ok(status.code().unwrap_or(1))
    }
}

fn read_op_frontmatter_to_string(script: &Path) -> Result<String, Box<dyn Error>> {
    let file = File::open(script)?;
    let mut reader = BufReader::new(file);
    let mut frontmatter = String::new();
    let mut line = String::new();

    let mut read_next_line = |line: &mut String| {
        line.clear();
        reader.read_line(line)
    };

    read_next_line(&mut line)?;

    if line.starts_with("#!") {
        read_next_line(&mut line)?;
    }

    while let Some(rest) = line.strip_prefix("#| ") {
        frontmatter.push_str(rest);

        if read_next_line(&mut line)? == 0 {
            break;
        }
    }

    Ok(frontmatter)
}

/// The Rscript executable to use: `$IR_RSCRIPT` if set, otherwise `Rscript`
/// resolved via `PATH`.
fn rscript_command() -> std::ffi::OsString {
    env::var_os("IR_RSCRIPT").unwrap_or_else(|| "Rscript".into())
}

/// The `ir` cache root, matching `tools::R_user_dir("ir", "cache")` unless
/// `IR_CACHE_DIR` overrides it.
fn ir_cache_dir() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = nonempty_env("IR_CACHE_DIR") {
        return Ok(PathBuf::from(path));
    }

    let rscript = rscript_command();
    let output = Command::new(&rscript)
        .arg("-e")
        .arg("writeLines(tools::R_user_dir(\"ir\", \"cache\"))")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| spawn_error(&rscript, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("failed to resolve cache dir with tools::R_user_dir: {stderr}").into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let path = stdout.trim();
    if path.is_empty() {
        return Err("tools::R_user_dir returned an empty cache dir".into());
    }

    Ok(PathBuf::from(path))
}

fn nonempty_env(name: &str) -> Option<std::ffi::OsString> {
    env::var_os(name).filter(|value| !value.is_empty())
}

fn count_files(path: &Path) -> io::Result<u64> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Ok(1);
    }

    let mut files = 0;
    for entry in fs::read_dir(path)? {
        files += count_files(&entry?.path())?;
    }
    Ok(files)
}

/// A unique path in `dir` for this process, e.g. `ir-resolve-1234-987.R`.
fn unique_path(dir: &Path, prefix: &str, ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    dir.join(format!("{prefix}-{}-{nanos}.{ext}", std::process::id()))
}

/// Turn a failure to launch Rscript into an actionable message.
fn spawn_error(rscript: &OsStr, err: io::Error) -> String {
    if err.kind() == io::ErrorKind::NotFound {
        format!(
            "could not find `{}` on PATH. Install R, or set IR_RSCRIPT to its path.",
            rscript.to_string_lossy()
        )
    } else {
        format!("failed to launch `{}`: {err}", rscript.to_string_lossy())
    }
}

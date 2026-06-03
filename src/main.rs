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
//!   1. Rust extracts and parses the leading `#| ` YAML frontmatter block. A
//!      private R session (`driver/resolve.R`) receives the dependency specs on
//!      stdin, resolves them with pak, hashes the resolved set into a
//!      content-addressed library path under the cache directory, and
//!      materialises that path as a light-weight library of symlinks into renv's
//!      package cache. The path is reported back to us.
//!
//!   2. We launch the user's script in a fresh, isolated R session whose
//!      library path is exactly that library plus base R.

use std::cmp::Ordering;
use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use saphyr::{LoadableYamlNode, Yaml};
use serde::Deserialize;

/// The R resolution driver, embedded at compile time so `ir` ships as one
/// self-contained binary while the source stays editable as real R.
const RESOLVE_DRIVER: &str = include_str!("../driver/resolve.R");

/// `rig available --json`, captured when this embedded dataset was refreshed.
/// Older `exclude after` dates can resolve R versions without consulting the
/// filesystem or making rig's network-backed `available` call.
const EMBEDDED_R_AVAILABLE_BUILD_DATE: &str = "2026-06-03";
const EMBEDDED_R_AVAILABLE: &[AvailableRVersion<'static>] = &[
    AvailableRVersion {
        date: "2022-03-10",
        version: "4.1.3",
    },
    AvailableRVersion {
        date: "2023-03-15",
        version: "4.2.3",
    },
    AvailableRVersion {
        date: "2024-02-29",
        version: "4.3.3",
    },
    AvailableRVersion {
        date: "2025-02-28",
        version: "4.4.3",
    },
    AvailableRVersion {
        date: "2026-03-11",
        version: "4.5.3",
    },
    AvailableRVersion {
        date: "2026-04-24",
        version: "4.6.0",
    },
];

#[derive(Debug, Default)]
struct ScriptSpec {
    dependencies: Vec<String>,
    exclude_after: Option<String>,
    r_requirement: Option<String>,
}

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
            cmd_run(
                &run.source,
                &run.rscript_args,
                &run.with_deps,
                &run.script_args,
            )
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

/// Where the user's program comes from: a script file, or one or more inline
/// `-e` expressions evaluated in its place (mirroring `Rscript -e`).
enum RunSource {
    Script(String),
    Expressions(Vec<String>),
}

struct RunArgs {
    rscript_args: Vec<String>,
    with_deps: Vec<String>,
    source: RunSource,
    script_args: Vec<String>,
}

/// Split the leading region of `ir run`'s arguments into Rscript options,
/// `--with` dependency specs, and the program source (a script path or `-e`
/// expressions), with everything after the source treated as program args.
///
/// `-e <expr>` and `--with <spec>` are `ir`-level flags handled here: `-e`
/// supplies inline R to run instead of a file, and `--with` declares extra
/// dependencies (not forwarded to Rscript). Any other `-…` argument is an
/// Rscript option, forwarded verbatim to the user-code phase. Scanning stops at
/// the first non-option, which is the script path unless `-e` was given (in
/// which case it, and everything after, are program args — as with Rscript).
fn parse_run_args(args: Vec<String>) -> Result<RunArgs, Box<dyn Error>> {
    let mut rscript_args = Vec::new();
    let mut with_deps = Vec::new();
    let mut expressions = Vec::new();
    let mut iter = args.into_iter();
    let mut positional = None;

    while let Some(arg) = iter.next() {
        if arg == "-e" {
            let expr = iter
                .next()
                .ok_or("`-e` requires an expression (try `ir run -e '1 + 1'`)")?;
            expressions.push(expr);
        } else if arg == "--with" {
            let value = iter
                .next()
                .ok_or("`--with` requires a package (try `ir run --with dplyr script.R`)")?;
            push_with_deps(&mut with_deps, &value);
        } else if let Some(value) = arg.strip_prefix("--with=") {
            push_with_deps(&mut with_deps, value);
        } else if arg.starts_with('-') {
            rscript_args.push(arg);
        } else {
            positional = Some(arg);
            break;
        }
    }

    let script_args: Vec<String> = iter.collect();

    let (source, script_args) = if expressions.is_empty() {
        let script = positional
            .ok_or("`ir run` requires a script path or -e expression (try `ir run script.R`)")?;
        (RunSource::Script(script), script_args)
    } else {
        // With `-e`, there is no script file; the first non-option and anything
        // after it are program args (commandArgs), matching Rscript.
        let mut program_args = Vec::new();
        program_args.extend(positional);
        program_args.extend(script_args);
        (RunSource::Expressions(expressions), program_args)
    };

    Ok(RunArgs {
        rscript_args,
        with_deps,
        source,
        script_args,
    })
}

/// Append the dependency specs in a `--with` value, which may be a single spec
/// or a comma-separated list, to `with_deps`. Blank entries are ignored.
fn push_with_deps(with_deps: &mut Vec<String>, value: &str) {
    for dep in value.split(',') {
        let dep = dep.trim();
        if !dep.is_empty() {
            with_deps.push(dep.to_string());
        }
    }
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
            "    ir run [Rscript-options...] [--with <pkg>]... <script.R> [args...]\n",
            "    ir run [Rscript-options...] [--with <pkg>]... -e <expr> [args...]\n",
            "    ir cache <command>\n",
            "\n",
            "`ir run` reads the YAML frontmatter from <script.R>, resolves its\n",
            "dependencies, builds a dedicated package library, and runs the script\n",
            "against it. With -e it evaluates inline R expressions instead of a file,\n",
            "and --with adds dependencies on the command line. Leading Rscript options\n",
            "are passed to Rscript for the user-code phase; trailing args are passed\n",
            "through to the program. `ir cache` manages the dependency resolution and\n",
            "materialised library cache.\n",
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
        "    ir run [Rscript-options...] [--with <pkg>]... <script.R> [args...]\n",
        "    ir run [Rscript-options...] [--with <pkg>]... -e <expr> [-e <expr>]... [args...]\n",
        "\n",
        "`ir run` reads the YAML frontmatter from <script.R>, resolves its\n",
        "dependencies, builds a dedicated package library, and runs the script\n",
        "against it. With -e it instead evaluates inline R expressions (mirroring\n",
        "Rscript) against the same isolated library. Leading Rscript options are\n",
        "passed to Rscript for the user-code phase; trailing args are passed\n",
        "through to the program.\n",
        "\n",
        "OPTIONS:\n",
        "    -e <expr>     Evaluate an inline R expression instead of a script file.\n",
        "                  May be repeated; runs in place of <script.R>.\n",
        "    --with <pkg>  Add a dependency for this run, merged with any declared\n",
        "                  in the script frontmatter. May be repeated and accepts a\n",
        "                  comma-separated list (e.g. --with dplyr,tidyr). Uses the\n",
        "                  same spec format as `dependencies:` (e.g. cli==3.6.6).\n",
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

/// Resolve dependencies for `source`, then run it against the resulting
/// library. Exits the process with the program's own exit code.
fn cmd_run(
    source: &RunSource,
    rscript_args: &[String],
    with_deps: &[String],
    script_args: &[String],
) -> Result<(), Box<dyn Error>> {
    // A script file declares its dependencies (and `exclude after` / `R`) in
    // YAML frontmatter and is canonicalised so the run is independent of the
    // working directory. An inline `-e` expression has no frontmatter; its deps
    // come solely from `--with`.
    let (script_path, mut spec) = match source {
        RunSource::Script(script) => {
            let path = fs::canonicalize(script)
                .map_err(|e| format!("cannot read script `{script}`: {e}"))?;
            let spec = read_script_spec(&path)?;
            (Some(path), spec)
        }
        RunSource::Expressions(_) => (None, ScriptSpec::default()),
    };
    spec.dependencies.extend(with_deps.iter().cloned());
    let rscript = rscript_command_for_spec(&spec)?;

    // Phase 1: private R session resolves deps and materialises the library.
    // Rust parses the frontmatter and sends the dependency specs on stdin.
    let library = resolve_library(&rscript, &spec)?;

    // Phase 2: run the user's program in an isolated R session.
    let expressions: &[String] = match source {
        RunSource::Expressions(exprs) => exprs,
        RunSource::Script(_) => &[],
    };
    let code = run_script(
        &rscript,
        library.as_deref(),
        script_path.as_deref(),
        expressions,
        rscript_args,
        script_args,
    )?;
    std::process::exit(code);
}

/// Phase 1 — run the embedded driver in a private R session and return the
/// path to the materialised library. The dependency specs in `spec` (the
/// script's frontmatter plus any `--with` packages) are streamed on stdin.
fn resolve_library(rscript: &OsStr, spec: &ScriptSpec) -> Result<Option<PathBuf>, Box<dyn Error>> {
    let tmp = env::temp_dir();
    let driver = unique_path(&tmp, "ir-resolve", "R");
    let result_file = unique_path(&tmp, "ir-libpath", "txt");
    fs::write(&driver, RESOLVE_DRIVER)?;

    let mut cmd = Command::new(rscript);
    cmd.arg(&driver)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .env("IR_RESOLVE_RESULT_FILE", &result_file)
        // pak suppresses progress in noninteractive Rscript unless this is set.
        // Resolution cache hits return before pak, so this adds no cache-hit pak output.
        .env("R_PKG_SHOW_PROGRESS", "true");
    if let Some(exclude_after) = &spec.exclude_after {
        cmd.env("IR_EXCLUDE_AFTER", exclude_after);
    }
    if let Some(r_requirement) = &spec.r_requirement {
        cmd.env("IR_R_REQUIREMENT", r_requirement);
    }

    let mut child = cmd.spawn().map_err(|e| spawn_error(rscript, e))?;
    {
        let mut stdin = child.stdin.take().ok_or("failed to open resolver stdin")?;
        for dependency in &spec.dependencies {
            writeln!(stdin, "{dependency}")?;
        }
    }
    let status = child
        .wait()
        .map_err(|e| format!("failed to wait for dependency resolver: {e}"))?;

    let _ = fs::remove_file(&driver);
    let result = fs::read_to_string(&result_file).unwrap_or_default();
    let _ = fs::remove_file(&result_file);

    if !status.success() {
        return Err("dependency resolution failed".into());
    }

    let path = result.trim();
    Ok(if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    })
}

fn read_script_spec(script: &Path) -> Result<ScriptSpec, Box<dyn Error>> {
    parse_frontmatter(&read_op_frontmatter_to_string(script)?)
}

fn parse_frontmatter(frontmatter: &str) -> Result<ScriptSpec, Box<dyn Error>> {
    if frontmatter.trim().is_empty() {
        return Ok(ScriptSpec::default());
    }

    let docs = Yaml::load_from_str(frontmatter)
        .map_err(|e| format!("could not parse script frontmatter as YAML: {e}"))?;
    if docs.len() != 1 {
        return Err("script frontmatter must contain exactly one YAML document".into());
    }
    if docs[0].is_null() {
        return Ok(ScriptSpec::default());
    }

    let doc = &docs[0];
    if !doc.is_mapping() {
        return Err("script frontmatter must be a YAML mapping".into());
    }

    Ok(ScriptSpec {
        dependencies: frontmatter_dependencies(doc)?,
        exclude_after: frontmatter_optional_string(doc, "exclude after")?,
        r_requirement: frontmatter_optional_string(doc, "R")?,
    })
}

fn frontmatter_dependencies(doc: &Yaml<'_>) -> Result<Vec<String>, Box<dyn Error>> {
    let Some(value) = doc.as_mapping_get("dependencies") else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }

    let mut dependencies = Vec::new();
    if let Some(seq) = value.as_vec() {
        for item in seq {
            push_dependency_words(&mut dependencies, item)?;
        }
    } else {
        push_dependency_words(&mut dependencies, value)?;
    }
    Ok(dependencies)
}

fn push_dependency_words(
    dependencies: &mut Vec<String>,
    value: &Yaml<'_>,
) -> Result<(), Box<dyn Error>> {
    let Some(value) = value.as_str() else {
        return Err("frontmatter `dependencies` entries must be strings".into());
    };
    dependencies.extend(value.split_whitespace().map(str::to_owned));
    Ok(())
}

fn frontmatter_optional_string(
    doc: &Yaml<'_>,
    key: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    let Some(value) = doc.as_mapping_get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }

    let Some(value) = value.as_str() else {
        return Err(format!("frontmatter `{key}` must be a string").into());
    };
    let value = value.trim();
    Ok(if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    })
}

/// Phase 2 — run the user's program in an ordinary R session pointed at
/// `library`. The program is either a script file (`script`) or, when that is
/// `None`, the inline `expressions` evaluated via `Rscript -e` in its place.
///
/// It runs as an ordinary `Rscript [Rscript-options...] (script.R | -e expr…)` —
/// its `.Renviron`, `.Rprofile` and site files are read unless the forwarded
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
    script: Option<&Path>,
    expressions: &[String],
    rscript_args: &[String],
    script_args: &[String],
) -> Result<i32, Box<dyn Error>> {
    let mut cmd = Command::new(rscript);
    cmd.args(rscript_args);
    match script {
        Some(script) => {
            cmd.arg(script);
        }
        None => {
            for expr in expressions {
                cmd.arg("-e").arg(expr);
            }
        }
    }
    cmd.args(script_args);

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

fn rscript_command_for_spec(spec: &ScriptSpec) -> Result<std::ffi::OsString, Box<dyn Error>> {
    if let Some(rscript) = nonempty_env("IR_RSCRIPT") {
        return Ok(rscript);
    }

    let (Some(r_requirement), Some(exclude_after)) =
        (spec.r_requirement.as_deref(), spec.exclude_after.as_deref())
    else {
        return Ok("Rscript".into());
    };
    let Some(requirement) = RRequirement::parse(r_requirement) else {
        return Ok("Rscript".into());
    };

    let exclude_after = SimpleDate::parse(exclude_after, "`exclude after`")?;
    let selected = select_r_version(&requirement, exclude_after)?;
    installed_rscript_for_r_version(&selected)
}

#[derive(Debug, Deserialize)]
struct CachedRigAvailableVersion {
    date: Option<String>,
    version: String,
}

#[derive(Clone, Copy, Debug)]
struct AvailableRVersion<'a> {
    date: &'a str,
    version: &'a str,
}

#[derive(Debug, Deserialize)]
struct RigInstalledVersion {
    name: String,
    version: Option<String>,
    binary: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SimpleDate {
    year: u16,
    month: u8,
    day: u8,
}

impl SimpleDate {
    fn parse(value: &str, label: &str) -> Result<Self, Box<dyn Error>> {
        let value = value.trim();
        let bytes = value.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return Err(format!("{label} must be a date string in YYYY-MM-DD format").into());
        }

        let Some(year) = parse_digits_u16(&bytes[0..4]) else {
            return Err(format!("{label} must be a date string in YYYY-MM-DD format").into());
        };
        let Some(month) = parse_digits_u8(&bytes[5..7]) else {
            return Err(format!("{label} must be a date string in YYYY-MM-DD format").into());
        };
        let Some(day) = parse_digits_u8(&bytes[8..10]) else {
            return Err(format!("{label} must be a date string in YYYY-MM-DD format").into());
        };

        if month == 0 || month > 12 {
            return Err(format!("{label} must be a date string in YYYY-MM-DD format").into());
        }
        let max_day = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if is_leap_year(year) => 29,
            2 => 28,
            _ => unreachable!(),
        };
        if day == 0 || day > max_day {
            return Err(format!("{label} must be a date string in YYYY-MM-DD format").into());
        }

        Ok(Self { year, month, day })
    }
}

fn parse_digits_u16(bytes: &[u8]) -> Option<u16> {
    let mut value = 0u16;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + u16::from(byte - b'0');
    }
    Some(value)
}

fn parse_digits_u8(bytes: &[u8]) -> Option<u8> {
    let value = parse_digits_u16(bytes)?;
    u8::try_from(value).ok()
}

fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RVersion(Vec<u32>);

impl RVersion {
    fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        for part in value.split('.') {
            if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            parts.push(part.parse().ok()?);
        }
        Some(Self(parts))
    }
}

impl Ord for RVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        let len = self.0.len().max(other.0.len());
        for idx in 0..len {
            let left = self.0.get(idx).copied().unwrap_or(0);
            let right = other.0.get(idx).copied().unwrap_or(0);
            match left.cmp(&right) {
                Ordering::Equal => {}
                ordering => return ordering,
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for RVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug)]
enum VersionOp {
    GreaterEqual,
    Greater,
    LessEqual,
    Less,
    Equal,
}

#[derive(Clone, Debug)]
struct RRequirement {
    op: VersionOp,
    version: RVersion,
}

impl RRequirement {
    fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let (op, rest) = if let Some(rest) = value.strip_prefix(">=") {
            (VersionOp::GreaterEqual, rest)
        } else if let Some(rest) = value.strip_prefix(">") {
            (VersionOp::Greater, rest)
        } else if let Some(rest) = value.strip_prefix("<=") {
            (VersionOp::LessEqual, rest)
        } else if let Some(rest) = value.strip_prefix("<") {
            (VersionOp::Less, rest)
        } else if let Some(rest) = value.strip_prefix("==") {
            (VersionOp::Equal, rest)
        } else {
            (VersionOp::GreaterEqual, value)
        };

        Some(Self {
            op,
            version: RVersion::parse(rest.trim())?,
        })
    }

    fn matches(&self, version: &RVersion) -> bool {
        match self.op {
            VersionOp::GreaterEqual => version >= &self.version,
            VersionOp::Greater => version > &self.version,
            VersionOp::LessEqual => version <= &self.version,
            VersionOp::Less => version < &self.version,
            VersionOp::Equal => version == &self.version,
        }
    }
}

fn select_r_version(
    requirement: &RRequirement,
    exclude_after: SimpleDate,
) -> Result<String, Box<dyn Error>> {
    let embedded_build_date = SimpleDate::parse(
        EMBEDDED_R_AVAILABLE_BUILD_DATE,
        "embedded R version build date",
    )?;
    if exclude_after <= embedded_build_date {
        return select_r_version_from_available(
            requirement,
            exclude_after,
            EMBEDDED_R_AVAILABLE.iter().copied(),
        );
    }

    let cached = cached_rig_available_versions()?;
    let versions = cached.iter().filter_map(|available| {
        Some(AvailableRVersion {
            date: available.date.as_deref()?,
            version: &available.version,
        })
    });
    select_r_version_from_available(requirement, exclude_after, versions)
}

fn select_r_version_from_available<'a>(
    requirement: &RRequirement,
    exclude_after: SimpleDate,
    versions: impl IntoIterator<Item = AvailableRVersion<'a>>,
) -> Result<String, Box<dyn Error>> {
    let mut best: Option<(RVersion, String)> = None;

    for available in versions {
        let date = SimpleDate::parse(
            available
                .date
                .get(..10)
                .ok_or("rig available date must start with YYYY-MM-DD")?,
            "rig available date",
        )?;
        if date > exclude_after {
            continue;
        }

        let version = RVersion::parse(available.version).ok_or_else(|| {
            format!(
                "rig available returned invalid R version `{}`",
                available.version
            )
        })?;
        if requirement.matches(&version)
            && best
                .as_ref()
                .map(|(best_version, _)| version > *best_version)
                .unwrap_or(true)
        {
            best = Some((version, available.version.to_owned()));
        }
    }

    best.map(|(_, version)| version)
        .ok_or_else(|| "no available R version matches `R:` before `exclude after`".into())
}

fn cached_rig_available_versions() -> Result<Vec<CachedRigAvailableVersion>, Box<dyn Error>> {
    let json = cached_rig_available_json()?;
    Ok(serde_json::from_str(&json)?)
}

fn cached_rig_available_json() -> Result<String, Box<dyn Error>> {
    let cache_file = ir_cache_dir()?.join("rig").join("available.json");
    if cache_file.exists() {
        return Ok(fs::read_to_string(&cache_file)?);
    }

    let json = fetch_rig_available_json()?;
    let _: Vec<CachedRigAvailableVersion> = serde_json::from_str(&json)?;
    dir_create_parent(&cache_file)?;
    fs::write(&cache_file, &json)?;
    Ok(json)
}

fn fetch_rig_available_json() -> Result<String, Box<dyn Error>> {
    let output = Command::new("rig")
        .args(["available", "--json"])
        .stdin(Stdio::null())
        .output()
        .map_err(rig_spawn_error)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("failed to run `rig available --json`: {stderr}").into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn installed_rscript_for_r_version(version: &str) -> Result<std::ffi::OsString, Box<dyn Error>> {
    let output = Command::new("rig")
        .args(["list", "--json"])
        .stdin(Stdio::null())
        .output()
        .map_err(rig_spawn_error)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("failed to run `rig list --json`: {stderr}").into());
    }

    let installed: Vec<RigInstalledVersion> = serde_json::from_slice(&output.stdout)?;
    let Some(installed) = installed
        .iter()
        .find(|installed| installed.version.as_deref() == Some(version))
        .or_else(|| installed.iter().find(|installed| installed.name == version))
    else {
        return Err(format!(
            "R {version} is required by `R:` and `exclude after`, but rig does not list it as installed. Install it with `rig add {version}`."
        )
        .into());
    };
    let binary = installed
        .binary
        .as_deref()
        .ok_or_else(|| format!("rig did not report an R binary for R {version}"))?;
    let rscript = rscript_from_r_binary(Path::new(binary))?;
    if !rscript.exists() {
        return Err(format!("derived Rscript path does not exist: {}", rscript.display()).into());
    }

    Ok(rscript.into_os_string())
}

fn rscript_from_r_binary(binary: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let file_name = binary
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| format!("invalid R binary path `{}`", binary.display()))?;
    let rscript_name = if file_name == "R" {
        "Rscript"
    } else if file_name.eq_ignore_ascii_case("R.exe") {
        "Rscript.exe"
    } else {
        return Err(format!(
            "rig reported unsupported R binary path `{}`",
            binary.display()
        )
        .into());
    };

    let parent = binary
        .parent()
        .ok_or_else(|| format!("invalid R binary path `{}`", binary.display()))?;
    if parent.file_name().and_then(OsStr::to_str) == Some("bin") {
        return Ok(parent.join(rscript_name));
    }
    if parent.file_name().and_then(OsStr::to_str) == Some("Resources") {
        return Ok(parent.join("bin").join(rscript_name));
    }

    Err(format!(
        "rig reported unsupported R binary path `{}`",
        binary.display()
    )
    .into())
}

fn dir_create_parent(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn rig_spawn_error(err: io::Error) -> String {
    if err.kind() == io::ErrorKind::NotFound {
        "could not find `rig` on PATH. Install rig, or set IR_RSCRIPT to an explicit Rscript path."
            .to_string()
    } else {
        format!("failed to launch `rig`: {err}")
    }
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

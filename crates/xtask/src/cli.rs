// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Build, stage, self-verify, run and install the one self-contained `gmeow` binary.
//!
//! `build` runs the producer once (unless `--skip-producer`), then compiles the
//! consumer under a forced portable recipe and stages `dist/bin/gmeow` with a
//! receipt. Before the staged pair is published, the candidate binary reports its
//! own identity from a scratch directory with a cleared environment, and every
//! digest and stamp it reports must equal the receipt. `install` copies a current,
//! verified staged binary into a directory by atomic rename. No corpus is produced
//! here other than through the producer runner's own `sync`.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use gmeow_action_cache::executable::sha256_file;
use gmeow_errors::Diag;
use serde_json::{Value, json};

use crate::producer::{self, Result};

/// The CPU baseline every distributable `gmeow` is compiled for.
const PORTABLE_CPU: &str = "x86-64-v3";
/// The C/C++ flags matching [`PORTABLE_CPU`] for vendored native code.
const PORTABLE_C_FLAGS: &str = "-O2 -march=x86-64-v3 -mtune=generic";
/// The host-tuned C/C++ flags `--native` selects.
const NATIVE_C_FLAGS: &str = "-O2 -march=native -mtune=native";
/// The only host a portable build is defined for.
const PORTABLE_TARGET: &str = "x86_64-unknown-linux-gnu";
/// The producer command that materializes both embedded payloads.
const SYNC_COMMAND: &str = "run -- sync --mode update --outputs generated";

/// Attach consumer-build context to the shared typed diagnostic.
fn fail(error: impl std::fmt::Display) -> Diag {
    crate::evidence::failure(format!("cli: {error}"))
}

/// Dispatch a `cargo xtask cli` operation, printing typed failures.
pub(super) fn command(args: Vec<String>) -> ExitCode {
    match parse(&args).and_then(|operation| execute(&crate::workspace_root(), operation)) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "expected: build [--skip-producer] [--allow-dirty] [--native] | run -- ARGS | \
                     install [--bin-dir DIR] [--allow-dirty] [--allow-native]";

/// Options selecting how the consumer is built.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct BuildOptions {
    skip_producer: bool,
    allow_dirty: bool,
    native: bool,
}

/// What an `install` is explicitly permitted to place on a PATH.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InstallOptions {
    bin_dir: Option<PathBuf>,
    allow_dirty: bool,
    allow_native: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Operation {
    Build(BuildOptions),
    Run(Vec<String>),
    Install(InstallOptions),
}

/// Parse the operation and its flags; any unknown or repeated argument is refused.
fn parse(args: &[String]) -> Result<Operation> {
    let Some((operation, rest)) = args.split_first() else {
        return Err(fail(USAGE));
    };
    let mut seen = BTreeSet::new();
    let mut flag = |name: &str| -> Result<()> {
        if seen.insert(name.to_owned()) {
            Ok(())
        } else {
            Err(fail(format!("{name} given twice")))
        }
    };
    match operation.as_str() {
        "build" => {
            let mut options = BuildOptions::default();
            for arg in rest {
                flag(arg)?;
                match arg.as_str() {
                    "--skip-producer" => options.skip_producer = true,
                    "--allow-dirty" => options.allow_dirty = true,
                    "--native" => options.native = true,
                    _ => return Err(fail(format!("unknown build argument {arg:?}; {USAGE}"))),
                }
            }
            Ok(Operation::Build(options))
        }
        "run" => match rest.split_first() {
            Some((separator, program_args)) if separator == "--" => {
                Ok(Operation::Run(program_args.to_vec()))
            }
            _ => Err(fail(format!(
                "run requires `--` before the gmeow arguments; {USAGE}"
            ))),
        },
        "install" => {
            let mut options = InstallOptions {
                bin_dir: None,
                allow_dirty: false,
                allow_native: false,
            };
            let mut iter = rest.iter();
            while let Some(arg) = iter.next() {
                flag(arg)?;
                match arg.as_str() {
                    "--bin-dir" => {
                        let dir = iter
                            .next()
                            .ok_or_else(|| fail("--bin-dir requires a directory"))?;
                        options.bin_dir = Some(PathBuf::from(dir));
                    }
                    "--allow-dirty" => options.allow_dirty = true,
                    "--allow-native" => options.allow_native = true,
                    _ => {
                        return Err(fail(format!("unknown install argument {arg:?}; {USAGE}")));
                    }
                }
            }
            Ok(Operation::Install(options))
        }
        _ => Err(fail(USAGE)),
    }
}

fn execute(root: &Path, operation: Operation) -> Result<ExitCode> {
    let staged = root.join("dist/bin/gmeow");
    match operation {
        Operation::Build(options) => {
            build(root, &staged, options)?;
            Ok(ExitCode::SUCCESS)
        }
        Operation::Run(args) => {
            ensure_current(root, &staged, false)?;
            let status = Command::new(&staged).args(&args).status().map_err(fail)?;
            Ok(ExitCode::from(
                u8::try_from(status.code().unwrap_or(1)).unwrap_or(1),
            ))
        }
        Operation::Install(options) => {
            let bin_dir = match options.bin_dir {
                Some(dir) => absolute(&dir)?,
                None => std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".local/bin"))
                    .ok_or_else(|| fail("HOME is unset; pass --bin-dir DIR"))?,
            };
            let receipt = ensure_current(root, &staged, options.allow_dirty)?;
            admit_install(&receipt, options.allow_dirty, options.allow_native)?;
            let (installed, sha256) = install(&staged, &bin_dir, &receipt)?;
            println!("installed {} sha256 {sha256}", installed.display());
            Ok(ExitCode::SUCCESS)
        }
    }
}

// ── source identity ─────────────────────────────────────────────────────────

/// Whether the source checkout matched its commit when the build read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tree {
    Clean,
    Dirty,
}

impl Tree {
    fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Dirty => "dirty",
        }
    }
}

/// The checkout's commit and tree state; untracked unignored files count as dirty.
fn source_state(root: &Path) -> Result<(String, Tree)> {
    let mut head = Command::new("git");
    head.arg("-C").arg(root).args(["rev-parse", "HEAD"]);
    let revision = producer::checked_output(head)?.trim().to_owned();
    if revision.len() != 40 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(fail(format!(
            "git HEAD {revision:?} is not a 40-hex commit"
        )));
    }
    let mut status = Command::new("git");
    status
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain=v1", "--untracked-files=normal"]);
    let tree = if producer::checked_output(status)?.trim().is_empty() {
        Tree::Clean
    } else {
        Tree::Dirty
    };
    Ok((revision, tree))
}

// ── the build recipe ────────────────────────────────────────────────────────

/// Everything the recipe is computed from, gathered from the host beforehand.
#[derive(Debug, Clone)]
struct RecipeInputs {
    /// Rustflags Cargo configuration would otherwise apply, already admitted.
    admitted: Vec<String>,
    root: PathBuf,
    cargo_home: PathBuf,
    target_dir: PathBuf,
    build_dir: Option<PathBuf>,
    native: bool,
    revision: String,
    tree: Tree,
    bundle: PathBuf,
    codebook: PathBuf,
}

/// The exact compilation recipe: what Cargo, rustc, cc and `build.rs` receive.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Recipe {
    rustflags: Vec<String>,
    c_flags: &'static str,
    cpu_baseline: &'static str,
    revision: String,
    tree: Tree,
    bundle: PathBuf,
    codebook: PathBuf,
}

/// Whether one (possibly two-token) rustflag selects a CPU.
fn names_target_cpu(flag: &str) -> bool {
    flag.trim_start_matches("--codegen=")
        .trim_start_matches("--codegen")
        .trim_start_matches("-C")
        .trim_start()
        .starts_with("target-cpu")
}

/// Compute the forced recipe. Any CPU selection inherited from configuration is
/// removed and replaced, so this recipe's baseline is the one rustc receives.
fn recipe(inputs: &RecipeInputs) -> Recipe {
    let mut rustflags = Vec::with_capacity(inputs.admitted.len() + 8);
    let mut tokens = inputs.admitted.iter().peekable();
    while let Some(flag) = tokens.next() {
        if (flag == "-C" || flag == "--codegen")
            && tokens.peek().is_some_and(|value| names_target_cpu(value))
        {
            tokens.next();
            continue;
        }
        if names_target_cpu(flag) {
            continue;
        }
        rustflags.push(flag.clone());
    }
    let cpu_baseline = if inputs.native {
        "native"
    } else {
        PORTABLE_CPU
    };
    rustflags.push(format!("-Ctarget-cpu={cpu_baseline}"));
    rustflags.push("-Cstrip=symbols".to_owned());
    rustflags.push("-Dwarnings".to_owned());
    // Later remaps win in rustc, so the more specific build directories come last.
    let mut remaps = vec![
        (inputs.root.clone(), "/gmeow"),
        (inputs.cargo_home.clone(), "/cargo"),
        (inputs.target_dir.clone(), "/target"),
    ];
    if let Some(build_dir) = &inputs.build_dir {
        remaps.push((build_dir.clone(), "/build"));
    }
    for (from, to) in remaps {
        rustflags.push(format!("--remap-path-prefix={}={to}", from.display()));
    }
    Recipe {
        rustflags,
        c_flags: if inputs.native {
            NATIVE_C_FLAGS
        } else {
            PORTABLE_C_FLAGS
        },
        cpu_baseline,
        revision: inputs.revision.clone(),
        tree: inputs.tree,
        bundle: inputs.bundle.clone(),
        codebook: inputs.codebook.clone(),
    }
}

/// The unexecuted Cargo command for `recipe`. The environment set here takes
/// precedence over every Cargo configuration source, including a wrapper's
/// `--config build.rustflags`, because `CARGO_ENCODED_RUSTFLAGS` outranks them all.
fn cargo_command(cargo: &OsStr, root: &Path, recipe: &Recipe) -> Command {
    let mut command = Command::new(cargo);
    command
        .current_dir(root)
        .env("CARGO_ENCODED_RUSTFLAGS", recipe.rustflags.join("\u{1f}"))
        .env_remove("RUSTFLAGS")
        .env("GMEOW_SOURCE_REVISION", &recipe.revision)
        .env("GMEOW_SOURCE_TREE", recipe.tree.as_str())
        .env("GMEOW_BUILD_CPU_BASELINE", recipe.cpu_baseline)
        .env("GMEOW_BUNDLE_PATH", &recipe.bundle)
        .env("GMEOW_CODEBOOK_PATH", &recipe.codebook)
        .args([
            "--config",
            &format!("env.CFLAGS.value={:?}", recipe.c_flags),
        ])
        .args([
            "--config",
            &format!("env.CXXFLAGS.value={:?}", recipe.c_flags),
        ])
        .args([
            "--config",
            "env.CFLAGS.force=true",
            "--config",
            "env.CXXFLAGS.force=true",
        ])
        .args(["--config", "profile.release.strip=\"symbols\""])
        .args([
            "build",
            "--locked",
            "--release",
            "-p",
            "gmeow-cli",
            "--bin",
            "gmeow",
            "--message-format=json-render-diagnostics",
        ]);
    command
}

/// Refuse environment variables through which cc would see other C flags.
fn refuse_competing_c_flags() -> Result<()> {
    for (name, _) in std::env::vars() {
        if name.starts_with("CFLAGS_")
            || name.starts_with("CXXFLAGS_")
            || [
                "TARGET_CFLAGS",
                "TARGET_CXXFLAGS",
                "HOST_CFLAGS",
                "HOST_CXXFLAGS",
            ]
            .contains(&name.as_str())
        {
            return Err(fail(format!(
                "{name} would override the forced C flags; unset it"
            )));
        }
    }
    Ok(())
}

/// An embedded payload path: the caller's override, or the producer's default.
fn payload_path(root: &Path, env_name: &str, default: &str) -> Result<PathBuf> {
    let path = match std::env::var_os(env_name) {
        Some(value) => absolute(Path::new(&value))?,
        None => root.join(default),
    };
    let metadata = std::fs::metadata(&path)
        .map_err(|error| fail(format!("{env_name} payload {}: {error}", path.display())))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(fail(format!(
            "{env_name} payload {} is empty or not a regular file",
            path.display()
        )));
    }
    path.canonicalize().map_err(fail)
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir().map_err(fail)?.join(path))
    }
}

/// The two embedded payload paths a build of this checkout would read.
fn payload_paths(root: &Path) -> Result<(PathBuf, PathBuf)> {
    Ok((
        payload_path(root, "GMEOW_BUNDLE_PATH", "generated/dist/gmeow.gts")?,
        payload_path(
            root,
            "GMEOW_CODEBOOK_PATH",
            "generated/projections/lang/gmn-codebook.cbor",
        )?,
    ))
}

fn rustc_program() -> String {
    std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into())
}

/// The host triple rustc compiles for when no `--target` is given.
fn host_triple(rustc: &str) -> Result<String> {
    producer::identity(rustc, &["-vV"])?
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .ok_or_else(|| fail("rustc -vV reports no host"))
}

/// The target features rustc enables under exactly these rustflags.
fn target_features(rustc: &str, rustflags: &[String]) -> Result<BTreeSet<String>> {
    let mut command = Command::new(rustc);
    command.args(["--print", "cfg"]).args(rustflags);
    Ok(producer::checked_output(command)?
        .lines()
        .filter_map(|line| line.strip_prefix("target_feature=\""))
        .filter_map(|rest| rest.strip_suffix('"'))
        .map(str::to_owned)
        .collect())
}

/// The version Cargo.lock selects for the single package `name`.
fn locked_version(lock: &str, name: &str) -> Result<String> {
    let wanted = format!("name = \"{name}\"");
    let versions: Vec<_> = lock
        .split("[[package]]")
        .filter_map(|block| {
            let mut lines = block.lines().map(str::trim);
            lines.find(|line| *line == wanted)?;
            lines
                .find_map(|line| line.strip_prefix("version = \""))
                .and_then(|rest| rest.strip_suffix('"'))
                .map(str::to_owned)
        })
        .collect();
    match versions.as_slice() {
        [version] => Ok(version.clone()),
        _ => Err(fail(format!(
            "Cargo.lock must select exactly one {name} package, found {versions:?}"
        ))),
    }
}

/// Cargo's resolved target and build directories for the workspace.
fn cargo_directories(root: &Path) -> Result<(PathBuf, Option<PathBuf>)> {
    let mut command = Command::new(producer::cargo_program());
    command
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"]);
    let metadata: Value =
        serde_json::from_str(&producer::checked_output(command)?).map_err(fail)?;
    let target = metadata["target_directory"]
        .as_str()
        .map(PathBuf::from)
        .ok_or_else(|| fail("cargo metadata reports no target_directory"))?;
    let build = metadata["build_directory"]
        .as_str()
        .map(PathBuf::from)
        .filter(|build| build != &target);
    Ok((target, build))
}

/// The value of the configured rustc wrapper, which the receipt records verbatim.
fn rustc_wrapper(config: &cargo_config2::Config) -> Value {
    for name in ["RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER"] {
        if let Ok(value) = std::env::var(name) {
            return json!({ "source": name, "value": value });
        }
    }
    match &config.build.rustc_wrapper {
        Some(path) => {
            json!({ "source": "cargo config build.rustc-wrapper", "value": path.display().to_string() })
        }
        None => Value::Null,
    }
}

// ── build ───────────────────────────────────────────────────────────────────

/// Produce (unless skipped), compile under the forced recipe, verify, and stage.
fn build(root: &Path, staged: &Path, options: BuildOptions) -> Result<Value> {
    let (revision, tree) = source_state(root)?;
    if tree == Tree::Dirty && !options.allow_dirty {
        return Err(fail(
            "the checkout has uncommitted or untracked changes; commit them, or pass \
             --allow-dirty to build a binary recorded as `dirty`",
        ));
    }
    let rustc = rustc_program();
    let host = host_triple(&rustc)?;
    if !options.native && host != PORTABLE_TARGET {
        return Err(fail(format!(
            "the portable {PORTABLE_CPU} baseline is defined for {PORTABLE_TARGET}, not {host}; \
             pass --native for a host-tuned build"
        )));
    }
    refuse_competing_c_flags()?;
    if !options.skip_producer {
        let args: Vec<String> = SYNC_COMMAND.split_whitespace().map(str::to_owned).collect();
        if producer::execute(&args)? != ExitCode::SUCCESS {
            return Err(fail("the producer sync failed; no consumer was built"));
        }
    }
    let (bundle, codebook) = payload_paths(root)?;
    let config = cargo_config2::Config::load_with_cwd(root).map_err(fail)?;
    let admitted = producer::admitted_config_flags(&config)?;
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .ok_or_else(|| fail("neither CARGO_HOME nor HOME is set"))?;
    let (target_dir, build_dir) = cargo_directories(root)?;
    let recipe = recipe(&RecipeInputs {
        admitted,
        root: root.to_path_buf(),
        cargo_home,
        target_dir,
        build_dir,
        native: options.native,
        revision,
        tree,
        bundle,
        codebook,
    });
    let expected_features = target_features(&rustc, &recipe.rustflags)?;

    let parent = staged
        .parent()
        .ok_or_else(|| fail("no staging directory"))?;
    std::fs::create_dir_all(parent).map_err(fail)?;
    let election = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(parent.join(".gmeow-build.lock"))
        .map_err(fail)?;
    election.lock().map_err(fail)?;

    let built = compile(root, &recipe)?;
    if source_state(root)? != (recipe.revision.clone(), recipe.tree) {
        return Err(fail(
            "the checkout changed during compilation; nothing was staged",
        ));
    }
    let temporary = staged.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::copy(&built, &temporary).map_err(fail)?;
    let lock_text = std::fs::read_to_string(root.join("Cargo.lock")).map_err(fail)?;
    let receipt = json!({
        "schema": 1,
        "binary": {
            "path": "dist/bin/gmeow",
            "sha256": sha256_file(&temporary).map_err(fail)?,
            "bytes": std::fs::metadata(&temporary).map_err(fail)?.len(),
        },
        "source": { "revision": recipe.revision, "tree": recipe.tree.as_str() },
        "producer": if options.skip_producer { "skipped" } else { "ran" },
        "rustc": producer::identity(&rustc, &["-vV"])?,
        "cargo": producer::identity(
            producer::cargo_program().to_str().ok_or_else(|| fail("Cargo path is not UTF-8"))?,
            &["-vV"],
        )?,
        "rustc_wrapper": rustc_wrapper(&config),
        "rustflags": recipe.rustflags,
        "cflags": recipe.c_flags,
        "cxxflags": recipe.c_flags,
        "target": host,
        "cpu_baseline": recipe.cpu_baseline,
        "target_features": expected_features,
        "profile": "release",
        "cargo_lock_sha256": sha256_file(&root.join("Cargo.lock")).map_err(fail)?,
        "purrdf_version": locked_version(&lock_text, "purrdf")?,
        "bundle": payload_record(&recipe.bundle)?,
        "codebook": payload_record(&recipe.codebook)?,
    });
    if let Err(error) = verify_identity(&temporary, &receipt) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    let receipt_path = receipt_path(staged);
    let receipt_temporary = receipt_path.with_extension(format!("tmp-{}", std::process::id()));
    let mut text = serde_json::to_string_pretty(&receipt).map_err(fail)?;
    text.push('\n');
    std::fs::write(&receipt_temporary, text).map_err(fail)?;
    // The receipt is withdrawn first so a crash between the two renames leaves no
    // receipt naming bytes that are not staged.
    match std::fs::remove_file(&receipt_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(fail(error)),
    }
    std::fs::rename(&temporary, staged).map_err(fail)?;
    std::fs::rename(&receipt_temporary, &receipt_path).map_err(fail)?;
    election.unlock().map_err(fail)?;
    println!(
        "gmeow staged at {} sha256 {} ({}, {}, {})",
        staged.display(),
        receipt["binary"]["sha256"].as_str().unwrap_or_default(),
        recipe.cpu_baseline,
        recipe.revision,
        recipe.tree.as_str()
    );
    Ok(receipt)
}

fn receipt_path(staged: &Path) -> PathBuf {
    staged.with_extension("receipt.json")
}

fn payload_record(path: &Path) -> Result<Value> {
    Ok(json!({
        "path": path.display().to_string(),
        "sha256": sha256_file(path).map_err(fail)?,
        "bytes": std::fs::metadata(path).map_err(fail)?.len(),
    }))
}

/// Run the release compile and return the linked executable Cargo reports.
fn compile(root: &Path, recipe: &Recipe) -> Result<PathBuf> {
    let mut command = cargo_command(&producer::cargo_program(), root, recipe);
    command.stdout(Stdio::piped());
    let mut child = command.spawn().map_err(fail)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| fail("missing Cargo output"))?;
    let mut built = None;
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(fail)?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message["reason"] == "compiler-message"
            && let Some(rendered) = message["message"]["rendered"].as_str()
        {
            eprint!("{rendered}");
        }
        if message["reason"] == "compiler-artifact"
            && message["target"]["name"] == "gmeow"
            && let Some(path) = message["executable"].as_str()
        {
            built = Some(PathBuf::from(path));
        }
    }
    if !child.wait().map_err(fail)?.success() {
        return Err(fail("the gmeow release compilation failed"));
    }
    built.ok_or_else(|| fail("Cargo did not report the gmeow executable"))
}

/// Run `binary version --verbose --format json` from a scratch directory with a
/// cleared environment and require every reported identity field to equal the receipt.
fn verify_identity(binary: &Path, receipt: &Value) -> Result<()> {
    let scratch = tempfile::tempdir().map_err(fail)?;
    let output = Command::new(binary)
        .args(["version", "--verbose", "--format", "json"])
        .current_dir(scratch.path())
        .env_clear()
        .output()
        .map_err(fail)?;
    if !output.status.success() {
        return Err(fail(format!(
            "{} version --verbose failed: {}",
            binary.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let reported: Value = serde_json::from_slice(&output.stdout).map_err(fail)?;
    let rustc_release = receipt["rustc"]
        .as_str()
        .and_then(|text| text.lines().next())
        .map(|line| Value::String(line.to_owned()));
    let expected = [
        (
            "source_revision",
            Some(receipt["source"]["revision"].clone()),
        ),
        ("source_tree", Some(receipt["source"]["tree"].clone())),
        ("purrdf_version", Some(receipt["purrdf_version"].clone())),
        ("bundle_sha256", Some(receipt["bundle"]["sha256"].clone())),
        ("bundle_bytes", Some(receipt["bundle"]["bytes"].clone())),
        (
            "codebook_sha256",
            Some(receipt["codebook"]["sha256"].clone()),
        ),
        ("codebook_bytes", Some(receipt["codebook"]["bytes"].clone())),
        ("target", Some(receipt["target"].clone())),
        ("cpu_baseline", Some(receipt["cpu_baseline"].clone())),
        ("profile", Some(receipt["profile"].clone())),
        ("rustc", rustc_release),
    ];
    let mut mismatches: Vec<String> = expected
        .into_iter()
        .filter(|(key, wanted)| wanted.as_ref() != Some(&reported[*key]))
        .map(|(key, wanted)| {
            format!(
                "{key}: binary reports {} but the receipt records {}",
                reported[key],
                wanted.unwrap_or(Value::Null)
            )
        })
        .collect();
    // Cargo's feature order is not part of the contract; the feature set is.
    let as_set = |value: &Value| -> Option<BTreeSet<String>> {
        value
            .as_array()?
            .iter()
            .map(|feature| feature.as_str().map(str::to_owned))
            .collect()
    };
    if as_set(&reported["target_features"]).is_none()
        || as_set(&reported["target_features"]) != as_set(&receipt["target_features"])
    {
        mismatches.push(format!(
            "target_features: binary reports {} but rustc enables {} under the recipe",
            reported["target_features"], receipt["target_features"]
        ));
    }
    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(fail(format!(
            "{} does not state the identity it was built with:\n  {}",
            binary.display(),
            mismatches.join("\n  ")
        )))
    }
}

// ── staged reuse, run, install ──────────────────────────────────────────────

/// Read the staged receipt if it still describes this checkout exactly: same
/// staged bytes, a clean tree at the current commit, the same lockfile and
/// compiler, and the same payloads a build would embed now.
fn current_receipt(root: &Path, staged: &Path) -> Result<Option<Value>> {
    let Ok(text) = std::fs::read_to_string(receipt_path(staged)) else {
        return Ok(None);
    };
    let receipt: Value = serde_json::from_str(&text).map_err(fail)?;
    if !staged.is_file()
        || receipt["binary"]["sha256"].as_str() != Some(sha256_file(staged).map_err(fail)?.as_str())
    {
        return Ok(None);
    }
    let (revision, tree) = source_state(root)?;
    // Absent payloads mean nothing current can exist; the build then produces them.
    let Ok((bundle, codebook)) = payload_paths(root) else {
        return Ok(None);
    };
    let current = receipt["source"]["revision"].as_str() == Some(revision.as_str())
        && tree == Tree::Clean
        && receipt["source"]["tree"] == "clean"
        && receipt["cargo_lock_sha256"].as_str()
            == Some(
                sha256_file(&root.join("Cargo.lock"))
                    .map_err(fail)?
                    .as_str(),
            )
        && receipt["rustc"].as_str()
            == Some(producer::identity(&rustc_program(), &["-vV"])?.as_str())
        && receipt["bundle"]["sha256"].as_str()
            == Some(sha256_file(&bundle).map_err(fail)?.as_str())
        && receipt["codebook"]["sha256"].as_str()
            == Some(sha256_file(&codebook).map_err(fail)?.as_str());
    Ok(current.then_some(receipt))
}

/// Reuse a current staged build or produce a portable one, then re-verify it.
fn ensure_current(root: &Path, staged: &Path, allow_dirty: bool) -> Result<Value> {
    let receipt = match current_receipt(root, staged)? {
        Some(receipt) => receipt,
        None => build(
            root,
            staged,
            BuildOptions {
                skip_producer: false,
                allow_dirty,
                native: false,
            },
        )?,
    };
    verify_identity(staged, &receipt)?;
    Ok(receipt)
}

/// Refuse to place a dirty or host-tuned build on a PATH without explicit consent.
fn admit_install(receipt: &Value, allow_dirty: bool, allow_native: bool) -> Result<()> {
    if receipt["source"]["tree"] != "clean" && !allow_dirty {
        return Err(fail(format!(
            "the staged gmeow was built from a {} tree; pass --allow-dirty to install it",
            receipt["source"]["tree"]
        )));
    }
    if receipt["cpu_baseline"] != PORTABLE_CPU && !allow_native {
        return Err(fail(format!(
            "the staged gmeow targets CPU {} rather than the portable {PORTABLE_CPU}; pass \
             --allow-native to install it",
            receipt["cpu_baseline"]
        )));
    }
    Ok(())
}

/// Write the binary to a temporary file in `bin_dir` and rename it over the target.
fn install(staged: &Path, bin_dir: &Path, receipt: &Value) -> Result<(PathBuf, String)> {
    std::fs::create_dir_all(bin_dir).map_err(fail)?;
    let destination = bin_dir.join("gmeow");
    let mut temporary = tempfile::Builder::new()
        .prefix(".gmeow.")
        .tempfile_in(bin_dir)
        .map_err(fail)?;
    let bytes = std::fs::read(staged).map_err(fail)?;
    temporary.write_all(&bytes).map_err(fail)?;
    temporary.as_file().sync_all().map_err(fail)?;
    std::fs::set_permissions(temporary.path(), std::fs::Permissions::from_mode(0o755))
        .map_err(fail)?;
    let sha256 = sha256_file(temporary.path()).map_err(fail)?;
    if receipt["binary"]["sha256"].as_str() != Some(sha256.as_str()) {
        return Err(fail("the copied bytes do not match the staged receipt"));
    }
    temporary
        .persist(&destination)
        .map_err(|error| fail(error.error))?;
    Ok((destination, sha256))
}

#[path = "cli.tests.rs"]
#[cfg(test)]
mod tests;

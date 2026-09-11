// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Build and authenticate the dedicated producer before entering its command surface.
//! No corpus is constructed here. Read-only receipt verification never builds.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use gmeow_action_cache::executable::{
    CompilationUnit, ExecutableReceipt, ExecutableRecipe, sha256_file, source_digest,
};
use gmeow_errors::Diag;
use serde_json::Value;

#[path = "../../../build-support/producer_inputs.rs"]
mod producer_inputs;

type Result<T> = gmeow_errors::Result<T>;
const TARGET_FLAGS: &str = "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS";
const CONTRACT_ENV: &str = "GMEOW_PRODUCER_BUILD_CONTRACT";

/// Attach producer-build context to the shared typed evidence diagnostic.
fn fail(error: impl std::fmt::Display) -> Diag {
    crate::evidence::failure(format!("producer build: {error}"))
}

/// Dispatch a producer operation, printing typed failures and returning its exit status.
pub(super) fn command(args: Vec<String>) -> ExitCode {
    let result = execute(&args);
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Resolve the admitted recipe and execute the selected build, verify, recipe, or run operation.
///
/// Verification never compiles. Build and run elect one builder, authenticate any
/// staged pair, and rebuild an authenticated stale recipe or an absent receipt.
/// Release the election lock before running the staged producer command.
fn execute(args: &[String]) -> Result<ExitCode> {
    let root = crate::workspace_root();
    let Some(operation) = args.first() else {
        return Err(fail("expected build, verify, recipe, or run -- COMMAND"));
    };
    let recipe = resolve_recipe(&root)?;
    if operation == "recipe" && args.len() == 1 {
        println!("{}", recipe.digest().map_err(fail)?);
        return Ok(ExitCode::SUCCESS);
    }
    let staged = root.join("dist/bin/gmeow-dev");
    let receipt_path = staged.with_extension("receipt.json");
    if operation == "verify" && args.len() == 1 {
        verify(&staged, &receipt_path, &recipe)?;
        return Ok(ExitCode::SUCCESS);
    }
    if operation != "build" && operation != "run" {
        return Err(fail("expected build, verify, recipe, or run -- COMMAND"));
    }
    if (operation == "build" && args.len() != 1)
        || (operation == "run" && (args.get(1).map(String::as_str) != Some("--") || args.len() < 3))
    {
        return Err(fail("expected build or run -- COMMAND"));
    }
    // Concurrent gate roots may all request the producer. Elect one builder and
    // recheck the published pair while holding the same worktree-local lock.
    let parent = staged
        .parent()
        .ok_or_else(|| fail("missing staging directory"))?;
    std::fs::create_dir_all(parent).map_err(fail)?;
    let election = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(parent.join(".producer-build.lock"))
        .map_err(fail)?;
    election.lock().map_err(fail)?;
    // A fresh hit already authenticates the executable bytes and complete recipe.
    let fresh = staged_is_fresh(&staged, &receipt_path, &recipe)?;
    if !fresh {
        build(&root, &staged, &receipt_path, &recipe)?;
        verify(&staged, &receipt_path, &recipe)?;
    }
    election.unlock().map_err(fail)?;
    if operation == "run" {
        let status = Command::new(&staged)
            .args(&args[2..])
            .current_dir(&root)
            .status()
            .map_err(fail)?;
        return Ok(ExitCode::from(
            u8::try_from(status.code().unwrap_or(1)).unwrap_or(1),
        ));
    }
    Ok(ExitCode::SUCCESS)
}

/// Capture a command's UTF-8 stdout, rejecting launch failure or an unsuccessful exit.
fn checked_output(mut command: Command) -> Result<String> {
    let output = command.output().map_err(fail)?;
    if !output.status.success() {
        return Err(fail(String::from_utf8_lossy(&output.stderr)));
    }
    String::from_utf8(output.stdout).map_err(fail)
}

/// Read a tool's trimmed identity output using the supplied version or configuration arguments.
fn identity(program: &str, args: &[&str]) -> Result<String> {
    let mut command = Command::new(program);
    command.args(args);
    Ok(checked_output(command)?.trim().to_owned())
}

/// Select Cargo from its explicit environment binding, or use normal executable discovery.
fn cargo_program() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

/// Resolve rustflags in Cargo environment precedence, then reject competing codegen policy.
///
/// Use encoded flags first, then generic or target-specific flags, and finally
/// Cargo's resolved x86-64 Linux target configuration. Missing or malformed
/// configuration is an error.
fn configured_flags(root: &Path) -> Result<Vec<String>> {
    let flags = if let Ok(encoded) = std::env::var("CARGO_ENCODED_RUSTFLAGS") {
        encoded.split('\u{1f}').map(str::to_owned).collect()
    } else if let Ok(flags) = std::env::var("RUSTFLAGS").or_else(|_| std::env::var(TARGET_FLAGS)) {
        flags.split_whitespace().map(str::to_owned).collect()
    } else {
        let mut command = Command::new(cargo_program());
        command.current_dir(root).args([
            "-Z",
            "unstable-options",
            "config",
            "get",
            "--format",
            "json",
            "target.x86_64-unknown-linux-gnu.rustflags",
        ]);
        let config: Value = serde_json::from_str(&checked_output(command)?).map_err(fail)?;
        let flags = config
            .pointer("/target/x86_64-unknown-linux-gnu/rustflags")
            .and_then(Value::as_array)
            .ok_or_else(|| fail("resolved target rustflags are missing"))?;
        flags
            .iter()
            .map(|flag| {
                flag.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| fail("non-string rustflag"))
            })
            .collect::<Result<Vec<_>>>()?
    };
    validate_flags(&flags)?;
    Ok(flags)
}

/// Reject caller flags that override the producer's optimization or runtime-check policy.
///
/// Accept unrelated diagnostics and linker flags; the builder separately supplies
/// its selected CPU and final symbol-stripping policy.
fn validate_flags(flags: &[String]) -> Result<()> {
    for flag in flags {
        if flag == "-g"
            || [
                "opt-level",
                "lto",
                "codegen-units",
                "debuginfo",
                "debug-assertions",
                "overflow-checks",
                "incremental",
                "strip",
                "codegen-backend",
                "target-feature",
            ]
            .iter()
            .any(|key| {
                flag.trim_start_matches("--codegen=")
                    .trim_start_matches("-C")
                    .trim_start_matches("-Z")
                    .starts_with(key)
            })
        {
            return Err(fail(format!(
                "code-generation override {flag:?} competes with the pipeline profile"
            )));
        }
    }
    Ok(())
}

/// Prepare a locked pipeline-profile Cargo build with admitted Rust and native flags.
///
/// Force the selected C/C++ settings and symbol stripping while selecting only the
/// developer producer binary. The returned command has not been executed.
fn cargo_build(root: &Path, flags: &str, native_flags: &str) -> Command {
    let mut command = Command::new(cargo_program());
    command
        .current_dir(root)
        .env("CARGO_ENCODED_RUSTFLAGS", flags)
        .env_remove("RUSTFLAGS")
        .args(["--config", &format!("env.CFLAGS.value={native_flags:?}")])
        .args(["--config", &format!("env.CXXFLAGS.value={native_flags:?}")])
        .args([
            "--config",
            "env.CFLAGS.force=true",
            "--config",
            "env.CXXFLAGS.force=true",
        ])
        .args(["--config", "profile.pipeline.strip=\"symbols\""])
        .args([
            "build",
            "--locked",
            "--profile",
            "pipeline",
            "-p",
            "gmeow-dev-cli",
            "--bin",
            "gmeow-dev",
        ]);
    command
}

/// Resolve and validate the producer's source, toolchain, flags, and runtime unit graph.
///
/// CI selects portable x86-64-v3 code; local builds bind native CPU identity.
/// Reject competing native overrides or weakened runtime profiles before creating
/// the recipe used for executable authentication and action compilation policy.
fn resolve_recipe(root: &Path) -> Result<ExecutableRecipe> {
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
                "{name} overrides the admitted native runtime flags"
            )));
        }
    }
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let mut flags = configured_flags(root)?;
    let portable = std::env::var("CI").is_ok_and(|v| v == "true");
    let cpu = if portable { "x86-64-v3" } else { "native" };
    flags.push(format!("-Ctarget-cpu={cpu}"));
    // Some Cargo versions resolve an inherited strip setting to debuginfo.
    // Pin the final rustc operation explicitly as part of the admitted recipe.
    flags.push("-Cstrip=symbols".into());
    flags.push("-Dwarnings".into());
    let native_flags = if portable {
        "-O3 -march=x86-64-v3 -mtune=generic"
    } else {
        "-O3 -march=native -mtune=native"
    };
    let encoded = flags.join("\u{1f}");
    let mut command = cargo_build(root, &encoded, native_flags);
    command.args(["--unit-graph", "-Z", "unstable-options"]);
    let graph: Value = serde_json::from_str(&checked_output(command)?).map_err(fail)?;
    let units = graph["units"]
        .as_array()
        .ok_or_else(|| fail("Cargo unit graph has no units"))?;
    let roots = indices(&graph["roots"])?;
    let mut recipe_units = Vec::new();
    for unit in units {
        let package =
            string(unit, "pkg_id")?.replace(root.to_string_lossy().as_ref(), "<workspace>");
        recipe_units.push(CompilationUnit {
            package,
            target: string(&unit["target"], "name")?.to_owned(),
            target_kinds: strings(&unit["target"]["kind"])?,
            mode: string(unit, "mode")?.to_owned(),
            platform: unit["platform"].as_str().map(str::to_owned),
            features: strings(&unit["features"])?,
            profile: unit["profile"].clone(),
            dependencies: unit["dependencies"]
                .as_array()
                .ok_or_else(|| fail("missing unit dependencies"))?
                .iter()
                .map(|dep| index(&dep["index"]))
                .collect::<Result<Vec<_>>>()?,
        });
    }
    validate_units(&recipe_units, &roots)?;
    let mut compiler_environment: BTreeMap<String, String> = std::env::vars()
        .filter(|(key, _)| {
            key.starts_with("CARGO_PROFILE_PIPELINE_")
                || [
                    "CC",
                    "CXX",
                    "AR",
                    "RANLIB",
                    "CARGO_INCREMENTAL",
                    "CARGO_BUILD_TARGET",
                ]
                .contains(&key.as_str())
        })
        .collect();
    compiler_environment.insert("CARGO_ENCODED_RUSTFLAGS".into(), encoded);
    compiler_environment.insert("CFLAGS".into(), native_flags.into());
    compiler_environment.insert("CXXFLAGS".into(), native_flags.into());
    if !portable {
        compiler_environment.insert("NATIVE_CPU".into(), native_cpu_identity()?);
    }
    compiler_environment.insert(
        "TARGET_CFG".into(),
        identity(&rustc, &["--print", "cfg", &format!("-Ctarget-cpu={cpu}")])?,
    );
    compiler_environment.insert(
        "CC_IDENTITY".into(),
        identity(
            &std::env::var("CC").unwrap_or_else(|_| "cc".into()),
            &["--version"],
        )?,
    );
    compiler_environment.insert(
        "CXX_IDENTITY".into(),
        identity(
            &std::env::var("CXX").unwrap_or_else(|_| "c++".into()),
            &["--version"],
        )?,
    );
    Ok(ExecutableRecipe {
        schema: 1,
        profile: "pipeline".into(),
        source_digest: source_digest(
            root,
            producer_inputs::paths(root, &root.join("crates/gmeow-dev-cli")),
        )
        .map_err(fail)?,
        rustc: identity(&rustc, &["-Vv"])?,
        cargo: identity(
            cargo_program()
                .to_str()
                .ok_or_else(|| fail("Cargo path is not UTF-8"))?,
            &["--version"],
        )?,
        compiler_environment,
        units: recipe_units,
        roots,
    })
}

/// Encode the first processor's model and feature fields in deterministic key order.
///
/// Require all six x86 identity fields from `/proc/cpuinfo`; incomplete host data
/// cannot identify a native-tuned producer.
fn native_cpu_identity() -> Result<String> {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").map_err(fail)?;
    let mut fields = BTreeMap::new();
    for line in cpuinfo.lines().take_while(|line| !line.is_empty()) {
        if let Some((key, value)) = line.split_once(':')
            && [
                "vendor_id",
                "cpu family",
                "model",
                "model name",
                "stepping",
                "flags",
            ]
            .contains(&key.trim())
        {
            fields.insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    if fields.len() != 6 {
        return Err(fail("native x86 CPU identity is incomplete"));
    }
    serde_json::to_string(&fields).map_err(fail)
}

/// Borrow a required string field from a Cargo unit, rejecting a missing or wrong-typed value.
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| fail(format!("missing Cargo unit {key}")))
}

/// Read a Cargo string array without coercing or dropping malformed entries.
fn strings(value: &Value) -> Result<Vec<String>> {
    value
        .as_array()
        .ok_or_else(|| fail("missing Cargo string array"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| fail("non-string array entry"))
        })
        .collect()
}

/// Read a nonnegative Cargo unit index that fits the host's address space.
fn index(value: &Value) -> Result<usize> {
    value
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| fail("invalid Cargo unit index"))
}

/// Read every Cargo root index, rejecting absent arrays or invalid members.
fn indices(value: &Value) -> Result<Vec<usize>> {
    value
        .as_array()
        .ok_or_else(|| fail("missing Cargo roots"))?
        .iter()
        .map(index)
        .collect()
}

/// Require each reachable runtime unit to satisfy the optimized pipeline profile.
///
/// Workspace units retain assertions and overflow checks; dependency units retain
/// their separate check policy. Host build scripts and procedural macros are not
/// runtime units. Empty roots and dangling runtime dependencies fail admission.
fn validate_units(units: &[CompilationUnit], roots: &[usize]) -> Result<()> {
    if roots.is_empty() {
        return Err(fail("producer graph has no executable root"));
    }
    let mut pending = roots.to_vec();
    let mut visited = BTreeSet::new();
    while let Some(index) = pending.pop() {
        if !visited.insert(index) {
            continue;
        }
        let unit = units
            .get(index)
            .ok_or_else(|| fail("dangling Cargo unit dependency"))?;
        if unit
            .target_kinds
            .iter()
            .any(|kind| kind == "custom-build" || kind == "proc-macro")
        {
            continue;
        }
        let profile = &unit.profile;
        let workspace = unit.package.starts_with("path+file://<workspace>/");
        let expected_checks = workspace;
        if profile["name"] != "pipeline"
            || profile["opt_level"] != "3"
            || profile["lto"] != "fat"
            || profile["codegen_units"] != 1
            || profile["debuginfo"] != 0
            || profile["incremental"] != false
            || !profile["codegen_backend"].is_null()
            || profile["debug_assertions"] != expected_checks
            || profile["overflow_checks"] != expected_checks
        {
            return Err(fail(format!(
                "unit {} violates the optimized producer contract: {profile}",
                unit.package
            )));
        }
        pending.extend(unit.dependencies.iter().copied());
    }
    Ok(())
}

/// Authenticate the staged bytes against a receipt with the exact currently resolved recipe.
///
/// A missing, malformed, stale, or substituted artifact is an error; this read-only
/// operation never rebuilds or repairs either file.
fn verify(binary: &Path, path: &Path, expected: &ExecutableRecipe) -> Result<()> {
    let receipt = ExecutableReceipt::read(path).map_err(fail)?;
    if &receipt.recipe != expected {
        return Err(fail(
            "producer build recipe is stale; run make producer-build",
        ));
    }
    receipt
        .verify(binary, &expected.digest().map_err(fail)?)
        .map_err(fail)
}

/// An interrupted first publication or replacement can leave a binary without its receipt.
/// It is a build miss, never an executable that can be reused or run. Once a
/// receipt exists, corrupt or substituted bytes continue to fail closed.
fn staged_is_fresh(staged: &Path, receipt_path: &Path, recipe: &ExecutableRecipe) -> Result<bool> {
    if !receipt_path.try_exists().map_err(fail)? {
        return Ok(false);
    }
    let receipt = ExecutableReceipt::read(receipt_path).map_err(fail)?;
    receipt
        .verify(staged, &receipt.recipe.digest().map_err(fail)?)
        .map_err(fail)?;
    Ok(&receipt.recipe == recipe)
}

/// Retire the prior receipt before replacing its executable under the builder election lock.
///
/// The caller authenticates any existing pair before building and publishes a new
/// receipt after this replacement. An interruption or rename failure leaves no
/// receipt, so a later builder recomputes instead of confusing old evidence with
/// substituted bytes. Other receipt-removal errors leave the executable untouched.
fn replace_staged_executable(temporary: &Path, staged: &Path, receipt_path: &Path) -> Result<()> {
    match std::fs::remove_file(receipt_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(fail(error)),
    }
    std::fs::rename(temporary, staged).map_err(fail)
}

/// Compile the admitted recipe and publish its executable with a matching receipt.
///
/// Re-resolve inputs after compilation and probe the linked identity before
/// publication. Hash the prepared copy, retire the prior receipt, replace the
/// executable, then atomically publish its receipt. Any compile, freshness,
/// identity, or publication error fails.
fn build(root: &Path, staged: &Path, receipt_path: &Path, recipe: &ExecutableRecipe) -> Result<()> {
    let environment = &recipe.compiler_environment;
    let mut command = cargo_build(
        root,
        &environment["CARGO_ENCODED_RUSTFLAGS"],
        &environment["CFLAGS"],
    );
    command.env(CONTRACT_ENV, recipe.digest().map_err(fail)?);
    command.env(
        "GMEOW_PRODUCER_COMPILATION_CONTRACT",
        recipe.compilation_digest().map_err(fail)?,
    );
    command
        .arg("--message-format=json-render-diagnostics")
        .stdout(Stdio::piped());
    let mut child = command.spawn().map_err(fail)?;
    let mut built: Option<PathBuf> = None;
    for line in BufReader::new(
        child
            .stdout
            .take()
            .ok_or_else(|| fail("missing Cargo output"))?,
    )
    .lines()
    {
        let value: Value = serde_json::from_str(&line.map_err(fail)?).map_err(fail)?;
        if value["reason"] == "compiler-artifact"
            && value["target"]["name"] == "gmeow-dev"
            && let Some(path) = value["executable"].as_str()
        {
            built = Some(PathBuf::from(path));
        }
    }
    if !child.wait().map_err(fail)?.success() {
        return Err(fail("optimized producer compilation failed"));
    }
    // Re-resolve after compiling: source/config changes during a build cannot be
    // stamped as the pre-build recipe.
    if &resolve_recipe(root)? != recipe {
        return Err(fail(
            "producer inputs changed during compilation; no receipt published",
        ));
    }
    let built = built.ok_or_else(|| fail("Cargo did not identify the producer executable"))?;
    let mut probe = Command::new(&built);
    probe.arg("build-identity");
    if checked_output(probe)?.trim() != recipe.digest().map_err(fail)? {
        return Err(fail(
            "linked producer did not embed the admitted build recipe",
        ));
    }
    std::fs::create_dir_all(
        staged
            .parent()
            .ok_or_else(|| fail("missing staging parent"))?,
    )
    .map_err(fail)?;
    let temporary = staged.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::copy(&built, &temporary).map_err(fail)?;
    let receipt = ExecutableReceipt {
        schema: 1,
        recipe: recipe.clone(),
        executable_sha256: sha256_file(&temporary).map_err(fail)?,
    };
    replace_staged_executable(&temporary, staged, receipt_path)?;
    receipt.write(receipt_path).map_err(fail)?;
    println!("optimized producer staged at {}", staged.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Create an admitted synthetic runtime unit with the selected workspace check policy.
    fn runtime_unit(workspace: bool) -> CompilationUnit {
        CompilationUnit {
            package: if workspace {
                "path+file://<workspace>/crates/example#0.1.0"
            } else {
                "registry+https://example.invalid/index#upstream@1.0.0"
            }
            .into(),
            target: "example".into(),
            target_kinds: vec!["lib".into()],
            mode: "build".into(),
            platform: None,
            features: vec![],
            profile: json!({
                "name": "pipeline", "opt_level": "3", "lto": "fat",
                "codegen_units": 1, "debuginfo": 0, "incremental": false,
                "debug_assertions": workspace, "overflow_checks": workspace,
                "strip": {"resolved": {"Named": "symbols"}}
            }),
            dependencies: vec![],
        }
    }

    /// Supply a tiny recipe for publication tests without resolving tools or repository inputs.
    fn publication_recipe() -> ExecutableRecipe {
        ExecutableRecipe {
            schema: 1,
            profile: "pipeline".into(),
            source_digest: "source".into(),
            rustc: "compiler".into(),
            cargo: "cargo".into(),
            compiler_environment: BTreeMap::new(),
            units: Vec::new(),
            roots: Vec::new(),
        }
    }

    /// Treat an absent receipt as a build miss; reject substituted or missing receipted bytes.
    #[test]
    fn interrupted_publication_is_a_miss_but_present_receipts_must_authenticate() {
        let scratch = tempfile::tempdir().expect("scratch");
        let binary = scratch.path().join("gmeow-dev");
        let path = binary.with_extension("receipt.json");
        let recipe = publication_recipe();
        assert!(!staged_is_fresh(&binary, &path, &recipe).expect("empty build miss"));
        std::fs::write(&binary, b"linked executable").expect("binary");
        assert!(!staged_is_fresh(&binary, &path, &recipe).expect("unreceipted build miss"));
        ExecutableReceipt {
            schema: 1,
            recipe: recipe.clone(),
            executable_sha256: sha256_file(&binary).expect("digest"),
        }
        .write(&path)
        .expect("receipt");
        assert!(staged_is_fresh(&binary, &path, &recipe).expect("authenticated hit"));
        let mut changed = recipe.clone();
        changed.source_digest = "changed source".into();
        assert!(!staged_is_fresh(&binary, &path, &changed).expect("stale build miss"));
        assert!(verify(&binary, &path, &changed).is_err());
        std::fs::write(&binary, b"substituted bytes").expect("replace");
        assert!(staged_is_fresh(&binary, &path, &recipe).is_err());
        std::fs::remove_file(&binary).expect("remove binary");
        assert!(staged_is_fresh(&binary, &path, &recipe).is_err());
        std::fs::write(&binary, b"linked executable").expect("restore binary");
        std::fs::write(&path, b"malformed receipt").expect("corrupt receipt");
        assert!(staged_is_fresh(&binary, &path, &recipe).is_err());
    }

    /// Recover an interrupted replacement of an authenticated pair through an unreceipted miss.
    #[test]
    fn interrupted_replacement_retires_old_receipt_before_publishing_new_bytes() {
        let scratch = tempfile::tempdir().expect("scratch");
        let binary = scratch.path().join("gmeow-dev");
        let path = binary.with_extension("receipt.json");
        let temporary = binary.with_extension("prepared");
        let old_recipe = publication_recipe();
        std::fs::write(&binary, b"old executable").expect("old binary");
        ExecutableReceipt {
            schema: 1,
            recipe: old_recipe.clone(),
            executable_sha256: sha256_file(&binary).expect("old digest"),
        }
        .write(&path)
        .expect("old receipt");
        assert!(staged_is_fresh(&binary, &path, &old_recipe).expect("old authenticated pair"));

        let mut new_recipe = old_recipe;
        new_recipe.source_digest = "new source".into();
        std::fs::write(&temporary, b"new executable").expect("prepared replacement");
        replace_staged_executable(&temporary, &binary, &path).expect("replace executable");
        // Stop at the actual production boundary before the new receipt is published.
        assert_eq!(
            std::fs::read(&binary).expect("new bytes"),
            b"new executable"
        );
        assert!(!path.exists(), "old evidence must not survive replacement");
        assert!(!staged_is_fresh(&binary, &path, &new_recipe).expect("recoverable build miss"));
        assert!(verify(&binary, &path, &new_recipe).is_err());

        std::fs::write(&temporary, b"new executable").expect("prepared retry");
        replace_staged_executable(&temporary, &binary, &path).expect("retry without a receipt");
        ExecutableReceipt {
            schema: 1,
            recipe: new_recipe.clone(),
            executable_sha256: sha256_file(&binary).expect("new digest"),
        }
        .write(&path)
        .expect("new receipt");
        verify(&binary, &path, &new_recipe).expect("recovered authenticated pair");
        assert!(staged_is_fresh(&binary, &path, &new_recipe).expect("recovered fresh hit"));
    }

    /// Reject weakened dependency optimization and disabled workspace runtime checks.
    #[test]
    fn runtime_dependency_policy_is_checked_transitively() {
        let mut root = runtime_unit(true);
        root.target_kinds = vec!["bin".into()];
        root.dependencies = vec![1];
        let dependency = runtime_unit(false);
        validate_units(&[root.clone(), dependency.clone()], &[0]).expect("admitted");
        for (key, weakened) in [
            ("opt_level", json!("2")),
            ("lto", json!("thin")),
            ("codegen_units", json!(16)),
            ("debuginfo", json!(1)),
            ("incremental", json!(true)),
        ] {
            let mut changed = dependency.clone();
            changed.profile[key] = weakened;
            assert!(
                validate_units(&[root.clone(), changed], &[0]).is_err(),
                "{key}"
            );
        }
        for key in ["debug_assertions", "overflow_checks"] {
            let mut changed = root.clone();
            changed.profile[key] = json!(false);
            assert!(
                validate_units(&[changed, dependency.clone()], &[0]).is_err(),
                "{key}"
            );
        }
    }

    /// Permit a host macro's distinct build profile without admitting the same profile for runtime code.
    #[test]
    fn host_build_tools_are_separate_from_runtime_code() {
        let mut root = runtime_unit(true);
        root.dependencies = vec![1];
        let mut build_tool = runtime_unit(false);
        build_tool.target_kinds = vec!["proc-macro".into()];
        build_tool.profile["opt_level"] = json!("0");
        validate_units(&[root.clone(), build_tool.clone()], &[0]).expect("host tool");
        build_tool.target_kinds = vec!["lib".into()];
        assert!(validate_units(&[root, build_tool], &[0]).is_err());
    }

    /// Reject alternate codegen flag spellings while retaining unrelated warning and linker settings.
    #[test]
    fn rustflags_cannot_override_admitted_codegen_policy() {
        for flags in [
            vec!["-C", "opt-level=2"],
            vec!["-Clto=thin"],
            vec!["--codegen=codegen-units=16"],
            vec!["--codegen", "debuginfo=2"],
            vec!["-Zcodegen-backend=cranelift"],
            vec!["-g"],
            vec!["-Ctarget-feature=-avx2"],
        ] {
            assert!(
                validate_flags(&flags.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err()
            );
        }
        validate_flags(&["-Dwarnings".into(), "-Clink-self-contained=+linker".into()])
            .expect("non-competing flags");
    }

    /// Track runtime sources and added embedded queries while excluding workflow-only edits.
    #[test]
    fn producer_inventory_tracks_embedded_queries_and_runtime_dependencies() {
        // Synthetic source tree only: inventory discovery must never invoke a
        // compiler, pipeline stage, or corpus fixture producer.
        let scratch = tempfile::tempdir().expect("source tree");
        let root = scratch.path();
        for (path, contents) in [
            ("Cargo.toml", "[workspace]"),
            (
                "crates/gmeow-dev-cli/Cargo.toml",
                "[dependencies]\nruntime = { path = \"../runtime\" }\n[dev-dependencies]\ntest-only = { path = \"../test-only\" }",
            ),
            ("crates/gmeow-dev-cli/src/main.rs", "fn main() {}"),
            ("crates/runtime/Cargo.toml", "[package]\nname = \"runtime\""),
            ("crates/runtime/src/lib.rs", "pub fn runtime() {}"),
            ("crates/runtime/src/bin/tool.rs", "fn main() {}"),
            ("queries/verify/root.rq", "ASK {}"),
            ("slices/group/one/queries/verify/first.rq", "ASK {}"),
        ] {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            std::fs::write(path, contents).expect("source");
        }
        let producer = root.join("crates/gmeow-dev-cli");
        let initial = producer_inputs::paths(root, &producer);
        assert!(initial.contains(&root.join("crates/runtime/src/lib.rs")));
        assert!(!initial.contains(&root.join("crates/runtime/src/bin/tool.rs")));
        let before = source_digest(root, initial).expect("source digest");
        std::fs::write(root.join("Makefile"), "producer-build:\n").expect("workflow edit");
        assert_eq!(
            before,
            source_digest(root, producer_inputs::paths(root, &producer))
                .expect("workflow inventory")
        );
        std::fs::write(
            root.join("slices/group/one/queries/verify/second.rq"),
            "ASK { ?s ?p ?o }",
        )
        .expect("new embedded query");
        assert_ne!(
            before,
            source_digest(root, producer_inputs::paths(root, &producer)).expect("query inventory")
        );
    }
}

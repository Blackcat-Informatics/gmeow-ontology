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

fn fail(error: impl std::fmt::Display) -> Diag {
    crate::evidence::failure(format!("producer build: {error}"))
}

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
    // A valid old recipe is a build miss. Corrupt or substituted bytes fail closed.
    let fresh = if staged.exists() || receipt_path.exists() {
        let receipt = ExecutableReceipt::read(&receipt_path).map_err(fail)?;
        receipt
            .verify(&staged, &receipt.recipe.digest().map_err(fail)?)
            .map_err(fail)?;
        receipt.recipe == recipe
    } else {
        false
    };
    if !fresh {
        build(&root, &staged, &receipt_path, &recipe)?;
    }
    verify(&staged, &receipt_path, &recipe)?;
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

fn checked_output(mut command: Command) -> Result<String> {
    let output = command.output().map_err(fail)?;
    if !output.status.success() {
        return Err(fail(String::from_utf8_lossy(&output.stderr)));
    }
    String::from_utf8(output.stdout).map_err(fail)
}

fn identity(program: &str, args: &[&str]) -> Result<String> {
    let mut command = Command::new(program);
    command.args(args);
    Ok(checked_output(command)?.trim().to_owned())
}

fn cargo_program() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

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

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| fail(format!("missing Cargo unit {key}")))
}

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

fn index(value: &Value) -> Result<usize> {
    value
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| fail("invalid Cargo unit index"))
}

fn indices(value: &Value) -> Result<Vec<usize>> {
    value
        .as_array()
        .ok_or_else(|| fail("missing Cargo roots"))?
        .iter()
        .map(index)
        .collect()
}

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
    std::fs::rename(&temporary, staged).map_err(fail)?;
    ExecutableReceipt {
        schema: 1,
        recipe: recipe.clone(),
        executable_sha256: sha256_file(staged).map_err(fail)?,
    }
    .write(receipt_path)
    .map_err(fail)?;
    println!("optimized producer staged at {}", staged.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

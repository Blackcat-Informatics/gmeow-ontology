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
    CompilationUnit, ExecutableReceipt, ExecutableRecipe, ReceiptDocument, sha256_file,
};
use gmeow_errors::Diag;
use serde_json::Value;

use gmeow_build_inputs::{
    CargoResolutionEvidence, CargoResolutionInputs, CfgContext, CompilerArtifact, CompilerInputs,
    GeneratedInputs, InputInventory, ProductionSelection, UnitSelection,
};

type Result<T> = gmeow_errors::Result<T>;
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
    if operation == "source-selection" && args.len() == 1 {
        let resolved = resolve_context(&root)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&resolved.source_selection).map_err(fail)?
        );
        return Ok(ExitCode::SUCCESS);
    }
    let admitted = resolve_admission(&root)?;
    let recipe = &admitted.recipe;
    if operation == "recipe" && args.len() == 1 {
        println!("{}", recipe.digest().map_err(fail)?);
        return Ok(ExitCode::SUCCESS);
    }
    let staged = root.join("dist/bin/gmeow-dev");
    let receipt_path = staged.with_extension("receipt.json");
    if operation == "verify" && args.len() == 1 {
        verify(&root, &staged, &receipt_path, recipe)?;
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
    admitted
        .resolution
        .verify_current(&root, &recipe.source_inventory.selection)
        .map_err(fail)?;
    let fresh = staged_is_fresh(&staged, &receipt_path, recipe)?;
    if !fresh {
        build(&root, &staged, &receipt_path, recipe)?;
    } else {
        refresh_resolution(&root, &staged, &receipt_path, &admitted)?;
    }
    verify(&root, &staged, &receipt_path, recipe)?;
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
/// Resolve environment, matching target/cfg entries and build-level defaults through
/// Cargo's configuration model. Absent flags mean the compiler defaults; malformed
/// configuration and competing producer codegen settings remain errors.
fn configured_flags(root: &Path) -> Result<Vec<String>> {
    admitted_config_flags(&cargo_config2::Config::load_with_cwd(root).map_err(fail)?)
}

/// Admit exactly the resolved flags that will be encoded into the producer recipe.
fn admitted_config_flags(config: &cargo_config2::Config) -> Result<Vec<String>> {
    let flags = config
        .rustflags("x86_64-unknown-linux-gnu")
        .map_err(fail)?
        .unwrap_or_default()
        .flags;
    validate_flags(&flags)?;
    // An embedded separator must not turn one admitted argument into additional
    // unvalidated arguments when the recipe is passed to Cargo.
    if flags.iter().any(|flag| flag.contains('\u{1f}')) {
        return Err(fail(
            "resolved rustflag contains the encoded argument separator",
        ));
    }
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
struct ResolvedProducer {
    source_selection: ProductionSelection,
    resolution: CargoResolutionEvidence,
    rustc: String,
    cargo: String,
    compiler_environment: BTreeMap<String, String>,
    units: Vec<CompilationUnit>,
    roots: Vec<usize>,
}

struct AdmittedProducer {
    recipe: ExecutableRecipe,
    resolution: CargoResolutionEvidence,
}

/// Cargo's unit graph indices are process-local presentation details. In particular,
/// Cargo may exchange two otherwise identical host units whose only distinction is
/// the recursively selected dependency feature set. Hashing those raw indices makes
/// a read-only recipe change from one resolution to the next.
///
/// Give every unit a recursive content identity, order units by that identity, and
/// rewrite every edge to the first canonical representative of its identity class.
/// Equivalent duplicate nodes remain in the inventory so its multiplicity is still
/// visible, but references no longer depend on which interchangeable occurrence Cargo
/// happened to number first.
fn canonicalize_compilation_graph(
    units: &mut Vec<CompilationUnit>,
    roots: &mut Vec<usize>,
) -> Result<()> {
    fn normalized(mut unit: CompilationUnit) -> CompilationUnit {
        unit.target_kinds.sort();
        unit.target_kinds.dedup();
        unit.features.sort();
        unit.features.dedup();
        unit.dependencies.clear();
        unit
    }

    fn identity(
        index: usize,
        units: &[CompilationUnit],
        states: &mut [u8],
        identities: &mut [Option<String>],
    ) -> Result<String> {
        if let Some(identity) = &identities[index] {
            return Ok(identity.clone());
        }
        if states[index] == 1 {
            return Err(fail("Cargo compilation unit graph contains a cycle"));
        }
        states[index] = 1;
        let unit = units
            .get(index)
            .ok_or_else(|| fail("dangling Cargo compilation unit"))?;
        let mut dependencies = Vec::with_capacity(unit.dependencies.len());
        for dependency in &unit.dependencies {
            if *dependency >= units.len() {
                return Err(fail("dangling Cargo compilation unit dependency"));
            }
            dependencies.push(identity(*dependency, units, states, identities)?);
        }
        dependencies.sort();
        let payload =
            serde_json::to_vec(&(normalized(unit.clone()), dependencies)).map_err(fail)?;
        let value =
            gmeow_action_cache::content_digest(&[b"gmeow-canonical-compilation-unit-v1", &payload]);
        states[index] = 2;
        identities[index] = Some(value.clone());
        Ok(value)
    }

    if units.is_empty() {
        return Err(fail("Cargo compilation unit graph is empty"));
    }
    let mut states = vec![0; units.len()];
    let mut identities = vec![None; units.len()];
    for index in 0..units.len() {
        identity(index, units, &mut states, &mut identities)?;
    }
    let identities: Vec<_> = identities
        .into_iter()
        .map(|identity| identity.expect("every unit received an identity"))
        .collect();
    let mut order: Vec<_> = (0..units.len()).collect();
    order.sort_by(|left, right| {
        identities[*left]
            .cmp(&identities[*right])
            .then_with(|| left.cmp(right))
    });
    let mut representatives = BTreeMap::new();
    for (canonical, original) in order.iter().copied().enumerate() {
        representatives
            .entry(identities[original].clone())
            .or_insert(canonical);
    }
    let remap: Vec<_> = identities
        .iter()
        .map(|identity| representatives[identity])
        .collect();
    let mut canonical = Vec::with_capacity(units.len());
    for original in order {
        let mut unit = normalized(units[original].clone());
        unit.dependencies = units[original]
            .dependencies
            .iter()
            .map(|dependency| remap[*dependency])
            .collect();
        unit.dependencies.sort_unstable();
        canonical.push(unit);
    }
    *roots = roots
        .iter()
        .map(|root| {
            remap
                .get(*root)
                .copied()
                .ok_or_else(|| fail("dangling Cargo compilation root"))
        })
        .collect::<Result<Vec<_>>>()?;
    roots.sort_unstable();
    *units = canonical;
    Ok(())
}

/// Canonicalize the richer source-selection graph while preserving the Cargo extern
/// name paired with each dependency. Two otherwise equal units that select different
/// feature lineages therefore remain distinct without retaining Cargo's raw indices.
fn canonicalize_source_graph(units: &mut Vec<UnitSelection>, roots: &mut Vec<usize>) -> Result<()> {
    fn normalized(mut unit: UnitSelection) -> UnitSelection {
        unit.kinds.sort();
        unit.kinds.dedup();
        unit.dependencies.clear();
        unit.dependency_names.clear();
        unit
    }

    fn identity(
        index: usize,
        units: &[UnitSelection],
        states: &mut [u8],
        identities: &mut [Option<String>],
    ) -> Result<String> {
        if let Some(identity) = &identities[index] {
            return Ok(identity.clone());
        }
        if states[index] == 1 {
            return Err(fail("Cargo source-selection graph contains a cycle"));
        }
        states[index] = 1;
        let unit = units
            .get(index)
            .ok_or_else(|| fail("dangling Cargo source-selection unit"))?;
        if unit.dependencies.len() != unit.dependency_names.len() {
            return Err(fail(
                "Cargo source-selection dependency lost its extern name",
            ));
        }
        let mut dependencies = Vec::with_capacity(unit.dependencies.len());
        for (name, dependency) in unit.dependency_names.iter().zip(&unit.dependencies) {
            if *dependency >= units.len() {
                return Err(fail("dangling Cargo source-selection dependency"));
            }
            dependencies.push((
                name.clone(),
                identity(*dependency, units, states, identities)?,
            ));
        }
        dependencies.sort();
        let payload =
            serde_json::to_vec(&(normalized(unit.clone()), dependencies)).map_err(fail)?;
        let value =
            gmeow_action_cache::content_digest(&[b"gmeow-canonical-source-unit-v1", &payload]);
        states[index] = 2;
        identities[index] = Some(value.clone());
        Ok(value)
    }

    if units.is_empty() {
        return Err(fail("Cargo source-selection graph is empty"));
    }
    let mut states = vec![0; units.len()];
    let mut identities = vec![None; units.len()];
    for index in 0..units.len() {
        identity(index, units, &mut states, &mut identities)?;
    }
    let identities: Vec<_> = identities
        .into_iter()
        .map(|identity| identity.expect("every unit received an identity"))
        .collect();
    let mut order: Vec<_> = (0..units.len()).collect();
    order.sort_by(|left, right| {
        identities[*left]
            .cmp(&identities[*right])
            .then_with(|| left.cmp(right))
    });
    let mut representatives = BTreeMap::new();
    for (canonical, original) in order.iter().copied().enumerate() {
        representatives
            .entry(identities[original].clone())
            .or_insert(canonical);
    }
    let remap: Vec<_> = identities
        .iter()
        .map(|identity| representatives[identity])
        .collect();
    let mut canonical = Vec::with_capacity(units.len());
    for original in order {
        let mut unit = normalized(units[original].clone());
        let mut dependencies: Vec<_> = units[original]
            .dependency_names
            .iter()
            .cloned()
            .zip(
                units[original]
                    .dependencies
                    .iter()
                    .map(|dependency| remap[*dependency]),
            )
            .collect();
        dependencies.sort();
        (unit.dependency_names, unit.dependencies) = dependencies.into_iter().unzip();
        canonical.push(unit);
    }
    *roots = roots
        .iter()
        .map(|root| {
            remap
                .get(*root)
                .copied()
                .ok_or_else(|| fail("dangling Cargo source-selection root"))
        })
        .collect::<Result<Vec<_>>>()?;
    roots.sort_unstable();
    *units = canonical;
    Ok(())
}

fn resolve_admission(root: &Path) -> Result<AdmittedProducer> {
    let resolved = resolve_context(root)?;
    let source_inventory =
        InputInventory::collect(root, &resolved.source_selection).map_err(fail)?;
    resolved
        .resolution
        .verify_current(root, &resolved.source_selection)
        .map_err(fail)?;
    Ok(AdmittedProducer {
        recipe: ExecutableRecipe {
            schema: 2,
            profile: "pipeline".into(),
            source_digest: source_inventory.digest().map_err(fail)?,
            source_inventory,
            rustc: resolved.rustc,
            cargo: resolved.cargo,
            compiler_environment: resolved.compiler_environment,
            units: resolved.units,
            roots: resolved.roots,
        },
        resolution: resolved.resolution,
    })
}

/// Resolve actual Cargo units without admitting source implementation. The
/// extraction planner uses this read-only context before the strict source
/// partition can be admitted; it never constructs an executable recipe.
fn resolve_context(root: &Path) -> Result<ResolvedProducer> {
    let manifests = metadata_manifests(root)?;
    let resolution_inputs = CargoResolutionInputs::capture(
        root,
        manifests
            .values()
            .map(|path| workspace_relative(root, Path::new(path)))
            .collect::<Result<_>>()?,
    )
    .map_err(fail)?;
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
    let mut roots = indices(&graph["roots"])?;
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
    canonicalize_compilation_graph(&mut recipe_units, &mut roots)?;
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
    let source_selection = resolve_source_selection(
        root,
        &graph,
        &manifests,
        &rustc,
        &compiler_environment["CARGO_ENCODED_RUSTFLAGS"],
    )?;
    if metadata_manifests(root)? != manifests {
        return Err(fail(
            "Cargo metadata membership changed during resolution; no admission published",
        ));
    }
    resolution_inputs.verify_current(root).map_err(fail)?;
    let resolution = resolution_inputs.bind(&source_selection).map_err(fail)?;
    Ok(ResolvedProducer {
        source_selection,
        resolution,
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

/// Capture actual Cargo roots for runtime and the build controller. This only
/// resolves source selection; it never creates a speculative compilation lane.
fn resolve_source_selection(
    root: &Path,
    graph: &Value,
    manifests: &BTreeMap<String, String>,
    rustc: &str,
    flags: &str,
) -> Result<ProductionSelection> {
    // This is the actual default-profile controller target used by the xtask
    // alias, resolved independently of the O3 corpus-producing executable.
    let mut controller = Command::new(cargo_program());
    controller.current_dir(root).args([
        "build",
        "--locked",
        "-p",
        "xtask",
        "--bin",
        "xtask",
        "--unit-graph",
        "-Z",
        "unstable-options",
    ]);
    let controller_flags = configured_flags(root)?.join("\u{1f}");
    let controller: Value = serde_json::from_str(&checked_output(controller)?).map_err(fail)?;
    let mut selection = ProductionSelection {
        schema: gmeow_build_inputs::SCHEMA,
        units: Vec::new(),
        roots: Vec::new(),
        policy_files: Vec::new(),
    };
    let mut cfg_outputs: BTreeMap<(Option<String>, bool), String> = BTreeMap::new();
    for (graph, controller, flags) in [
        (graph, false, flags),
        (&controller, true, controller_flags.as_str()),
    ] {
        let explicit_target = graph["units"]
            .as_array()
            .ok_or_else(|| fail("missing selected units"))?
            .iter()
            .any(|unit| !unit["platform"].is_null());
        let offset = selection.units.len();
        for unit in graph["units"]
            .as_array()
            .ok_or_else(|| fail("missing selected units"))?
        {
            let id = string(unit, "pkg_id")?;
            let manifest = manifests
                .get(id)
                .map(|path| workspace_relative(root, Path::new(path)))
                .transpose()?;
            if manifest.is_none() && id.starts_with("path+") {
                return Err(fail(format!(
                    "local dependency is outside the admitted workspace: {id}"
                )));
            }
            let source = manifest
                .as_ref()
                .map(|_| {
                    string(&unit["target"], "src_path")
                        .and_then(|path| workspace_relative(root, Path::new(path)))
                })
                .transpose()?;
            let features = strings(&unit["features"])?;
            let platform = if unit["platform"].is_null() {
                None
            } else {
                Some(string(unit, "platform")?.to_owned())
            };
            let cfg_key = (platform.clone(), controller);
            if !cfg_outputs.contains_key(&cfg_key) {
                let mut compiler = Command::new(rustc);
                compiler.args(["--print", "cfg"]);
                if let Some(target) = &platform {
                    compiler.args(["--target", target]);
                }
                if platform.is_some() || !explicit_target {
                    compiler.args(flags.split('\u{1f}').filter(|flag| !flag.is_empty()));
                }
                cfg_outputs.insert(cfg_key.clone(), checked_output(compiler)?);
            }
            let mut cfg = CfgContext::from_rustc(
                &cfg_outputs[&cfg_key],
                &features,
                unit["profile"]["debug_assertions"]
                    .as_bool()
                    .ok_or_else(|| fail("missing selected debug assertion policy"))?,
            )
            .map_err(fail)?;
            if let Some(panic) = unit["profile"]["panic"].as_str() {
                cfg.values
                    .insert("panic".into(), [panic.to_owned()].into_iter().collect());
            }
            let dependency_names = unit["dependencies"]
                .as_array()
                .ok_or_else(|| fail("missing dependency names"))?
                .iter()
                .map(|dependency| string(dependency, "extern_crate_name").map(str::to_owned))
                .collect::<Result<_>>()?;
            selection.units.push(UnitSelection {
                package: id.replace(root.to_string_lossy().as_ref(), "<workspace>"),
                manifest,
                source,
                target: string(&unit["target"], "name")?.to_owned(),
                kinds: strings(&unit["target"]["kind"])?,
                cfg,
                dependency_names,
                dependencies: unit["dependencies"]
                    .as_array()
                    .ok_or_else(|| fail("missing dependencies"))?
                    .iter()
                    .map(|dependency| index(&dependency["index"]).map(|index| index + offset))
                    .collect::<Result<_>>()?,
                controller,
            });
        }
        selection.roots.extend(
            indices(&graph["roots"])?
                .into_iter()
                .map(|index| index + offset),
        );
    }
    selection.policy_files = gmeow_build_inputs::cargo_policy_files(root).map_err(fail)?;
    canonicalize_source_graph(&mut selection.units, &mut selection.roots)?;
    selection.validate().map_err(fail)?;
    Ok(selection)
}

fn metadata_manifests(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut command = Command::new(cargo_program());
    command
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"]);
    let metadata: Value = serde_json::from_str(&checked_output(command)?).map_err(fail)?;
    metadata["packages"]
        .as_array()
        .ok_or_else(|| fail("Cargo metadata has no packages"))?
        .iter()
        .map(|package| {
            Ok((
                string(package, "id")?.to_owned(),
                string(package, "manifest_path")?.to_owned(),
            ))
        })
        .collect()
}
fn workspace_relative(root: &Path, path: &Path) -> Result<String> {
    path.strip_prefix(root)
        .map_err(fail)?
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| fail("source path is not UTF-8"))
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
fn verify(root: &Path, binary: &Path, path: &Path, expected: &ExecutableRecipe) -> Result<()> {
    let receipt = ExecutableReceipt::read(path).map_err(fail)?;
    if &receipt.recipe != expected {
        return Err(fail(
            "producer build recipe is stale; run make producer-build",
        ));
    }
    receipt
        .verify(binary, &expected.digest().map_err(fail)?)
        .map_err(fail)?;
    receipt.verify_current_inputs(root).map_err(fail)
}

/// Re-admit a byte-identical executable after Cargo resolved the same production
/// recipe. Only resolution evidence is replaced; no compilation is requested.
fn refresh_resolution(
    root: &Path,
    binary: &Path,
    path: &Path,
    admitted: &AdmittedProducer,
) -> Result<()> {
    admitted
        .resolution
        .verify_current(root, &admitted.recipe.source_inventory.selection)
        .map_err(fail)?;
    let mut receipt = ExecutableReceipt::read(path).map_err(fail)?;
    receipt
        .verify(binary, &admitted.recipe.digest().map_err(fail)?)
        .map_err(fail)?;
    if receipt.recipe != admitted.recipe {
        return Err(fail(
            "cannot refresh resolution evidence for a different producer recipe",
        ));
    }
    if receipt.resolution != admitted.resolution {
        receipt.resolution = admitted.resolution.clone();
        receipt.write(path).map_err(fail)?;
    }
    Ok(())
}

/// An interrupted first publication or replacement can leave a binary without its receipt.
/// It is a build miss, never an executable that can be reused or run. Once a
/// receipt exists, corrupt or substituted bytes continue to fail closed.
fn staged_is_fresh(staged: &Path, receipt_path: &Path, recipe: &ExecutableRecipe) -> Result<bool> {
    if !receipt_path.try_exists().map_err(fail)? {
        return Ok(false);
    }
    let receipt = match ExecutableReceipt::read_versioned(receipt_path).map_err(fail)? {
        ReceiptDocument::Current(receipt) => receipt,
        ReceiptDocument::Unsupported { .. } => return Ok(false),
    };
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
    // A resolved workspace graph can exceed the platform's single-environment-
    // value bound. Transfer the exact document by an owned, digest-bound file.
    let selection_file = tempfile::NamedTempFile::new().map_err(fail)?;
    std::fs::write(
        selection_file.path(),
        serde_json::to_vec(&recipe.source_inventory.selection).map_err(fail)?,
    )
    .map_err(fail)?;
    command.env(gmeow_build_inputs::SELECTION_ENV, selection_file.path());
    command.env(
        gmeow_build_inputs::SELECTION_DIGEST_ENV,
        sha256_file(selection_file.path()).map_err(fail)?,
    );
    command.env(
        "GMEOW_PRODUCER_COMPILATION_CONTRACT",
        recipe.compilation_digest().map_err(fail)?,
    );
    command
        .arg("--message-format=json-render-diagnostics")
        .stdout(Stdio::piped());
    let mut child = command.spawn().map_err(fail)?;
    let mut built: Option<PathBuf> = None;
    let mut artifacts = Vec::new();
    let mut generated_roots = Vec::new();
    for line in BufReader::new(
        child
            .stdout
            .take()
            .ok_or_else(|| fail("missing Cargo output"))?,
    )
    .lines()
    {
        let value: Value = serde_json::from_str(&line.map_err(fail)?).map_err(fail)?;
        if value["reason"] == "compiler-message" {
            if let Some(message) = value["message"]["rendered"].as_str() {
                eprint!("{message}");
            }
        }
        if value["reason"] == "build-script-executed" {
            generated_roots.push(GeneratedInputs {
                directory: PathBuf::from(string(&value, "out_dir")?),
                package: string(&value, "package_id")?
                    .replace(root.to_string_lossy().as_ref(), "<workspace>"),
            });
        }
        if value["reason"] == "compiler-artifact" {
            let package = string(&value, "package_id")?
                .replace(root.to_string_lossy().as_ref(), "<workspace>");
            if recipe
                .source_inventory
                .selection
                .units
                .iter()
                .any(|unit| unit.package == package && unit.manifest.is_some() && !unit.controller)
            {
                artifacts.push(CompilerArtifact {
                    files: strings(&value["filenames"])?
                        .into_iter()
                        .map(PathBuf::from)
                        .collect(),
                    source: PathBuf::from(string(&value["target"], "src_path")?),
                    build_script: strings(&value["target"]["kind"])?
                        .iter()
                        .any(|kind| kind == "custom-build"),
                    package,
                });
            }
        }
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
    let after = resolve_admission(root)?;
    if &after.recipe != recipe {
        return Err(fail(
            "producer inputs changed during compilation; no receipt published",
        ));
    }
    let built = built.ok_or_else(|| fail("Cargo did not identify the producer executable"))?;
    let compiler_inputs = CompilerInputs::from_artifacts(root, &artifacts).map_err(fail)?;
    let mut metadata = Command::new(cargo_program());
    metadata
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--locked"]);
    let metadata: Value = serde_json::from_str(&checked_output(metadata)?).map_err(fail)?;
    let selected: BTreeSet<_> = recipe
        .source_inventory
        .selection
        .units
        .iter()
        .filter(|unit| unit.manifest.is_none())
        .map(|unit| unit.package.as_str())
        .collect();
    let external_roots = metadata["packages"]
        .as_array()
        .ok_or_else(|| fail("missing resolved packages"))?
        .iter()
        .filter(|package| {
            package["id"]
                .as_str()
                .is_some_and(|id| selected.contains(id))
        })
        .map(|package| {
            Ok(PathBuf::from(string(package, "manifest_path")?)
                .parent()
                .ok_or_else(|| fail("package root absent"))?
                .to_path_buf())
        })
        .collect::<Result<Vec<_>>>()?;
    compiler_inputs
        .verify(
            root,
            &recipe.source_inventory,
            &after.resolution,
            &external_roots,
            &generated_roots,
        )
        .map_err(fail)?;
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
        schema: 2,
        recipe: recipe.clone(),
        executable_sha256: sha256_file(&temporary).map_err(fail)?,
        resolution: after.resolution,
    };
    receipt.verify_current_inputs(root).map_err(fail)?;
    replace_staged_executable(&temporary, staged, receipt_path)?;
    receipt.write(receipt_path).map_err(fail)?;
    println!("optimized producer staged at {}", staged.display());
    Ok(())
}

#[path = "producer.tests.rs"]
#[cfg(test)]
mod tests;

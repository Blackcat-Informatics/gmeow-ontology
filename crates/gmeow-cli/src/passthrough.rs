// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native/shim command surfaces: `gmeow gts` (a shim to the external `gts`
//! binary), `gmeow music` (the native `gmeow_music` engine), `gmeow affect`
//! (the native `gmeow_affect` intensity-geometry engine), and `gmeow math` (the
//! native `gmeow_math_lift` ingestion bridges).

use std::path::{Path, PathBuf};

use gmeow_cli_core::Reporter;

use crate::commands::{fail, fail_code};
use crate::{
    AffectCommands, BUNDLE_GTS, ClassifyMetric, FinanceCommands, MathCommands, MusicCommands,
};

/// The canonical core-affect metric profile — the default `gmeow affect classify`
/// vantage (bipolar [-1, 1] PAD, `gmeow:coreAffectGram`).
const CORE_AFFECT_METRIC_PAD: &str = "https://blackcatinformatics.ca/gmeow/coreAffectMetricPAD";

/// The IRI base every `gmeow math lift-*` run and codomain node is minted under.
///
/// It is a CONSTANT, and that is load-bearing rather than incidental. `RunFrame::mint`
/// content-addresses the run IRI on the source bytes, so with a fixed base the whole lift is
/// a pure function of the artifact: lifting the same file twice — on two machines, from two
/// working directories, a year apart — yields byte-identical Turtle. Deriving the base from
/// the clock, the cwd, the input path, or a counter would silently destroy that idempotence
/// and make every re-lift a new, incomparable graph. It is also what
/// `tests/self_sufficiency.rs` pins: that harness runs the binary from a blinded cwd with a
/// cleared environment and demands identical bytes.
///
/// The namespace is the repo-wide example convention (`gmeow:examples/<area>/…`, as used by
/// the `logic:`, `lang:`, and `affect` example IRIs) because that is exactly what a CLI lift
/// of a user's own file is: an EXAMPLE instance, not shipped ontology content. The bundle's
/// own in-bundle `math:` producers mint under their own producer namespace, so a
/// CLI-produced run can never be mistaken for — or collide with — a bundle-resident one.
const MATH_LIFT_MINT_BASE: &str = "https://blackcatinformatics.ca/gmeow/examples/math/lift/";

/// The install hint printed when the external `gts` binary cannot be found.
pub(crate) const GTS_INSTALL_HINT: &str = "gts binary not found. Install gmeow-gts: pip install gmeow-gts \
     (or cargo install gmeow-gts, etc.), or set GMEOW_GTS_BIN to its path.";

/// The GTS subcommands that expect a snapshot file argument — the bundled
/// snapshot is injected for these when the user gives none.
const FILE_SUBCOMMANDS: &[&str] = &["info", "verify", "ls", "fold", "extract-key"];

/// Resolve the external `gts` binary: `GMEOW_GTS_BIN` wins, then a `gts` on
/// `PATH`. Returns `None` when neither resolves (the caller HARD-FAILS).
pub(crate) fn resolve_gts_binary() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("GMEOW_GTS_BIN") {
        let path = PathBuf::from(&explicit);
        if !explicit.is_empty() && path.is_file() {
            return Some(path);
        }
    }
    which("gts")
}

/// Locate `name` on `PATH` (the executable-search fallback; no external crate).
fn which(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    // Append the platform executable suffix (empty on Unix, `.exe` on Windows)
    // so the lookup resolves `gts.exe` where the OS requires it.
    let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(&filename);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// `gmeow gts …` — forward arguments verbatim to the external `gts` binary,
/// injecting the bundled snapshot path for file-expecting subcommands. Propagates
/// the child's exit code; HARD-FAILS when the binary is absent.
pub(crate) fn gts(reporter: &dyn Reporter, args: &[String]) -> i32 {
    let Some(exe) = resolve_gts_binary() else {
        return fail(reporter, "gmeow-cli.gts.missing", GTS_INSTALL_HINT);
    };

    let mut forwarded: Vec<String> = args.to_vec();
    // Keep the embedded snapshot staged for the child's lifetime.
    let mut staged: Option<StagedBundle> = None;

    if forwarded.is_empty() {
        forwarded.push("--help".to_owned());
    } else if FILE_SUBCOMMANDS.contains(&forwarded[0].as_str()) {
        let tail = &forwarded[1..];
        let has_file_arg = if let Some(marker) = tail.iter().position(|a| a == "--") {
            marker + 1 < tail.len()
        } else {
            tail.iter().any(|a| !a.starts_with('-'))
        };
        if !has_file_arg {
            match StagedBundle::write(BUNDLE_GTS) {
                Ok(s) => {
                    let path = s.path().to_string_lossy().into_owned();
                    staged = Some(s);
                    forwarded.insert(1, path);
                }
                Err(e) => {
                    return fail(
                        reporter,
                        "gmeow-cli.gts.stage-bundle",
                        format!("cannot stage bundled snapshot: {e}"),
                    );
                }
            }
        }
    }

    let status = std::process::Command::new(&exe).args(&forwarded).status();
    drop(staged);
    match status {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => fail(
            reporter,
            "gmeow-cli.gts.spawn",
            format!("failed to run gts: {e}"),
        ),
    }
}

/// `gmeow music …` — the native music-package projection engine.
pub(crate) fn music(reporter: &dyn Reporter, command: &MusicCommands) -> i32 {
    let result = match command {
        MusicCommands::Render { source, to, out } => {
            gmeow_music::render_file(source, &to.to_lowercase(), out)
        }
        MusicCommands::Import { source, out } => gmeow_music::import_file(source, out),
    };
    match result {
        Ok(paths) => {
            for path in paths {
                println!("wrote {}", path.display());
            }
            0
        }
        Err(diag) => {
            // Mirror `ext/music/cli.py`: an unsupported-format failure maps to a
            // usage error (exit 2); any other failure is a runtime error (exit 1).
            // The class is read structurally off the typed music `Diag`, never by
            // sniffing the message text.
            let code = if diag.is::<gmeow_music::error::UnsupportedFormat>()
                || diag.is::<gmeow_music::error::UnsupportedImportSuffix>()
            {
                2
            } else {
                1
            };
            fail_code(
                reporter,
                "gmeow-cli.music.failed",
                format!("Error: {diag}"),
                code,
            )
        }
    }
}

/// Read a command source: standard input when the path is `-`, else the file.
fn read_source(path: &std::path::Path) -> std::io::Result<String> {
    if path.as_os_str() == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        std::fs::read_to_string(path)
    }
}

/// Read a command source as RAW BYTES: standard input when the path is `-`, else
/// the file.
///
/// Distinct from [`read_source`] because not every ingestible artifact is text —
/// an `.onnx` export is a binary protobuf message, and reading it through a
/// UTF-8-validating path would reject the very files the ONNX bridge exists to
/// lift. Text bridges keep their own UTF-8 check (their typed `SourceNotUtf8`
/// diagnostic), so nothing is weakened by handing them bytes here.
fn read_source_bytes(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    if path.as_os_str() == "-" {
        use std::io::Read;
        let mut buf = Vec::new();
        std::io::stdin().read_to_end(&mut buf)?;
        Ok(buf)
    } else {
        std::fs::read(path)
    }
}

/// Read the embedded bundle's SSSOM correspondence blob(s): the reviewed
/// label→emotion `skos:closeMatch` cells the pipeline keeps out of the base graph
/// (the claim-routing map's single source of truth). The registered label set and
/// canonical EmotionType typing come from the base graph itself.
fn bundled_sssom_texts() -> gmeow_errors::Result<Vec<String>> {
    let sssom = gmeow_pipeline::bundle_blobs::bundled_sssom(BUNDLE_GTS).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::BundleReadFailed {
            detail: format!("cannot read bundled SSSOM: {e}"),
        })
    })?;
    Ok(sssom
        .values()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .collect())
}

/// `gmeow affect …` — the native affect-intensity geometry engine.
///
/// Product results → stdout with stable, greppable key prefixes; any failure →
/// `Error: <message>` on stderr with exit `1`. The geometry is COMPUTED from the
/// snapshot's Gram matrix and appraisal vectors (the metric-tensor norm), never
/// read from a stored magnitude.
pub(crate) fn affect(reporter: &dyn Reporter, command: &AffectCommands) -> i32 {
    match command {
        AffectCommands::Intensity {
            source,
            observation,
            to,
        } => {
            let bytes = match std::fs::read(source) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return fail(
                        reporter,
                        "gmeow-cli.io.read",
                        format!("Error: cannot read {}: {e}", source.display()),
                    );
                }
            };
            if let Some(to) = to {
                // clap enforces `--to requires --observation`, so `observation`
                // is always present here.
                let observation = observation
                    .as_ref()
                    .expect("clap `requires` guarantees --observation when --to is set");
                let graph = purrdf::gts::reader::read(&bytes, false, None);
                match gmeow_affect::distance_and_cosine(&graph, observation, to) {
                    Ok((distance, cosine)) => {
                        println!("distance {distance}");
                        println!("cosine {cosine}");
                        0
                    }
                    Err(message) => fail(
                        reporter,
                        "gmeow-cli.affect.distance",
                        format!("Error: {message}"),
                    ),
                }
            } else {
                match gmeow_affect::geometry_from_gts_bytes(&bytes, observation.as_deref()) {
                    Ok(geometries) => {
                        for geom in geometries {
                            println!("observation {}", geom.observation);
                            println!("intensity {}", geom.intensity);
                            println!("quadratic-form {}", geom.quadratic_form);
                            println!("dominant-axis {}", geom.dominant_axis);
                            println!("pd-pivots {}", geom.pivots.join(" "));
                            for axis in &geom.normalized {
                                println!("axis {} normalized {}", axis.dimension, axis.value);
                            }
                        }
                        0
                    }
                    Err(message) => fail(
                        reporter,
                        "gmeow-cli.affect.geometry",
                        format!("Error: {message}"),
                    ),
                }
            }
        }
        AffectCommands::Classify {
            source,
            observation,
            prototype,
            metric,
            metric_profile,
            top_k,
        } => {
            if *top_k == Some(0) {
                return fail(
                    reporter,
                    "gmeow-cli.affect.classify",
                    "Error: --top-k must be at least 1".to_owned(),
                );
            }
            let bytes = match std::fs::read(source) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return fail(
                        reporter,
                        "gmeow-cli.io.read",
                        format!("Error: cannot read {}: {e}", source.display()),
                    );
                }
            };
            let graph = purrdf::gts::reader::read(&bytes, false, None);
            // Default to the full canonical prototype set when none is named.
            let prototypes = if prototype.is_empty() {
                gmeow_affect::affect_prototypes(&graph)
            } else {
                prototype.clone()
            };
            let lens = match metric {
                ClassifyMetric::Distance => gmeow_affect::MetricLens::GDistance,
                ClassifyMetric::Cosine => gmeow_affect::MetricLens::Cosine,
            };
            let profile = metric_profile
                .clone()
                .unwrap_or_else(|| CORE_AFFECT_METRIC_PAD.to_owned());
            match gmeow_affect::classify(&graph, observation, &prototypes, &profile, lens, *top_k) {
                Ok(classification) => {
                    println!("metric {}", classification.metric.tag());
                    println!("vantage {}", classification.vantage_profile);
                    for (rank, ranked) in classification.ranked.iter().enumerate() {
                        match &ranked.cosine {
                            Some(cosine) => println!(
                                "rank {} {} squared-distance {} distance {} cosine {}",
                                rank + 1,
                                ranked.prototype,
                                ranked.squared_distance,
                                ranked.distance,
                                cosine
                            ),
                            None => println!(
                                "rank {} {} squared-distance {} distance {}",
                                rank + 1,
                                ranked.prototype,
                                ranked.squared_distance,
                                ranked.distance
                            ),
                        }
                    }
                    if let Some(best) = classification.ranked.first() {
                        // "nearest" (distance) vs "most-aligned" (cosine) — the two
                        // lenses rank different quantities; the winner line names which.
                        match metric {
                            ClassifyMetric::Distance => println!("nearest {}", best.prototype),
                            ClassifyMetric::Cosine => println!("most-aligned {}", best.prototype),
                        }
                    }
                    println!(
                        "margin {} {}",
                        classification.margin_squared, classification.margin
                    );
                    0
                }
                Err(message) => fail(
                    reporter,
                    "gmeow-cli.affect.classify",
                    format!("Error: {message}"),
                ),
            }
        }
        AffectCommands::Ingest { source, out } => {
            let json = match read_source(source) {
                Ok(json) => json,
                Err(e) => {
                    return fail(
                        reporter,
                        "gmeow-cli.io.read",
                        format!("Error: cannot read {}: {e}", source.display()),
                    );
                }
            };
            let capture = match purrdf_lex::json::record::from_slice::<
                gmeow_affect_ingest::ClassifierRunCapture,
            >(json.as_bytes())
            {
                Ok(capture) => capture,
                Err(e) => {
                    return fail(
                        reporter,
                        "gmeow-cli.affect.capture-json",
                        format!("Error: invalid classifier capture JSON: {e}"),
                    );
                }
            };
            let sssom_texts = match bundled_sssom_texts() {
                Ok(texts) => texts,
                Err(e) => return fail(reporter, "gmeow-cli.affect.sssom", format!("Error: {e}")),
            };
            let config =
                match gmeow_affect_ingest::config_for_capture(BUNDLE_GTS, &sssom_texts, &capture) {
                    Ok(config) => config,
                    Err(e) => {
                        return fail(reporter, "gmeow-cli.affect.config", format!("Error: {e}"));
                    }
                };
            match gmeow_affect_ingest::produce(&capture, &config) {
                Ok(ttl) => match out {
                    Some(out) => match std::fs::write(out, &ttl) {
                        Ok(()) => 0,
                        Err(e) => fail(
                            reporter,
                            "gmeow-cli.io.write",
                            format!("Error: cannot write {}: {e}", out.display()),
                        ),
                    },
                    None => {
                        print!("{ttl}");
                        0
                    }
                },
                Err(e) => fail(reporter, "gmeow-cli.affect.produce", format!("Error: {e}")),
            }
        }
        AffectCommands::Recover { source } => {
            let ttl = match read_source(source) {
                Ok(ttl) => ttl,
                Err(e) => {
                    return fail(
                        reporter,
                        "gmeow-cli.io.read",
                        format!("Error: cannot read {}: {e}", source.display()),
                    );
                }
            };
            let sssom_texts = match bundled_sssom_texts() {
                Ok(texts) => texts,
                Err(e) => return fail(reporter, "gmeow-cli.affect.sssom", format!("Error: {e}")),
            };
            let config =
                match gmeow_affect_ingest::config_for_evidence(BUNDLE_GTS, &sssom_texts, &ttl) {
                    Ok(config) => config,
                    Err(e) => {
                        return fail(reporter, "gmeow-cli.affect.config", format!("Error: {e}"));
                    }
                };
            match gmeow_affect_ingest::recover(&ttl, &config) {
                Ok(capture) => {
                    let value = purrdf_lex::json::record::ToJson::to_json(&capture);
                    println!("{}", purrdf_lex::json::write_pretty(&value));
                    0
                }
                Err(e) => fail(reporter, "gmeow-cli.affect.recover", format!("Error: {e}")),
            }
        }
    }
}

/// Admit installed financial contracts and publish derived results with native proof.
pub(crate) fn finance(reporter: &dyn Reporter, command: &FinanceCommands) -> i32 {
    let FinanceCommands::Evaluate {
        source,
        subject,
        out,
    } = command;
    let result = (|| -> gmeow_errors::Result<()> {
        let bytes = read_source_bytes(source)?;
        let input = purrdf::parse_dataset(&bytes, "text/turtle", None)?;
        // Both recipes/signatures and the generated validation products come
        // from this exact authenticated installed snapshot, never a checkout.
        let bundle = gmeow_pipeline::bundle_blobs::Bundle::from_snapshot(BUNDLE_GTS)?;
        let owner = bundle.dataset()?;
        let mut shapes_text = String::new();
        for (path, bytes) in bundle.shapes()? {
            shapes_text.push_str(std::str::from_utf8(&bytes).map_err(|error| {
                gmeow_errors::Diag::of_kind(gmeow_logic::error::Result {
                    detail: format!("invalid bundled shape {path}: {error}"),
                })
            })?);
            shapes_text.push('\n');
        }
        let shapes = purrdf::shapes::engine::parse_shapes(&shapes_text, None).map_err(|error| {
            gmeow_errors::Diag::of_kind(gmeow_logic::error::Result {
                detail: format!("invalid complete bundled financial projections: {error}"),
            })
        })?;
        let identity = format!(
            "blake3:{}",
            purrdf_hash::hex::encode(purrdf_hash::blake3::hash(BUNDLE_GTS).as_bytes())
        );
        let evaluation = gmeow_logic::finance_evaluation::evaluate(
            &input,
            gmeow_logic::finance_evaluation::FinanceOwner {
                dataset: &owner,
                shapes: &shapes,
                identity: &identity,
            },
            subject,
        )?;
        let mut turtle = gmeow_logic::reason::artifacts::build_inferred_closure_ttl(
            &evaluation.result,
            None,
            &[],
        )?;
        turtle.push_str(&gmeow_logic::result_rdf::project_reasoning_result(
            &evaluation.result,
        )?);
        let output = math_output_path(out)?;
        write_math_bytes(&output, turtle.as_bytes())?;
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(diag) => gmeow_cli_core::emit_and_exit(
            reporter,
            diag.with_context("gmeow finance evaluate")
                .with_location(gmeow_errors::Location {
                    path: Some(source.display().to_string()),
                    ..gmeow_errors::Location::default()
                }),
            "gmeow",
        ),
    }
}

/// `gmeow math …` — the native `math:` ingestion bridges (R / ONNX / TSTP proof).
///
/// Product (canonical Turtle) → stdout, or `--out FILE` byte-for-byte; diagnostics → stderr
/// through the shared reporter. The lift is a pure function of the source bytes and
/// [`MATH_LIFT_MINT_BASE`], so re-running the same command is byte-identical.
///
/// # Failure surface
///
/// A lift failure is emitted as the ENGINE's own typed `Diag`, forwarded verbatim through
/// [`gmeow_cli_core::emit_and_exit`] — never re-coded, never flattened to a rendered string.
/// The finding therefore carries the `math-lift` kind's real code (`math.lift.r.parse`,
/// `math.lift.onnx.wire`, `math.lift.proof.unliftable`, …), its real
/// [`FindingCategory`](gmeow_errors::FindingCategory), and — because every kind is raised
/// with `Diag::of_kind` — its `detail`. This handler adds only what the engine cannot know:
/// the artifact PATH as the finding's source location, and a context frame naming the
/// invoked bridge.
///
/// That is load-bearing, not cosmetic. A finding's identity fingerprint (and so its
/// `finding_iri`) is `blake3` over `(code, category, source anchor)`. Collapsing eight engine
/// kinds onto two CLI codes with an empty anchor made a truncated protobuf and an R syntax
/// error the SAME witness — one fingerprint, indistinguishable to `gmeow explain` and to the
/// diagnostics substrate. Forwarding the typed diagnostic keeps each kind, and each artifact,
/// a distinct witness.
///
/// The two-class taxonomy the bridges observe is carried by the engine's `category`, which is
/// where it belongs:
///
/// - [`FindingCategory::DataShapeViolation`](gmeow_errors::FindingCategory::DataShapeViolation)
///   (`SourceNotUtf8`, `RParse`, `OnnxWire`, `TstpParse`) — the artifact is not a well-formed
///   instance of its own format. The user's file is broken; no bridge, however capable, could
///   read it.
/// - [`FindingCategory::ModelingDisciplineViolation`](gmeow_errors::FindingCategory::ModelingDisciplineViolation)
///   (`RUnliftable`, `OnnxUnliftable`, `ProofUnliftable`, `EmptyCodomain`) — the artifact IS
///   well-formed, and is still not carryable into the `math:` codomain: an R script that is
///   only control flow, an ONNX model with no computation node, a derivation with a dangling
///   parent. Nothing is wrong with the file; it simply states nothing this bridge can
///   honestly claim to have carried across.
///
/// # Exit-code policy
///
/// Every lift failure — malformed or unliftable — exits `1`, the handled-failure code
/// [`gmeow_cli_core::exit_code`] maps an Error-grade report to. That is deliberate, not a
/// conflation:
///
/// - Exit `2` is reserved by this CLI's console convention for USAGE errors: defects in the
///   argument vector, which clap detects before a single byte of the input is read. Both lift
///   failures happen on a correctly spelled invocation over a file that was found and read,
///   so neither is a usage error. (`gmeow music` maps its unsupported-format failures to `2`
///   because there the defect genuinely IS an argument value — a `--to` the engine has no
///   writer for. Nothing here is argument-shaped.)
/// - No distinction is lost by sharing an exit code, because the exit code is not where the
///   distinction lives. A corpus sweep branches on the finding's `code` and `category` — the
///   channel that actually carries all eight kinds — which is exactly what forwarding the
///   typed diagnostic guarantees. A process exit status cannot carry an eight-way kind
///   distinction, and minting a third integer for a lossy copy of information the reporter
///   already emits exactly would invite callers to branch on the weaker channel.
pub(crate) fn math(reporter: &dyn Reporter, command: &MathCommands) -> i32 {
    if matches!(
        command,
        MathCommands::Project { .. } | MathCommands::Lift { .. } | MathCommands::Evaluate { .. }
    ) {
        return math_interchange(reporter, command);
    }
    // One shared body for the three bridges: they differ only in which `lift` they call,
    // because every bridge has the identical `(&[u8], &str) -> Result<Lifted>` contract.
    type LiftFn = fn(&[u8], &str) -> gmeow_errors::Result<gmeow_math_lift::Lifted>;
    // `leaf` is the invoked subcommand name, carried onto a failure's context frame so the
    // finding says which bridge refused without the reader having to infer it from the code.
    let (source, out, lift, leaf): (&PathBuf, &Option<PathBuf>, LiftFn, &str) = match command {
        MathCommands::Project { .. }
        | MathCommands::Lift { .. }
        | MathCommands::Evaluate { .. } => {
            unreachable!("interchange commands dispatched above")
        }
        MathCommands::LiftR { source, out } => (source, out, gmeow_math_lift::r::lift, "lift-r"),
        MathCommands::LiftOnnx { source, out } => {
            (source, out, gmeow_math_lift::onnx::lift, "lift-onnx")
        }
        MathCommands::LiftProof { source, out } => {
            (source, out, gmeow_math_lift::proof::lift, "lift-proof")
        }
    };

    // Bytes, never a UTF-8 string: an `.onnx` export is binary protobuf. Each text bridge
    // runs its own UTF-8 check and raises its own typed `SourceNotUtf8`.
    let bytes = match read_source_bytes(source) {
        Ok(bytes) => bytes,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.io.read",
                format!("Error: cannot read {}: {e}", source.display()),
            );
        }
    };

    match lift(&bytes, MATH_LIFT_MINT_BASE) {
        Ok(lifted) => match out {
            Some(out) => match std::fs::write(out, &lifted.turtle) {
                Ok(()) => 0,
                Err(e) => fail(
                    reporter,
                    "gmeow-cli.io.write",
                    format!("Error: cannot write {}: {e}", out.display()),
                ),
            },
            None => {
                // `print!`, not `println!`: the serialized Turtle already ends in a
                // newline, and `--out` must receive the SAME bytes stdout does.
                print!("{}", lifted.turtle);
                0
            }
        },
        // The engine's typed diagnostic, forwarded WHOLE: its own code, category, grade, and
        // `detail` reach the reporter untouched. Only the two facts the engine cannot know are
        // added — the artifact path (the finding's source anchor, which also makes two broken
        // files distinct witnesses) and the invoked bridge (a context frame, projected into the
        // finding's `detail`). Re-coding or `format!`-ing the diagnostic here would discard
        // exactly the structure an agent consuming `--console jsonl` branches on.
        Err(diag) => gmeow_cli_core::emit_and_exit(
            reporter,
            diag.with_location(gmeow_errors::Location {
                path: Some(source.display().to_string()),
                ..gmeow_errors::Location::default()
            })
            .with_context(format!("gmeow math {leaf}")),
            "gmeow",
        ),
    }
}

/// A scoped temp copy of the embedded bundle, removed on drop.
///
/// Backed by [`tempfile::NamedTempFile`], which creates the file atomically with
/// `O_EXCL` semantics and a randomized name — closing the TOCTOU / symlink race a
/// predictable pid+timestamp name would open — and unlinks it on drop.
struct StagedBundle {
    file: tempfile::NamedTempFile,
}

impl StagedBundle {
    fn write(bytes: &[u8]) -> std::io::Result<Self> {
        use std::io::Write;
        let mut file = tempfile::Builder::new()
            .prefix("gmeow-gts-")
            .suffix(".gts")
            .tempfile()?;
        file.write_all(bytes)?;
        file.flush()?;
        Ok(Self { file })
    }

    fn path(&self) -> &Path {
        self.file.path()
    }
}

fn math_diag_at(diag: gmeow_errors::Diag, path: &Path, leaf: &str) -> gmeow_errors::Diag {
    let mut location = diag.inner().source_ctx.location.clone();
    location.path = Some(path.display().to_string());
    diag.with_location(location)
        .with_context(format!("gmeow math {leaf}"))
}

fn math_reference(
    path: &Path,
    leaf: &str,
) -> gmeow_errors::Result<std::sync::Arc<purrdf::RdfDataset>> {
    let bytes = std::fs::read(path).map_err(|error| math_diag_at(error.into(), path, leaf))?;
    purrdf::parse_dataset(&bytes, "text/turtle", None)
        .map_err(|error| math_diag_at(error.into(), path, leaf))
}

/// Resolve existing parents and final symlinks, including a dangling link to a new output.
fn math_output_path(path: &Path) -> std::io::Result<PathBuf> {
    let mut selected = std::path::absolute(path)?;
    let mut seen = std::collections::HashSet::with_hasher(purrdf_hash::fixed::FixedState::new());
    loop {
        let name = selected.file_name().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "output needs a file name")
        })?;
        let parent = selected.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "output needs a parent directory",
            )
        })?;
        let parent = std::fs::canonicalize(parent)?;
        let candidate = parent.join(name);
        if !seen.insert(candidate.clone()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "cyclic output symlink",
            ));
        }
        match std::fs::symlink_metadata(&candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target = std::fs::read_link(&candidate)?;
                selected = if target.is_absolute() {
                    target
                } else {
                    parent.join(target)
                };
            }
            Ok(_) => return std::fs::canonicalize(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidate),
            Err(error) => return Err(error),
        }
    }
}

fn write_math_bytes(path: &Path, bytes: &[u8]) -> gmeow_errors::Result<()> {
    use std::io::Write;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "output needs a parent directory",
        )
    })?;
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Creation applies the caller's umask, just as an ordinary output write does.
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut file = builder.tempfile_in(parent)?;
    file.write_all(bytes)?;
    file.flush()?;
    match std::fs::metadata(path) {
        Ok(metadata) => file.as_file().set_permissions(metadata.permissions())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    file.persist(path)?;
    Ok(())
}

fn require_distinct_math_outputs(out: &Path, ledger: &Path) -> gmeow_errors::Result<()> {
    if out == ledger {
        return Err(gmeow_errors::Diag::of_kind(
            gmeow_math_lift::error::ProjectionInput {
                detail: "artifact and ledger paths must differ".to_owned(),
            },
        ));
    }
    Ok(())
}

/// Installed interchange entry point: all inputs are explicit user artifacts.
fn math_interchange(reporter: &dyn Reporter, command: &MathCommands) -> i32 {
    use gmeow_math_lift::projection::{self, Target};
    let result = (|| -> gmeow_errors::Result<()> {
        match command {
            MathCommands::Evaluate {
                source,
                subject,
                bindings,
                out,
            } => {
                let bytes = read_source_bytes(source)
                    .map_err(|error| math_diag_at(error.into(), source, "evaluate"))?;
                let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
                    .map_err(|error| math_diag_at(error.into(), source, "evaluate"))?;
                let mut selected = std::collections::BTreeMap::new();
                for binding in bindings {
                    let (symbol, quantity) = binding
                        .split_once('=')
                        .filter(|(s, q)| !s.is_empty() && !q.is_empty())
                        .ok_or_else(|| {
                            gmeow_errors::Diag::of_kind(gmeow_logic::error::Result {
                                detail: "--bind requires SYMBOL=QUANTITY".into(),
                            })
                            .with_context("gmeow math evaluate")
                            .with_location(gmeow_errors::Location {
                                path: Some(source.display().to_string()),
                                ..gmeow_errors::Location::default()
                            })
                        })?;
                    if selected
                        .insert(symbol.to_owned(), quantity.to_owned())
                        .is_some()
                    {
                        return Err(gmeow_errors::Diag::of_kind(gmeow_logic::error::Result {
                            detail: format!("duplicate --bind for {symbol}"),
                        })
                        .with_context("gmeow math evaluate")
                        .with_location(gmeow_errors::Location {
                            path: Some(source.display().to_string()),
                            ..gmeow_errors::Location::default()
                        }));
                    }
                }
                let evaluation =
                    gmeow_logic::math_evaluation::evaluate(&dataset, subject, &selected)
                        .map_err(|diag| math_diag_at(diag, source, "evaluate"))?;
                let mut turtle = gmeow_logic::reason::artifacts::build_inferred_closure_ttl(
                    &evaluation.result,
                    None,
                    &[],
                )?;
                turtle.push_str(&gmeow_logic::result_rdf::project_reasoning_result(
                    &evaluation.result,
                )?);
                let output = math_output_path(out)
                    .map_err(|error| math_diag_at(error.into(), out, "evaluate"))?;
                write_math_bytes(&output, turtle.as_bytes())
                    .map_err(|diag| math_diag_at(diag, out, "evaluate"))?;
            }
            MathCommands::Project {
                source,
                subject,
                to,
                reference,
                out,
                ledger,
            } => {
                let output = math_output_path(out)
                    .map_err(|error| math_diag_at(error.into(), out, "project"))?;
                let ledger_output = math_output_path(ledger)
                    .map_err(|error| math_diag_at(error.into(), ledger, "project"))?;
                require_distinct_math_outputs(&output, &ledger_output)
                    .map_err(|diag| math_diag_at(diag, out, "project"))?;
                let bytes = read_source_bytes(source)
                    .map_err(|error| math_diag_at(error.into(), source, "project"))?;
                let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
                    .map_err(|error| math_diag_at(error.into(), source, "project"))?;
                let reference = math_reference(reference, "project")?;
                let emission =
                    projection::project(&dataset, subject, Target::parse(to)?, &reference)
                        .map_err(|diag| math_diag_at(diag, source, "project"))?;
                write_math_bytes(&ledger_output, emission.ledger.as_bytes())
                    .map_err(|diag| math_diag_at(diag, ledger, "project"))?;
                // Publishing the ledger can expose a previously absent case-insensitive alias.
                let output = math_output_path(&output)
                    .map_err(|error| math_diag_at(error.into(), out, "project"))?;
                let ledger_output = math_output_path(&ledger_output)
                    .map_err(|error| math_diag_at(error.into(), ledger, "project"))?;
                require_distinct_math_outputs(&output, &ledger_output)
                    .map_err(|diag| math_diag_at(diag, out, "project"))?;
                write_math_bytes(&output, &emission.bytes)
                    .map_err(|diag| math_diag_at(diag, out, "project"))?;
                let published_ledger = std::fs::read(&ledger_output)
                    .map_err(|error| math_diag_at(error.into(), ledger, "project"))?;
                if published_ledger.as_slice() != emission.ledger.as_bytes() {
                    return Err(math_diag_at(
                        gmeow_errors::Diag::of_kind(gmeow_math_lift::error::ProjectionInput {
                            detail:
                                "artifact publication did not preserve the mandatory loss ledger"
                                    .to_owned(),
                        }),
                        ledger,
                        "project",
                    ));
                }
            }
            MathCommands::Lift {
                source,
                from,
                subject,
                reference,
                out,
            } => {
                let bytes = read_source_bytes(source)
                    .map_err(|error| math_diag_at(error.into(), source, "lift"))?;
                let reference = math_reference(reference, "lift")?;
                let lifted = projection::lift(
                    &bytes,
                    Target::parse(from)?,
                    subject,
                    &reference,
                    MATH_LIFT_MINT_BASE,
                )
                .map_err(|diag| math_diag_at(diag, source, "lift"))?;
                std::fs::write(out, lifted.turtle)
                    .map_err(|error| math_diag_at(error.into(), out, "lift"))?;
            }
            _ => unreachable!("only interchange commands reach this entry point"),
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(diag) => gmeow_cli_core::emit_and_exit(reporter, diag, "gmeow"),
    }
}

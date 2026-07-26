// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native/shim command surfaces: `gmeow gts` (a shim to the external `gts`
//! binary), `gmeow music` (the native `gmeow_music` engine), `gmeow affect`
//! (the native `gmeow_affect` intensity-geometry engine), and `gmeow math` (the
//! native `gmeow_math_lift` ingestion bridges).

use std::path::{Path, PathBuf};

use gmeow_cli_core::Reporter;

use crate::commands::{fail, fail_code};
use crate::{AffectCommands, BUNDLE_GTS, ClassifyMetric, MathCommands, MusicCommands};

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
            let capture =
                match serde_json::from_str::<gmeow_affect_ingest::ClassifierRunCapture>(&json) {
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
                Ok(capture) => match serde_json::to_string_pretty(&capture) {
                    Ok(json) => {
                        println!("{json}");
                        0
                    }
                    Err(e) => fail(
                        reporter,
                        "gmeow-cli.affect.recover-json",
                        format!("Error: {e}"),
                    ),
                },
                Err(e) => fail(reporter, "gmeow-cli.affect.recover", format!("Error: {e}")),
            }
        }
    }
}

/// Classify a `gmeow_math_lift` failure STRUCTURALLY, off the typed `Diag`.
///
/// The class is read with `Diag::is::<T>()` against the concrete diagnostic kinds the lift
/// crate declares — never by sniffing the rendered message, which is prose and not a
/// contract. The returned string is the CLI-side diagnostic code the reporter emits, so an
/// agent consuming `--console jsonl` can branch on the distinction losslessly.
///
/// Two classes exist, and they are genuinely different failures:
///
/// - **malformed** (`SourceNotUtf8`, `RParse`, `OnnxWire`, `TstpParse`) — the artifact is not
///   a well-formed instance of its own format. The user's file is broken; no bridge, however
///   capable, could read it.
/// - **unliftable** (`RUnliftable`, `OnnxUnliftable`, `ProofUnliftable`, `EmptyCodomain`) —
///   the artifact IS well-formed, and is still not carryable into the `math:` codomain: an R
///   script that is only control flow, an ONNX model with no computation node, a derivation
///   with a dangling parent. Nothing is wrong with the file; it simply states nothing this
///   bridge can honestly claim to have carried across.
fn math_lift_diag_code(diag: &gmeow_errors::Diag) -> &'static str {
    use gmeow_math_lift::error::{
        EmptyCodomain, OnnxUnliftable, OnnxWire, ProofUnliftable, RParse, RUnliftable,
        SourceNotUtf8, TstpParse,
    };
    if diag.is::<SourceNotUtf8>()
        || diag.is::<RParse>()
        || diag.is::<OnnxWire>()
        || diag.is::<TstpParse>()
    {
        "gmeow-cli.math.lift.malformed"
    } else if diag.is::<RUnliftable>()
        || diag.is::<OnnxUnliftable>()
        || diag.is::<ProofUnliftable>()
        || diag.is::<EmptyCodomain>()
    {
        "gmeow-cli.math.lift.unliftable"
    } else {
        "gmeow-cli.math.lift.failed"
    }
}

/// `gmeow math …` — the native `math:` ingestion bridges (R / ONNX / TSTP proof).
///
/// Product (canonical Turtle) → stdout, or `--out FILE` byte-for-byte; diagnostics → stderr
/// through the shared reporter. The lift is a pure function of the source bytes and
/// [`MATH_LIFT_MINT_BASE`], so re-running the same command is byte-identical.
///
/// # Exit-code policy
///
/// Both failure classes [`math_lift_diag_code`] distinguishes — a malformed artifact and a
/// well-formed-but-unliftable one — exit `1`, the handled-failure code. That is deliberate,
/// not a conflation:
///
/// - Exit `2` is reserved by this CLI's console convention for USAGE errors: defects in the
///   argument vector, which clap detects before a single byte of the input is read. Both lift
///   failures happen on a correctly spelled invocation over a file that was found and read,
///   so neither is a usage error. (`gmeow music` maps its unsupported-format failures to `2`
///   because there the defect genuinely IS an argument value — a `--to` the engine has no
///   writer for. Nothing here is argument-shaped.)
/// - The malformed/unliftable distinction is not lost by sharing an exit code. It travels
///   losslessly on the diagnostic channel as a structural code — `math.lift.r.parse` versus
///   `math.lift.r.unliftable` from the engine, plus this handler's own
///   `gmeow-cli.math.lift.{malformed,unliftable}` — which is what a script sweeping a corpus
///   should branch on. A process exit status cannot carry an eight-way kind distinction, and
///   minting a third integer for a lossy copy of information the reporter already emits
///   exactly would invite callers to branch on the weaker channel.
pub(crate) fn math(reporter: &dyn Reporter, command: &MathCommands) -> i32 {
    // One shared body for the three bridges: they differ only in which `lift` they call,
    // because every bridge has the identical `(&[u8], &str) -> Result<Lifted>` contract.
    type LiftFn = fn(&[u8], &str) -> gmeow_errors::Result<gmeow_math_lift::Lifted>;
    let (source, out, lift): (&PathBuf, &Option<PathBuf>, LiftFn) = match command {
        MathCommands::LiftR { source, out } => (source, out, gmeow_math_lift::r::lift),
        MathCommands::LiftOnnx { source, out } => (source, out, gmeow_math_lift::onnx::lift),
        MathCommands::LiftProof { source, out } => (source, out, gmeow_math_lift::proof::lift),
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
        Err(diag) => fail(
            reporter,
            math_lift_diag_code(&diag),
            format!("Error: {diag}"),
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

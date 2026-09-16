// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated `gmeow-dev medium-sweep` and `medium-seed` maintenance commands.
//!
//! The CLI admits its O3/full-LTO producer before dispatching either command.
//!
//! It runs the real DAG once, then the full `(strategy × target length)` grid per
//! MEASURABLE declared dictionary plus the global `(codec × level)` grid, and writes
//! the winner table with its whole grid as evidence. `make maint-medium-sweep` wires
//! it; the build then CONSUMES only the committed winners, so
//! `stage-medium-dictionaries` stays deterministic and sweep-free.
//!
//! Mirrors `bench-compare --emit-baseline` and
//! `bench-engines --emit-cost`: a deliberate, hand-committed refresh, never auto-drift.
//!
//! # One condition it STOPS on, and one it merely RECORDS
//!
//! * **STOP** — a dictionary whose winning cell still does not pay for itself. Silently
//!   dropping it would retire a shipped dictionary, which orphans every artifact already
//!   primed with it, so the numbers are written and the process exits non-zero for a
//!   human. (Three dictionaries have been retired this way; `bench/README.md` records
//!   the rule the three cases agree on.)
//! * **RECORD** — the mandated `zstd-rsyncable` @ 12 chain is not the codec grid's
//!   argmin. It is not, and the artifact says so (`mandated_is_argmin: false`) with the
//!   full grid beside it. That was raised as a STOP once and ANSWERED: the chain is
//!   KEPT, because the grid prices SIZE ONLY while GTS §8.4 rsyncable framing buys
//!   delta-transfer locality no size grid can see, and changing it is a normative
//!   Rule 6 change. Re-raising a settled question on every refresh would make the lane
//!   fail forever and teach a maintainer to ignore its exit code, so the evidence is
//!   printed on every run and the reasoning lives in `bench/README.md`.
//!
//! In both cases the artifact IS written first: the evidence is the point, and a
//! refusal that also discarded its own measurements would leave nothing to act on.

use std::path::Path;

use gmeow_pipeline::medium::registry::MediumRegistry;
use gmeow_pipeline::medium::sweep::{self, MediumBaseline};

/// The emitting tool name.
const TOOL: &str = "medium-sweep";

/// Measure the selected checkout and write its full evidence to `out`.
///
/// The CLI admits the producer first. Returns 0 on success, or 1 on a sweep,
/// serialization, or write failure, or when a dictionary does not pay for itself.
/// Measurement evidence is written before reporting a dictionary stop condition.
pub(crate) fn run(out: &Path) -> i32 {
    let root = crate::dev_common::project_root();
    let root = root.as_path();
    let baseline = match sweep::run_sweep(root) {
        Ok(baseline) => baseline,
        Err(err) => {
            eprintln!("{TOOL}: the sweep failed: {err}");
            return 1;
        }
    };

    let json = match baseline.to_json() {
        Ok(json) => json,
        Err(err) => {
            eprintln!("{TOOL}: cannot serialize the winner table: {err}");
            return 1;
        }
    };
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty())
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        eprintln!("{TOOL}: cannot create {}: {err}", parent.display());
        return 1;
    }
    if let Err(err) = std::fs::write(out, &json) {
        eprintln!("{TOOL}: cannot write {}: {err}", out.display());
        return 1;
    }

    report(&baseline, out, json.len());
    if stop_conditions(&baseline) {
        return 1;
    }
    0
}

/// Write the BOOTSTRAP table derived from the authored declarations alone.
///
/// It exists solely to break a start-up cycle: the sweep runs the whole DAG to
/// measure, and `stage-medium-dictionaries` refuses to run without a committed table,
/// so the first sweep in a fresh tree has no way in. Every measured field is zero and
/// the sweep overwrites the file moments later;
/// `the_committed_winner_table_carries_real_measurements` refuses a seed that was ever
/// committed.
/// The CLI admits the producer first. Returns 0 after writing `out`, or 1 if
/// source loading, declaration parsing, serialization, or writing fails.
pub(crate) fn seed(out: &Path) -> i32 {
    let root = crate::dev_common::project_root();
    let module = root.join("slices/core/gts/module.ttl");
    let text = match std::fs::read_to_string(&module) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("{TOOL}: cannot read {}: {err}", module.display());
            return 1;
        }
    };
    let dataset = match purrdf::parse_dataset(
        text.as_bytes(),
        "text/turtle",
        Some("https://blackcatinformatics.ca/gmeow/"),
    ) {
        Ok(dataset) => dataset,
        Err(err) => {
            eprintln!("{TOOL}: the gts slice does not parse: {err}");
            return 1;
        }
    };
    let registry = match MediumRegistry::from_dataset(&dataset) {
        Ok(registry) => registry,
        Err(err) => {
            eprintln!("{TOOL}: the medium axis does not read: {err}");
            return 1;
        }
    };
    let json = match sweep::seed_from_registry(&registry).to_json() {
        Ok(json) => json,
        Err(err) => {
            eprintln!("{TOOL}: cannot serialize the seed table: {err}");
            return 1;
        }
    };
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty())
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        eprintln!("{TOOL}: cannot create {}: {err}", parent.display());
        return 1;
    }
    if let Err(err) = std::fs::write(out, &json) {
        eprintln!("{TOOL}: cannot write {}: {err}", out.display());
        return 1;
    }
    eprintln!(
        "{TOOL}: wrote a BOOTSTRAP {} with ZERO measurements — it is not evidence and must never \
         be committed. Run `make maint-medium-sweep` now; it overwrites this file with the real \
         grid.",
        out.display()
    );
    0
}

/// Print the winner table and the codec evidence to stdout — the maintainer reads
/// this, not the JSON.
fn report(baseline: &MediumBaseline, out: &Path, bytes: usize) {
    println!("wrote {} ({bytes} bytes)\n", out.display());
    println!(
        "codec sweep over {} frame(s) / {} B (excluding {:?}):",
        baseline.codec_sweep.corpus_frame_count,
        baseline.codec_sweep.corpus_bytes,
        baseline.codec_sweep.excluded_reps
    );
    for row in &baseline.codec_sweep.rows {
        let mandated = row.codec == baseline.codec_sweep.mandated_codec
            && row.level == baseline.codec_sweep.mandated_level;
        println!(
            "  {:<16} level {:>2}  {:>12} B{}",
            row.codec,
            row.level,
            row.bytes,
            if mandated { "   <- MANDATED" } else { "" }
        );
    }
    println!(
        "  mandated cell is the argmin: {}\n",
        baseline.codec_sweep.mandated_is_argmin
    );

    println!("dictionary winners:");
    for row in &baseline.dictionaries {
        println!(
            "  {:<26} {:<22} winner {}/{} (declared {}/{}{})  two-part {} B vs baseline {} B  \
             gain {}  frames {}  corpus {} trained / {} held out  corpus-id {}",
            row.id,
            row.population,
            row.winning_strategy,
            row.winning_target_length,
            row.declared_strategy,
            row.declared_target_length,
            if row.declared_is_argmin {
                ""
            } else {
                " — DIVERGES"
            },
            row.two_part_code_bytes,
            row.bytes_on_disk_baseline,
            row.dictionary_gain_fraction,
            row.evaluated_frame_count,
            row.corpus_sample_count,
            row.held_out_sample_count,
            row.corpus_digest,
        );
    }
    println!();
}

/// Whether a DECLARED stop-and-ask condition fired. Every finding is reported; none is
/// resolved here.
fn stop_conditions(baseline: &MediumBaseline) -> bool {
    let mut stop = false;
    if !baseline.codec_sweep.mandated_is_argmin {
        // RECORDED, not a stop: this question was raised and ANSWERED — the mandated
        // chain is kept. See the module docs and `bench/README.md`.
        eprintln!(
            "{TOOL}: FINDING — the mandated {} @ level {} chain is NOT the codec grid's argmin, \
             and it is KEPT anyway. The grid prices SIZE ONLY; GTS §8.4 rsyncable framing buys \
             delta-transfer locality no size grid can see, and the mandated profile is normative \
             Rule 6 doctrine. The full grid is in the artifact and the reasoning — including what \
             would have to change for the answer to move — is recorded in bench/README.md. This \
             is not a stop-and-ask; re-raising a settled question on every refresh would teach a \
             maintainer to ignore this lane's exit code.",
            baseline.codec_sweep.mandated_codec, baseline.codec_sweep.mandated_level
        );
    }
    for row in &baseline.dictionaries {
        if row.two_part_code_bytes >= row.bytes_on_disk_baseline {
            eprintln!(
                "{TOOL}: STOP — dictionary {:?} does not pay for itself at its BEST cell: \
                 two-part code {} B vs baseline {} B over population `{}`. The numbers are \
                 written. Retiring a shipped dictionary orphans every artifact already primed \
                 with it, so this needs a human decision, never a silent removal.",
                row.id, row.two_part_code_bytes, row.bytes_on_disk_baseline, row.population
            );
            stop = true;
        }
        if !row.declared_is_argmin {
            eprintln!(
                "{TOOL}: FINDING — dictionary {:?} declares {}/{} but the measured argmin is \
                 {}/{}. The declaration is NOT overwritten here: reconcile \
                 slices/core/gts/module.ttl with the evidence (or record why the declaration \
                 stands) — `the_declared_training_points_are_the_committed_winners` reds until you do.",
                row.id,
                row.declared_strategy,
                row.declared_target_length,
                row.winning_strategy,
                row.winning_target_length
            );
        }
    }
    stop
}

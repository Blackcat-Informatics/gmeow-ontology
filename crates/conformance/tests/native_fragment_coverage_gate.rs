// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native OWL 2 DL/Full refutation-kernel PER-FAMILY coverage gate.
//!
//! The refutation kernel (`crates/logic/src/reason/refute.rs` + its datatype /
//! counting / case-split sub-deciders) decides a precisely-characterized COMPLETE
//! fragment of the beyond-Horn OWL 2 DL/Full constructs the forward chase cannot
//! forward-derive, and honestly WITHHOLDS outside it. The issue's acceptance
//! criterion — "every named construct family reaches a DECIDE-or-documented-withhold
//! pole" — was prose ("adversarially confirm"); this gate makes it an EXECUTABLE
//! production-surface check.
//!
//! For each of the SEVEN named construct families (Family 6 splits into the
//! arithmetic-identity 6a and malformed-list 6b patterns, so eight fragment ids
//! total) the gate proves the family reaches EXACTLY ONE pole:
//!
//! 1. **Decided pole (all seven today):** the family's representative
//!    slug/fixture is present and its committed decided verdict is REPRODUCED live
//!    by `gmeow_logic::reason::dl_consistency` on the committed `input.nq` — a clean
//!    `consistent` / `inconsistent` (never `incomplete`) that equals the case's own
//!    committed goldens, AND the W3C published verdict for the W3C-sourced
//!    decided-corpus cases. Each per-case run is wrapped in a bounded-join worker
//!    thread (mirroring `full_divergence_gate`) so a wedged chase can never hang the
//!    gate; none of these eight representatives are the memory/compute-heavy ones.
//!    The decided pole is ALSO pinned in the shipped registry: every named family's
//!    fragment id appears as a `logic:DecidedFragment` individual in
//!    `slices/grounding/logic/module.ttl`.
//! 2. **Withhold pole (the retained boundaries):** the constructs the kernel
//!    deliberately does NOT decide ship as `logic:expressivenessBoundary` records
//!    with a non-empty technical `logic:fragmentBoundaryReason`. This gate asserts
//!    that machinery is real (every retained boundary id has a non-empty reason) and
//!    that NO shipped technical characterization — boundary reason OR completeness
//!    bound — carries a PROCESS REFERENCE (`#<digit>`, `issue`, a bare `PR` token, or
//!    `per #`, case-insensitive). That is the issue's "no process references"
//!    acceptance criterion as an executable check.
//!
//! A family that reaches NEITHER pole (its representative does not decide AND its
//! fragment id ships neither as a decided fragment nor as a retained boundary) FAILS
//! here — the machine replacement for the removed prose.
//!
//! Two further gates close the residual honesty gaps the family-substitute keying left
//! open:
//!
//! * [`native_fragment_named_divergence_slugs_reach_exactly_one_documented_pole`] pins
//!   the THREE issue-NAMED W3C-divergence slugs (`one-two`,
//!   `webont-description-logic-035`, `rolechainviolationlumen`) DIRECTLY — on their own
//!   committed `input.nq`, not via a family substitute fixture — so each is proven to
//!   reach decide-and-agree-with-W3C OR a DOCUMENTED, LEDGER-IDENTIFIED capability-gap
//!   withhold naming its out-of-fragment construct. This closes the silent narrowing
//!   where a substitute fixture stood in for these harder, entangled cases.
//! * [`native_fragment_retained_boundaries_bound_to_live_withhold`] binds every shipped
//!   `logic:expressivenessBoundary` record to a LIVE representative the reasoner
//!   actually withholds over (with a ledger-identified capability-gap finding), so the
//!   authored boundary surface cannot drift from behavior — the WITHHOLD-pole peer of
//!   the DECIDE-pole `check_representative` bindings.
//!
//! The shipped `logic:DecidedFragment` / `logic:expressivenessBoundary` manifest is
//! proven to be EXACTLY the projection of the Rust kernel registry
//! (`decided_fragments()` / `retained_boundaries()`) by the agreement test
//! `refute::tests::module_ttl_projects_the_kernel_registry`; this gate therefore
//! reads that shipped manifest as the authoritative, cross-crate-visible view of the
//! kernel registry (the registry functions are `pub(crate)` to `gmeow-logic`).
//!
//! Every check here — the eight live decides and the boundary/process-ref reads — is
//! cheap and on the DEFAULT gate, so `make check` covers the capability. The
//! whole-corpus heavy re-runs stay off-gate in `full_decided_gate` /
//! `full_divergence_gate`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_conformance::paths::{cases_root, repo_root};
use gmeow_logic::reason::DlVerdict;
use purrdf::{NativeRdfFormat, RdfTerm, dataset_from_bytes};

/// The `logic:` grounding namespace whose local names key the shipped registry.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// A per-case wall-clock budget for the guarded live re-run, matching the sibling
/// full-corpus gates. A timeout surfaces as `incomplete`, which fails the "must
/// decide" assertion loudly rather than wedging the gate; none of the eight
/// representatives trip it.
const PER_CASE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// One representative case pinning a family to its DECIDED pole.
struct Representative {
    /// The case directory RELATIVE to `cases/` (`external/<corpus>/<slug>` for a
    /// W3C-sourced decided-corpus case, `<category>/<slug>` for an on-gate fixture).
    rel_path: &'static str,
    /// The committed decided token the live reasoner must reproduce.
    expected: &'static str,
    /// Whether the case is W3C-sourced (its `profile.json` carries
    /// `w3c_published_verdict` / `native_verdict`, which must also agree). On-gate
    /// fixtures carry only the `expected/verdicts.json` world status.
    w3c_sourced: bool,
}

/// One named construct family: its human label, the `logic:DecidedFragment` id it
/// maps to (its decided pole in the shipped registry), and the representative
/// case(s) that must reproduce a decided verdict on the production surface.
struct Family {
    label: &'static str,
    fragment_id: &'static str,
    reps: &'static [Representative],
}

/// The seven named families (Family 6 folds 6a + 6b), each mapped onto its decided
/// fragment id and its representative(s). Family 5 carries two representatives (a
/// W3C-sourced discrete-float case and an on-gate length-facet fixture) so both the
/// finite value-space and the facet-emptiness deciders are exercised.
const FAMILIES: &[Family] = &[
    Family {
        label: "Family 1 — complement refutation",
        fragment_id: "complement-refutation",
        reps: &[Representative {
            rel_path: "external/w3c-owl2-full-decided/webont-description-logic-001",
            expected: "inconsistent",
            w3c_sourced: true,
        }],
    },
    Family {
        label: "Family 2 — qualified / number cardinality counting",
        fragment_id: "number-cardinality-counting",
        reps: &[Representative {
            rel_path: "external/w3c-owl2-full-decided/webont-cardinality-002",
            expected: "consistent",
            w3c_sourced: true,
        }],
    },
    Family {
        label: "Family 3 — union + disjoint case-split",
        fragment_id: "union-disjoint-case-split",
        reps: &[Representative {
            rel_path: "external/w3c-owl2-full-decided/new-feature-disjointunion-001",
            expected: "consistent",
            w3c_sourced: true,
        }],
    },
    Family {
        label: "Family 4 — nominal enumeration counting",
        fragment_id: "nominal-enumeration-counting",
        reps: &[Representative {
            rel_path: "nominal-counting/oneof-differentfrom-clash",
            expected: "inconsistent",
            w3c_sourced: false,
        }],
    },
    Family {
        label: "Family 5 — datatype value-space + facet",
        fragment_id: "datatype-value-space",
        reps: &[
            Representative {
                rel_path: "external/w3c-owl2-full-decided/datatype-float-discrete-001",
                expected: "inconsistent",
                w3c_sourced: true,
            },
            Representative {
                rel_path: "datatype-value-space/length-facet-empty",
                expected: "inconsistent",
                w3c_sourced: false,
            },
        ],
    },
    Family {
        label: "Family 6a — inverse-functional identity collapse",
        fragment_id: "inverse-functional-identity-collapse",
        reps: &[Representative {
            rel_path: "external/w3c-owl2-full-decided/webont-inversefunctionalproperty-001",
            expected: "consistent",
            w3c_sourced: true,
        }],
    },
    Family {
        label: "Family 6b — malformed rdf:List",
        fragment_id: "malformed-rdf-list",
        reps: &[Representative {
            rel_path: "external/w3c-owl2-full-decided/webont-i5-5-003",
            expected: "inconsistent",
            w3c_sourced: true,
        }],
    },
    Family {
        label: "Family 7 — owl:hasSelf membership",
        fragment_id: "has-self-membership",
        reps: &[Representative {
            rel_path: "external/w3c-owl2-full-decided/footnote-not-about-self",
            expected: "inconsistent",
            w3c_sourced: true,
        }],
    },
];

/// The retained-withhold ids the kernel deliberately does NOT decide, each of which
/// must ship as a `logic:expressivenessBoundary` record with a non-empty technical
/// reason. Kept in sync with the kernel's `retained_boundaries()` by the agreement
/// test `module_ttl_projects_the_kernel_registry`; this gate independently pins that
/// each is present and process-reference-free on the shipped surface.
const RETAINED_BOUNDARY_IDS: &[&str] = &[
    "entangled-existential-cardinality",
    "non-binary-property-chain",
    "xsd-pattern-facet",
];

/// The native verdict token for one case, computed exactly as the grader/runner
/// does (a non-empty `gaps` is `incomplete`; otherwise the consistency boolean),
/// wrapped in a bounded-join worker thread so a wedged chase can never hang the gate
/// — a timeout surfaces as `incomplete`, failing the "must decide" assertion loudly.
fn native_token(input_nq: &Path) -> String {
    let path = input_nq.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::NQuads)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        let verdict = gmeow_logic::reason::dl_consistency(dataset.as_ref())
            .unwrap_or_else(|e| panic!("dl_consistency on {}: {e}", path.display()));
        let token = if !verdict.gaps.is_empty() {
            "incomplete"
        } else if verdict.consistent {
            "consistent"
        } else {
            "inconsistent"
        };
        let _ = tx.send(token.to_owned());
    });
    match rx.recv_timeout(PER_CASE_TIMEOUT) {
        Ok(token) => {
            let _ = worker.join();
            token
        }
        Err(_) => "incomplete".to_owned(),
    }
}

/// The FULL native [`DlVerdict`] for one case, computed on the exact production
/// consistency path (`gmeow_logic::reason::dl_consistency`), wrapped in a bounded-join
/// worker thread (mirroring [`native_token`]) so a wedged chase can never hang the
/// gate; `None` on timeout. None of the named-slug / boundary representatives below is
/// one of the memory-heavy chase cases, so the timeout never fires for them — a
/// `None` here is reported as an explicit failure rather than silently swallowed.
fn native_verdict(input_nq: &Path) -> Option<DlVerdict> {
    let path = input_nq.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::NQuads)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        let verdict = gmeow_logic::reason::dl_consistency(dataset.as_ref())
            .unwrap_or_else(|e| panic!("dl_consistency on {}: {e}", path.display()));
        let _ = tx.send(verdict);
    });
    match rx.recv_timeout(PER_CASE_TIMEOUT) {
        Ok(verdict) => {
            let _ = worker.join();
            Some(verdict)
        }
        Err(_) => None,
    }
}

/// The native verdict token (`consistent` / `inconsistent` / `incomplete`) read off a
/// full [`DlVerdict`], computed exactly as the grader/runner does.
fn token_of(verdict: &DlVerdict) -> &'static str {
    if !verdict.gaps.is_empty() {
        "incomplete"
    } else if verdict.consistent {
        "consistent"
    } else {
        "inconsistent"
    }
}

/// The `reason.dl-gap.<construct>` suffixes named by a verdict's honest gaps.
fn gap_codes(verdict: &DlVerdict) -> Vec<&str> {
    verdict.gaps.iter().map(|g| g.code.as_str()).collect()
}

/// Assert `verdict` is a WITHHOLD carrying at least one DECLARED, LEDGER-IDENTIFIED
/// refutation-kernel capability-gap boundary finding — the doctrine's "declared,
/// ledgered limitation, not an implicit gap." Each such finding must carry a canonical
/// `finding_iri` AND a source `anchor_iri` (routed through the `DiagLedger` via
/// `boundary_diag_ledger`), a `reason.refutation-kernel.*` code, and a non-empty
/// capability-gap reason. Returns a failure message, or `None` when the withhold is a
/// properly ledgered capability gap.
fn ledgered_capability_gap_failure(label: &str, verdict: &DlVerdict) -> Option<String> {
    if verdict.boundary_findings.is_empty() {
        return Some(format!(
            "{label}: withheld but carries NO refutation-kernel boundary finding — the withhold \
             is a bare `incomplete`, not a declared, ledger-identified capability gap"
        ));
    }
    for f in &verdict.boundary_findings {
        if f.finding_iri.is_none() || f.anchor_iri.is_none() {
            return Some(format!(
                "{label}: boundary finding {:?} lacks ledger identity (finding_iri={}, \
                 anchor_iri={}) — a capability gap must route through the DiagLedger",
                f.code,
                f.finding_iri.is_some(),
                f.anchor_iri.is_some()
            ));
        }
        if !f.code.starts_with("reason.refutation-kernel.") {
            return Some(format!(
                "{label}: boundary finding code {:?} is not a refutation-kernel capability gap",
                f.code
            ));
        }
        if f.message.trim().is_empty() {
            return Some(format!(
                "{label}: boundary finding {:?} carries an empty capability-gap reason",
                f.code
            ));
        }
    }
    None
}

/// Read and parse a case's `profile.json`.
fn read_profile(case: &Path) -> serde_json::Value {
    let path = case.join("profile.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// The single world's `status` string in a case's `expected/verdicts.json`.
fn read_expected_status(case: &Path, rel: &str) -> String {
    let path = case.join("expected").join("verdicts.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let verdicts: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    verdicts
        .as_object()
        .and_then(|o| o.values().next())
        .and_then(|w| w["status"].as_str())
        .unwrap_or_else(|| panic!("{rel}: expected/verdicts.json has no world status"))
        .to_owned()
}

/// Reproduce one representative's committed decided verdict on the production
/// surface. Returns a failure message, or `None` when the family reaches the decided
/// pole cleanly for this representative.
fn check_representative(rep: &Representative) -> Option<String> {
    let case = cases_root().join(rep.rel_path);
    if !case.join("input.nq").is_file() {
        return Some(format!(
            "{}: representative case missing (no input.nq)",
            rep.rel_path
        ));
    }

    let native = native_token(&case.join("input.nq"));
    if native == "incomplete" {
        return Some(format!(
            "{}: native returned an honest gap (incomplete) — a decided-pole \
             representative MUST decide on the production surface",
            rep.rel_path
        ));
    }
    if native != rep.expected {
        return Some(format!(
            "{}: native decided {native:?}, expected the committed decided verdict {:?}",
            rep.rel_path, rep.expected
        ));
    }

    let golden = read_expected_status(&case, rep.rel_path);
    if golden != native {
        return Some(format!(
            "{}: expected/verdicts.json world status is {golden:?}, live reasoner decided \
             {native:?}",
            rep.rel_path
        ));
    }

    if rep.w3c_sourced {
        let profile = read_profile(&case);
        let published = profile["w3c_published_verdict"]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "{}: profile.json missing w3c_published_verdict",
                    rep.rel_path
                )
            });
        if native != published {
            return Some(format!(
                "{}: native decided {native:?} but W3C published {published:?} — a \
                 W3C-sourced decided case MUST agree with W3C",
                rep.rel_path
            ));
        }
        let frozen = profile["native_verdict"]
            .as_str()
            .unwrap_or_else(|| panic!("{}: profile.json missing native_verdict", rep.rel_path));
        if native != frozen {
            return Some(format!(
                "{}: profile.json native_verdict is {frozen:?}, live reasoner decided {native:?}",
                rep.rel_path
            ));
        }
    }
    None
}

/// The shipped kernel-registry surface read from `slices/grounding/logic/module.ttl`
/// (the projection of the Rust `decided_fragments()` / `retained_boundaries()`
/// registry, proven equal by the kernel's own agreement test).
struct RegistrySurface {
    /// Local names of every `logic:DecidedFragment` individual.
    decided_ids: BTreeSet<String>,
    /// Local names of every `logic:expressivenessBoundary` record.
    boundary_ids: BTreeSet<String>,
    /// Each decided fragment id → its `logic:fragmentCompletenessBound` literal.
    completeness_bounds: BTreeMap<String, String>,
    /// Each boundary id → its `logic:fragmentBoundaryReason` literal.
    boundary_reasons: BTreeMap<String, String>,
}

fn module_ttl_path() -> PathBuf {
    repo_root()
        .join("slices")
        .join("grounding")
        .join("logic")
        .join("module.ttl")
}

/// Fold the shipped `logic:` registry surface out of `module.ttl`.
fn load_registry_surface() -> RegistrySurface {
    let decided_class = format!("{LOGIC_NS}DecidedFragment");
    let boundary_pred = format!("{LOGIC_NS}expressivenessBoundary");
    let completeness_bound = format!("{LOGIC_NS}fragmentCompletenessBound");
    let boundary_reason = format!("{LOGIC_NS}fragmentBoundaryReason");

    let path = module_ttl_path();
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::Turtle)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));

    let local = |iri: &str| iri.strip_prefix(LOGIC_NS).map(str::to_owned);

    let mut surface = RegistrySurface {
        decided_ids: BTreeSet::new(),
        boundary_ids: BTreeSet::new(),
        completeness_bounds: BTreeMap::new(),
        boundary_reasons: BTreeMap::new(),
    };

    for quad in dataset.owned_quads() {
        let RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        let Some(subj) = local(subject) else {
            continue;
        };
        match quad.predicate.as_str() {
            RDF_TYPE => {
                if let RdfTerm::Iri(o) = &quad.object
                    && *o == decided_class
                {
                    surface.decided_ids.insert(subj);
                }
            }
            p if p == boundary_pred => {
                surface.boundary_ids.insert(subj);
            }
            p if p == completeness_bound => {
                if let RdfTerm::Literal(l) = &quad.object {
                    surface
                        .completeness_bounds
                        .insert(subj, l.lexical_form.clone());
                }
            }
            p if p == boundary_reason => {
                if let RdfTerm::Literal(l) = &quad.object {
                    surface
                        .boundary_reasons
                        .insert(subj, l.lexical_form.clone());
                }
            }
            _ => {}
        }
    }
    surface
}

/// Detect a PROCESS REFERENCE in a technical characterization: a `#<digit>` issue/PR
/// number, the substring `issue`, the substring `per #`, or a bare `PR` token (all
/// case-insensitive). `PR` is matched only as a WORD-BOUNDED token so it never
/// false-fires on `property`, `expression`, `appropriate`, etc. Returns the matched
/// pattern name, or `None` when the text is a clean technical characterization.
fn process_reference(text: &str) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();

    // `#<digit>` — an issue/PR number reference.
    for i in 0..bytes.len() {
        if bytes[i] == b'#' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
            return Some("#<digit>");
        }
    }
    if lower.contains("issue") {
        return Some("issue");
    }
    if lower.contains("per #") {
        return Some("per #");
    }
    // A word-bounded `pr` token (a bare pull-request reference).
    let is_word = |c: u8| c.is_ascii_alphanumeric();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'p' && bytes[i + 1] == b'r' {
            let before_ok = i == 0 || !is_word(bytes[i - 1]);
            let after_ok = i + 2 >= bytes.len() || !is_word(bytes[i + 2]);
            if before_ok && after_ok {
                return Some("PR");
            }
        }
    }
    None
}

/// THE per-family pole gate (default gate): every named construct family reaches
/// EXACTLY ONE pole. Today all seven reach the DECIDED pole, so for each family this
/// asserts (a) its fragment id ships as a `logic:DecidedFragment` and NOT as a
/// retained boundary (exactly-one-pole), and (b) each representative reproduces its
/// committed decided verdict live on the production surface — agreeing with W3C for
/// the W3C-sourced cases. A family reaching NEITHER pole fails here; all violations
/// are collected and reported together.
#[test]
fn native_fragment_coverage_every_family_reaches_exactly_one_pole_and_decides_live() {
    let surface = load_registry_surface();
    let mut failures: Vec<String> = Vec::new();

    for family in FAMILIES {
        let decided = surface.decided_ids.contains(family.fragment_id);
        let withheld = surface.boundary_ids.contains(family.fragment_id);
        if decided && withheld {
            failures.push(format!(
                "{}: fragment id {:?} reaches BOTH poles (decided AND withheld) — must be \
                 exactly one",
                family.label, family.fragment_id
            ));
        }
        if !decided && !withheld {
            failures.push(format!(
                "{}: fragment id {:?} reaches NEITHER pole — it ships as neither a \
                 logic:DecidedFragment nor a logic:expressivenessBoundary record",
                family.label, family.fragment_id
            ));
            // No decided pole means the live-decide expectation below is moot.
            continue;
        }
        for rep in family.reps {
            if let Some(f) = check_representative(rep) {
                failures.push(format!("{}: {f}", family.label));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "native fragment per-family coverage failure(s) — a named family did not reach its \
         DECIDE pole on the production surface:\n  • {}",
        failures.join("\n  • ")
    );
}

/// The WITHHOLD-pole machinery gate (default gate): the retained boundaries are real,
/// shipped, and technically characterized — every `retained_boundaries()` id ships as
/// a `logic:expressivenessBoundary` record with a NON-EMPTY `logic:fragmentBoundaryReason`
/// — and NO shipped technical characterization (a boundary reason OR a decided
/// fragment's completeness bound) carries a PROCESS REFERENCE. This is the issue's
/// "no process references" acceptance criterion as an executable check.
#[test]
fn native_fragment_coverage_retained_boundaries_free_of_process_references() {
    let surface = load_registry_surface();
    let mut failures: Vec<String> = Vec::new();

    // (1) Every retained boundary ships with a non-empty technical reason.
    for id in RETAINED_BOUNDARY_IDS {
        if !surface.boundary_ids.contains(*id) {
            failures.push(format!(
                "retained boundary {id:?} does not ship as a logic:expressivenessBoundary record"
            ));
            continue;
        }
        match surface.boundary_reasons.get(*id) {
            Some(reason) if !reason.trim().is_empty() => {}
            _ => failures.push(format!(
                "retained boundary {id:?} has no non-empty logic:fragmentBoundaryReason"
            )),
        }
    }

    // (2) No shipped technical characterization carries a process reference. The
    // boundary reasons are the acceptance-criterion target; the completeness bounds
    // are swept too, since they are the same class of shipped technical text.
    for (id, reason) in &surface.boundary_reasons {
        if let Some(pat) = process_reference(reason) {
            failures.push(format!(
                "boundary reason for {id:?} contains a process reference ({pat:?}) — technical \
                 characterizations must be free of issue/PR references"
            ));
        }
    }
    for (id, bound) in &surface.completeness_bounds {
        if let Some(pat) = process_reference(bound) {
            failures.push(format!(
                "completeness bound for {id:?} contains a process reference ({pat:?}) — technical \
                 characterizations must be free of issue/PR references"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "retained-boundary machinery / no-process-references failure(s):\n  • {}",
        failures.join("\n  • ")
    );
}

/// Coverage completeness (default gate): the shipped `logic:DecidedFragment` set
/// covers ALL of the named families' fragment ids — the eight ids fold onto the
/// seven named families and none is missing. This pins the family→pattern coverage:
/// a family losing its decided fragment (or the manifest losing an id) fails here.
#[test]
fn native_fragment_coverage_decided_fragments_cover_every_family() {
    let surface = load_registry_surface();
    let expected_ids: BTreeSet<String> =
        FAMILIES.iter().map(|f| f.fragment_id.to_owned()).collect();

    // Sanity: the eight named-family fragment ids are distinct (Family 6 splits into
    // two, so eight ids over seven families).
    assert_eq!(
        expected_ids.len(),
        8,
        "the named families must map onto eight distinct fragment ids, got {expected_ids:?}"
    );

    let missing: Vec<&String> = expected_ids.difference(&surface.decided_ids).collect();
    assert!(
        missing.is_empty(),
        "named-family fragment id(s) missing from the shipped logic:DecidedFragment set: \
         {missing:?} (shipped: {:?})",
        surface.decided_ids
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// The issue-NAMED W3C-divergence slugs, pinned DIRECTLY (not via a family substitute
// fixture).
//
// The family-pole gate above pins each of the seven families to its DECIDED pole
// through a representative fixture. For Families 4 and 7 that representative is a
// SUBSTITUTE (`nominal-counting/oneof-differentfrom-clash`, `footnote-not-about-self`)
// that proves the certified-complete decider decides — but NOT that the harder,
// entangled cases the issue NAMES reach a pole. Those three named cases
// (`one-two`, `webont-description-logic-035`, `rolechainviolationlumen`) are exactly
// the beyond-fragment configurations W3C decided under OWL 2 Full semantics the native
// DL path does not implement, so they WITHHOLD. Nothing on-gate proved they reach
// decide-or-documented-withhold on the ACTUAL slug — a silent narrowing of the named
// acceptance targets. These tests close that: each named slug must reach EXACTLY ONE
// pole on its OWN committed `input.nq`, run through the production consistency path.
// ─────────────────────────────────────────────────────────────────────────────

/// One issue-named W3C-divergence slug and the out-of-fragment construct(s) its honest
/// `DlVerdict::gaps` must name (the `reason.dl-gap.<construct>` suffix) — the DOCUMENTED
/// reason the withhold is retained with.
struct NamedDivergenceSlug {
    slug: &'static str,
    gap_constructs: &'static [&'static str],
}

/// The three issue-named slugs, each keyed to the out-of-fragment construct(s) its
/// documented withhold must name. Every one lives under
/// `external/w3c-owl2-full-divergence` and is one of the 122 cases `full_divergence_gate`
/// independently pins as `incomplete`.
const NAMED_DIVERGENCE_SLUGS: &[NamedDivergenceSlug] = &[
    // Family 4 nominal — owl:oneOf + owl:disjointWith + owl:AllDifferent +
    // (inverse-)functional + existentials; W3C inconsistent, native incomplete (the
    // inverse-functional identity collapse is entangled with the nominal/existential
    // machinery beyond the certified fragment).
    NamedDivergenceSlug {
        slug: "one-two",
        gap_constructs: &["inverseFunctionalProperty"],
    },
    // Family 4 nominal — the spy-point universe-cardinality construction; W3C
    // inconsistent, native incomplete (the min-cardinality count is entangled with
    // someValuesFrom / inverseOf).
    NamedDivergenceSlug {
        slug: "webont-description-logic-035",
        gap_constructs: &["minCardinality"],
    },
    // Family 7 hasSelf — owl:hasSelf + a length-5 owl:propertyChainAxiom; W3C
    // consistent, native incomplete (the self-membership is entangled with a non-binary
    // property chain outside the certified fragment).
    NamedDivergenceSlug {
        slug: "rolechainviolationlumen",
        gap_constructs: &["hasSelf", "propertyChainAxiom"],
    },
];

fn divergence_corpus_root() -> PathBuf {
    cases_root()
        .join("external")
        .join("w3c-owl2-full-divergence")
}

/// THE per-NAMED-slug pole gate (default gate): each of the three issue-named
/// W3C-divergence slugs reaches EXACTLY ONE pole on its OWN committed `input.nq`, run
/// through the production consistency path — NOT via a family substitute fixture.
///
/// Pole A (decide-and-agree): the native reasoner decides `consistent` / `inconsistent`
/// AND agrees with the W3C published verdict (never a wrong decided verdict — the
/// soundness floor). A named slug that decides-and-agrees is a legitimate future
/// improvement; `full_divergence_gate`'s drift pin then couples it to a corpus
/// relocation.
///
/// Pole B (documented, ledger-identified withhold — all three today): the native
/// reasoner WITHHOLDS (`incomplete`), the frozen `profile.json` / `expected/verdicts.json`
/// document that gap, the honest `DlVerdict::gaps` NAME the out-of-fragment construct,
/// AND the withhold routes to a DECLARED, LEDGER-IDENTIFIED refutation-kernel
/// capability-gap finding (a `finding_iri` + source `anchor_iri`, per the divergence
/// doctrine's "declared, ledgered limitation, not an implicit gap").
///
/// A named slug that reaches NEITHER pole — decides but disagrees with W3C, withholds
/// without naming its construct, or withholds with a bare `incomplete` carrying no
/// ledger-identified capability gap — FAILS here. All violations are collected.
#[test]
fn native_fragment_named_divergence_slugs_reach_exactly_one_documented_pole() {
    let root = divergence_corpus_root();
    let mut failures: Vec<String> = Vec::new();

    for named in NAMED_DIVERGENCE_SLUGS {
        let case = root.join(named.slug);
        let input = case.join("input.nq");
        if !input.is_file() {
            failures.push(format!(
                "{}: named divergence slug missing its committed input.nq at {}",
                named.slug,
                input.display()
            ));
            continue;
        }
        let Some(verdict) = native_verdict(&input) else {
            failures.push(format!(
                "{}: production consistency path timed out — cannot confirm the pole",
                named.slug
            ));
            continue;
        };
        let token = token_of(&verdict);

        let profile = read_profile(&case);
        let published = profile["w3c_published_verdict"]
            .as_str()
            .unwrap_or_else(|| {
                panic!("{}: profile.json missing w3c_published_verdict", named.slug)
            });
        if published != "consistent" && published != "inconsistent" {
            failures.push(format!(
                "{}: profile.json w3c_published_verdict must be a decided W3C verdict, got {:?}",
                named.slug, published
            ));
        }

        if token == "consistent" || token == "inconsistent" {
            // Pole A — decided. It MUST agree with the W3C published verdict.
            if token != published {
                failures.push(format!(
                    "{}: native decided {:?} but W3C published {:?} — a WRONG decided verdict, \
                     unsound",
                    named.slug, token, published
                ));
            }
            continue;
        }

        // Pole B — the documented, ledger-identified withhold.
        let frozen_native = profile["native_verdict"]
            .as_str()
            .unwrap_or_else(|| panic!("{}: profile.json missing native_verdict", named.slug));
        if frozen_native != "incomplete" {
            failures.push(format!(
                "{}: native withholds but profile.json native_verdict is {:?}, expected \
                 \"incomplete\"",
                named.slug, frozen_native
            ));
        }
        let golden = read_expected_status(&case, named.slug);
        if golden != "incomplete" {
            failures.push(format!(
                "{}: native withholds but expected/verdicts.json world status is {:?}, expected \
                 \"incomplete\"",
                named.slug, golden
            ));
        }
        // The honest gaps must NAME the documented out-of-fragment construct(s).
        let codes = gap_codes(&verdict);
        for construct in named.gap_constructs {
            let want = format!("reason.dl-gap.{construct}");
            if !codes.iter().any(|c| *c == want) {
                failures.push(format!(
                    "{}: honest gaps {:?} do not name the documented out-of-fragment construct \
                     {:?}",
                    named.slug, codes, construct
                ));
            }
        }
        // The withhold must be a DECLARED, LEDGER-IDENTIFIED capability gap.
        if let Some(f) = ledgered_capability_gap_failure(named.slug, &verdict) {
            failures.push(f);
        }
    }

    assert!(
        failures.is_empty(),
        "issue-named W3C-divergence slug(s) did not reach a documented pole on the production \
         surface (silent-narrowing regression):\n  • {}",
        failures.join("\n  • ")
    );
}

/// One retained boundary bound to a LIVE representative the reasoner withholds over.
struct BoundaryRepresentative {
    /// The shipped `logic:expressivenessBoundary` local name this representative
    /// exercises.
    boundary_id: &'static str,
    /// A committed `external/w3c-owl2-full-divergence` slug, or `None` to use the
    /// embedded synthetic `xsd:pattern` EDB below (the corpus carries no pattern-facet
    /// case, so its representative is authored inline).
    slug: Option<&'static str>,
    /// A construct name the live withhold surface (the honest gap codes OR any boundary
    /// finding message) must contain, tying the withhold to THIS boundary's construct.
    must_name: &'static str,
}

/// One representative per retained boundary. Two reuse the issue-named slugs (whose
/// live withhold names their construct); the `xsd:pattern` boundary has no corpus case,
/// so it is exercised by the inline [`XSD_PATTERN_REPRESENTATIVE_NQ`] EDB.
const BOUNDARY_REPRESENTATIVES: &[BoundaryRepresentative] = &[
    BoundaryRepresentative {
        boundary_id: "non-binary-property-chain",
        slug: Some("rolechainviolationlumen"),
        must_name: "propertyChainAxiom",
    },
    BoundaryRepresentative {
        boundary_id: "entangled-existential-cardinality",
        slug: Some("webont-description-logic-035"),
        must_name: "someValuesFrom",
    },
    BoundaryRepresentative {
        boundary_id: "xsd-pattern-facet",
        slug: None,
        must_name: "pattern",
    },
];

/// A minimal EDB that lands on the `xsd-pattern-facet` retained boundary: a datatype
/// value-space obligation (`owl:someValuesFrom` a faceted `rdfs:Datatype`) whose only
/// facet is an `xsd:pattern`. Sound XSD-pattern value-space reasoning needs an XSD regex
/// evaluator the kernel does not carry, so the datatype value-space decider WITHHOLDS
/// with a structured obstruction rather than guessing — the live behavior the shipped
/// boundary record documents.
const XSD_PATTERN_REPRESENTATIVE_NQ: &str = concat!(
    "<http://ex/a> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> _:restr <http://ex/w> .\n",
    "<http://ex/dp> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
     <http://www.w3.org/2002/07/owl#DatatypeProperty> <http://ex/w> .\n",
    "_:restr <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
     <http://www.w3.org/2002/07/owl#Restriction> <http://ex/w> .\n",
    "_:restr <http://www.w3.org/2002/07/owl#onProperty> <http://ex/dp> <http://ex/w> .\n",
    "_:restr <http://www.w3.org/2002/07/owl#someValuesFrom> _:dt <http://ex/w> .\n",
    "_:dt <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
     <http://www.w3.org/2000/01/rdf-schema#Datatype> <http://ex/w> .\n",
    "_:dt <http://www.w3.org/2002/07/owl#onDatatype> \
     <http://www.w3.org/2001/XMLSchema#string> <http://ex/w> .\n",
    "_:dt <http://www.w3.org/2002/07/owl#withRestrictions> _:l1 <http://ex/w> .\n",
    "_:l1 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> _:f1 <http://ex/w> .\n",
    "_:l1 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> \
     <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> <http://ex/w> .\n",
    "_:f1 <http://www.w3.org/2001/XMLSchema#pattern> \"[a-z]+\" <http://ex/w> .\n",
);

/// The WITHHOLD-pole LIVE binding gate (default gate): every shipped
/// `logic:expressivenessBoundary` record is a boundary the LIVE reasoner actually
/// withholds over — not a free-floating hand-authored twin. For each retained boundary,
/// a representative input (a committed named slug, or the inline pattern EDB) must
/// (a) WITHHOLD (`incomplete`), (b) surface a DECLARED, LEDGER-IDENTIFIED
/// refutation-kernel capability-gap finding, and (c) name the boundary's construct in
/// its live withhold surface (gap codes or boundary messages). Together with the family
/// DECIDE-pole bindings above (`check_representative`) and the kernel-registry agreement
/// test, this makes BOTH halves of the shipped decidability surface — decided fragments
/// AND expressiveness boundaries — provably a projection of live reasoner behavior.
#[test]
fn native_fragment_retained_boundaries_bound_to_live_withhold() {
    let surface = load_registry_surface();
    let root = divergence_corpus_root();
    let mut failures: Vec<String> = Vec::new();

    for rep in BOUNDARY_REPRESENTATIVES {
        // The boundary must actually ship (defense-in-depth with the machinery gate).
        if !surface.boundary_ids.contains(rep.boundary_id) {
            failures.push(format!(
                "{}: representative maps to a boundary id that does not ship as a \
                 logic:expressivenessBoundary record",
                rep.boundary_id
            ));
            continue;
        }

        let verdict = match rep.slug {
            Some(slug) => {
                let input = root.join(slug).join("input.nq");
                if !input.is_file() {
                    failures.push(format!(
                        "{}: representative case {slug} missing input.nq",
                        rep.boundary_id
                    ));
                    continue;
                }
                match native_verdict(&input) {
                    Some(v) => v,
                    None => {
                        failures.push(format!(
                            "{}: representative case {slug} timed out",
                            rep.boundary_id
                        ));
                        continue;
                    }
                }
            }
            None => {
                let dataset = dataset_from_bytes(
                    XSD_PATTERN_REPRESENTATIVE_NQ.as_bytes(),
                    NativeRdfFormat::NQuads,
                )
                .expect("parse the inline xsd:pattern representative EDB");
                gmeow_logic::reason::dl_consistency(dataset.as_ref())
                    .expect("dl_consistency on the inline xsd:pattern representative")
            }
        };

        // (a) The representative WITHHOLDS.
        if token_of(&verdict) != "incomplete" {
            failures.push(format!(
                "{}: representative did not withhold (token {:?}) — the shipped boundary claims a \
                 withhold the live reasoner does not produce",
                rep.boundary_id,
                token_of(&verdict)
            ));
            continue;
        }
        // (b) The withhold is a DECLARED, LEDGER-IDENTIFIED capability gap.
        if let Some(f) = ledgered_capability_gap_failure(rep.boundary_id, &verdict) {
            failures.push(f);
        }
        // (c) The live withhold surface NAMES the boundary's construct.
        let named_in_gaps = gap_codes(&verdict)
            .iter()
            .any(|c| c.contains(rep.must_name));
        let named_in_boundary = verdict
            .boundary_findings
            .iter()
            .any(|f| f.message.contains(rep.must_name));
        if !named_in_gaps && !named_in_boundary {
            failures.push(format!(
                "{}: live withhold surface (gaps {:?} + boundary findings) does not name the \
                 boundary construct {:?}",
                rep.boundary_id,
                gap_codes(&verdict),
                rep.must_name
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "shipped logic:expressivenessBoundary record(s) not bound to a live reasoner \
         withhold:\n  • {}",
        failures.join("\n  • ")
    );
}

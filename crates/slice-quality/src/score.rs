// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The scoring context handed to each axis primitive, and the raw axis result.
//!
//! A primitive reads `ctx.graph` (at the breadth its `ContextScope` licenses) over
//! `ctx.terms` (the slice's own authored terms) and returns an [`AxisScore`]: a
//! normalized 0.0–1.0 score plus the advisory findings it wants surfaced. Advice
//! output is always about the one target slice, whatever the read scope.
//!
//! Every SLICE-LOCAL file the file-shaped axes read (`shapes.ttl`, `docs.md`, the
//! `i18n/*.po` catalogs, the `mappings/` correspondence surface, the `tests/`
//! counter-example fixtures, …) is served from [`ScoreContext::files`] — an
//! in-memory map, never a directory. That is what lets the whole scoring kernel run
//! on a target with no filesystem at all (the browser/wasm32 console) and lets a
//! caller score a slice that only ever existed as bytes (a bundle projection, an
//! upload, a git blob). The ONE thing that legitimately still needs a real path is
//! the surrounding CHECKOUT the repo-anchored axis arms walk up to, so that path
//! lives on [`ScoringEnv::Repo`] — the environment it is a property of — and
//! [`ScoringEnv::Bundle`] is reachable with no directory whatsoever.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use gmeow_errors::{Finding, Severity, Standpoint};
use gmeow_lang_bridge::GmnDictionary;
use purrdf::RdfDataset;

use crate::graph;

/// Where the two repo-anchored axes (`gmn1_coverage`, `DocMaturity`) source their
/// wide-scope inputs — the shared `slices/grounding/lang/` dictionary and the
/// documentation model — from.
///
/// The repo-anchored axes read more than the one slice's own `.ttl`: they need a
/// dictionary and a documentation model that normally live in the surrounding
/// checkout. This seam lets a foreign slice pulled in on its own (with no repo
/// around it) instead carry those inputs in an embedded bundle. Every in-repo
/// caller stays [`ScoringEnv::Repo`], byte-for-byte the pre-seam behaviour.
#[derive(Clone)]
pub enum ScoringEnv {
    /// Source wide-scope inputs from the surrounding repo checkout (the verbatim
    /// pre-seam behaviour): read `slices/grounding/lang/module.ttl` off disk and
    /// build the documentation model with a repo-wide `slices/` sweep.
    ///
    /// `slice_dir` is the scored slice's directory INSIDE that checkout. It is
    /// carried here rather than on the context because the surrounding checkout is
    /// a property of THIS environment alone: the repo-anchored arms walk `slice_dir`
    /// up to the first `slices/` component to find the repo root
    /// ([`crate::axes::repo_root_of`]), and no other arm may read a path. A slice
    /// scored in [`ScoringEnv::Bundle`] has no checkout and therefore no such path —
    /// making the field variant-local is what makes that state unrepresentable.
    Repo {
        /// The scored slice's directory inside the surrounding checkout.
        slice_dir: PathBuf,
    },
    /// Source wide-scope inputs from an embedded bundle. The carried dictionary is
    /// ALREADY loaded and validated (constructed with `?` at bundle-build time, so a
    /// corrupt wheel hard-fails there): the `gmn1_coverage` arm uses it directly with
    /// no tolerant advisory. `DocMaturity` ignores the payload and builds a fresh
    /// single-slice model from the slice's own carried files.
    Bundle(Arc<GmnDictionary>),
}

/// The result of asking [`ScoreContext::text`] for one slice-relative path as text.
///
/// The on-disk predecessor of the file map distinguished `io::ErrorKind::NotFound`
/// (a file the slice honestly does not ship) from any OTHER read error (a file that
/// IS there but is broken, which must surface a finding rather than score as a clean
/// absence). A map has only "key present" / "key absent", so the second arm would
/// silently vanish — except that a present key whose bytes are not valid UTF-8 is
/// exactly the map's form of "a broken input": the slice ships the file, but it
/// cannot be read as text. This three-way enum keeps those two failure modes
/// distinct at every call site instead of collapsing them into one `Option`.
pub enum FileText<'a> {
    /// The key is present and its bytes decode as UTF-8.
    Present(&'a str),
    /// The key is absent — HONEST ABSENCE, the map twin of `ErrorKind::NotFound`.
    Absent,
    /// The key is present but its bytes are not UTF-8 — a BROKEN INPUT, the map twin
    /// of "any other read error", never to be treated as absence.
    Invalid(std::str::Utf8Error),
}

/// Everything an axis primitive may read about the slice under assessment.
pub struct ScoreContext<'a> {
    /// The slice ontology IRI (`…/slices/<name>`).
    pub slice_iri: String,
    /// The slice's OWN files, keyed by slice-relative forward-slash path
    /// (`"manifest.ttl"`, `"module.ttl"`, `"shapes.ttl"`, `"docs.md"`,
    /// `"examples/foo.ttl"`, `"tests/counter-examples/bar.ttl"`, `"i18n/fr.po"`,
    /// `"mappings/equivalences.ttl"`, …).
    ///
    /// A `BTreeMap` (not a `HashMap`) because every axis that scans a subtree does so
    /// by prefix in KEY ORDER, and that order must be deterministic and must agree
    /// with the sorted-path order the on-disk predecessor produced — otherwise two
    /// runs of the same scorer could union the same Turtle documents in different
    /// orders.
    pub files: &'a BTreeMap<String, Vec<u8>>,
    /// The dataset to read, already assembled at the axis's licensed scope.
    pub graph: &'a RdfDataset,
    /// The slice's own authored term IRIs (typed subjects `rdfs:isDefinedBy`
    /// the slice), sorted — the population most per-term axes score over.
    pub terms: Vec<String>,
    /// Where the two repo-anchored axes source their wide-scope inputs.
    pub env: ScoringEnv,
}

impl<'a> ScoreContext<'a> {
    /// Build a context for `slice_iri` over the slice's own in-memory `files`,
    /// computing the slice's own term set from the graph (subjects whose
    /// `rdfs:isDefinedBy` is the slice IRI).
    #[must_use]
    pub fn new(
        slice_iri: String,
        files: &'a BTreeMap<String, Vec<u8>>,
        graph: &'a RdfDataset,
        env: ScoringEnv,
    ) -> Self {
        let terms = slice_terms(graph, &slice_iri);
        Self {
            slice_iri,
            files,
            graph,
            terms,
            env,
        }
    }

    /// Whether the slice ships `key` — the map twin of `Path::is_file`.
    #[must_use]
    pub fn has(&self, key: &str) -> bool {
        self.files.contains_key(key)
    }

    /// The raw bytes the slice ships at `key`, or `None` when it ships no such file.
    #[must_use]
    pub fn bytes(&self, key: &str) -> Option<&[u8]> {
        self.files.get(key).map(Vec::as_slice)
    }

    /// `key`'s bytes decoded as text, keeping honest absence and a broken (non-UTF-8)
    /// file distinguishable — see [`FileText`].
    #[must_use]
    pub fn text(&self, key: &str) -> FileText<'_> {
        match self.files.get(key) {
            None => FileText::Absent,
            Some(bytes) => match std::str::from_utf8(bytes) {
                Ok(text) => FileText::Present(text),
                Err(e) => FileText::Invalid(e),
            },
        }
    }

    /// Every file the slice ships directly under `prefix` (which must end in `/`)
    /// whose key ends in `suffix`, in BTreeMap key order.
    ///
    /// `recursive` selects the sweep shape the replaced on-disk walk had: `true`
    /// matches the whole subtree (a `read_dir` recursion), `false` matches only the
    /// directory's immediate children (a single-level `read_dir`). Both are exact
    /// replacements — an axis that deliberately reads `examples/*.ttl` but NOT
    /// `examples/nested/*.ttl` keeps that scope.
    #[must_use]
    pub fn keys_under(&self, prefix: &str, suffix: &str, recursive: bool) -> Vec<&str> {
        debug_assert!(prefix.ends_with('/'), "a directory prefix ends in '/'");
        self.files
            .keys()
            .map(String::as_str)
            .filter(|key| key.starts_with(prefix) && key.ends_with(suffix))
            .filter(|key| recursive || !key[prefix.len()..].contains('/'))
            .collect()
    }

    /// `keys_under` paired with each key's bytes — the shape every "parse this
    /// subtree into one dataset" call site wants, in the same deterministic key
    /// order.
    #[must_use]
    pub fn docs_under(&self, prefix: &str, suffix: &str, recursive: bool) -> Vec<(&str, &[u8])> {
        self.keys_under(prefix, suffix, recursive)
            .into_iter()
            .map(|key| (key, self.files[key].as_slice()))
            .collect()
    }
}

/// The slice's own authored terms: typed IRI subjects that declare
/// `rdfs:isDefinedBy <slice_iri>`.
///
/// Namespace is deliberately irrelevant: grounding slices own `logic:`, `lang:`,
/// and `math:` terms alongside occasional `gmeow:` terms. A graph without explicit
/// ownership yields an empty population instead of silently scoring unrelated terms.
#[must_use]
pub fn slice_terms(ds: &RdfDataset, slice_iri: &str) -> Vec<String> {
    use purrdf::{DatasetView, GraphMatch, TermRef};

    let (Some(type_p), Some(defined_by_p), Some(slice_id)) = (
        graph::id(ds, graph::RDF_TYPE),
        graph::id(ds, "http://www.w3.org/2000/01/rdf-schema#isDefinedBy"),
        graph::id(ds, slice_iri),
    ) else {
        return Vec::new();
    };

    let mut out: Vec<String> = ds
        .quads_for_pattern(None, Some(defined_by_p), Some(slice_id), GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.s) {
            TermRef::Iri(iri) if graph::has_any(ds, q.s, type_p) => Some(iri.to_owned()),
            _ => None,
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The raw result of one axis primitive: a normalized score and its advisories.
pub struct AxisScore {
    /// The normalized score in 0.0–1.0 (clamped by the caller).
    pub score: f64,
    /// The advisory findings the primitive wants surfaced (never gating).
    pub findings: Vec<Finding>,
}

impl AxisScore {
    /// A clean pass with no advice.
    #[must_use]
    pub fn clean(score: f64) -> Self {
        Self {
            score,
            findings: Vec::new(),
        }
    }
}

/// Build one advisory finding on the slice-quality tool at the given code+message.
#[must_use]
pub fn advisory(code: &str, message: impl Into<String>) -> Finding {
    Finding::new(Severity::Warning, code, message)
        .with_tool("slice-quality")
        .with_standpoint(Standpoint::Advisory)
}

/// An [`advisory`] that additionally carries, STRUCTURALLY, the documented term it
/// concerns.
///
/// The term IRI is already in the message prose, but prose is not a join key: the
/// gate mints these advisories onto the diagnostics ledger and needs the ANCHOR term
/// as data, not as something to be re-parsed out of an English sentence. Populating
/// [`Finding::documented_terms`] is exactly that carrier (and it rides only the
/// full-fidelity JSON `Report`, so no SARIF/RDF/HTML bytes move).
#[must_use]
pub fn advisory_about(code: &str, term_iri: &str, message: impl Into<String>) -> Finding {
    let mut finding = advisory(code, message);
    finding.documented_terms.push(term_iri.to_owned());
    finding
}

/// The structural provenance an advisory carries for the diagnostics LEDGER.
///
/// The ledger content-addresses a witness on `(code, category, source position,
/// focus)` and deliberately NEVER on the message. Two advisories that differ only in
/// their prose — `fr does not cover X` and `cmn does not cover X` — therefore merge
/// into ONE node unless they carry different structural positions. These two builders
/// are how an axis says what actually distinguishes its findings.
pub trait AdvisoryProvenance {
    /// Set the advisory's own POSITION: the stable key that distinguishes it from
    /// every sibling advisory sharing its code and anchor (e.g. the target language,
    /// or `<term>|<predicate>#<lang>`). Carried in [`Finding::locations`] — where the
    /// finding IS — so the ledger fingerprint separates siblings instead of
    /// collapsing them.
    #[must_use]
    fn with_position(self, key: impl Into<String>) -> Self;

    /// Attach a WITNESS identifier — the concrete artifact the finding derives FROM
    /// (for a translation advisory, the catalog entry's `msgctxt`).
    ///
    /// Carried as a bare [`gmeow_errors::Location`] in
    /// [`Finding::related_locations`] (that field's documented purpose: a provenance
    /// edge with no message of its own). The gate interns each witness as its own
    /// ledger node and hangs the advisory's ANTECEDENT edge on it, so a reasoner pass
    /// over the finding graph walks from "this axis is below its floor" down to the
    /// exact catalog entry.
    #[must_use]
    fn with_witness(self, witness: impl Into<String>) -> Self;
}

impl AdvisoryProvenance for Finding {
    fn with_position(mut self, key: impl Into<String>) -> Self {
        self.locations.push(gmeow_errors::Location {
            logical: Some(key.into()),
            ..gmeow_errors::Location::default()
        });
        self
    }

    fn with_witness(mut self, witness: impl Into<String>) -> Self {
        self.related_locations.push(gmeow_errors::Location {
            logical: Some(witness.into()),
            ..gmeow_errors::Location::default()
        });
        self
    }
}

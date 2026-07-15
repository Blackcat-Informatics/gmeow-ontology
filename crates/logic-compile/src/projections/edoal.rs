// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The EDOAL correspondence lowering: the get leg + relation lattice + measure → one
//! `align:Alignment` (EDOAL level-2) per profile.
//!
//! EDOAL is an under-approximation of the correspondence: it drops the SOL caveats,
//! the put leg, and world/standpoint scope, so the ledger-row preservation is
//! `SoundUnder`. It renders from the SAME [`crate::projections::get_leg`] model
//! the SPARQL lowering uses.
//! The triples are built as N-Triples with a deterministic first-seen blank order and
//! rendered through the **wasm-clean** canonical-Turtle serializer
//! ([`purrdf::turtle_render`]) — no oxigraph: the serializer's object ordering is a
//! pure function of subtree content (every EDOAL blank inlines), so building the graph
//! and rendering reproduces the committed bytes byte-for-byte.

use std::collections::BTreeMap;

use purrdf::{NativeRdfFormat, parse_dataset};

use gmeow_errors::Diag;

use crate::ingest::DslView;
use crate::ingest::prefixes::registry_pairs;
use crate::loss_ledger::LossLedger;
use crate::projections::correspondence_frontend::CorrespondenceLookup;
use crate::projections::correspondence_gate::assert_relation_no_overclaim;
use crate::projections::get_leg::{
    Atom, MappingPattern, PROFILES, ProfileBinding, ProjectionCell, curie, local, projections,
};
use crate::projections::{ProjectionResult, correspondence_result};

const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";
const DCTERMS_IS_PART_OF: &str = "http://purl.org/dc/terms/isPartOf";

const ALIGN: &str = "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#";
const EDOAL: &str = "http://ns.inria.org/edoal/1.0/#";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

/// The OWL characters GMEOW declares on its own terms; the EDOAL entity kind of a
/// correspondence target is DERIVED from the source term's character here, never guessed.
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
/// The six OWL 2 object-property subtypes (RL/DL vocabulary axioms that only ever apply
/// to object properties): a term typed with ONE of these but no explicit
/// `owl:ObjectProperty` co-assertion is still, by OWL 2 semantics, an object property —
/// so it carries the same `relation` EDOAL character as an explicit `owl:ObjectProperty`.
const OWL_SYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
const OWL_TRANSITIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const OWL_INVERSE_FUNCTIONAL_PROPERTY: &str =
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty";
const OWL_REFLEXIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ReflexiveProperty";
const OWL_ASYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AsymmetricProperty";
const OWL_IRREFLEXIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#IrreflexiveProperty";
/// The object-property subtype markers, for the `is_object` membership test in
/// [`gmeow_entity_kind`].
const OWL_OBJECT_PROPERTY_SUBTYPES: &[&str] = &[
    OWL_SYMMETRIC_PROPERTY,
    OWL_TRANSITIVE_PROPERTY,
    OWL_INVERSE_FUNCTIONAL_PROPERTY,
    OWL_REFLEXIVE_PROPERTY,
    OWL_ASYMMETRIC_PROPERTY,
    OWL_IRREFLEXIVE_PROPERTY,
];
/// An annotation property carries no object/datatype OWL character, so its EDOAL kind is
/// read from its declared `rdfs:range` (a datatype/literal range → `property`).
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_LITERAL: &str = "http://www.w3.org/2000/01/rdf-schema#Literal";
const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema#";

/// The internal carrier tag (the `x-gmeow-*` private-use tag) rides
/// `lang:carrierTag` on the three carrier varieties since the lang: graft.
const LANG_CARRIER_TAG: &str = "https://blackcatinformatics.ca/lang/carrierTag";
/// A carrier variety names its parent sign system through `lang:varietyOf`.
const LANG_VARIETY_OF: &str = "https://blackcatinformatics.ca/lang/varietyOf";
/// A sign system's ISO 639 primary subtag is its `skos:notation` — the source of
/// the carrier's derived BCP-47 tag (the same bare tag the `bcp47` projection folds).
const SKOS_NOTATION: &str = "http://www.w3.org/2004/02/skos/core#notation";

const GENERATED_BANNER: &str =
    "GENERATED by `gmeow regenerate` (mappings) from canonical mapping sources — DO NOT EDIT.";
const EN_TAG: &str = "x-gmeow-english";

/// The artifacts + per-correspondence loss ledger of the EDOAL lowering.
pub struct EdoalLowering {
    /// `<profile>.edoal.ttl` → Turtle.
    pub alignments: BTreeMap<String, String>,
    /// One [`ProjectionResult`] per correspondence (cell::profile binding) that drops
    /// something — the per-correspondence preservation rows for the loss ledger.
    pub ledger: Vec<ProjectionResult>,
    /// The per-correspondence loss store this lowering interned every drop into (keyed by
    /// target focus). The mappings stage unions it into the single report loss store so the
    /// EDOAL rows' `gmeow:lossyDrop` records read back from the SAME substrate ledger.
    pub loss: LossLedger,
}

/// Lower every profile's EDOAL alignment, keyed `<profile>.edoal.ttl`, plus the
/// per-correspondence loss ledger. `dsl` is the merged mapping-DSL view; `onto` the
/// merged ontology view (the language-tag map).
pub fn lower_edoal(
    dsl: &DslView,
    onto: &DslView,
    lookup: &CorrespondenceLookup,
) -> gmeow_errors::Result<EdoalLowering> {
    let cells = projections(dsl)?;
    let tag_map = build_tag_map(onto);
    let prefixes = registry_pairs();
    let mut alignments = BTreeMap::new();
    let mut ledger: Vec<ProjectionResult> = Vec::new();
    let mut loss = LossLedger::new();
    for profile in PROFILES {
        let emitted = emit_edoal_nt(&cells, profile, onto, &tag_map, lookup, &mut loss)?;
        // Parse the freshly-built N-Triples into a wasm-clean dataset and render it
        // through the canonical-Turtle serializer (object order is content-derived, so
        // no oxigraph re-dump is needed to fix the blank order).
        let dataset = parse_dataset(
            emitted.nt.as_bytes(),
            NativeRdfFormat::NTriples.media_type(),
            None,
        )
        .map_err(|e| {
            Diag::of_kind(crate::error::Edoal {
                detail: format!("EDOAL NT parse error: {e}"),
            })
        })?;
        let body = purrdf::turtle_render::render(&dataset, &prefixes);
        let text = format!("{}\n", body.trim_end_matches('\n'));
        alignments.insert(format!("{profile}.edoal.ttl"), text);
        ledger.extend(emitted.ledger);
    }
    Ok(EdoalLowering {
        alignments,
        ledger,
        loss,
    })
}

// ── Language-tag retag map ───────────────────────────────────────────────────────

fn build_tag_map(onto: &DslView) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    // The internal `x-gmeow-*` tag rides `lang:carrierTag` on a carrier variety; the
    // public BCP-47 tag is DERIVED over the model (never authored per language) — the
    // same bare tag the `bcp47` projection folds: the carrier's `lang:varietyOf` parent
    // sign system carries the ISO 639 primary subtag as `skos:notation`, and the carrier
    // orthography's script equals the parent default, so the script subtag is suppressed.
    for (subject, object) in onto.quads_with_predicate(LANG_CARRIER_TAG) {
        let (Some(internal), Some(subj_iri)) = (object.as_literal(), subject.as_iri()) else {
            continue;
        };
        let Some(parent) = onto.object_iri(subj_iri, LANG_VARIETY_OF) else {
            continue;
        };
        if let Some(ext) = onto.object_literal(&parent, SKOS_NOTATION) {
            map.insert(internal.to_owned(), ext.trim().to_ascii_lowercase());
        }
    }
    map
}

fn retag(tag: &str, tag_map: &BTreeMap<String, String>) -> String {
    tag_map.get(tag).cloned().unwrap_or_else(|| tag.to_owned())
}

// ── N-Triples builder ────────────────────────────────────────────────────────────

struct Nt {
    lines: String,
    counter: usize,
}

impl Nt {
    fn new() -> Self {
        Self {
            lines: String::new(),
            counter: 0,
        }
    }

    fn fresh_bnode(&mut self) -> String {
        let label = format!("_:b{}", self.counter);
        self.counter += 1;
        label
    }

    /// A deterministic, **injective** blank-node id for `label`. ASCII alphanumerics
    /// pass through verbatim; every other byte is hex-escaped as `_XX` (and a literal
    /// `_` is itself escaped, so the escape is unambiguous). This avoids the old
    /// many-to-one `non-alnum → '_'` collapse, under which `cell-a_b-0` and
    /// `cell-a-b-0` mapped to the same node and silently merged two cells.
    fn stable_bnode(label: &str) -> String {
        let mut safe = String::with_capacity(label.len());
        for &byte in label.as_bytes() {
            if byte.is_ascii_alphanumeric() {
                safe.push(byte as char);
            } else {
                safe.push_str(&format!("_{byte:02x}"));
            }
        }
        format!("_:n{safe}")
    }

    fn add_iri(&mut self, s: &str, p: &str, o: &str) {
        self.add_raw(s, p, &format!("<{o}>"));
    }

    fn add_bnode_obj(&mut self, s: &str, p: &str, o: &str) {
        self.add_raw(s, p, o);
    }

    fn add_node(&mut self, s: &str, p: &str, o: &str) {
        if o.starts_with("_:") {
            self.add_raw(s, p, o);
        } else {
            self.add_iri(s, p, o);
        }
    }

    fn add_raw(&mut self, subject: &str, pred: &str, object: &str) {
        let subj = if subject.starts_with("_:") {
            subject.to_owned()
        } else {
            format!("<{subject}>")
        };
        self.lines
            .push_str(&format!("{subj} <{pred}> {object} .\n"));
    }

    fn add_lang_literal(&mut self, s: &str, p: &str, text: &str, lang: &str) {
        let obj = format!("{}@{lang}", nt_quote(text));
        self.add_raw(s, p, &obj);
    }

    fn add_string_literal(&mut self, s: &str, p: &str, text: &str) {
        self.add_raw(s, p, &nt_quote(text));
    }

    fn add_typed_literal(&mut self, s: &str, p: &str, text: &str, datatype: &str) {
        let obj = format!("{}^^<{datatype}>", nt_quote(text));
        self.add_raw(s, p, &obj);
    }

    fn attach_list(&mut self, subject: &str, predicate: &str, items: &[String]) {
        let head = self.fresh_bnode();
        self.add_bnode_obj(subject, predicate, &head);
        let mut cur = head;
        for (i, item) in items.iter().enumerate() {
            self.add_node(&cur, RDF_FIRST, item);
            if i + 1 < items.len() {
                let next = self.fresh_bnode();
                self.add_bnode_obj(&cur, RDF_REST, &next);
                cur = next;
            } else {
                self.add_iri(&cur, RDF_REST, RDF_NIL);
            }
        }
    }
}

fn nt_quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ── EDOAL emission ────────────────────────────────────────────────────────────

/// The N-Triples body of one profile's EDOAL alignment plus its per-correspondence
/// loss ledger.
#[derive(Debug)]
struct EmittedEdoal {
    nt: String,
    ledger: Vec<ProjectionResult>,
}

fn emit_edoal_nt(
    cells: &[ProjectionCell],
    profile: &str,
    onto: &DslView,
    tag_map: &BTreeMap<String, String>,
    lookup: &CorrespondenceLookup,
    loss: &mut LossLedger,
) -> gmeow_errors::Result<EmittedEdoal> {
    let mut nt = Nt::new();
    let mut ledger: Vec<ProjectionResult> = Vec::new();
    let align = format!("{ONTOLOGY_IRI}/projections/{profile}");
    let en = retag(EN_TAG, tag_map);

    nt.add_iri(&align, RDF_TYPE, &format!("{ALIGN}Alignment"));
    nt.add_lang_literal(
        &align,
        RDFS_LABEL,
        &format!("GMEOW → {profile} (EDOAL)"),
        &en,
    );
    nt.add_iri(&align, DCTERMS_IS_PART_OF, ONTOLOGY_IRI);
    nt.add_lang_literal(&align, RDFS_COMMENT, GENERATED_BANNER, &en);
    nt.add_string_literal(&align, &format!("{ALIGN}level"), "2EDOAL");
    for (n, name) in [
        (format!("{ALIGN}onto1"), "GMEOW"),
        (format!("{ALIGN}onto2"), profile),
    ] {
        let onto = nt.fresh_bnode();
        nt.add_iri(&onto, RDF_TYPE, &format!("{ALIGN}Ontology"));
        let formalism = nt.fresh_bnode();
        nt.add_string_literal(&formalism, &format!("{ALIGN}name"), name);
        nt.add_bnode_obj(&onto, &format!("{ALIGN}formalism"), &formalism);
        nt.add_bnode_obj(&align, &n, &onto);
    }

    for cell in cells {
        for b in &cell.bindings {
            if b.profile != profile {
                continue;
            }
            // Overclaim gate (Constitution Principle 5): the EDOAL `align:relation`
            // token `b.relation` is emitted verbatim in `make_cell`. A bridge / caveated
            // relation emitting the equivalence token `=` is a build failure. (Corpus
            // relations are `=`/`<=`; the `=` cells are genuine Equiv, so the gate
            // passes for the committed corpus.) The typed `(relation, class, kind)` is
            // CONSUMED from the materialized correspondence keyed by this cell's natural
            // identity `(cell IRI, profile)` — the single source of truth — not re-derived
            // inline. A miss is a HARD FAIL: every authored binding is transpiled.
            let typed = lookup.binding(&cell.iri, &b.profile)?;
            assert_relation_no_overclaim(
                "edoal",
                typed.relation,
                typed.morphism_class,
                typed.morphism_kind,
                &b.relation,
            )
            .map_err(|e| Diag::of_kind(crate::error::Edoal { detail: e.0 }))?;

            for map_cell in edoal_cells(&mut nt, onto, cell, b, &en)? {
                nt.add_bnode_obj(&align, &format!("{ALIGN}map"), &map_cell);
            }

            // One preservation row per correspondence (cell::profile) that drops
            // something: EDOAL drops the SOL caveats, the put leg, and world/standpoint
            // scope (the dialect structural drops), plus any authored profile losses —
            // all attributed to the get leg.
            let mut residue: Vec<String> = Vec::new();
            for d in &b.lossy_drops {
                residue.push(format!("get-leg profile loss: {d}"));
            }
            residue.push("get-leg: the put leg is not carried by EDOAL".to_owned());
            residue.push("get-leg: world/standpoint scope is not carried by EDOAL".to_owned());
            let key = format!("{}::{}", local(&cell.iri), b.profile);
            ledger.push(correspondence_result(loss, "edoal", &key, residue, None));
        }
    }
    Ok(EmittedEdoal {
        nt: nt.lines,
        ledger,
    })
}

/// Map a canonical kind token to its EDOAL type local name. An unrecognized token is a
/// HARD FAIL (never the old silent `_ => "Property"` collapse) — Constitution
/// no-optionality: a mistyped `gmeow:edoal*Kind` must stop the build, not mistype a cell.
fn edoal_kind(kind: &str) -> gmeow_errors::Result<&'static str> {
    match kind {
        "class" => Ok("Class"),
        "relation" => Ok("Relation"),
        "property" => Ok("Property"),
        other => Err(Diag::of_kind(crate::error::Edoal {
            detail: format!(
                "unknown EDOAL entity kind {other:?} (expected class/relation/property)"
            ),
        })),
    }
}

/// Validate an authored `gmeow:edoal*Kind` override to a canonical kind token.
fn valid_kind(kind: &str) -> gmeow_errors::Result<&'static str> {
    match kind {
        "class" => Ok("class"),
        "relation" => Ok("relation"),
        "property" => Ok("property"),
        other => Err(Diag::of_kind(crate::error::Edoal {
            detail: format!(
                "unknown gmeow:edoal*Kind override {other:?} (expected class/relation/property)"
            ),
        })),
    }
}

/// The EDOAL entity kind of a GMEOW term, DERIVED from its OWL character in the ontology
/// view: an object property is a `relation`, a datatype property a `property`, a class a
/// `class`. `None` when the term carries none of those (or an ambiguous mix) — the caller
/// then requires an explicit override or hard-fails.
fn gmeow_entity_kind(onto: &DslView, iri: &str) -> Option<&'static str> {
    let types = onto.object_iris(iri, RDF_TYPE);
    // A term typed ONLY with an OWL 2 object-property subtype (Symmetric/Transitive/
    // InverseFunctional/Reflexive/Asymmetric/Irreflexive), without a co-asserted
    // `owl:ObjectProperty`, is still an object property by OWL 2 semantics.
    let is_object = types
        .iter()
        .any(|t| t == OWL_OBJECT_PROPERTY || OWL_OBJECT_PROPERTY_SUBTYPES.contains(&t.as_str()));
    let is_data = types.iter().any(|t| t == OWL_DATATYPE_PROPERTY);
    let is_class = types.iter().any(|t| t == OWL_CLASS);
    if is_object && !is_data && !is_class {
        return Some("relation");
    }
    if is_data && !is_object && !is_class {
        return Some("property");
    }
    if is_class && !is_object && !is_data {
        return Some("class");
    }
    // An annotation property has no object/datatype character; derive from its range —
    // a datatype/literal range is a `property`, an IRI/class range a `relation`.
    if types.iter().any(|t| t == OWL_ANNOTATION_PROPERTY) {
        return range_entity_kind(onto, iri);
    }
    None
}

/// The EDOAL kind implied by a term's `rdfs:range`: all datatype/literal ranges → a
/// `property`, any resource/class range → a `relation`. `None` when no range is declared.
fn range_entity_kind(onto: &DslView, iri: &str) -> Option<&'static str> {
    let ranges = onto.object_iris(iri, RDFS_RANGE);
    if ranges.is_empty() {
        return None;
    }
    // A datatype/literal range also includes the RDF namespace's own datatypes
    // (`rdf:langString`, `rdf:HTML`, `rdf:PlainLiteral`, …) — an annotation property
    // ranged on one of those is a literal-valued `property`, not an object `relation`.
    let is_datatype = |r: &String| {
        r.starts_with(XSD_NS) || r == RDFS_LITERAL || r.starts_with(crate::projections::RDF_NS)
    };
    if ranges.iter().all(is_datatype) {
        Some("property")
    } else {
        Some("relation")
    }
}

/// Resolve the EDOAL entity kind of one side of a correspondence: an authored override
/// wins, else derive from `term`'s GMEOW OWL character, else HARD FAIL naming the cell.
fn resolve_entity_kind(
    onto: &DslView,
    term: Option<&str>,
    authored: Option<&str>,
    cell_iri: &str,
    side: &str,
) -> gmeow_errors::Result<&'static str> {
    if let Some(a) = authored {
        return valid_kind(a);
    }
    if let Some(t) = term
        && let Some(k) = gmeow_entity_kind(onto, t)
    {
        return Ok(k);
    }
    Err(Diag::of_kind(crate::error::Edoal {
        detail: format!(
            "{cell_iri}: EDOAL {side} entity kind indeterminate — GMEOW term {t} carries no \
             owl:ObjectProperty/DatatypeProperty/Class type and no gmeow:edoal*Kind override was \
             authored",
            t = term.unwrap_or("(none)"),
        ),
    }))
}

/// The `gmeow:opIri` expression operator: the sole GMEOW construct that mints a fresh
/// IRI. A `gmeow:mint`/`gmeow:bind` whose expression is NOT this operator (nor a bare
/// constant IRI) produces a literal, not an individual — e.g. `gedcom.ttl`'s
/// `gmeow:mint [ gmeow:bindVar "gsexmVal" ; gmeow:bindExpr "M" ]` mints a bare status
/// LETTER, not an IRI.
const GM_OP_IRI: &str = "https://blackcatinformatics.ca/gmeow/opIri";

/// Whether a `gmeow:bind`/`gmeow:mint` expression manifestly produces an IRI (an
/// individual), as opposed to a literal.
fn expr_mints_iri(expr: &crate::projections::get_leg::Expr) -> bool {
    use crate::projections::get_leg::Expr;
    matches!(expr, Expr::ConstIri(_)) || matches!(expr, Expr::Op { op, .. } if op == GM_OP_IRI)
}

/// Derive entity2's (the EDOAL target's) kind from the correspondence's own TEMPLATE —
/// never from the GMEOW source predicate's OWL character, and never from a guess at the
/// external vocabulary's semantics. EDOAL is a lossy projection OF the `logic:`/GMEOW
/// core (Principle 17): the target predicate's kind is fully determined by GMEOW's own
/// `gmeow:templateAtoms`, the correspondence's authored construction of the target
/// triples (never the SOURCE predicate's character, which is what the historical bug
/// conflated: `gmeow:startedAtTime` is a `owl:DatatypeProperty`, but the TEMPLATE shows
/// `time:hasBeginning` points at a minted `time:Instant` individual, not a literal).
/// `None` when the binding names no `to_predicate`, no template atom names that
/// predicate, or the atom's object position is genuinely indeterminate from the template
/// (and the GMEOW source pattern that feeds it) — the caller then requires an explicit
/// `gmeow:edoalTargetKind` override or hard-fails (never silently guesses).
///
/// Classification of the matched atom's object position:
/// - a fixed literal (`gmeow:objectLiteral`) → `property` (a datatype edge).
/// - a fixed IRI value (`gmeow:tObjValue`) → `relation` (an object edge to a known term).
/// - a variable (`gmeow:tObj`):
///   - bound by a `gmeow:mint`/`gmeow:bind` → the bind expression decides: an IRI-minting
///     expression ([`expr_mints_iri`]) → `relation`; any other (e.g. a literal status
///     code) → `property`.
///   - used as the SUBJECT of some other atom — in the template (e.g. a `rdf:type` atom
///     declaring its class) OR in the GMEOW source pattern — → `relation`. Only an
///     individual can ever be an RDF subject, so a variable the model predicates further
///     facts about is structurally an individual, proven by the model's own shape.
///   - otherwise a pure leaf: traced to the GMEOW source atom that binds it, and
///     classified by THAT edge's OWL character in the GMEOW ontology
///     ([`gmeow_entity_kind`]) — an object property produces an individual (`relation`),
///     a datatype/annotation property a literal (`property`). This reads GMEOW's own
///     declared character of the SOURCE edge that fills the variable, never the
///     EXTERNAL target predicate's assumed semantics.
///
/// `pub(crate)` so [`crate::projections::correspondence_soundness`]'s entity2 coherence
/// check can re-run this SAME derivation against the committed EDOAL bytes instead of
/// duplicating it (or, worse, validating entity2 against the external target vocabulary —
/// EDOAL is DERIVED FROM GMEOW's own templates, never validated against the target).
pub(crate) fn template_target_kind(
    onto: &DslView,
    binding: &ProfileBinding,
    pattern: &MappingPattern,
) -> Option<&'static str> {
    let to_pred = binding.to_predicate.as_deref()?;
    let atom = binding
        .template_atoms
        .iter()
        .find(|a| a.predicate.as_deref() == Some(to_pred))?;
    if atom.object_literal.is_some() {
        return Some("property");
    }
    if atom.object_value.is_some() {
        return Some("relation");
    }
    let var = atom.object_var.as_deref()?;

    if let Some(bind) = pattern.mints.iter().find(|m| m.var == var) {
        return Some(if expr_mints_iri(&bind.expr) {
            "relation"
        } else {
            "property"
        });
    }

    let is_subject_somewhere = binding.template_atoms.iter().any(|a| a.subject_var == var)
        || pattern.flat_atoms().iter().any(|a| a.subject_var == var);
    if is_subject_somewhere {
        return Some("relation");
    }

    let source_pred = pattern
        .flat_atoms()
        .into_iter()
        .find(|a| a.object_var.as_deref() == Some(var))
        .and_then(|a| a.predicate)?;
    match gmeow_entity_kind(onto, &source_pred) {
        Some("property") => Some("property"),
        Some(_) => Some("relation"),
        None => None,
    }
}

fn edoal_entity(nt: &mut Nt, term: &str, kind: &str) -> gmeow_errors::Result<String> {
    let node = nt.fresh_bnode();
    nt.add_iri(&node, RDF_TYPE, &format!("{EDOAL}{}", edoal_kind(kind)?));
    nt.add_iri(&node, &format!("{EDOAL}uri"), term);
    Ok(node)
}

fn edoal_restriction(
    nt: &mut Nt,
    source: &str,
    attr: &str,
    value: &str,
) -> gmeow_errors::Result<String> {
    let cls = nt.fresh_bnode();
    nt.add_iri(&cls, RDF_TYPE, &format!("{EDOAL}Class"));
    let restriction = nt.fresh_bnode();
    nt.add_iri(
        &restriction,
        RDF_TYPE,
        &format!("{EDOAL}AttributeValueRestriction"),
    );
    let on_attr = nt.fresh_bnode();
    nt.add_iri(&on_attr, RDF_TYPE, &format!("{EDOAL}Relation"));
    nt.add_iri(&on_attr, &format!("{EDOAL}uri"), attr);
    nt.add_bnode_obj(&restriction, &format!("{EDOAL}onAttribute"), &on_attr);
    nt.add_iri(
        &restriction,
        &format!("{EDOAL}comparator"),
        &format!("{EDOAL}equals"),
    );
    let val = nt.fresh_bnode();
    nt.add_iri(&val, &format!("{EDOAL}uri"), value);
    nt.add_bnode_obj(&restriction, &format!("{EDOAL}value"), &val);
    let base = edoal_entity(nt, source, "class")?;
    nt.attach_list(&cls, &format!("{EDOAL}and"), &[base, restriction]);
    Ok(cls)
}

fn edoal_cells(
    nt: &mut Nt,
    onto: &DslView,
    cell: &ProjectionCell,
    b: &ProfileBinding,
    en: &str,
) -> gmeow_errors::Result<Vec<String>> {
    let pattern = &cell.pattern;
    let mut cells = Vec::new();

    if !b.value_class_map.is_empty() {
        let Some(edoal_source) = &pattern.edoal_source else {
            return Err(Diag::of_kind(crate::error::Edoal {
                detail: format!("{}: value-class map needs edoalSource", cell.iri),
            }));
        };
        let attr = attr_of(pattern)?;
        for (i, vc) in b.value_class_map.iter().enumerate() {
            let entity1 = edoal_restriction(nt, edoal_source, &attr, &vc.when_value)?;
            let entity2 = edoal_entity(nt, &vc.to_class, "class")?;
            let label = format!(
                "{} [{}] → {}",
                curie(edoal_source),
                curie(&vc.when_value),
                curie(&vc.to_class)
            );
            let key = format!("{i}-{}", local(&vc.to_class));
            cells.push(make_cell(nt, cell, b, entity1, entity2, &label, &key, en));
        }
        return Ok(cells);
    }

    let Some((target, sort)) = edoal_target(b) else {
        return Ok(cells);
    };

    // entity1 (the GMEOW source) and, for a property target, the value-producing
    // predicate whose OWL character DERIVES the cell's entity kind. A path source is a
    // structural `edoal:Relation` compose; its terminal predicate carries the kind.
    let (source, value_pred): (String, Option<String>) = if pattern.edoal_path {
        match edoal_path(nt, pattern)? {
            Some((node, terminal)) => (node, terminal),
            None => {
                return Err(Diag::of_kind(crate::error::Edoal {
                    detail: format!("{}: edoalPath set but no anchor→value path", cell.iri),
                }));
            }
        }
    } else if let Some(es) = &pattern.edoal_source {
        let src_kind = resolve_entity_kind(
            onto,
            Some(es),
            pattern.edoal_source_kind.as_deref(),
            &cell.iri,
            "source (entity1)",
        )?;
        (edoal_entity(nt, es, src_kind)?, Some(es.clone()))
    } else {
        return Ok(cells);
    };

    // entity2 (the external target): a class target is unambiguously `edoal:Class`. A
    // predicate target whose correspondence is authored with a TEMPLATE
    // (`b.template_atoms` non-empty — a multi-triple target shape, e.g. the owl-time
    // minted-Instant idiom) derives its kind from that template (or an explicit
    // override) — NEVER from the GMEOW source predicate's OWL character, which
    // characterizes the SOURCE, not the target (the historical bug: `time:hasBeginning`
    // mistyped as `edoal:Property` from `gmeow:startedAtTime`'s DatatypeProperty
    // character, when the template shows it is an object edge to a minted `time:Instant`
    // individual). A direct 1:1 predicate target (no template) still derives from the
    // GMEOW source's OWL character, since there is no template to consult.
    let target_kind: &'static str = match sort {
        TargetSort::Class => match b.edoal_target_kind.as_deref() {
            Some(k) => valid_kind(k)?,
            None => "class",
        },
        TargetSort::Predicate if !b.template_atoms.is_empty() => {
            match b.edoal_target_kind.as_deref() {
                Some(k) => valid_kind(k)?,
                None => template_target_kind(onto, b, pattern).ok_or_else(|| {
                    Diag::of_kind(crate::error::Edoal {
                        detail: format!(
                            "{}: EDOAL target entity kind indeterminate from correspondence \
                         template and no gmeow:edoalTargetKind override",
                            cell.iri
                        ),
                    })
                })?,
            }
        }
        TargetSort::Predicate => resolve_entity_kind(
            onto,
            value_pred.as_deref(),
            b.edoal_target_kind.as_deref(),
            &cell.iri,
            "target (entity2)",
        )?,
    };
    let target_entity = edoal_entity(nt, &target, target_kind)?;
    let label = if cell.label.is_empty() {
        format!("→ {}", curie(&target))
    } else {
        cell.label.clone()
    };
    cells.push(make_cell(
        nt,
        cell,
        b,
        source,
        target_entity,
        &label,
        "0",
        en,
    ));
    Ok(cells)
}

#[allow(clippy::too_many_arguments)]
fn make_cell(
    nt: &mut Nt,
    cell: &ProjectionCell,
    b: &ProfileBinding,
    entity1: String,
    entity2: String,
    label: &str,
    key: &str,
    en: &str,
) -> String {
    let node = Nt::stable_bnode(&format!("cell-{}-{}-{}", b.profile, local(&cell.iri), key));
    nt.add_iri(&node, RDF_TYPE, &format!("{ALIGN}Cell"));
    nt.add_lang_literal(&node, RDFS_LABEL, label, en);
    nt.add_bnode_obj(&node, &format!("{ALIGN}entity1"), &entity1);
    nt.add_bnode_obj(&node, &format!("{ALIGN}entity2"), &entity2);
    nt.add_string_literal(&node, &format!("{ALIGN}relation"), &b.relation);
    if let Some(transform) = &b.transform {
        let trans = nt.fresh_bnode();
        nt.add_iri(&trans, RDFS_SEE_ALSO, transform);
        nt.add_bnode_obj(&node, &format!("{EDOAL}transformation"), &trans);
    }
    if let Some(conf) = b.confidence {
        nt.add_typed_literal(
            &node,
            &format!("{EDOAL}measure"),
            &format_double(conf),
            XSD_DOUBLE,
        );
    }
    node
}

fn attr_of(pattern: &MappingPattern) -> gmeow_errors::Result<String> {
    for atom in pattern.flat_atoms() {
        if atom.object_var.as_deref() == pattern.value.as_deref()
            && pattern.value.is_some()
            && atom.predicate.is_some()
        {
            return Ok(atom.predicate.clone().unwrap());
        }
    }
    Err(Diag::of_kind(crate::error::Edoal {
        detail: "value-class pattern has no value-binding predicate".to_owned(),
    }))
}

/// Whether an EDOAL target is a class (unambiguously `edoal:Class`) or a predicate
/// (kind DERIVED from the GMEOW source's OWL character).
enum TargetSort {
    Class,
    Predicate,
}

/// The EDOAL target term of a binding and its sort. `None` when the binding names no
/// EDOAL target (the caller emits no cell for it).
fn edoal_target(b: &ProfileBinding) -> Option<(String, TargetSort)> {
    if let Some(t) = &b.edoal_target {
        return Some((t.clone(), TargetSort::Class));
    }
    if let Some(t) = &b.to_class {
        return Some((t.clone(), TargetSort::Class));
    }
    if let Some(t) = &b.to_predicate {
        return Some((t.clone(), TargetSort::Predicate));
    }
    None
}

/// Build entity1 for a path source (an `edoal:Relation` compose) and report the path's
/// **terminal** predicate — the value-producing edge whose GMEOW OWL character derives
/// the target's entity kind. `None` terminal (path-alt / predicate-var edge) leaves the
/// kind to an explicit override or a hard fail upstream.
fn edoal_path(
    nt: &mut Nt,
    pattern: &MappingPattern,
) -> gmeow_errors::Result<Option<(String, Option<String>)>> {
    let Some(value) = &pattern.value else {
        return Ok(None);
    };
    let edges = nav_edges(pattern);
    let Some(steps) = find_var_path(&edges, &pattern.anchor, value) else {
        return Ok(None);
    };
    if steps.is_empty() {
        return Ok(None);
    }
    let terminal_pred = steps
        .last()
        .and_then(|(idx, _)| edges[*idx].2.predicate.clone());
    let mut relations = Vec::new();
    for (atom_idx, forward) in &steps {
        relations.push(edoal_relation_step(nt, &edges[*atom_idx].2, *forward)?);
    }
    if relations.len() == 1 {
        return Ok(Some((relations.into_iter().next().unwrap(), terminal_pred)));
    }
    let compose = nt.fresh_bnode();
    nt.add_iri(&compose, RDF_TYPE, &format!("{EDOAL}Relation"));
    nt.attach_list(&compose, &format!("{EDOAL}compose"), &relations);
    Ok(Some((compose, terminal_pred)))
}

type NavEdge = (String, String, Atom);

fn nav_edges(pattern: &MappingPattern) -> Vec<NavEdge> {
    let mut edges = Vec::new();
    for atom in pattern.flat_atoms() {
        let has_pred = atom.predicate.is_some()
            || atom.predicate_var.is_some()
            || atom.path.is_some()
            || !atom.path_alts.is_empty();
        if let Some(obj) = &atom.object_var
            && has_pred
        {
            edges.push((atom.subject_var.clone(), obj.clone(), atom.clone()));
        }
    }
    edges
}

fn find_var_path(edges: &[NavEdge], anchor: &str, value: &str) -> Option<Vec<(usize, bool)>> {
    use std::collections::{HashMap, HashSet, VecDeque};
    let mut adj: HashMap<String, Vec<(String, usize, bool)>> = HashMap::new();
    for (idx, (subj, obj, _)) in edges.iter().enumerate() {
        adj.entry(subj.clone())
            .or_default()
            .push((obj.clone(), idx, true));
        adj.entry(obj.clone())
            .or_default()
            .push((subj.clone(), idx, false));
    }
    let mut queue: VecDeque<(String, Vec<(usize, bool)>)> = VecDeque::new();
    queue.push_back((anchor.to_owned(), Vec::new()));
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(anchor.to_owned());
    while let Some((node, steps)) = queue.pop_front() {
        if node == value {
            return Some(steps);
        }
        if let Some(neighbours) = adj.get(&node) {
            for (nbr, idx, forward) in neighbours {
                if !seen.contains(nbr) {
                    seen.insert(nbr.clone());
                    let mut next = steps.clone();
                    next.push((*idx, *forward));
                    queue.push_back((nbr.clone(), next));
                }
            }
        }
    }
    None
}

fn edoal_relation_step(nt: &mut Nt, atom: &Atom, forward: bool) -> gmeow_errors::Result<String> {
    let base = if !atom.path_alts.is_empty() {
        if atom.path_alts.len() == 1 {
            edoal_entity(nt, &atom.path_alts[0], "relation")?
        } else {
            let base = nt.fresh_bnode();
            nt.add_iri(&base, RDF_TYPE, &format!("{EDOAL}Relation"));
            let members: Vec<String> = atom
                .path_alts
                .iter()
                .map(|a| edoal_entity(nt, a, "relation"))
                .collect::<gmeow_errors::Result<_>>()?;
            nt.attach_list(&base, &format!("{EDOAL}or"), &members);
            base
        }
    } else if let Some(pred) = &atom.predicate {
        edoal_entity(nt, pred, "relation")?
    } else {
        let base = nt.fresh_bnode();
        nt.add_iri(&base, RDF_TYPE, &format!("{EDOAL}Relation"));
        let comment = atom
            .path
            .clone()
            .or_else(|| atom.predicate_var.clone())
            .unwrap_or_else(|| "path".to_owned());
        nt.add_string_literal(&base, RDFS_COMMENT, &comment);
        base
    };
    if forward {
        return Ok(base);
    }
    let inverse = nt.fresh_bnode();
    nt.add_iri(&inverse, RDF_TYPE, &format!("{EDOAL}Relation"));
    nt.add_bnode_obj(&inverse, &format!("{EDOAL}inverse"), &base);
    Ok(inverse)
}

fn format_double(v: f64) -> String {
    // Rust's default f64 Display is the shortest round-tripping form and matches the
    // committed canonical-Turtle doubles exactly: 1.0 → "1", 0.8 → "0.8". (The old
    // emitter relied on an oxigraph re-dump to normalize "1.0" → "1"; rendering
    // directly from the IR, we mint the canonical lexical here.)
    format!("{v}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_bnode_encodes_injectively() {
        // Alphanumerics pass through; `-` (0x2d) is hex-escaped.
        assert_eq!(Nt::stable_bnode("cell-foaf-x-0"), "_:ncell_2dfoaf_2dx_2d0");
        // The old `non-alnum → '_'` collapse mapped these two distinct labels to the
        // same id; the injective encoding keeps them distinct.
        assert_ne!(
            Nt::stable_bnode("cell-a_b-0"),
            Nt::stable_bnode("cell-a-b-0")
        );
        // A literal underscore is itself escaped, so the encoding is unambiguous.
        assert_eq!(Nt::stable_bnode("a_b"), "_:na_5fb");
    }

    #[test]
    fn format_double_matches_corpus() {
        assert_eq!(format_double(0.8), "0.8");
        assert_eq!(format_double(0.95), "0.95");
        assert_eq!(format_double(0.6), "0.6");
        assert_eq!(format_double(1.0), "1");
    }

    /// RED witness driving the overclaim gate THROUGH the real EDOAL lowering: a cell
    /// authored as a `BridgeView` whose EDOAL relation symbol is the equivalence token
    /// `=` must make the lowering return `Err` (Constitution Principle 5 — a bridge view
    /// may never assert equivalence). This exercises the gate at the production call
    /// site, not only the bare gate function.
    #[test]
    fn bridge_cell_emitting_equivalence_fails_the_lowering() {
        use crate::ir::MorphismClass;
        use crate::projections::correspondence_frontend::{CorrespondenceLookup, TypedRelation};

        let gm = "https://blackcatinformatics.ca/gmeow/";
        let bridge_cell = ProjectionCell {
            iri: format!("{gm}cellBridge"),
            label: "bridge".to_owned(),
            pattern: MappingPattern {
                anchor: "x".to_owned(),
                value: None,
                atoms: Vec::new(),
                suppress_when: Vec::new(),
                project_when: Vec::new(),
                exclude_when: Vec::new(),
                filters: Vec::new(),
                binds: Vec::new(),
                mints: Vec::new(),
                edoal_source: Some(format!("{gm}Foo")),
                edoal_source_kind: Some("class".to_owned()),
                edoal_path: false,
            },
            bindings: vec![ProfileBinding {
                profile: "schema-org".to_owned(),
                to_predicate: None,
                to_class: Some(format!("{gm}Bar")),
                template_atoms: Vec::new(),
                value_class_map: Vec::new(),
                // The EDOAL relation symbol is the equivalence token `=` …
                relation: "=".to_owned(),
                transform: None,
                confidence: None,
                lossy_drops: Vec::new(),
                edoal_target: None,
                edoal_target_kind: Some("class".to_owned()),
                // … but the correspondence is authored as a by-reference BridgeView.
                morphism_class: Some(MorphismClass::BridgeView),
                ingest_claim: None,
                ingest_residue: Vec::new(),
                mnemomorphic: false,
                emit_sssom: false,
                sssom_predicate: None,
                sssom_file: None,
            }],
            grounding: None,
        };
        let tag_map = BTreeMap::new();
        // The materialized correspondence for this binding: the relation `=` lattices to
        // Equiv, the authored class is BridgeView, the kind is InstitutionMorphism — the
        // exact triple `b.lattice()` (and so the transpiler) would mint. The gate consumes
        // this typed envelope, which forbids a BridgeView surfacing equivalence.
        let lookup = CorrespondenceLookup::for_binding_test(
            &format!("{gm}cellBridge"),
            "schema-org",
            TypedRelation {
                relation: crate::ir::CorrespondenceRelation::Equiv,
                morphism_class: MorphismClass::BridgeView,
                morphism_kind: crate::ir::MorphismKind::InstitutionMorphism,
            },
        );
        let mut loss = LossLedger::new();
        // The overclaim gate fires before any entity kind is resolved, so the ontology
        // view is unused here — an empty view suffices.
        let onto_ds = ds("");
        let onto = DslView::new(&onto_ds);
        let err = emit_edoal_nt(
            &[bridge_cell],
            "schema-org",
            &onto,
            &tag_map,
            &lookup,
            &mut loss,
        )
        .expect_err("a bridge view emitting `=` must be rejected by the lowering");
        assert!(err.message().contains("bridge"), "{err}");
        assert!(err.message().contains("Principle 5"), "{err}");
    }

    // ── Entity-kind derivation (issue: EDOAL mistyped predicates) ──────────────────

    /// Parse Turtle into a frozen dataset for an ontology view (native lenient codec so
    /// `@x-gmeow-*` tags parse — mirrors the pipeline file edge, which reads file bytes).
    fn ds(ttl: &str) -> std::sync::Arc<purrdf::RdfDataset> {
        parse_dataset(ttl.as_bytes(), NativeRdfFormat::Turtle.media_type(), None)
            .expect("parse fixture turtle")
    }

    const GM: &str = "https://blackcatinformatics.ca/gmeow/";
    const OWL_PREFIX: &str = "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
                              @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n";

    /// A minimal `toPredicate` projection cell whose single GMEOW `edoalSource` feeds the
    /// derivation, with optionally-authored source/target kind overrides.
    fn predicate_cell(
        source: &str,
        source_kind: Option<&str>,
        target: &str,
        target_kind: Option<&str>,
    ) -> ProjectionCell {
        ProjectionCell {
            iri: format!("{GM}cellDerive"),
            label: String::new(),
            pattern: MappingPattern {
                anchor: "x".to_owned(),
                value: None,
                atoms: Vec::new(),
                suppress_when: Vec::new(),
                project_when: Vec::new(),
                exclude_when: Vec::new(),
                filters: Vec::new(),
                binds: Vec::new(),
                mints: Vec::new(),
                edoal_source: Some(source.to_owned()),
                edoal_source_kind: source_kind.map(str::to_owned),
                edoal_path: false,
            },
            bindings: vec![ProfileBinding {
                profile: "sioc".to_owned(),
                to_predicate: Some(target.to_owned()),
                to_class: None,
                template_atoms: Vec::new(),
                value_class_map: Vec::new(),
                relation: "<=".to_owned(),
                transform: None,
                confidence: None,
                lossy_drops: Vec::new(),
                edoal_target: None,
                edoal_target_kind: target_kind.map(str::to_owned),
                morphism_class: None,
                ingest_claim: None,
                ingest_residue: Vec::new(),
                mnemomorphic: false,
                emit_sssom: false,
                sssom_predicate: None,
                sssom_file: None,
            }],
            grounding: None,
        }
    }

    /// Run `edoal_cells` (bypassing the overclaim gate) and return the emitted N-Triples.
    fn emit_kind_nt(onto: &DslView, cell: &ProjectionCell) -> gmeow_errors::Result<String> {
        let mut nt = Nt::new();
        let b = &cell.bindings[0];
        let cells = edoal_cells(&mut nt, onto, cell, b, "x-gmeow-english")?;
        assert!(!cells.is_empty(), "expected a cell to be emitted");
        Ok(nt.lines)
    }

    #[test]
    fn object_property_source_derives_relation() {
        let onto_ds = ds(&format!(
            "{OWL_PREFIX} gm:hasCreator a owl:ObjectProperty ."
        ));
        let onto = DslView::new(&onto_ds);
        // No authored kind on either side: the target kind is DERIVED from the object
        // property source, so entity2 is edoal:Relation (not the old silent Property).
        let cell = predicate_cell(
            &format!("{GM}hasCreator"),
            None,
            "http://rdfs.org/sioc/ns#has_creator",
            None,
        );
        let nt = emit_kind_nt(&onto, &cell).expect("derivation succeeds");
        assert!(nt.contains(&format!("{EDOAL}Relation")), "{nt}");
        assert!(!nt.contains(&format!("{EDOAL}Property")), "{nt}");
    }

    #[test]
    fn datatype_property_source_derives_property() {
        let onto_ds = ds(&format!(
            "{OWL_PREFIX} gm:fullName a owl:DatatypeProperty ."
        ));
        let onto = DslView::new(&onto_ds);
        let cell = predicate_cell(
            &format!("{GM}fullName"),
            None,
            "http://rdfs.org/sioc/ns#name",
            None,
        );
        let nt = emit_kind_nt(&onto, &cell).expect("derivation succeeds");
        assert!(nt.contains(&format!("{EDOAL}Property")), "{nt}");
        assert!(!nt.contains(&format!("{EDOAL}Relation")), "{nt}");
    }

    #[test]
    fn authored_target_kind_overrides_derivation() {
        // Source is an object property (would derive Relation) but the binding authors an
        // explicit override — the override wins.
        let onto_ds = ds(&format!(
            "{OWL_PREFIX} gm:hasCreator a owl:ObjectProperty ."
        ));
        let onto = DslView::new(&onto_ds);
        let cell = predicate_cell(
            &format!("{GM}hasCreator"),
            Some("relation"),
            "http://rdfs.org/sioc/ns#name",
            Some("property"),
        );
        let nt = emit_kind_nt(&onto, &cell).expect("override succeeds");
        // entity2 (target) honors the "property" override.
        assert!(nt.contains(&format!("{EDOAL}Property")), "{nt}");
    }

    #[test]
    fn indeterminate_source_kind_is_a_hard_fail() {
        // The GMEOW source carries no owl:*Property/Class type and no override is authored.
        let onto_ds = ds(OWL_PREFIX);
        let onto = DslView::new(&onto_ds);
        let cell = predicate_cell(
            &format!("{GM}untyped"),
            None,
            "http://rdfs.org/sioc/ns#name",
            None,
        );
        let err = emit_kind_nt(&onto, &cell).expect_err("indeterminate kind must hard-fail");
        assert!(err.message().contains("indeterminate"), "{err}");
    }

    #[test]
    fn annotation_property_derives_kind_from_range() {
        // An annotation property carries no object/datatype OWL character; a datatype
        // (xsd) range makes it a `property`, a class/IRI range a `relation`.
        let onto_ds = ds(&format!(
            "{OWL_PREFIX}@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             gm:validFrom a owl:AnnotationProperty ; rdfs:range xsd:dateTime .\n\
             gm:seeThing a owl:AnnotationProperty ; rdfs:range gm:Thing .",
        ));
        let onto = DslView::new(&onto_ds);
        assert_eq!(
            gmeow_entity_kind(&onto, &format!("{GM}validFrom")),
            Some("property")
        );
        assert_eq!(
            gmeow_entity_kind(&onto, &format!("{GM}seeThing")),
            Some("relation")
        );
        // No range → indeterminate (the caller then requires an override or hard-fails).
        let bare_ds = ds(&format!("{OWL_PREFIX} gm:bare a owl:AnnotationProperty ."));
        let bare = DslView::new(&bare_ds);
        assert_eq!(gmeow_entity_kind(&bare, &format!("{GM}bare")), None);
    }

    // ── G3: OWL 2 object-property subtypes carry object character even without an
    // explicit `owl:ObjectProperty` co-assertion ────────────────────────────────────

    #[test]
    fn object_property_subtype_alone_derives_relation() {
        // A term typed ONLY `owl:SymmetricProperty` (no co-asserted `owl:ObjectProperty`)
        // is still, by OWL 2 semantics, an object property — `gmeow_entity_kind` must not
        // derive `None` (which would HARD-FAIL the build) for it.
        let sym_ds = ds(&format!(
            "{OWL_PREFIX} gm:sibling a owl:SymmetricProperty ."
        ));
        let sym = DslView::new(&sym_ds);
        assert_eq!(
            gmeow_entity_kind(&sym, &format!("{GM}sibling")),
            Some("relation")
        );

        let trans_ds = ds(&format!(
            "{OWL_PREFIX} gm:ancestor a owl:TransitiveProperty ."
        ));
        let trans = DslView::new(&trans_ds);
        assert_eq!(
            gmeow_entity_kind(&trans, &format!("{GM}ancestor")),
            Some("relation")
        );
    }

    // ── G4: an annotation property ranged on an RDF-namespace datatype (rdf:langString,
    // rdf:HTML, rdf:PlainLiteral) is a literal-valued `property`, not a `relation` ────

    #[test]
    fn annotation_property_range_rdf_lang_string_derives_property() {
        let onto_ds = ds(&format!(
            "{OWL_PREFIX}@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             gm:label2 a owl:AnnotationProperty ; rdfs:range rdf:langString .\n",
        ));
        let onto = DslView::new(&onto_ds);
        assert_eq!(
            range_entity_kind(&onto, &format!("{GM}label2")),
            Some("property")
        );
    }

    #[test]
    fn edoal_kind_rejects_unknown_token() {
        assert_eq!(edoal_kind("relation").unwrap(), "Relation");
        assert_eq!(edoal_kind("property").unwrap(), "Property");
        assert_eq!(edoal_kind("class").unwrap(), "Class");
        assert!(edoal_kind("relaton").is_err());
        assert!(valid_kind("relaton").is_err());
    }

    // ── Template-derived target kind (G1: EDOAL target kind must come from the
    // correspondence TEMPLATE, never the GMEOW source predicate's OWL character) ──────

    use crate::projections::get_leg::{Bind, Expr, Item};

    fn plain_atom(subject_var: &str, predicate: &str, object_var: &str) -> Atom {
        Atom {
            subject_var: subject_var.to_owned(),
            predicate: Some(predicate.to_owned()),
            predicate_var: None,
            path: None,
            path_alts: Vec::new(),
            object_var: Some(object_var.to_owned()),
            object_value: None,
            object_literal: None,
            optional: false,
        }
    }

    fn typed_atom(subject_var: &str, class_iri: &str) -> Atom {
        Atom {
            subject_var: subject_var.to_owned(),
            predicate: Some(RDF_TYPE.to_owned()),
            predicate_var: None,
            path: None,
            path_alts: Vec::new(),
            object_var: None,
            object_value: Some(class_iri.to_owned()),
            object_literal: None,
            optional: false,
        }
    }

    /// A `toPredicate` binding whose target is built from a TEMPLATE (`templateAtoms`),
    /// not a direct 1:1 predicate — the shape `owl-time`'s `mapTimeHasBeginning` and
    /// friends use.
    fn templated_cell(
        source_pred: &str,
        source_kind_decl: &str,
        mints: Vec<Bind>,
        template_atoms: Vec<Atom>,
        to_predicate: &str,
    ) -> (ProjectionCell, std::sync::Arc<purrdf::RdfDataset>) {
        let onto_ds = ds(&format!(
            "{OWL_PREFIX} gm:{source_pred} a owl:{source_kind_decl} ."
        ));
        let cell = ProjectionCell {
            iri: format!("{GM}cellTemplated"),
            label: String::new(),
            pattern: MappingPattern {
                anchor: "s".to_owned(),
                value: None,
                atoms: vec![Item::Atom(plain_atom(
                    "s",
                    &format!("{GM}{source_pred}"),
                    "v",
                ))],
                suppress_when: Vec::new(),
                project_when: Vec::new(),
                exclude_when: Vec::new(),
                filters: Vec::new(),
                binds: Vec::new(),
                mints,
                edoal_source: Some(format!("{GM}{source_pred}")),
                edoal_source_kind: None,
                edoal_path: false,
            },
            bindings: vec![ProfileBinding {
                profile: "owl-time".to_owned(),
                to_predicate: Some(to_predicate.to_owned()),
                to_class: None,
                template_atoms,
                value_class_map: Vec::new(),
                relation: "<=".to_owned(),
                transform: None,
                confidence: None,
                lossy_drops: Vec::new(),
                edoal_target: None,
                edoal_target_kind: None,
                morphism_class: None,
                ingest_claim: None,
                ingest_residue: Vec::new(),
                mnemomorphic: false,
                emit_sssom: false,
                sssom_predicate: None,
                sssom_file: None,
            }],
            grounding: None,
        };
        (cell, onto_ds)
    }

    #[test]
    fn template_minted_iri_object_derives_relation_even_though_source_is_a_datatype_property() {
        // The `owl-time` shape: `gm:startedAtTime` (source, DatatypeProperty) feeds the
        // pattern's plain value var "v", but the TEMPLATE's `time:hasBeginning` atom
        // points at a MINTED "inst" var (a fresh IRI, via `opIri`), then types it
        // `time:Instant`. The target is manifestly an individual — `relation` — even
        // though the source predicate carries a literal (DatatypeProperty) character.
        // This is the exact G1 regression: the old code derived entity2 from the
        // source's OWL character and got `Property`, not `Relation`.
        let ex_beginning = "http://example.org/hasBeginning";
        let ex_instant = "http://example.org/Instant";
        let (cell, onto_ds) = templated_cell(
            "startedAtTime",
            "DatatypeProperty",
            vec![Bind {
                var: "inst".to_owned(),
                expr: Expr::Op {
                    op: GM_OP_IRI.to_owned(),
                    args: Vec::new(),
                },
            }],
            vec![
                plain_atom("s", ex_beginning, "inst"),
                typed_atom("inst", ex_instant),
            ],
            ex_beginning,
        );
        let onto = DslView::new(&onto_ds);
        let b = &cell.bindings[0];
        assert_eq!(
            template_target_kind(&onto, b, &cell.pattern),
            Some("relation"),
            "a minted-IRI template object is an individual, not a literal"
        );
        let nt = emit_kind_nt(&onto, &cell).expect("template derivation succeeds");
        // entity1 (source `gm:startedAtTime`) still derives its OWN kind from ITS OWN
        // OWL character (`Property`, DatatypeProperty) — entity1 resolution is
        // untouched by this fix. entity2 (target) derives `Relation` from the
        // template. The cross-kind pairing is legal under `<=` (subsumption).
        assert!(nt.contains(&format!("{EDOAL}Property")), "{nt}");
        assert!(nt.contains(&format!("{EDOAL}Relation")), "{nt}");
    }

    #[test]
    fn template_literal_var_derives_property_even_though_no_mint_or_subject_use() {
        // The `spdx:checksumValue` shape (dcat.ttl's `mapDcatChecksum`): the template's
        // `spdx:checksumValue` atom points at "digest", a var that is never minted and
        // never a template/source SUBJECT — a pure leaf. It traces back to the GMEOW
        // source atom `gm:contentDigest` (a DatatypeProperty), so it is a literal.
        let ex_checksum_value = "http://example.org/checksumValue";
        let (cell, onto_ds) = templated_cell(
            "contentDigest",
            "DatatypeProperty",
            Vec::new(),
            vec![plain_atom("chk", ex_checksum_value, "v")],
            ex_checksum_value,
        );
        let onto = DslView::new(&onto_ds);
        let b = &cell.bindings[0];
        assert_eq!(
            template_target_kind(&onto, b, &cell.pattern),
            Some("property")
        );
        let nt = emit_kind_nt(&onto, &cell).expect("template derivation succeeds");
        assert!(nt.contains(&format!("{EDOAL}Property")), "{nt}");
        assert!(!nt.contains(&format!("{EDOAL}Relation")), "{nt}");
    }

    #[test]
    fn template_atoms_present_but_no_atom_names_to_predicate_is_a_hard_fail_without_override() {
        // `template_atoms` is non-empty (so the direct source-derived fallback must NOT
        // silently kick in — Constitution no-optionality) but NO template atom names
        // `to_predicate`: `template_target_kind` returns `None`, and with no authored
        // `gmeow:edoalTargetKind` override, `edoal_cells` must hard-fail rather than
        // guess from the source (the historical bug).
        let ex_target = "http://example.org/unrelatedTarget";
        let (cell, onto_ds) = templated_cell(
            "startedAtTime",
            "DatatypeProperty",
            Vec::new(),
            vec![plain_atom("s", "http://example.org/somethingElse", "v")],
            ex_target,
        );
        let onto = DslView::new(&onto_ds);
        let b = &cell.bindings[0];
        assert_eq!(template_target_kind(&onto, b, &cell.pattern), None);
        let err = emit_kind_nt(&onto, &cell)
            .expect_err("an indeterminate template target with no override must hard-fail");
        assert!(err.message().contains("indeterminate"), "{err}");
        assert!(err.message().contains("template"), "{err}");
    }
}

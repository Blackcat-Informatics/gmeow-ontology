// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native operator derivation over explicit default-graph input and prepared shipped rules.
//! Original worlds are never silently merged; source diagnostics and lowering residue
//! remain part of every result. Label re-derivation is a separate declared audit.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::operator_rules::PreparedOperatorRules;
use purrdf::{RdfTerm, TermValue};

pub mod refinement;
pub mod scene;

const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// The synthetic world the CLI reasons in.
pub const CLI_WORLD: &str = "https://blackcatinformatics.ca/gmeow/cli/world";

/// The `rdf:reifies` predicate — the RDF 1.2 statement layer's binding edge.
///
/// `purrdf` parses `<r> rdf:reifies <<( s p o )>>` into the dataset's reifier SIDE TABLE
/// rather than a base quad, so a reader that only walks `quads()` never sees a reifier at
/// all. The CLI lifts the side tables back into explicit base quads (below) so attributed
/// provenance reaches the reasoner instead of being silently absent from its world.
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

/// `xsd:string` — the implied datatype of a plain literal, elided in the display form
/// exactly as Turtle elides it.
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// `rdf:langString` — the implied datatype of a language-tagged literal.
const RDF_LANGSTRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

/// The RDF kind of one derived row's subject or object.
///
/// Carried explicitly so an IRI can never be confused with a literal whose lexical form
/// happens to spell one, and so a consumer folding the JSON back into a graph rebuilds
/// the SAME term rather than guessing from a bare string.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum RowTermKind {
    Iri,
    BlankNode,
    Literal,
    TripleTerm,
}

impl RowTermKind {
    /// The stable JSON tag for this kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Iri => "iri",
            Self::BlankNode => "blank",
            Self::Literal => "literal",
            Self::TripleTerm => "triple-term",
        }
    }
}

/// One term of a derived row, kept LOSSLESSLY.
///
/// The retired form was a bare `String`, which collapsed four different RDF terms into one
/// spelling and threw the datatype, the language tag and the base direction away on the
/// way past. A typed literal came back as its lexical form, a blank node and a triple term
/// came back as nothing at all, and an operator had no way to tell any of that had
/// happened.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct RowTerm {
    pub kind: RowTermKind,
    /// The IRI, the blank-node label, the literal's lexical form, or a triple term's
    /// N-Triples text.
    pub value: String,
    /// The literal's datatype IRI. `None` for every non-literal.
    pub datatype: Option<String>,
    /// The literal's language tag, lowercased as RDF requires.
    pub language: Option<String>,
    /// The RDF 1.2 base direction (`ltr` / `rtl`) of a directional language-tagged string.
    pub direction: Option<String>,
}

impl RowTerm {
    /// This term as an IRI, or `None` when it is any other kind.
    pub fn iri(&self) -> Option<&str> {
        match self.kind {
            RowTermKind::Iri => Some(self.value.as_str()),
            _ => None,
        }
    }

    /// The term in Turtle's own abbreviating syntax: a bare IRI, `_:label`, a quoted
    /// literal carrying whichever of `@lang` / `^^<datatype>` is not implied, or a
    /// `<<( … )>>` triple term. Faithful — reading it back reconstructs the term.
    pub fn display(&self) -> String {
        match self.kind {
            RowTermKind::Iri | RowTermKind::TripleTerm => self.value.clone(),
            RowTermKind::BlankNode => format!("_:{}", self.value),
            RowTermKind::Literal => {
                let mut out = format!("{:?}", self.value);
                if let Some(lang) = &self.language {
                    out.push('@');
                    out.push_str(lang);
                    if let Some(dir) = &self.direction {
                        out.push_str("--");
                        out.push_str(dir);
                    }
                } else if let Some(dt) = &self.datatype
                    && dt != XSD_STRING
                {
                    out.push_str("^^<");
                    out.push_str(dt);
                    out.push('>');
                }
                out
            }
        }
    }

    /// The term in strict N-Triples syntax: `<iri>`, `_:label`, a quoted literal, or a
    /// `<<( … )>>` triple term. Used INSIDE a triple term, where a bare IRI would not be
    /// re-readable; [`RowTerm::display`] keeps the bare form for the table columns.
    fn n3(&self) -> String {
        match self.kind {
            RowTermKind::Iri => format!("<{}>", self.value),
            _ => self.display(),
        }
    }

    /// The JSON object for this term: kind, value, and every literal facet that exists.
    pub fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("kind".to_owned(), self.kind.as_str().into());
        map.insert("value".to_owned(), self.value.clone().into());
        if let Some(dt) = &self.datatype {
            map.insert("datatype".to_owned(), dt.clone().into());
        }
        if let Some(lang) = &self.language {
            map.insert("language".to_owned(), lang.clone().into());
        }
        if let Some(dir) = &self.direction {
            map.insert("direction".to_owned(), dir.clone().into());
        }
        serde_json::Value::Object(map)
    }

    /// Build a row term from an owned [`purrdf::RdfTerm`] — the parse-side form.
    pub fn from_rdf_term(term: &RdfTerm) -> Self {
        match term {
            RdfTerm::Iri(iri) => Self {
                kind: RowTermKind::Iri,
                value: iri.clone(),
                datatype: None,
                language: None,
                direction: None,
            },
            RdfTerm::BlankNode(label) => Self {
                kind: RowTermKind::BlankNode,
                value: label.clone(),
                datatype: None,
                language: None,
                direction: None,
            },
            RdfTerm::Literal(lit) => Self {
                kind: RowTermKind::Literal,
                value: lit.lexical_form.clone(),
                // `None` on the wire means the IMPLIED datatype, which is
                // `rdf:langString` for a tagged string and `xsd:string` otherwise. Naming
                // it is the whole point: the retired code wrote `datatype: None` back out
                // and lost the authored `^^<…>` entirely.
                datatype: Some(lit.datatype.clone().unwrap_or_else(|| {
                    if lit.language.is_some() {
                        RDF_LANGSTRING.to_owned()
                    } else {
                        XSD_STRING.to_owned()
                    }
                })),
                language: lit.language.clone(),
                direction: lit.direction.map(|d| text_direction_str(d).to_owned()),
            },
            RdfTerm::Triple(triple) => Self {
                kind: RowTermKind::TripleTerm,
                value: format!(
                    "<<( {} <{}> {} )>>",
                    Self::from_rdf_term(&triple.subject).n3(),
                    triple.predicate,
                    Self::from_rdf_term(&triple.object).n3()
                ),
                datatype: None,
                language: None,
                direction: None,
            },
        }
    }

    /// Build a row term from a reasoner-side [`purrdf::TermValue`].
    pub fn from_term_value(term: &TermValue) -> Self {
        match term {
            TermValue::Iri(iri) => Self {
                kind: RowTermKind::Iri,
                value: iri.clone(),
                datatype: None,
                language: None,
                direction: None,
            },
            TermValue::Blank { label, .. } => Self {
                kind: RowTermKind::BlankNode,
                value: label.clone(),
                datatype: None,
                language: None,
                direction: None,
            },
            TermValue::Literal {
                lexical_form,
                datatype,
                language,
                direction,
            } => Self {
                kind: RowTermKind::Literal,
                value: lexical_form.clone(),
                datatype: Some(datatype.clone()),
                language: language.clone(),
                direction: direction.map(|d| text_direction_str(d).to_owned()),
            },
            TermValue::Triple { s, p, o } => Self {
                kind: RowTermKind::TripleTerm,
                value: format!(
                    "<<( {} <{p_iri}> {} )>>",
                    Self::from_term_value(s).n3(),
                    Self::from_term_value(o).n3(),
                    p_iri = match p.as_ref() {
                        TermValue::Iri(iri) => iri.clone(),
                        other => Self::from_term_value(other).display(),
                    }
                ),
                datatype: None,
                language: None,
                direction: None,
            },
        }
    }
}

/// The BCP-47 / RDF 1.2 spelling of a base direction.
fn text_direction_str(direction: purrdf::RdfTextDirection) -> &'static str {
    match direction {
        purrdf::RdfTextDirection::Ltr => "ltr",
        purrdf::RdfTextDirection::Rtl => "rtl",
    }
}

/// One `(subject, predicate, object)` row of the reasoning result, carrying WHERE it came
/// from.
///
/// The two flags are independent rather than an either/or enum, because a row can be both:
/// an author asserts a triple AND a rule re-derives it. That coincidence is the *agreement*
/// case, and collapsing it into one "source" would make agreement indistinguishable from a
/// label nothing checked. Keeping them apart is what lets the frontier command separate
/// "the reasoner concluded this", "the reasoner concluded something else", and "an author
/// typed this and no rule looked at it".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct FactRow {
    pub subject: RowTerm,
    pub predicate: String,
    pub object: RowTerm,
    /// The row is in the asserted EDB — a human or an upstream tool wrote it.
    pub asserted: bool,
    /// A shipped `logic:Rule` concluded the row. Recognised by the derivation's `rule_iri`
    /// being something other than [`crate::provenance::ASSERT_RULE_IRI`], which is
    /// the same EDB/IDB split `crates/logic` itself uses in `derivation_graph.rs` and in
    /// the chase — not a scheme invented here.
    pub derived: bool,
}

impl FactRow {
    /// The row's provenance as the one word the operator surface prints.
    pub fn provenance(&self) -> &'static str {
        match (self.asserted, self.derived) {
            (true, true) => "derived (input agrees)",
            (false, true) => "derived",
            _ => "ASSERTED-UNCHECKED",
        }
    }

    /// The row as one JSON object: both terms in full, the predicate, and the provenance
    /// split spelled out AND kept as the two independent flags it really is.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "subject": self.subject.to_json(),
            "predicate": self.predicate,
            "object": self.object.to_json(),
            "asserted": self.asserted,
            "derived": self.derived,
            "provenance": self.provenance(),
        })
    }
}

/// The complete operator row view and its exact selected evaluation context.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Derivation {
    pub world: String,
    pub source_digest: String,
    pub rows: Vec<FactRow>,
    pub nested_statements: Vec<RowTerm>,
    pub preservation: crate::result::PreservationClaim,
    pub audit_preservation: crate::result::PreservationClaim,
    pub diagnostics: Vec<gmeow_logic_compile::frontend::Diagnostic>,
    /// Actual native rule applications, including source IDs and the separate audit.
    pub applications: Vec<Application>,
}

/// A native row's original provenance, retained independently of the display flags.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Application {
    pub world: String,
    pub subject: RowTerm,
    pub predicate: String,
    pub object: RowTerm,
    pub rule_iri: String,
    pub source_quad_ids: Vec<String>,
    pub derivation_id: String,
    pub profile: String,
    pub budget_status: crate::seam::BudgetStatus,
    pub from_label_audit: bool,
}

impl Application {
    fn from_quad(row: &crate::seam::DerivedQuad, from_label_audit: bool) -> Self {
        Self {
            world: row.graph.clone(),
            subject: RowTerm::from_term_value(&row.subject),
            predicate: row.predicate.clone(),
            object: RowTerm::from_term_value(&row.object),
            rule_iri: row.rule_iri.clone(),
            source_quad_ids: row.source_quad_ids.clone(),
            derivation_id: row.derivation_id.0.clone(),
            profile: row.profile.clone(),
            budget_status: row.budget_status,
            from_label_audit,
        }
    }
}

/// Execute the original input and the independent entry-label audit from one native
/// source bridge. The audit consumes the same native terms and prepared rules.
///
/// # Errors
/// Rejects named input worlds, failed statement lowering or native materialization.
pub fn derive(
    parsed: &purrdf::RdfDataset,
    rules: &PreparedOperatorRules,
) -> gmeow_errors::Result<Derivation> {
    let world = cli_world_dataset(parsed)?;
    let edb = &world.edb;
    let full_store = crate::store::WorldStore::new();
    full_store.load_dataset(edb)?;
    let audit_store = crate::store::WorldStore::new();
    for quad in edb.quads() {
        if matches!(
            edb.resolve(quad.p),
            purrdf::TermRef::Iri("https://blackcatinformatics.ca/logic/entryLabel")
        ) {
            continue;
        }
        audit_store.insert_quad_terms(
            CLI_WORLD,
            edb.term_value(quad.s),
            edb.term_value(quad.p),
            edb.term_value(quad.o),
        )?;
    }
    let mat = rules
        .materialize_store(&full_store)
        .map_err(|error| gmeow_errors::Diag::from(error).with_context("materialization failed"))?;
    let audit = rules
        .materialize_store(&audit_store)
        .map_err(|error| gmeow_errors::Diag::from(error).with_context("materialization failed"))?;
    // A row can be reached twice — once as an assertion, once as a conclusion — so the
    // flags are OR-merged per triple rather than the rows being pushed twice and deduped.
    // Deduping tagged rows would keep both copies and make an agreeing label look like a
    // disagreement with itself.
    let mut by_triple: BTreeMap<(RowTerm, String, RowTerm), (bool, bool)> = BTreeMap::new();
    // The asserted input, so a caller sees the whole picture rather than only the delta.
    // EVERY quad, whatever its terms: a blank-node subject, a typed literal and an RDF 1.2
    // triple term are all carried whole. The retired reader `continue`d past each of them
    // and rebuilt literals with the datatype stripped, so an operator read a reduced world
    // with nothing on screen to say so.
    for q in edb.owned_quads() {
        by_triple
            .entry((
                RowTerm::from_rdf_term(&q.subject),
                q.predicate.clone(),
                RowTerm::from_rdf_term(&q.object),
            ))
            .or_default()
            .0 = true;
    }
    // The RDF 1.2 statement metadata the engine's own EDB-echo term surface cannot
    // round-trip. It IS carried — as asserted rows, with its triple term whole — and the
    // boundary is REPORTED below, because an operator reading a frontier over a graph
    // whose attributions the reasoner never saw must be told that, not left to assume it.
    for (subject, predicate, object) in &world.unreasoned {
        by_triple
            .entry((
                RowTerm::from_rdf_term(subject),
                predicate.clone(),
                RowTerm::from_rdf_term(object),
            ))
            .or_default()
            .0 = true;
    }
    // The residue, and ONLY the residue. The rule set now DOES run over flat statement
    // metadata: `crate::statement_lowering` decomposes each `rdf:reifies` triple term
    // into three ordinary joinable edges before the world is built, so an attribution's
    // subject, predicate and object are premises like any other. What survives as a
    // withhold is exactly what `logic:rdf12-nested-triple-term` records — a statement whose
    // own subject or object is itself a triple term, which has no non-term component to
    // decompose into. The `rdf:reifies` rows themselves are still reported unreasoned,
    // because the TERM is not a fact; that is the second half of the same boundary.
    // The audit run contributes ONLY its `logic:entryLabel` conclusions. It is a narrower
    // world than the full one, so letting it speak about anything else could only ever
    // subtract information, and scoping it to the one question it was run to answer keeps
    // the closure the other commands read identical to the single-run closure.
    let label_predicate = format!("{LOGIC_NS}entryLabel");
    let applications = mat
        .quads
        .iter()
        .map(|row| Application::from_quad(row, false))
        .chain(
            audit
                .quads
                .iter()
                .filter(|row| row.predicate == label_predicate)
                .map(|row| Application::from_quad(row, true)),
        )
        .collect();
    let audited = audit
        .quads
        .iter()
        .filter(|d| d.predicate == label_predicate);
    for d in mat.quads.iter().chain(audited) {
        let slot = by_triple
            .entry((
                RowTerm::from_term_value(&d.subject),
                d.predicate.clone(),
                RowTerm::from_term_value(&d.object),
            ))
            .or_default();
        // The materialization echoes the EDB back under the assert pseudo-rule. Treating
        // that echo as a derivation is exactly the bug this split exists to prevent: every
        // authored label would come back stamped "derived".
        if d.rule_iri == crate::provenance::ASSERT_RULE_IRI {
            slot.0 = true;
        } else {
            slot.1 = true;
        }
    }
    let rows = by_triple
        .into_iter()
        .map(
            |((subject, predicate, object), (asserted, derived))| FactRow {
                subject,
                predicate,
                object,
                asserted,
                derived,
            },
        )
        .collect();
    Ok(Derivation {
        world: CLI_WORLD.to_owned(),
        source_digest: rules.source_digest().to_owned(),
        rows,
        nested_statements: world
            .nested_statements
            .iter()
            .map(RowTerm::from_rdf_term)
            .collect(),
        preservation: mat.preservation,
        audit_preservation: audit.preservation,
        diagnostics: rules.diagnostics().to_vec(),
        applications,
    })
}

struct CliWorld {
    /// The world the shipped rule set is materialized over.
    edb: Arc<purrdf::RdfDataset>,
    /// `(subject, predicate, object)` facts carried to the caller but WITHHELD from the
    /// reasoning world — every one of them a triple-term-bearing statement-metadata fact.
    ///
    /// The `rdf:reifies` rows stay here even for a statement that WAS lowered: the term is
    /// not a fact, and reporting it as reasoned-over would claim the engine quantified over
    /// the statement itself. What it CAN do is join the three lowered components, which is
    /// the derivation `logic:contestedByAttribution` rests on.
    unreasoned: Vec<(RdfTerm, String, RdfTerm)>,
    /// The reifiers whose statement NESTS a triple term, and which therefore carry no
    /// lowering at all — the narrow residue `logic:rdf12-nested-triple-term` records.
    nested_statements: Vec<RdfTerm>,
}

/// True when `term` is (or contains) an RDF 1.2 triple term.
fn bears_triple_term(term: &RdfTerm) -> bool {
    match term {
        RdfTerm::Triple(_) => true,
        RdfTerm::Iri(_) | RdfTerm::BlankNode(_) | RdfTerm::Literal(_) => false,
    }
}

fn cli_world_dataset(parsed: &purrdf::RdfDataset) -> gmeow_errors::Result<CliWorld> {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let world = builder.intern_iri(CLI_WORLD);

    let mut named_graphs: BTreeSet<String> = BTreeSet::new();
    let mut unreasoned: Vec<(RdfTerm, String, RdfTerm)> = Vec::new();
    let push = |builder: &mut purrdf::RdfDatasetBuilder,
                unreasoned: &mut Vec<(RdfTerm, String, RdfTerm)>,
                subject: RdfTerm,
                predicate: String,
                object: RdfTerm| {
        if bears_triple_term(&subject) || bears_triple_term(&object) {
            unreasoned.push((subject, predicate, object));
            return;
        }
        let s = builder.intern_owned_term(&subject);
        let p = builder.intern_iri(&predicate);
        let o = builder.intern_owned_term(&object);
        builder.push_quad(s, p, o, Some(world));
    };

    for q in parsed.owned_quads() {
        if let Some(g) = &q.graph_name {
            named_graphs.insert(RowTerm::from_rdf_term(g).display());
            continue;
        }
        push(
            &mut builder,
            &mut unreasoned,
            q.subject,
            q.predicate,
            q.object,
        );
    }
    for reifier in parsed.owned_reifiers() {
        if let Some(g) = &reifier.graph {
            named_graphs.insert(RowTerm::from_rdf_term(g).display());
            continue;
        }
        push(
            &mut builder,
            &mut unreasoned,
            reifier.reifier,
            RDF_REIFIES.to_owned(),
            RdfTerm::Triple(Box::new(reifier.statement)),
        );
    }
    // The statement-metadata LOWERING (Principle 17): the `rdf:reifies` term above stays
    // withheld — a term is not a fact — while its three components enter the world as
    // ordinary triples, so a rule can join a reifier to the statement it reifies. The
    // authored dataset is untouched; this is derived from it, on the way in.
    let lowering = crate::statement_lowering::lower_reifiers(parsed);
    for (subject, predicate, object) in lowering.rows {
        push(&mut builder, &mut unreasoned, subject, predicate, object);
    }
    let nested_statements = lowering.nested;
    for annotation in parsed.owned_annotations() {
        if let Some(g) = &annotation.graph {
            named_graphs.insert(RowTerm::from_rdf_term(g).display());
            continue;
        }
        push(
            &mut builder,
            &mut unreasoned,
            annotation.reifier,
            annotation.predicate,
            annotation.object,
        );
    }

    if !named_graphs.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: format!(
                "input asserts content in {} named graph(s) ({}). The shipped rule set reasons \
             in ONE world, and merging distinct worlds into it would let a fact asserted \
             in one satisfy a rule body about another — so this is refused rather than \
             re-homed behind your back. Project the world you want reasoned over into the \
             default graph and pass that.",
                named_graphs.len(),
                named_graphs.into_iter().collect::<Vec<_>>().join(", ")
            ),
        }));
    }
    let edb = builder.freeze().map_err(|error| {
        gmeow_errors::Diag::from(error).with_context("cannot build the reasoning world")
    })?;
    Ok(CliWorld {
        edb,
        unreasoned,
        nested_statements,
    })
}

// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `constraint-catalog` export leaf: the machine-readable "what GMEOW
//! enforces" surface, generated (never hand-authored) from the validator's rule
//! registry and the reasoned ontology.
//!
//! Every [`gmeow_validate::rule_catalog::RuleSeed`] the validator can emit becomes
//! one `gmeow:ValidationRule` individual: its code, its default grade, its category
//! (a `logic:FindingCategory` derived from the enforcement kind), and its stable
//! catalog help URI. For the two disciplines whose governed terms are resolvable
//! from the authored ontology (frame-completeness and relator-mediation), the rule
//! is additionally enriched with `gmeow:appliesToTerm` / `logic:formalizes` /
//! `skos:definition` — resolved from the graph, never fabricated. The result rides
//! the bundle as the `graph/fanout/catalog/constraint-catalog.nq` named graph and
//! is fanned out to the committed `generated/catalog/constraint-catalog.nq`, which
//! the superset gate reconstructs byte-for-byte from that graph.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use gmeow_errors::Severity;
use gmeow_validate::rule_catalog::{Enforcement, RuleSeed, all_rules, help_uri_for, slugify};
use purrdf::slice::rdf_query::{Dataset, Object};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::source_load::module_files;

/// Committed logical path of the generated constraint catalog.
pub const CONSTRAINT_CATALOG_RDF_PATH: &str = "generated/catalog/constraint-catalog.nq";

/// The RDF-fanout named graph the catalog rides in (auto-derived from the committed
/// path by [`crate::stages::superset::rdf_fanout_graph_iri`]); it is ALSO the
/// 4th-column label of the committed `.nq`, so the fold reconstructs it exactly.
pub const CATALOG_GRAPH_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/graph/fanout/catalog/constraint-catalog.nq";

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const XSD_ANY_URI: &str = "http://www.w3.org/2001/XMLSchema#anyURI";

/// `logic:Relator` — the mediated-relation base class the relator-mediation
/// discipline enforces (the actual type used in the vocabulary; there is no
/// `gmeow:Relator`).
const LOGIC_RELATOR: &str = "https://blackcatinformatics.ca/logic/Relator";
/// `gmeow:requiresFrame` — the frame-relativity declaration the frame-completeness
/// discipline enforces.
const GMEOW_REQUIRES_FRAME: &str = "https://blackcatinformatics.ca/gmeow/requiresFrame";

// ── Advice-catalog projection ────────────────────────────────────────────────
// The realized advice carriers whose verbatim prose the advice section documents.
/// `logic:Constraint` — the class of the advisory anti-pattern carriers (an
/// `avoidWhen` prohibition realized at `logic:severity "Info"`).
const LOGIC_CONSTRAINT: &str = "https://blackcatinformatics.ca/logic/Constraint";
/// `logic:AdviceGuidance` — the class of the positive `useWhen` guidance carriers.
const LOGIC_ADVICE_GUIDANCE: &str = "https://blackcatinformatics.ca/logic/AdviceGuidance";
/// `logic:formalizes` — the carrier → governed-term back-link.
const LOGIC_FORMALIZES: &str = "https://blackcatinformatics.ca/logic/formalizes";
/// `logic:message` — the verbatim prose a carrier surfaces (gate-bound to the term's
/// source annotation by `check_advice_message_prose_binding`).
const LOGIC_MESSAGE: &str = "https://blackcatinformatics.ca/logic/message";
/// `logic:severity` — the carrier grade; the advisory tier is exactly `"Info"`.
const LOGIC_SEVERITY: &str = "https://blackcatinformatics.ca/logic/severity";
/// `logic:adviceSourceField` — which `logic:ProseField` the carrier's message binds to.
const LOGIC_ADVICE_SOURCE_FIELD: &str = "https://blackcatinformatics.ca/logic/adviceSourceField";
/// `logic:ProseFieldAvoidWhen` — the prohibition source field.
const LOGIC_PROSE_FIELD_AVOID_WHEN: &str =
    "https://blackcatinformatics.ca/logic/ProseFieldAvoidWhen";
/// `logic:ProseFieldUseWhen` — the conditional-permission source field.
const LOGIC_PROSE_FIELD_USE_WHEN: &str = "https://blackcatinformatics.ca/logic/ProseFieldUseWhen";
/// `gmeow:howToUse` — the term's positive-directive annotation (the sub-ideal repair).
const GMEOW_HOW_TO_USE: &str = "https://blackcatinformatics.ca/gmeow/howToUse";
/// The advice `logic:severity` wire token selecting the never-gating advisory tier.
const ADVISORY_SEVERITY_INFO: &str = "Info";

/// Map a `purrdf` slice-query error into a pipeline `Parse` diagnostic (shared by the
/// advice-projection accessors below).
fn parse_err(e: impl std::fmt::Display) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Parse {
        message: e.to_string(),
    })
}

/// The local name of an IRI — the segment after the last `/` or `#`. Used to mint a
/// stable, readable advice-entry subject/slug from a governed term IRI.
fn local_name(iri: &str) -> &str {
    iri.rsplit(['/', '#']).next().unwrap_or(iri)
}

/// The verbatim advice prose harvested for one governed term, each field a sorted set
/// (a term MAY bear several `avoidWhen` constraints; sets keep the projection stable
/// and non-lossy as the advice-harvest uplift lane harvests more).
#[derive(Default)]
struct AdviceProse {
    /// `gmeow:avoidWhen` prohibition prose (each realized Info `logic:Constraint`).
    avoid_when: BTreeSet<String>,
    /// `gmeow:useWhen` conditional-permission prose (each `logic:AdviceGuidance`).
    use_when: BTreeSet<String>,
    /// `gmeow:howToUse` positive-directive prose (the term's own annotation).
    how_to_use: BTreeSet<String>,
}

/// Collect, per governed term, the verbatim prose of its REALIZED advice carriers —
/// only terms with a machine-active carrier (an Info `logic:Constraint` and/or a
/// `logic:AdviceGuidance`) appear, so every entry is reachable from a real `advice.*`
/// finding code. Deterministic: `BTreeMap`/`BTreeSet` ordering throughout.
fn collect_advice(dataset: &Dataset) -> Result<BTreeMap<String, AdviceProse>, gmeow_errors::Diag> {
    let mut advice: BTreeMap<String, AdviceProse> = BTreeMap::new();

    // avoidWhen: an advisory logic:Constraint at logic:severity "Info" whose
    // logic:adviceSourceField is ProseFieldAvoidWhen; its logic:message is the term's
    // verbatim gmeow:avoidWhen prose (gate-bound equal).
    for constraint in dataset
        .subjects_of_type(LOGIC_CONSTRAINT)
        .map_err(parse_err)?
    {
        if dataset
            .object_literal(&constraint, LOGIC_SEVERITY)
            .map_err(parse_err)?
            .as_deref()
            != Some(ADVISORY_SEVERITY_INFO)
        {
            continue;
        }
        if dataset
            .first_object_iri(&constraint, LOGIC_ADVICE_SOURCE_FIELD)
            .map_err(parse_err)?
            .as_deref()
            != Some(LOGIC_PROSE_FIELD_AVOID_WHEN)
        {
            continue;
        }
        let Some(term) = dataset
            .first_object_iri(&constraint, LOGIC_FORMALIZES)
            .map_err(parse_err)?
        else {
            continue;
        };
        if let Some(message) = dataset
            .object_literal(&constraint, LOGIC_MESSAGE)
            .map_err(parse_err)?
        {
            advice.entry(term).or_default().avoid_when.insert(message);
        }
    }

    // useWhen: a logic:AdviceGuidance carrier (source field ProseFieldUseWhen); its
    // logic:message is the term's verbatim gmeow:useWhen prose.
    for guidance in dataset
        .subjects_of_type(LOGIC_ADVICE_GUIDANCE)
        .map_err(parse_err)?
    {
        if dataset
            .first_object_iri(&guidance, LOGIC_ADVICE_SOURCE_FIELD)
            .map_err(parse_err)?
            .as_deref()
            != Some(LOGIC_PROSE_FIELD_USE_WHEN)
        {
            continue;
        }
        let Some(term) = dataset
            .first_object_iri(&guidance, LOGIC_FORMALIZES)
            .map_err(parse_err)?
        else {
            continue;
        };
        if let Some(message) = dataset
            .object_literal(&guidance, LOGIC_MESSAGE)
            .map_err(parse_err)?
        {
            advice.entry(term).or_default().use_when.insert(message);
        }
    }

    // howToUse: the term's OWN gmeow:howToUse annotation — only for terms that already
    // have a realized carrier (present in the map). Multi-valued.
    for (term, prose) in advice.iter_mut() {
        for object in dataset.objects(term, GMEOW_HOW_TO_USE).map_err(parse_err)? {
            if let Object::Literal { value, .. } = object {
                prose.how_to_use.insert(value);
            }
        }
    }

    Ok(advice)
}

/// Emit the `gmeow:AdviceEntry` subjects into `out` — one per governed term with a
/// realized advice carrier, each hung beneath the `advice.` family `gmeow:ValidationRule`
/// via `gmeow:documentedByRule` so the single `#advice-` anchor documents them all.
fn emit_advice_entries(out: &mut String, dataset: &Dataset) -> Result<(), gmeow_errors::Diag> {
    let advice = collect_advice(dataset)?;
    let advice_family_rule = format!("{GMEOW}rule/family/advice");
    let mut seen_slugs: BTreeSet<String> = BTreeSet::new();
    for (term, prose) in &advice {
        let slug = slugify(local_name(term));
        // Slug-distinctness: two governed terms whose local names collide would mint
        // the same entry subject — a hard invariant break, never silently merged.
        assert!(
            seen_slugs.insert(slug.clone()),
            "advice-entry slug {slug} (term {term}) collides with another governed term"
        );
        let entry_iri = format!("{GMEOW}advice/{slug}");
        quad_iri(out, &entry_iri, RDF_TYPE, &format!("{GMEOW}AdviceEntry"));
        quad_iri(out, &entry_iri, RDFS_IS_DEFINED_BY, CATALOG_GRAPH_IRI);
        quad_iri(
            out,
            &entry_iri,
            &format!("{GMEOW}graphBoxRole"),
            &format!("{GMEOW}boxABox"),
        );
        quad_iri(
            out,
            &entry_iri,
            &format!("{GMEOW}documentedByRule"),
            &advice_family_rule,
        );
        quad_iri(out, &entry_iri, &format!("{LOGIC}formalizes"), term);
        quad_iri(out, &entry_iri, &format!("{GMEOW}appliesToTerm"), term);
        // Heading prose from the term itself; honest fallback to the term IRI as label
        // when the term carries no rdfs:label (never an empty literal).
        match dataset
            .object_literal(term, RDFS_LABEL)
            .map_err(parse_err)?
        {
            Some(label) => quad_str(out, &entry_iri, RDFS_LABEL, &label),
            None => quad_str(out, &entry_iri, RDFS_LABEL, term),
        }
        if let Some(def) = dataset
            .object_literal(term, SKOS_DEFINITION)
            .map_err(parse_err)?
        {
            quad_str(out, &entry_iri, SKOS_DEFINITION, &def);
        }
        // The three deontic-modality prose legs, each verbatim and honest-absent.
        for avoid in &prose.avoid_when {
            quad_str(out, &entry_iri, &format!("{GMEOW}adviceAvoidWhen"), avoid);
        }
        for use_when in &prose.use_when {
            quad_str(out, &entry_iri, &format!("{GMEOW}adviceUseWhen"), use_when);
        }
        for how in &prose.how_to_use {
            quad_str(out, &entry_iri, &format!("{GMEOW}adviceHowToUse"), how);
        }
    }
    Ok(())
}

/// Load the root ontology + every slice module into one frozen dataset (NO imports),
/// the source the term-enrichment resolves against. Mirrors
/// `frame_shapes::load_authored_no_imports`.
fn load_authored_no_imports(root: &Path) -> Result<Dataset, gmeow_errors::Diag> {
    let mut acc = purrdf::slice::rdf_query::DatasetAccumulator::new();
    let mut files = vec![root.join("ontology").join("gmeow.ttl")];
    files.extend(module_files(root)?);
    for path in files {
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        acc.add_turtle(&bytes, &path.display().to_string())
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: e.to_string(),
                })
            })?;
    }
    acc.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: e.to_string(),
        })
    })
}

/// The `logic:FindingCategory` individual IRI a seed's enforcement kind (and, for a
/// deep-reason outcome, its code suffix) resolves to. Every arm names a declared
/// `logic:FindingCategory` member.
fn finding_category_iri(seed: &RuleSeed) -> String {
    let local = match seed.enforcement {
        Enforcement::Discipline => "FindingModelingDisciplineViolation",
        Enforcement::Shacl | Enforcement::Parse => "FindingDataShapeViolation",
        Enforcement::Signature | Enforcement::Governance | Enforcement::Advisory => {
            "FindingPolicyWarning"
        }
        Enforcement::DeepReason => {
            if seed.code.ends_with(".inconsistent") {
                "FindingContradictionWitness"
            } else if seed.code.ends_with(".permitted-conflict") {
                "FindingPermittedEpistemicConflict"
            } else if seed.code.ends_with(".incomplete")
                || seed.code.ends_with(".skipped")
                || seed.code.ends_with(".unavailable")
            {
                "FindingIncompleteCheck"
            } else if seed.code.ends_with(".projection-loss") {
                "FindingProjectionLoss"
            } else if seed.code.ends_with(".unsupported-construct") {
                "FindingUnsupportedSemanticFeature"
            } else {
                "FindingPolicyWarning"
            }
        }
    };
    format!("{LOGIC}{local}")
}

/// The rule IRI a seed mints: a family seed gets a `rule/family/<slug>` IRI (with
/// the leading/trailing separator trimmed off the slug), a concrete seed a
/// `rule/<slug>` IRI.
fn rule_iri(seed: &RuleSeed) -> String {
    let slug = slugify(seed.code);
    if seed.family {
        let trimmed = slug.trim_matches('-');
        format!("{GMEOW}rule/family/{trimmed}")
    } else {
        format!("{GMEOW}rule/{slug}")
    }
}

/// The `gmeow:ruleSeverity` wire token: `binding` for an error-grade check,
/// `advisory` otherwise.
fn severity_token(severity: Severity) -> &'static str {
    if severity == Severity::Error {
        "binding"
    } else {
        "advisory"
    }
}

/// Escape a string to a valid N-Triples quoted-literal body (without the surrounding
/// quotes). Mirrors `provenance_graph::escape_literal`.
fn escape_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn quad_iri(out: &mut String, s: &str, p: &str, o: &str) {
    writeln!(out, "<{s}> <{p}> <{o}> <{CATALOG_GRAPH_IRI}> .").expect("write to String");
}

fn quad_str(out: &mut String, s: &str, p: &str, lit: &str) {
    writeln!(
        out,
        "<{s}> <{p}> \"{}\" <{CATALOG_GRAPH_IRI}> .",
        escape_literal(lit)
    )
    .expect("write to String");
}

fn quad_typed(out: &mut String, s: &str, p: &str, lit: &str, datatype: &str) {
    writeln!(
        out,
        "<{s}> <{p}> \"{}\"^^<{datatype}> <{CATALOG_GRAPH_IRI}> .",
        escape_literal(lit)
    )
    .expect("write to String");
}

/// Build the N-Quads document for the constraint catalog (every quad in
/// [`CATALOG_GRAPH_IRI`]). Deterministic: seeds arrive in registry order, resolved
/// term lists are sorted, and the whole document is re-sorted + deduped before it is
/// parsed and canonicalized.
fn build_catalog_nquads(dataset: &Dataset) -> Result<String, gmeow_errors::Diag> {
    // The frame-carrier classes (subjects of gmeow:requiresFrame), sorted — the
    // governed terms of the frame-completeness discipline.
    let mut frame_carriers = dataset
        .subject_object_iri_pairs(GMEOW_REQUIRES_FRAME)
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: e.to_string(),
            })
        })?
        .into_iter()
        .map(|(s, _)| s)
        .collect::<Vec<_>>();
    frame_carriers.sort();
    frame_carriers.dedup();
    // The verbatim skos:definition of gmeow:requiresFrame, if present.
    let requires_frame_def = dataset
        .object_literal(GMEOW_REQUIRES_FRAME, SKOS_DEFINITION)
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: e.to_string(),
            })
        })?;

    // The direct subclasses of logic:Relator, sorted — the governed terms of the
    // relator-mediation discipline (may be empty, then omitted).
    let mut relator_subclasses = dataset
        .subjects_with_object(
            "http://www.w3.org/2000/01/rdf-schema#subClassOf",
            LOGIC_RELATOR,
        )
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: e.to_string(),
            })
        })?;
    // Exclude a reflexive `logic:Relator rdfs:subClassOf logic:Relator` edge: the
    // governed terms are the proper subclasses, not the class itself.
    relator_subclasses.retain(|s| s != LOGIC_RELATOR);
    relator_subclasses.sort();
    relator_subclasses.dedup();
    let relator_def = dataset
        .object_literal(LOGIC_RELATOR, SKOS_DEFINITION)
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: e.to_string(),
            })
        })?;

    let mut out = String::new();
    for seed in all_rules() {
        let iri = rule_iri(&seed);
        // The self-describing instance skeleton the validator's data-graph accepts
        // (mirrors the diagnostics Finding projection: type + label + isDefinedBy +
        // boxABox), plus the rule facets.
        quad_iri(&mut out, &iri, RDF_TYPE, &format!("{GMEOW}ValidationRule"));
        quad_iri(&mut out, &iri, RDFS_IS_DEFINED_BY, CATALOG_GRAPH_IRI);
        quad_iri(
            &mut out,
            &iri,
            &format!("{GMEOW}graphBoxRole"),
            &format!("{GMEOW}boxABox"),
        );
        quad_str(&mut out, &iri, RDFS_LABEL, seed.code);
        quad_str(&mut out, &iri, &format!("{GMEOW}ruleCode"), seed.code);
        quad_typed(
            &mut out,
            &iri,
            &format!("{GMEOW}ruleHelpUri"),
            &help_uri_for(seed.code),
            XSD_ANY_URI,
        );
        quad_str(
            &mut out,
            &iri,
            &format!("{GMEOW}ruleSeverity"),
            severity_token(seed.default_severity),
        );
        quad_iri(
            &mut out,
            &iri,
            &format!("{GMEOW}ruleCategory"),
            &finding_category_iri(&seed),
        );
        // `gmeow:ruleRemediation` — the registry-authored rule-level fix prose. Honest
        // absence: a code on the remediation-allowlist (`RuleSeed.remediation ==
        // None`) emits no triple at all, never an empty/placeholder literal.
        if let Some(remediation) = seed.remediation {
            quad_str(
                &mut out,
                &iri,
                &format!("{GMEOW}ruleRemediation"),
                remediation,
            );
        }

        // ── Resolved enrichment (graph-sourced; OMIT when nothing resolves) ─────
        match seed.code {
            "discipline/frame-completeness" => {
                quad_iri(
                    &mut out,
                    &iri,
                    &format!("{LOGIC}formalizes"),
                    GMEOW_REQUIRES_FRAME,
                );
                for carrier in &frame_carriers {
                    quad_iri(&mut out, &iri, &format!("{GMEOW}appliesToTerm"), carrier);
                }
                if let Some(def) = &requires_frame_def {
                    quad_str(&mut out, &iri, SKOS_DEFINITION, def);
                }
            }
            "discipline/relator-mediation" => {
                quad_iri(&mut out, &iri, &format!("{LOGIC}formalizes"), LOGIC_RELATOR);
                for sub in &relator_subclasses {
                    quad_iri(&mut out, &iri, &format!("{GMEOW}appliesToTerm"), sub);
                }
                if let Some(def) = &relator_def {
                    quad_str(&mut out, &iri, SKOS_DEFINITION, def);
                }
            }
            _ => {}
        }
    }

    // ── Advice-catalog entries: the recommendation tier, projected from the
    // realized advice carriers and hung beneath the `advice.` family rule. Emitted
    // into the same buffer so they ride the same sort/dedup/canonical fold. ────────
    emit_advice_entries(&mut out, dataset)?;

    // Byte-stable regardless of emission order (canonicalization re-sorts anyway,
    // but keeping the intermediate deterministic makes the parse input stable).
    let mut lines: Vec<&str> = out.lines().collect();
    lines.sort_unstable();
    lines.dedup();
    let mut sorted = lines.join("\n");
    sorted.push('\n');
    Ok(sorted)
}

/// Render the committed constraint-catalog bytes: build the N-Quads, parse them, and
/// re-serialize as RDFC-1.0 canonical N-Quads (the SAME fold the superset gate
/// reconstructs from the carrier graph, so `file == fold` holds by construction).
pub fn render_constraint_catalog(root: &Path) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let dataset = load_authored_no_imports(root)?;
    let nq = build_catalog_nquads(&dataset)?;
    let ds = purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("parse constraint-catalog N-Quads: {e}"),
        })
    })?;
    crate::stages::superset::canonical_ntriples(&ds).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("canonicalize constraint-catalog: {e}"),
        })
    })
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `constraint-catalog` export-leaf stage.
pub struct ConstraintCatalogStage {
    consumes: Vec<String>,
}

impl ConstraintCatalogStage {
    /// Construct the stage. It consumes `stage-reason` so it runs after the reasoned
    /// closure is available (and, transitively, over the composed ontology it reads
    /// the governed terms from).
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-reason".to_string()],
        }
    }
}

impl Default for ConstraintCatalogStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for ConstraintCatalogStage {
    fn id(&self) -> &str {
        "stage-constraint-catalog"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "constraint_catalog.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // The governed-term enrichment reads the authored default graph (root
        // ontology + slice modules); declare them so a vocabulary edit that changes
        // a frame-carrier or a relator subclass busts the cache.
        let mut files = vec![root.join("ontology").join("gmeow.ttl")];
        files.extend(module_files(root)?);
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let bytes = render_constraint_catalog(input.root)?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(CONSTRAINT_CATALOG_RDF_PATH.to_string(), bytes);
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    fn authenticated_catalog() -> String {
        let root = repo_root();
        let artifacts = crate::fixture::stage_artifacts(&root, 1, "stage-constraint-catalog")
            .expect("authenticated constraint-catalog fixture");
        String::from_utf8(
            artifacts
                .get(CONSTRAINT_CATALOG_RDF_PATH)
                .expect("constraint-catalog artifact")
                .clone(),
        )
        .expect("constraint-catalog utf8")
    }

    #[test]
    fn catalog_fanout_iri_is_auto_derived() {
        // The declared graph IRI must equal what the superset helper derives from the
        // committed path, so the fold reconstructs the committed 4th column.
        assert_eq!(
            crate::stages::superset::rdf_fanout_graph_iri(CONSTRAINT_CATALOG_RDF_PATH).as_deref(),
            Some(CATALOG_GRAPH_IRI)
        );
    }

    #[test]
    fn every_registry_seed_becomes_a_rule() {
        let text = authenticated_catalog();
        for seed in all_rules() {
            let iri = rule_iri(&seed);
            assert!(
                text.contains(&format!("<{iri}> ")),
                "missing rule IRI for code {}",
                seed.code
            );
        }
        // The catalog is non-empty and every quad carries the fanout 4th column.
        assert!(text.contains(CATALOG_GRAPH_IRI));
        assert!(text.contains(&format!("{GMEOW}ValidationRule")));
    }

    #[test]
    fn frame_completeness_is_enriched_from_the_graph() {
        let text = authenticated_catalog();
        // The frame-completeness rule formalizes gmeow:requiresFrame and applies to
        // at least one frame-carrier class resolved from the authored ontology.
        let rule = format!("{GMEOW}rule/discipline-frame-completeness");
        assert!(text.contains(&format!(
            "<{rule}> <{LOGIC}formalizes> <{GMEOW_REQUIRES_FRAME}>"
        )));
        assert!(text.contains(&format!("<{rule}> <{GMEOW}appliesToTerm>")));
    }

    /// Task 7 Part B (adversary F2/N1): the projection producer
    /// The authenticated producer output emits a `gmeow:ruleRemediation` triple
    /// for EVERY enforced rule whose code is NOT on
    /// [`gmeow_validate::rule_catalog::REMEDIATION_ABSENT`], and NO such triple
    /// for a code that IS on the allowlist — the honest-absence twin. Falsifiable:
    /// if the `if let Some(remediation) = seed.remediation` guard in
    /// `build_catalog_nquads` were dropped (or a remediation were fabricated for
    /// an allowlisted code), this test fails.
    #[test]
    fn every_enforced_rule_carries_remediation_except_the_honest_absence_allowlist() {
        use gmeow_validate::rule_catalog::REMEDIATION_ABSENT;

        let nq = authenticated_catalog();

        let mut checked_present = 0usize;
        let mut checked_absent = 0usize;
        for seed in all_rules() {
            let iri = rule_iri(&seed);
            let prefix = format!("<{iri}> <{GMEOW}ruleRemediation> ");
            let has_remediation_triple = nq.lines().any(|line| line.starts_with(&prefix));
            if REMEDIATION_ABSENT.contains(&seed.code) {
                assert!(
                    !has_remediation_triple,
                    "honest-absence code {} (rule {iri}) must carry NO gmeow:ruleRemediation \
                     triple in the projection",
                    seed.code
                );
                checked_absent += 1;
            } else {
                assert!(
                    has_remediation_triple,
                    "enforced rule {iri} (code {}) must carry a gmeow:ruleRemediation triple \
                     in the projection",
                    seed.code
                );
                checked_present += 1;
            }
        }
        assert!(
            checked_absent > 0,
            "the honest-absence allowlist must cover at least one seed in this catalog"
        );
        assert!(
            checked_present > 0,
            "at least one enforced rule must carry a projected remediation"
        );
    }

    /// The advice-catalog projection emits one `gmeow:AdviceEntry`
    /// per governed term with a realized advice carrier (today `gmeow:Entity` and
    /// `gmeow:Event`), each hung beneath the `advice.` family rule and carrying the
    /// three deontic-modality prose legs. Falsifiable: if `emit_advice_entries` were
    /// dropped, or a term lost its realized carrier, this fails.
    #[test]
    fn advice_entries_are_projected_for_realized_carriers() {
        let nq = authenticated_catalog();
        for term in ["Entity", "Event"] {
            let entry = format!("{GMEOW}advice/{term}");
            assert!(
                nq.contains(&format!("<{entry}> <{RDF_TYPE}> <{GMEOW}AdviceEntry>")),
                "missing gmeow:AdviceEntry projection for {term}"
            );
            assert!(
                nq.contains(&format!(
                    "<{entry}> <{GMEOW}documentedByRule> <{GMEOW}rule/family/advice>"
                )),
                "AdviceEntry for {term} must hang beneath the advice family rule"
            );
            assert!(
                nq.contains(&format!("<{entry}> <{LOGIC}formalizes> <{GMEOW}{term}>")),
                "AdviceEntry for {term} must formalize its governed term"
            );
            assert!(
                nq.contains(&format!("<{entry}> <{GMEOW}adviceAvoidWhen>")),
                "AdviceEntry for {term} must carry its avoidWhen prohibition prose"
            );
            assert!(
                nq.contains(&format!("<{entry}> <{GMEOW}adviceUseWhen>")),
                "AdviceEntry for {term} must carry its useWhen permission prose"
            );
            assert!(
                nq.contains(&format!("<{entry}> <{GMEOW}adviceHowToUse>")),
                "AdviceEntry for {term} must carry its howToUse directive prose"
            );
        }
    }

    /// Prose-binding over a controlled graph: the projected advice fields are copied
    /// verbatim from their realized carriers and governed term. This exercises the
    /// producer logic without loading or rebuilding the repository corpus.
    #[test]
    fn advice_prose_is_projected_verbatim_from_a_synthetic_graph() {
        let dataset = Dataset::parse_turtle(
            br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .

gmeow:SyntheticTerm
    rdfs:label "Synthetic term" ;
    skos:definition "Synthetic definition" ;
    gmeow:howToUse "Use exactly this way" .

gmeow:syntheticAvoid a logic:Constraint ;
    logic:severity "Info" ;
    logic:adviceSourceField logic:ProseFieldAvoidWhen ;
    logic:formalizes gmeow:SyntheticTerm ;
    logic:message "Avoid exactly this case" .

gmeow:syntheticUse a logic:AdviceGuidance ;
    logic:adviceSourceField logic:ProseFieldUseWhen ;
    logic:formalizes gmeow:SyntheticTerm ;
    logic:message "Use exactly this case" .
"#,
            "synthetic advice graph",
        )
        .expect("parse synthetic advice graph");
        let advice = collect_advice(&dataset).expect("collect advice");
        let entity = format!("{GMEOW}SyntheticTerm");
        let prose = advice
            .get(&entity)
            .expect("synthetic term has realized advice");
        assert_eq!(
            prose.avoid_when,
            BTreeSet::from(["Avoid exactly this case".to_string()])
        );
        assert_eq!(
            prose.use_when,
            BTreeSet::from(["Use exactly this case".to_string()])
        );
        assert_eq!(
            prose.how_to_use,
            BTreeSet::from(["Use exactly this way".to_string()])
        );

        let projected = build_catalog_nquads(&dataset).expect("project synthetic advice");
        for literal in [
            "Avoid exactly this case",
            "Use exactly this case",
            "Use exactly this way",
        ] {
            assert!(
                projected.contains(literal),
                "missing projected prose {literal:?}"
            );
        }
    }

    /// The minted advice-entry slugs are distinct across governed terms (two terms
    /// whose local names collided would mint the same subject). `emit_advice_entries`
    /// asserts this at build time; this pins it at the collector level too.
    #[test]
    fn advice_entry_slugs_are_distinct() {
        let catalog = authenticated_catalog();
        let mut slugs = BTreeSet::new();
        let mut count = 0usize;
        for line in catalog
            .lines()
            .filter(|line| line.contains(&format!("<{RDF_TYPE}> <{GMEOW}AdviceEntry>")))
        {
            let subject = line.split_whitespace().next().expect("N-Quads subject");
            assert!(
                slugs.insert(subject.to_string()),
                "duplicate advice-entry subject {subject}"
            );
            count += 1;
        }
        assert!(
            count >= 2,
            "expected at least the Entity + Event authenticated advice entries, got {count}"
        );
    }
}

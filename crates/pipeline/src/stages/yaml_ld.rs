// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `yaml_ld` export leaf: RDF → YAML-LD-star / JSON-LD-star.
//!
//! Emits both the JSON-LD-star lead artifact and a deterministic YAML-LD-star
//! derivative, plus a small serialization-preservation ledger.
//!
//! The JSON-LD-star / YAML-LD-star CODEC now lives in the lowest crate the rdf /
//! validate / pipeline consumers share (`purrdf::native_codecs::jsonld`). The
//! production functions in this stage are thin wrappers over it; only the
//! stage-specific code (the stage entry, the preservation ledger, the build-time
//! round-trip gate) lives here.
//!
//! # Peak residency
//!
//! Both codec entry points ([`jsonld::serialize_dataset_to_jsonld`] /
//! [`jsonld::serialize_dataset_to_yamlld`]) build their own whole-carrier
//! intermediate and return the finished document as one `String`, so this leaf's
//! measured allocation peak is 8.37 GiB. gmeow cannot share the intermediate between
//! the two calls (purrdf's `build_ser_graph` is crate-private) and must not grow a
//! second serializer to work around it, so the stage instead declares
//! [`crate::node::SERIALIZATION_BUFFER_RESOURCE`] and serializes against the other
//! whole-dataset leaf. Retiring the peak rather than scheduling around it is a purrdf
//! change: either expose the built serialization graph so one build feeds both
//! documents, or give the codecs an incremental `io::Write` sink.

use std::collections::BTreeMap;
use std::sync::Arc;

use purrdf::RdfDataset;
use purrdf::native_codecs::jsonld;
use serde_json::Value;

use crate::node::{
    CachePolicy, SERIALIZATION_BUFFER_RESOURCE, Stage, StageInput, StageOutput, StageProduct,
};

/// Logical path of the JSON-LD-star artifact emitted by this stage.
pub const JSON_LD_PATH: &str = "dist/gmeow.jsonld";
/// Logical path of the YAML-LD-star artifact emitted by this stage.
pub const YAML_LD_PATH: &str = "dist/gmeow.yamlld";
/// Logical path of the serialization-preservation ledger.
pub const PRESERVATION_PATH: &str = "generated/metadata/preservation.json";

// The statement-metadata reification vocabulary is gmeow's OWN ontology surface
// (`gmeow:StatementMetadata` / `gmeow:qSubject…`, defined in the kernel/provenance/
// standpoint slices and SKOS-aligned). purrdf's JSON-LD-star codec is namespace-
// parametric (`StatementMetadataVocab`); gmeow supplies these gmeow: IRIs so the
// downcast emits the ontology's terms, not purrdf's neutral default.

/// GMEOW quoted subject property.
pub const GMEOW_QSUBJECT: &str = "https://blackcatinformatics.ca/gmeow/qSubject";
/// GMEOW quoted predicate property.
pub const GMEOW_QPREDICATE: &str = "https://blackcatinformatics.ca/gmeow/qPredicate";
/// GMEOW quoted object property (IRI / blank-node objects).
pub const GMEOW_QOBJECT: &str = "https://blackcatinformatics.ca/gmeow/qObject";
/// GMEOW quoted literal object property.
pub const GMEOW_QOBJECTLITERAL: &str = "https://blackcatinformatics.ca/gmeow/qObjectLiteral";
/// GMEOW statement-metadata class.
pub const GMEOW_STATEMENT_METADATA: &str = "https://blackcatinformatics.ca/gmeow/StatementMetadata";
/// RDF 1.2 reifier predicate (re-exported from the codec so the tests + downcast share
/// one definition).
pub use jsonld::RDF_REIFIES;

/// The gmeow-namespace reification vocab handed to purrdf's parametric downcast.
fn gmeow_statement_metadata_vocab() -> jsonld::StatementMetadataVocab<'static> {
    jsonld::StatementMetadataVocab {
        statement_metadata: GMEOW_STATEMENT_METADATA,
        q_subject: GMEOW_QSUBJECT,
        q_predicate: GMEOW_QPREDICATE,
        q_object: GMEOW_QOBJECT,
        q_object_literal: GMEOW_QOBJECTLITERAL,
    }
}

/// Map a JSON-LD/YAML-LD codec diagnostic onto a pipeline decode error.
fn codec_err(e: purrdf::RdfDiagnostic) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Decode {
        message: e.to_string(),
    })
}

/// The `yaml_ld` export-leaf stage.
pub struct YamlLdStage {
    consumes: Vec<String>,
    resources: Vec<String>,
}

impl YamlLdStage {
    /// Construct the stage; it consumes THIS run's snapshot fold.
    ///
    /// It requires [`SERIALIZATION_BUFFER_RESOURCE`]: both codec calls below build a
    /// whole-carrier intermediate and return a whole-document `String` (870 MB of
    /// JSON-LD-star + 673 MB of YAML-LD-star on the shipped corpus), so its measured
    /// peak allocation is 8.37 GiB — second only to `stage-export-export`, and fatal
    /// on a 16 GB runner if the two overlap. Mirrored by
    /// `gmeow:stage-export-yaml-ld gmeow:requiresResource
    /// gmeow:serializationBufferResource` in `slices/core/pipeline/module.ttl`; the
    /// loader HARD-fails on disagreement.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-snapshot".to_string()],
            resources: vec![SERIALIZATION_BUFFER_RESOURCE.to_string()],
        }
    }
}

impl Default for YamlLdStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for YamlLdStage {
    fn id(&self) -> &str {
        "stage-export-yaml-ld"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn resources(&self) -> &[String] {
        &self.resources
    }
    fn cache_policy(&self) -> CachePolicy {
        // Measured contribution: 1.555 GB serialized / ~79.5 s rebuild, with an
        // 8.37-GiB renderer peak. The whole-document pair is not a bounded cache unit.
        CachePolicy::Recompute
    }
    fn impl_version(&self) -> &str {
        // v2: adds deterministic YAML-LD-star output and the preservation ledger.
        "yaml_ld.jsonld_star.v2-yaml-ld"
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // THIS run's carrier dataset, read directly off the snapshot product's bundle
        // — no re-parse of the gmeow.gts bytes (GTS is exit-only).
        let dataset = crate::stages::carrier::snapshot_dataset(_input.upstream)?;
        let json = serialize_graph(dataset.as_ref())?;
        let yaml = serialize_graph_yaml(dataset.as_ref(), None)?;
        let preservation = preservation_ledger();
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(JSON_LD_PATH.to_string(), json.into_bytes());
        artifacts.insert(YAML_LD_PATH.to_string(), yaml.into_bytes());
        artifacts.insert(PRESERVATION_PATH.to_string(), preservation.into_bytes());
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
    }
}

/// Convert a sorted BTreeMap into a serde_json object value.
fn to_json_object(map: BTreeMap<String, Value>) -> Value {
    Value::Object(map.into_iter().collect())
}

/// Serialize the carrier dataset to a deterministic JSON-LD-star document (thin wrapper
/// over the first-party rdf codec).
pub fn serialize_graph(dataset: &RdfDataset) -> Result<String, gmeow_errors::Diag> {
    jsonld::serialize_dataset_to_jsonld(dataset).map_err(codec_err)
}

/// Serialize the carrier dataset to deterministic YAML-LD-star bytes (thin wrapper over
/// the first-party rdf codec).
///
/// The JSON-LD-star document is re-serialized to YAML with sorted keys, block style, no
/// anchors/aliases, and an explicit `@context`. The header carries a YAML
/// language-server schema reference.
pub fn serialize_graph_yaml(
    dataset: &RdfDataset,
    schema_url: Option<&str>,
) -> Result<String, gmeow_errors::Diag> {
    // purrdf is namespace-neutral: with no schema_url it stamps its own
    // `purrdf.schema.json` header. gmeow's bundled YAML-LD schema is
    // `gmeow.schema.json`, so default `None` to it (the consumer's schema).
    let schema_url = schema_url.or(Some(GMEOW_BUNDLED_SCHEMA));
    jsonld::serialize_dataset_to_yamlld(dataset, schema_url).map_err(codec_err)
}

/// gmeow's bundled YAML-LD language-server schema reference (resolves inside the
/// `gmeow.gts` snapshot as a bare member name).
pub const GMEOW_BUNDLED_SCHEMA: &str = "gmeow.schema.json";

/// Serialization-preservation ledger: records JSON-LD-star and YAML-LD-star as lossless.
pub(crate) fn preservation_ledger() -> String {
    // A deliberately simple, versioned JSON ledger. It is intentionally NOT
    // conflated with the logic-projection PreservationKind vocabulary.
    let mut map: BTreeMap<String, Value> = BTreeMap::new();
    let mut entry: BTreeMap<String, Value> = BTreeMap::new();
    entry.insert(
        "preservation".to_string(),
        Value::String("lossless".to_string()),
    );
    entry.insert("roundTrips".to_string(), Value::Bool(true));
    entry.insert(
        "note".to_string(),
        Value::String("RDF 1.2-star quoted triples and annotations round-trip through the JSON-LD-star / YAML-LD-star surface.".to_string()),
    );
    map.insert("json-ld-star".to_string(), to_json_object(entry.clone()));
    map.insert("yaml-ld-star".to_string(), to_json_object(entry));
    serde_json::to_string_pretty(&to_json_object(map))
        .expect("preservation ledger is serializable JSON")
}

/// Parse JSON-LD-star bytes into the native carrier [`RdfDataset`] (thin wrapper over
/// the first-party rdf codec).
pub fn parse_jsonld_star(json_bytes: &[u8]) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    jsonld::parse_jsonld(json_bytes, None).map_err(codec_err)
}

/// Convert a JSON-LD-star document to GMEOW statement-metadata N-Quads (thin wrapper
/// over the first-party rdf codec). The output contains no quoted triple terms, so it is
/// safe for the rdflib-compat up-projection lane.
pub fn jsonld_star_to_gmeow_statement_metadata_nquads(
    json_bytes: &[u8],
) -> Result<String, gmeow_errors::Diag> {
    jsonld::jsonld_to_statement_metadata_nquads(json_bytes, None, Some(&gmeow_statement_metadata_vocab()))
        .map_err(codec_err)
}

/// Convert YAML-LD-star bytes to JSON-LD-star JSON (thin wrapper over the first-party
/// rdf codec), hard-failing on YAML anchors/aliases (extended YAML is out of scope).
pub fn yaml_ld_star_to_json(yaml_bytes: &[u8]) -> Result<String, gmeow_errors::Diag> {
    jsonld::yamlld_to_jsonld(yaml_bytes).map_err(codec_err)
}

/// Downcast YAML-LD-star bytes to GMEOW statement-metadata N-Quads (thin wrapper over
/// the first-party rdf codec).
pub fn yaml_ld_star_to_gmeow_statement_metadata_nquads(
    yaml_bytes: &[u8],
) -> Result<String, gmeow_errors::Diag> {
    jsonld::yamlld_to_statement_metadata_nquads(yaml_bytes, None, Some(&gmeow_statement_metadata_vocab()))
        .map_err(codec_err)
}

/// Return an RDFC-1.0 canonical, deterministically sorted quad representation.
///
/// The build-time round-trip gate ([`roundtrip_isomorphic`]) and the tests share one
/// canonicalizer.
pub(crate) fn canonical_lines(dataset: &RdfDataset) -> Vec<String> {
    // Native full RDFC-1.0 over the FLATTENED carrier: `canonical_flat_nquads`
    // re-materializes the RDF 1.2 statement overlay to plain `rdf:reifies` / annotation
    // triples before canonicalizing.
    let canonical = purrdf::canonical_flat_nquads(dataset)
        .expect("RDFC-1.0 canonicalization of parsed dataset");
    let mut lines: Vec<String> = canonical.lines().map(str::to_owned).collect();
    lines.sort();
    lines
}

/// Parse N-Quads-star text into the native carrier [`RdfDataset`], preserving the
/// RDF 1.2 statement layer (quoted triple terms fold to the reifier table). Used by
/// [`roundtrip_isomorphic`].
fn dataset_from_nquads(nquads: &[u8]) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    // The native codec folds the RDF 1.2 statement layer to the IR reifier table at parse
    // time; `canonical_lines` un-folds it back to the equivalent flat `<reifier> rdf:reifies
    // <<( s p o )>>` rows (exact inverses), so the star structure the RDFC-1.0 canonical
    // comparison depends on is preserved.
    purrdf::parse_dataset(nquads, "application/n-quads", None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("parse N-Quads: {e}"),
        })
    })
}

/// Return whether `star_bytes` (format `"jsonld"`|`"yamlld"`) re-parses to a
/// dataset isomorphic (RDFC-1.0 canonical) to the original N-Quads-star input.
/// This is the Rust authority for the build-time serialization-isomorphism gate,
/// replacing the Python `_round_trip_star`.
pub fn roundtrip_isomorphic(
    original_nquads: &[u8],
    star_bytes: &[u8],
    format: &str,
) -> Result<bool, gmeow_errors::Diag> {
    let original = dataset_from_nquads(original_nquads)?;
    let roundtrip = match format {
        "jsonld" => parse_jsonld_star(star_bytes)?,
        "yamlld" => parse_jsonld_star(yaml_ld_star_to_json(star_bytes)?.as_bytes())?,
        other => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!("unknown star format {other:?}; expected 'jsonld' or 'yamlld'"),
            }));
        }
    };
    Ok(canonical_lines(&original) == canonical_lines(&roundtrip))
}

#[cfg(test)]
mod tests {
    use super::*;

    use purrdf::{
        BlankScope, RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm, RdfTextDirection,
        RdfTriple, TermId,
    };

    use std::path::PathBuf;
    use std::sync::Arc;

    // Literal datatype sentinels for the synthetic-fixture → dataset bridge.
    const RDF_DIR_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString";
    const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    // ── synthetic fixture model ─────────────────────────────────────────────────────
    //
    // A flat term-arena fixture shape (terms indexed by `usize`, `(s,p,o,g?)` quads, and
    // the RDF 1.2 reifier / annotation side-tables) that the serializer-fixture builders
    // construct as ground truth, then bridge to the native carrier [`RdfDataset`] via
    // [`synth_to_dataset`] before feeding the production `serialize_graph` entrypoint.
    // This replaces the former GTS-archive model test fixtures: GTS is exit-only and
    // the test fixtures must not depend on the archive model.

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum TermKind {
        Iri,
        Bnode,
        Literal,
    }

    #[derive(Clone, Debug)]
    struct Term {
        kind: TermKind,
        /// IRI string / blank-node label / literal lexical form.
        value: Option<String>,
        lang: Option<String>,
        direction: Option<String>,
        /// Datatype term id for literals (`None` = inferred xsd:string / rdf:langString).
        datatype: Option<usize>,
        /// Unused arena column kept for fixture-shape parity; never read.
        #[allow(dead_code)]
        reifier: Option<usize>,
    }

    /// `(s, p, o, graph?)` base-quad row.
    type SynthQuad = (usize, usize, usize, Option<usize>);
    /// `(reifier, (s, p, o), graph?)` reifier-binding row.
    type SynthReifier = (usize, (usize, usize, usize), Option<usize>);
    /// `(reifier, predicate, value, graph?)` annotation row.
    type SynthAnnotation = (usize, usize, usize, Option<usize>);

    #[derive(Clone, Default, Debug)]
    struct Graph {
        terms: Vec<Term>,
        quads: Vec<SynthQuad>,
        reifiers: Vec<SynthReifier>,
        annotations: Vec<SynthAnnotation>,
    }

    /// The datatype IRI of a literal fixture term (`""` for non-literals / unset).
    fn synth_datatype_iri(g: &Graph, term: &Term) -> String {
        match term.datatype {
            Some(dt) => g.terms[dt].value.clone().unwrap_or_default(),
            None => String::new(),
        }
    }

    /// Build the native carrier [`RdfDataset`] from a synthetic fixture graph, the native
    /// twin of the former `gmeow-gts` N-Quads bridge. Literal datatypes are resolved the
    /// way the carrier does (a directional-language literal stores `rdf:langString` + an
    /// out-of-band direction; a plain literal is `xsd:string`).
    fn synth_to_dataset(g: &Graph) -> Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        let mut ids: Vec<TermId> = Vec::with_capacity(g.terms.len());
        for term in &g.terms {
            let id = match term.kind {
                TermKind::Iri => builder.intern_iri(&term.value.clone().unwrap_or_default()),
                TermKind::Bnode => {
                    builder.intern_blank(&term.value.clone().unwrap_or_default(), BlankScope(0))
                }
                TermKind::Literal => {
                    let lexical = term.value.clone().unwrap_or_default();
                    let lit = match (&term.lang, &term.direction) {
                        (Some(lang), Some(dir)) => RdfLiteral {
                            lexical_form: lexical,
                            datatype: None,
                            language: Some(lang.clone()),
                            direction: Some(match dir.as_str() {
                                "ltr" => RdfTextDirection::Ltr,
                                "rtl" => RdfTextDirection::Rtl,
                                other => panic!("invalid direction {other}"),
                            }),
                        },
                        (Some(lang), None) => RdfLiteral::language_tagged(lexical, lang.clone()),
                        (None, _) => {
                            let dt = synth_datatype_iri(g, term);
                            if dt.is_empty() || dt == XSD_STRING {
                                RdfLiteral::simple(lexical)
                            } else {
                                RdfLiteral::typed(lexical, dt)
                            }
                        }
                    };
                    builder.intern_literal(lit)
                }
            };
            ids.push(id);
        }

        for &(s, p, o, gname) in &g.quads {
            builder.push_quad(ids[s], ids[p], ids[o], gname.map(|gi| ids[gi]));
        }
        for &(rid, (s, p, o), _gname) in &g.reifiers {
            let triple = builder.intern_triple(ids[s], ids[p], ids[o]);
            builder.push_reifier(ids[rid], triple);
        }
        for &(r, p, v, _gname) in &g.annotations {
            builder.push_annotation(ids[r], ids[p], ids[v]);
        }

        builder
            .freeze()
            .expect("synthetic fixture freeze into carrier dataset")
    }

    /// The flattened source-faithful quad stream of a carrier dataset: the RDF 1.2
    /// statement overlay (reifier bindings + annotations) is re-materialized to plain
    /// `rdf:reifies` / annotation quads.
    fn flat_quads(dataset: &RdfDataset) -> Vec<RdfQuad> {
        purrdf::flat_rdf_quads_from_dataset(dataset)
    }

    /// Assert no quad object is an RDF 1.2 quoted triple term (over the flattened
    /// carrier — the downcast output must be plain N-Quads).
    fn assert_no_triple_terms(dataset: &RdfDataset) {
        assert!(
            !flat_quads(dataset)
                .iter()
                .any(|q| matches!(q.object, RdfTerm::Triple(_))),
            "downcast output must contain no quoted triple terms"
        );
    }

    /// A language-tagged literal term.
    fn ox_lang_literal(lex: &str, lang: &str) -> RdfTerm {
        RdfTerm::literal(RdfLiteral::language_tagged(lex, lang))
    }

    /// A typed literal term.
    fn ox_typed_literal(lex: &str, datatype: &str) -> RdfTerm {
        RdfTerm::literal(RdfLiteral::typed(lex, datatype))
    }

    /// A directional language-tagged literal term.
    fn ox_dir_lang_literal(lex: &str, lang: &str, direction: RdfTextDirection) -> RdfTerm {
        RdfTerm::literal(RdfLiteral {
            lexical_form: lex.to_string(),
            datatype: None,
            language: Some(lang.to_string()),
            direction: Some(direction),
        })
    }

    fn iri_term(value: &str) -> Term {
        Term {
            kind: TermKind::Iri,
            value: Some(value.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        }
    }

    fn bnode_term(label: &str) -> Term {
        Term {
            kind: TermKind::Bnode,
            value: Some(label.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        }
    }

    fn literal_term(value: &str) -> Term {
        Term {
            kind: TermKind::Literal,
            value: Some(value.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        }
    }

    #[allow(dead_code)]
    fn lang_term(value: &str, lang: &str) -> Term {
        Term {
            kind: TermKind::Literal,
            value: Some(value.to_string()),
            datatype: None,
            lang: Some(lang.to_string()),
            direction: None,
            reifier: None,
        }
    }

    #[allow(dead_code)]
    fn dir_lang_term(value: &str, lang: &str, direction: &str) -> Term {
        Term {
            kind: TermKind::Literal,
            value: Some(value.to_string()),
            datatype: None,
            lang: Some(lang.to_string()),
            direction: Some(direction.to_string()),
            reifier: None,
        }
    }

    /// Parse N-Quads-star text into the native carrier dataset (native codec round-trip).
    fn parse_nquads(nq: &str) -> Arc<RdfDataset> {
        super::dataset_from_nquads(nq.as_bytes()).unwrap()
    }

    fn minimal_graph() -> Graph {
        let mut graph = Graph::default();
        // 0: subject
        graph.terms.push(iri_term("https://example.org/s"));
        // 1: predicate
        graph.terms.push(iri_term("https://example.org/p"));
        // 2: object
        graph.terms.push(iri_term("https://example.org/o"));
        // 3: reifier
        graph.terms.push(iri_term("https://example.org/r"));
        // 4: annotation predicate
        graph.terms.push(iri_term("https://example.org/ap"));
        // 5: annotation value
        graph.terms.push(literal_term("meta"));

        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2), None));
        graph.annotations.push((3, 4, 5, None));
        graph
    }

    /// The IRI lexical form of an IRI term.
    fn iri_str(term: &RdfTerm) -> &str {
        match term {
            RdfTerm::Iri(iri) => iri.as_str(),
            other => panic!("expected an IRI term, got {other:?}"),
        }
    }

    /// An IRI term.
    fn ox_named_node(iri: &str) -> RdfTerm {
        RdfTerm::iri(iri)
    }

    fn ox_simple_literal(lex: &str) -> RdfTerm {
        RdfTerm::literal(RdfLiteral::simple(lex))
    }

    fn ox_quoted_triple(s: RdfTerm, p: RdfTerm, o: RdfTerm) -> RdfTerm {
        let predicate = iri_str(&p).to_string();
        RdfTerm::triple(RdfTriple::new(s, predicate, o))
    }

    /// Normalize a term so a hand-built `RdfLiteral` compares equal to the carrier's
    /// fully-materialized literal. The carrier ALWAYS resolves a literal's datatype
    /// explicitly (a hand-built one may leave it `None`), and it stores a
    /// language-tagged literal as `rdf:langString` even when a base DIRECTION is
    /// present (the direction is carried out-of-band). So the canonical datatype is
    /// keyed off language presence ALONE, and a hand-built `rdf:dirLangString` is
    /// dropped to `rdf:langString` to match. Recurses into triple terms.
    fn normalize_term(term: &RdfTerm) -> RdfTerm {
        match term {
            RdfTerm::Literal(lit) => {
                let datatype = match &lit.datatype {
                    Some(dt) if dt == RDF_DIR_LANG_STRING => RDF_LANG_STRING.to_string(),
                    Some(dt) => dt.clone(),
                    None if lit.language.is_some() => RDF_LANG_STRING.to_string(),
                    None => XSD_STRING.to_string(),
                };
                RdfTerm::literal(RdfLiteral {
                    lexical_form: lit.lexical_form.clone(),
                    datatype: Some(datatype),
                    language: lit.language.clone(),
                    direction: lit.direction,
                })
            }
            RdfTerm::Triple(triple) => RdfTerm::triple(RdfTriple::new(
                normalize_term(&triple.subject),
                triple.predicate.clone(),
                normalize_term(&triple.object),
            )),
            other => other.clone(),
        }
    }

    /// Membership test over the flattened carrier quad stream (base quads + the
    /// re-materialized `rdf:reifies` / annotation rows). `predicate` is an IRI term.
    fn dataset_has(
        dataset: &RdfDataset,
        subject: &RdfTerm,
        predicate: &RdfTerm,
        object: &RdfTerm,
    ) -> bool {
        let pred_iri = iri_str(predicate);
        let want_subject = normalize_term(subject);
        let want_object = normalize_term(object);
        flat_quads(dataset).iter().any(|q| {
            normalize_term(&q.subject) == want_subject
                && q.predicate == pred_iri
                && normalize_term(&q.object) == want_object
        })
    }

    fn assert_no_gmeow_at_id_leak(dataset: &RdfDataset, json: &str) {
        use gmeow_ns::GMEOW_NS;
        let at_id = format!("{GMEOW_NS}@id");
        let quads = flat_quads(dataset);
        assert!(
            !quads.iter().any(|q| q.predicate == at_id),
            "gmeow:@id must not leak as a property triple: {json}"
        );
        assert!(
            !quads.iter().any(|q| {
                q.predicate.starts_with(GMEOW_NS)
                    && matches!(
                        &q.object,
                        RdfTerm::Iri(n) if n == "http://example.org/reifier"
                    )
            }),
            "reifier IRI must not appear as object of any gmeow-prefixed predicate: {json}"
        );
    }

    #[test]
    fn minimal_rdf12_roundtrips_through_carrier() {
        let graph = minimal_graph();
        let dataset = synth_to_dataset(&graph);
        let json = serialize_graph(dataset.as_ref()).expect("serialize");

        let expected = dataset;
        let actual = parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star");

        assert_eq!(
            canonical_nquads(&expected),
            canonical_nquads(&actual),
            "JSON-LD-star round-trip diverged from the carrier baseline"
        );
    }

    #[test]
    fn multiple_reifiers_on_same_triple_roundtrip() {
        // RDF 1.2 allows several distinct explicit reifiers for the same triple
        // content. The JSON-LD-star emitter serializes them as an @annotation array;
        // the parser must reconstruct each one.
        let mut graph = Graph::default();
        graph.terms.push(iri_term("https://example.org/s"));
        graph.terms.push(iri_term("https://example.org/p"));
        graph.terms.push(iri_term("https://example.org/o"));
        graph.terms.push(iri_term("https://example.org/r1"));
        graph.terms.push(iri_term("https://example.org/r2"));
        graph
            .terms
            .push(iri_term("https://example.org/accordingTo"));
        graph.terms.push(iri_term("https://example.org/source-a"));
        graph.terms.push(iri_term("https://example.org/confidence"));
        graph.terms.push(literal_term("0.9"));
        graph.terms.push(literal_term("0.7"));

        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2), None));
        graph.reifiers.push((4, (0, 1, 2), None));
        graph.annotations.push((3, 5, 6, None));
        graph.annotations.push((4, 7, 8, None));

        let dataset = synth_to_dataset(&graph);
        let json = serialize_graph(dataset.as_ref()).expect("serialize");
        let expected = dataset;
        let actual = parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star");

        assert_eq!(
            canonical_nquads(&expected),
            canonical_nquads(&actual),
            "multiple reifiers must round-trip through JSON-LD-star"
        );
    }

    #[test]
    fn serialization_is_byte_deterministic() {
        let graph = minimal_graph();
        let dataset = synth_to_dataset(&graph);
        let first = serialize_graph(dataset.as_ref()).expect("serialize first");
        let second = serialize_graph(dataset.as_ref()).expect("serialize second");
        assert_eq!(first, second, "JSON-LD output must be byte-deterministic");
    }

    #[test]
    fn directional_language_string_emits_direction() {
        // Build the carrier directly from RDF 1.2 directional-language N-Quads
        // (`"lex"@lang--ltr`) — the production path.
        let nq = b"<https://example.org/s> <https://example.org/p> \"hello\"@en--ltr .\n";
        let dataset = purrdf::dataset_from_bytes(nq, purrdf::NativeRdfFormat::NQuads)
            .expect("parse directional-language N-Quads into the carrier");

        let json = serialize_graph(dataset.as_ref()).expect("serialize");
        assert!(
            json.contains("\"@direction\": \"ltr\""),
            "directional language literal must emit @direction: {json}"
        );
        assert!(
            json.contains("\"@language\": \"en\""),
            "directional language literal must also emit @language: {json}"
        );
    }

    #[test]
    fn yaml_ld_is_byte_deterministic() {
        let graph = minimal_graph();
        let dataset = synth_to_dataset(&graph);
        let first = serialize_graph_yaml(dataset.as_ref(), None).expect("serialize first");
        let second = serialize_graph_yaml(dataset.as_ref(), None).expect("serialize second");
        assert_eq!(first, second, "YAML-LD output must be byte-deterministic");
    }

    /// Build a non-trivial graph through hash-map collections seeded with `seed`.
    ///
    /// The returned graph has the same RDF content regardless of seed, but the
    /// append order of terms, quads, reifiers, and annotations varies with the
    /// hash-map iteration order. This lets determinism tests prove that the
    /// serializer normalizes away any input-order dependency.
    fn build_nontrivial_graph_with_seed(seed: usize) -> Graph {
        use ahash::{AHashMap, RandomState};

        // Terms are collected in a seed-dependent map so their ids vary by seed.
        let mut term_inputs: AHashMap<&'static str, Term> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        term_inputs.insert("s", iri_term("https://example.org/s"));
        term_inputs.insert("p1", iri_term("https://example.org/p1"));
        term_inputs.insert("p2", iri_term("https://example.org/p2"));
        term_inputs.insert("o1", iri_term("https://example.org/o1"));
        term_inputs.insert("o2", dir_lang_term("bonjour", "fr", "rtl"));
        term_inputs.insert("r1", iri_term("https://example.org/r1"));
        term_inputs.insert("r2", iri_term("https://example.org/r2"));
        term_inputs.insert("ap", iri_term("https://example.org/ap"));
        term_inputs.insert("av1", literal_term("meta-one"));
        term_inputs.insert("av2", literal_term("meta-two"));
        term_inputs.insert("type", iri_term("https://example.org/SomeType"));
        term_inputs.insert("rdf_type", iri_term(RDF_TYPE));

        let mut graph = Graph::default();
        let mut term_idx: AHashMap<&'static str, usize> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        for (key, term) in term_inputs {
            let id = graph.terms.len();
            graph.terms.push(term);
            term_idx.insert(key, id);
        }

        // Quads are collected in a seed-dependent map so their row order varies by seed.
        let mut quad_inputs: AHashMap<&'static str, SynthQuad> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        quad_inputs.insert(
            "type",
            (term_idx["s"], term_idx["rdf_type"], term_idx["type"], None),
        );
        quad_inputs.insert("q1", (term_idx["s"], term_idx["p1"], term_idx["o1"], None));
        quad_inputs.insert("q2", (term_idx["s"], term_idx["p2"], term_idx["o2"], None));

        let mut quad_idx: AHashMap<&'static str, usize> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        for (key, quad) in quad_inputs {
            let id = graph.quads.len();
            graph.quads.push(quad);
            quad_idx.insert(key, id);
        }

        // Reifiers are collected in a seed-dependent map.
        let mut reifier_inputs: AHashMap<&'static str, (usize, &'static str)> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        reifier_inputs.insert("r1", (term_idx["r1"], "q1"));
        reifier_inputs.insert("r2", (term_idx["r2"], "q1"));

        let mut reifier_idx: AHashMap<&'static str, usize> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        for (key, (term_id, quad_key)) in reifier_inputs {
            let q = graph.quads[quad_idx[quad_key]];
            let id = graph.reifiers.len();
            graph.reifiers.push((term_id, (q.0, q.1, q.2), None));
            reifier_idx.insert(key, id);
        }

        // Annotations are collected in a seed-dependent map.
        let mut annotation_inputs: AHashMap<&'static str, (usize, usize, usize)> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        annotation_inputs.insert("a1", (term_idx["r1"], term_idx["ap"], term_idx["av1"]));
        annotation_inputs.insert("a2", (term_idx["r2"], term_idx["ap"], term_idx["av2"]));
        for (_, ann) in annotation_inputs {
            // Annotation rows carry an optional graph slot (`None` here).
            graph.annotations.push((ann.0, ann.1, ann.2, None));
        }

        graph
    }

    /// JSON-LD-star output is byte-identical even when the input graph is constructed
    /// through hash maps seeded with different values. The serializer orders every map
    /// and array deterministically, so output must not depend on input append order.
    #[test]
    fn hash_seed_determinism_jsonld_star() {
        let seed_a = 0x1111_1111_1111_1111_usize;
        let seed_b = 0x2222_2222_2222_2222_usize;
        let graph_a = build_nontrivial_graph_with_seed(seed_a);
        let graph_b = build_nontrivial_graph_with_seed(seed_b);

        // The input graphs must differ in append order; otherwise the test is not
        // exercising hash-seed normalization.
        assert_ne!(
            graph_a
                .terms
                .iter()
                .map(|t| t.value.clone())
                .collect::<Vec<_>>(),
            graph_b
                .terms
                .iter()
                .map(|t| t.value.clone())
                .collect::<Vec<_>>(),
            "seeds must produce different term append orders"
        );
        assert_ne!(
            graph_a.quads, graph_b.quads,
            "seeds must produce different quad append orders"
        );

        let json_a =
            serialize_graph(synth_to_dataset(&graph_a).as_ref()).expect("serialize graph A");
        let json_b =
            serialize_graph(synth_to_dataset(&graph_b).as_ref()).expect("serialize graph B");
        assert_eq!(
            json_a, json_b,
            "JSON-LD-star output must be identical under different hash-map seeds"
        );
    }

    /// YAML-LD-star output is byte-identical even when the input graph is constructed
    /// through hash maps seeded with different values.
    #[test]
    fn hash_seed_determinism_yaml_ld_star() {
        let seed_a = 0x1111_1111_1111_1111_usize;
        let seed_b = 0x2222_2222_2222_2222_usize;
        let graph_a = build_nontrivial_graph_with_seed(seed_a);
        let graph_b = build_nontrivial_graph_with_seed(seed_b);

        let yaml_a = serialize_graph_yaml(synth_to_dataset(&graph_a).as_ref(), None)
            .expect("serialize YAML-LD A");
        let yaml_b = serialize_graph_yaml(synth_to_dataset(&graph_b).as_ref(), None)
            .expect("serialize YAML-LD B");
        assert_eq!(
            yaml_a, yaml_b,
            "YAML-LD-star output must be identical under different hash-map seeds"
        );
    }

    #[test]
    fn yaml_ld_has_explicit_context_and_no_anchors() {
        let graph = minimal_graph();
        let yaml = serialize_graph_yaml(synth_to_dataset(&graph).as_ref(), None)
            .expect("serialize YAML-LD");
        assert!(
            yaml.contains("@context"),
            "YAML-LD must carry an explicit @context: {yaml}"
        );
        assert!(
            yaml.contains("@graph"),
            "YAML-LD must carry an explicit @graph: {yaml}"
        );
        // Anchor/alias tokens appear as whitespace-delimited `&id` or `*id`.
        assert!(
            !yaml
                .split_whitespace()
                .any(|t| t.starts_with('&') || t.starts_with('*')),
            "YAML-LD must not use anchors or aliases: {yaml}"
        );
        assert!(
            yaml.contains("yaml-language-server: $schema=gmeow.schema.json"),
            "YAML-LD must carry a language-server schema header pointing to the bundled schema: {yaml}"
        );
    }

    #[test]
    fn yaml_ld_roundtrips_through_carrier() {
        let graph = minimal_graph();
        let dataset = synth_to_dataset(&graph);
        let yaml = serialize_graph_yaml(dataset.as_ref(), None).expect("serialize YAML-LD");
        // The test parser works over JSON-LD-star; convert YAML back to JSON first.
        let yaml_value: serde_yaml::Value =
            serde_yaml::from_str(&yaml).expect("parse emitted YAML-LD");
        let json = serde_json::to_string(&yaml_value).expect("YAML -> JSON");

        let expected = dataset;
        let actual =
            parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star from YAML round-trip");

        assert_eq!(
            canonical_nquads(&expected),
            canonical_nquads(&actual),
            "YAML-LD round-trip diverged from the carrier baseline"
        );
    }

    #[test]
    fn annotation_reifier_explicit_id_on_node_object() {
        let mut graph = Graph::default();
        graph.terms.push(iri_term("http://example.org/s"));
        graph.terms.push(iri_term("http://example.org/p"));
        graph.terms.push(iri_term("http://example.org/o"));
        graph.terms.push(iri_term("http://example.org/reifier"));
        graph.terms.push(iri_term("http://example.org/confidence"));
        graph.terms.push(literal_term("0.9"));
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2), None));
        graph.annotations.push((3, 4, 5, None));

        let json = serialize_graph(synth_to_dataset(&graph).as_ref()).expect("serialize");
        let dataset = parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star");

        let s = ox_named_node("http://example.org/s");
        let p = ox_named_node("http://example.org/p");
        let o = ox_named_node("http://example.org/o");
        let reifier = ox_named_node("http://example.org/reifier");
        let reifies = ox_named_node(RDF_REIFIES);
        let confidence = ox_named_node("http://example.org/confidence");
        let meta = ox_simple_literal("0.9");
        let quoted = ox_quoted_triple(s.clone(), p.clone(), o.clone());

        assert!(dataset_has(&dataset, &s, &p, &o));
        assert!(dataset_has(&dataset, &reifier, &reifies, &quoted));
        assert!(dataset_has(&dataset, &reifier, &confidence, &meta));
        assert_no_gmeow_at_id_leak(&dataset, &json);
    }

    #[test]
    fn annotation_reifier_explicit_id_on_value_object() {
        let mut graph = Graph::default();
        graph.terms.push(iri_term("http://example.org/s"));
        graph.terms.push(iri_term("http://example.org/p"));
        graph.terms.push(literal_term("hello"));
        graph.terms.push(iri_term("http://example.org/reifier"));
        graph.terms.push(iri_term("http://example.org/confidence"));
        graph.terms.push(literal_term("0.9"));
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2), None));
        graph.annotations.push((3, 4, 5, None));

        let json = serialize_graph(synth_to_dataset(&graph).as_ref()).expect("serialize");
        let dataset = parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star");

        let s = ox_named_node("http://example.org/s");
        let p = ox_named_node("http://example.org/p");
        let o = ox_simple_literal("hello");
        let reifier = ox_named_node("http://example.org/reifier");
        let reifies = ox_named_node(RDF_REIFIES);
        let confidence = ox_named_node("http://example.org/confidence");
        let meta = ox_simple_literal("0.9");
        let quoted = ox_quoted_triple(s.clone(), p.clone(), o.clone());

        assert!(dataset_has(&dataset, &s, &p, &o));
        assert!(dataset_has(&dataset, &reifier, &reifies, &quoted));
        assert!(dataset_has(&dataset, &reifier, &confidence, &meta));
        assert_no_gmeow_at_id_leak(&dataset, &json);
    }

    #[test]
    fn annotation_reifier_blank_fallback() {
        let mut graph = Graph::default();
        graph.terms.push(iri_term("http://example.org/s"));
        graph.terms.push(iri_term("http://example.org/p"));
        graph.terms.push(iri_term("http://example.org/o"));
        graph.terms.push(bnode_term("r1"));
        graph.terms.push(iri_term("http://example.org/confidence"));
        graph.terms.push(literal_term("0.9"));
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2), None));
        graph.annotations.push((3, 4, 5, None));

        let json = serialize_graph(synth_to_dataset(&graph).as_ref()).expect("serialize");
        let dataset = parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star");

        let s = ox_named_node("http://example.org/s");
        let p = ox_named_node("http://example.org/p");
        let o = ox_named_node("http://example.org/o");
        let reifies_iri = RDF_REIFIES;
        let confidence = ox_named_node("http://example.org/confidence");
        let meta = ox_simple_literal("0.9");
        let quoted = ox_quoted_triple(s.clone(), p.clone(), o.clone());

        assert!(dataset_has(&dataset, &s, &p, &o));

        let flat = flat_quads(&dataset);
        let reifier_quads: Vec<&RdfQuad> = flat
            .iter()
            .filter(|q| q.predicate == reifies_iri && q.object == quoted)
            .collect();
        assert_eq!(
            reifier_quads.len(),
            1,
            "expected exactly one rdf:reifies quad for the base triple"
        );
        assert!(
            matches!(reifier_quads[0].subject, RdfTerm::BlankNode(_)),
            "blank reifier fallback must use a blank node subject: {json}"
        );
        assert!(dataset_has(
            &dataset,
            &reifier_quads[0].subject,
            &confidence,
            &meta
        ));
    }

    #[test]
    fn jsonld_star_downcast_to_gmeow_statement_metadata() {
        let mut graph = Graph::default();
        graph.terms.push(iri_term("http://example.org/s"));
        graph.terms.push(iri_term("http://example.org/p"));
        graph.terms.push(iri_term("http://example.org/o"));
        graph.terms.push(iri_term("http://example.org/r"));
        graph.terms.push(iri_term("http://example.org/confidence"));
        graph.terms.push(literal_term("0.9"));
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2), None));
        graph.annotations.push((3, 4, 5, None));

        let json = serialize_graph(synth_to_dataset(&graph).as_ref()).expect("serialize");
        let nquads = jsonld_star_to_gmeow_statement_metadata_nquads(json.as_bytes())
            .expect("downcast JSON-LD-star to GMEOW statement metadata");

        // The output must be parseable plain N-Quads (no quoted triple terms).
        let dataset = parse_nquads(&nquads);
        assert_no_triple_terms(&dataset);

        let s = ox_named_node("http://example.org/s");
        let p = ox_named_node("http://example.org/p");
        let o = ox_named_node("http://example.org/o");
        let r = ox_named_node("http://example.org/r");
        let rdf_type = ox_named_node(RDF_TYPE);
        let statement_metadata = ox_named_node(GMEOW_STATEMENT_METADATA);
        let q_subject = ox_named_node(GMEOW_QSUBJECT);
        let q_predicate = ox_named_node(GMEOW_QPREDICATE);
        let q_object = ox_named_node(GMEOW_QOBJECT);
        let confidence = ox_named_node("http://example.org/confidence");
        let meta = ox_simple_literal("0.9");

        // Base triple is preserved.
        assert!(
            dataset_has(&dataset, &s, &p, &o),
            "base triple must survive downcast"
        );

        // GMEOW statement-metadata skeleton is emitted for the reifier.
        assert!(
            dataset_has(&dataset, &r, &rdf_type, &statement_metadata),
            "reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(&dataset, &r, &q_subject, &s),
            "gmeow:qSubject must point to quoted subject"
        );
        assert!(
            dataset_has(&dataset, &r, &q_predicate, &p),
            "gmeow:qPredicate must point to quoted predicate"
        );
        assert!(
            dataset_has(&dataset, &r, &q_object, &o),
            "gmeow:qObject must point to quoted IRI object"
        );

        // Annotation triple on the reifier is preserved.
        assert!(
            dataset_has(&dataset, &r, &confidence, &meta),
            "annotation triple must survive downcast"
        );
    }

    #[test]
    fn jsonld_star_downcast_preserves_literal_object() {
        let mut graph = Graph::default();
        graph.terms.push(iri_term("http://example.org/s"));
        graph.terms.push(iri_term("http://example.org/p"));
        graph.terms.push(lang_term("hello", "en"));
        graph.terms.push(iri_term("http://example.org/r"));
        graph.terms.push(iri_term("http://example.org/confidence"));
        graph.terms.push(literal_term("0.95"));
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2), None));
        graph.annotations.push((3, 4, 5, None));

        let json = serialize_graph(synth_to_dataset(&graph).as_ref()).expect("serialize");
        let nquads = jsonld_star_to_gmeow_statement_metadata_nquads(json.as_bytes())
            .expect("downcast literal-valued JSON-LD-star");
        let dataset = parse_nquads(&nquads);

        let s = ox_named_node("http://example.org/s");
        let p = ox_named_node("http://example.org/p");
        let o = ox_lang_literal("hello", "en");
        let r = ox_named_node("http://example.org/r");
        let q_object_literal = ox_named_node(GMEOW_QOBJECTLITERAL);

        assert!(
            dataset_has(&dataset, &s, &p, &o),
            "base literal triple must survive"
        );
        assert!(
            dataset_has(&dataset, &r, &q_object_literal, &o),
            "gmeow:qObjectLiteral must be the literal object"
        );
    }

    #[test]
    fn jsonld_star_downcast_preserves_simple_literal_object() {
        let mut graph = Graph::default();
        graph.terms.push(iri_term("http://example.org/s"));
        graph.terms.push(iri_term("http://example.org/p"));
        graph.terms.push(literal_term("hello"));
        graph.terms.push(iri_term("http://example.org/r"));
        graph
            .terms
            .push(iri_term("https://blackcatinformatics.ca/gmeow/confidence"));
        graph.terms.push(literal_term("0.9"));
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2), None));
        graph.annotations.push((3, 4, 5, None));

        let json = serialize_graph(synth_to_dataset(&graph).as_ref()).expect("serialize");
        let nquads = jsonld_star_to_gmeow_statement_metadata_nquads(json.as_bytes())
            .expect("downcast simple-literal JSON-LD-star");

        // The output must be parseable plain N-Quads (no quoted triple terms).
        let dataset = parse_nquads(&nquads);
        assert_no_triple_terms(&dataset);

        let s = ox_named_node("http://example.org/s");
        let p = ox_named_node("http://example.org/p");
        let o = ox_simple_literal("hello");
        let r = ox_named_node("http://example.org/r");
        let rdf_type = ox_named_node(RDF_TYPE);
        let statement_metadata = ox_named_node(GMEOW_STATEMENT_METADATA);
        let q_subject = ox_named_node(GMEOW_QSUBJECT);
        let q_predicate = ox_named_node(GMEOW_QPREDICATE);
        let q_object_literal = ox_named_node(GMEOW_QOBJECTLITERAL);
        let confidence = ox_named_node("https://blackcatinformatics.ca/gmeow/confidence");
        let meta = ox_simple_literal("0.9");

        assert!(
            dataset_has(&dataset, &s, &p, &o),
            "base triple must survive"
        );
        assert!(
            dataset_has(&dataset, &r, &rdf_type, &statement_metadata),
            "reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(&dataset, &r, &q_subject, &s),
            "gmeow:qSubject must point to quoted subject"
        );
        assert!(
            dataset_has(&dataset, &r, &q_predicate, &p),
            "gmeow:qPredicate must point to quoted predicate"
        );
        assert!(
            dataset_has(&dataset, &r, &q_object_literal, &o),
            "gmeow:qObjectLiteral must point to quoted literal object"
        );
        assert!(
            dataset_has(&dataset, &r, &confidence, &meta),
            "annotation triple must survive downcast"
        );
    }

    #[test]
    fn jsonld_star_downcast_preserves_typed_literal_annotation() {
        // The Rust side cannot run the Python up-projection lane, so this test
        // verifies the prerequisite: the JSON-LD-star downcast emits native GMEOW
        // statement-metadata structural terms and preserves the typed annotation,
        // which is what lets the up-projection pass them through.
        let mut graph = Graph::default();
        graph.terms.push(iri_term("https://example.org/alice"));
        graph.terms.push(iri_term("https://schema.org/name"));
        graph.terms.push(literal_term("Alice"));
        graph
            .terms
            .push(iri_term("https://example.org/claim-alice-name"));
        graph
            .terms
            .push(iri_term("https://blackcatinformatics.ca/gmeow/confidence"));
        graph
            .terms
            .push(iri_term("http://www.w3.org/2001/XMLSchema#decimal"));
        graph.terms.push(Term {
            kind: TermKind::Literal,
            value: Some("0.9".to_string()),
            datatype: Some(5),
            lang: None,
            direction: None,
            reifier: None,
        });
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2), None));
        graph.annotations.push((3, 4, 6, None));

        let json = serialize_graph(synth_to_dataset(&graph).as_ref()).expect("serialize");
        let nquads = jsonld_star_to_gmeow_statement_metadata_nquads(json.as_bytes())
            .expect("downcast schema-org-like JSON-LD-star");

        let dataset = parse_nquads(&nquads);
        assert_no_triple_terms(&dataset);

        let alice = ox_named_node("https://example.org/alice");
        let schema_name = ox_named_node("https://schema.org/name");
        let alice_name = ox_simple_literal("Alice");
        let claim = ox_named_node("https://example.org/claim-alice-name");
        let rdf_type = ox_named_node(RDF_TYPE);
        let statement_metadata = ox_named_node(GMEOW_STATEMENT_METADATA);
        let q_subject = ox_named_node(GMEOW_QSUBJECT);
        let q_predicate = ox_named_node(GMEOW_QPREDICATE);
        let q_object_literal = ox_named_node(GMEOW_QOBJECTLITERAL);
        let confidence = ox_named_node("https://blackcatinformatics.ca/gmeow/confidence");
        let meta = ox_typed_literal("0.9", "http://www.w3.org/2001/XMLSchema#decimal");

        assert!(
            dataset_has(&dataset, &alice, &schema_name, &alice_name),
            "base triple must survive"
        );
        assert!(
            dataset_has(&dataset, &claim, &rdf_type, &statement_metadata),
            "reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(&dataset, &claim, &q_subject, &alice),
            "gmeow:qSubject must point to quoted subject"
        );
        assert!(
            dataset_has(&dataset, &claim, &q_predicate, &schema_name),
            "gmeow:qPredicate must point to quoted predicate"
        );
        assert!(
            dataset_has(&dataset, &claim, &q_object_literal, &alice_name),
            "gmeow:qObjectLiteral must point to quoted literal object"
        );
        assert!(
            dataset_has(&dataset, &claim, &confidence, &meta),
            "typed annotation triple must survive downcast"
        );
    }

    fn repo_root() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest
            .parent()
            .expect("workspace parent")
            .parent()
            .expect("repository root")
            .to_path_buf()
    }

    /// A hand-authored YAML-LD-star statement-layer fixture losslessly transpiles into
    /// GMEOW through the Rust native downcast path.
    #[test]
    fn hand_authored_yaml_ld_star_fixture_transpiles_to_gmeow() {
        let path = repo_root().join("slices/core/standpoint/examples/claim-bullshit.yamlld");
        let yaml = std::fs::read(&path).expect("read claim-bullshit.yamlld fixture");
        let nquads = yaml_ld_star_to_gmeow_statement_metadata_nquads(&yaml)
            .expect("downcast YAML-LD-star fixture to GMEOW statement metadata");

        let dataset = parse_nquads(&nquads);
        assert!(
            !flat_quads(&dataset)
                .iter()
                .any(|q| matches!(q.object, RdfTerm::Triple(_))),
            "transpiled output must contain no RDF 1.2 quoted triple terms"
        );

        let claim = ox_named_node("https://example.org/claim-001");
        let alice = ox_named_node("https://example.org/alice");
        let analyst = ox_named_node("https://example.org/analyst-standpoint");
        let bullshit = ox_named_node("https://blackcatinformatics.ca/gmeow/bullshit");

        let rdf_type = ox_named_node(RDF_TYPE);
        let standpoint_claim = ox_named_node(GMEOW_STATEMENT_METADATA);
        let claim_modality = ox_named_node("https://blackcatinformatics.ca/gmeow/claimModality");
        let observed_feature =
            ox_named_node("https://blackcatinformatics.ca/gmeow/observedFeature");
        let name = ox_named_node("https://blackcatinformatics.ca/gmeow/name");
        let q_subject = ox_named_node(GMEOW_QSUBJECT);
        let q_predicate = ox_named_node(GMEOW_QPREDICATE);
        let q_object = ox_named_node(GMEOW_QOBJECT);
        let q_object_literal = ox_named_node(GMEOW_QOBJECTLITERAL);
        let according_to = ox_named_node("https://blackcatinformatics.ca/gmeow/accordingTo");
        let confidence = ox_named_node("https://blackcatinformatics.ca/gmeow/confidence");
        let asserted_at = ox_named_node("https://blackcatinformatics.ca/gmeow/assertedAt");

        // Base triples survive.
        assert!(
            dataset_has(&dataset, &claim, &claim_modality, &bullshit),
            "claimModality base triple must survive transpile"
        );
        assert!(
            dataset_has(&dataset, &claim, &observed_feature, &alice),
            "observedFeature base triple must survive transpile"
        );

        // Directional language string is preserved on the base literal triple.
        let alice_name = ox_dir_lang_literal("Alice", "en", RdfTextDirection::Ltr);
        assert!(
            dataset_has(&dataset, &alice, &name, &alice_name),
            "directional language-tagged name must survive transpile"
        );

        // Explicit reifier for the claim modality is typed StatementMetadata and
        // carries the quoted subject/predicate/object skeleton.
        let claim_annotation = ox_named_node("https://example.org/claim-001-annotation");
        assert!(
            dataset_has(&dataset, &claim_annotation, &rdf_type, &standpoint_claim),
            "explicit reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(&dataset, &claim_annotation, &q_subject, &claim),
            "gmeow:qSubject must point to the claim"
        );
        assert!(
            dataset_has(&dataset, &claim_annotation, &q_predicate, &claim_modality),
            "gmeow:qPredicate must point to claimModality"
        );
        assert!(
            dataset_has(&dataset, &claim_annotation, &q_object, &bullshit),
            "gmeow:qObject must point to the IRI object"
        );

        // Annotation triples on the explicit reifier survive.
        assert!(
            dataset_has(&dataset, &claim_annotation, &according_to, &analyst),
            "accordingTo annotation must survive transpile"
        );
        let confidence_value = ox_typed_literal("0.65", "http://www.w3.org/2001/XMLSchema#decimal");
        assert!(
            dataset_has(&dataset, &claim_annotation, &confidence, &confidence_value),
            "confidence annotation must survive transpile"
        );
        let asserted_value = ox_typed_literal(
            "2026-06-05T00:00:00Z",
            "http://www.w3.org/2001/XMLSchema#dateTime",
        );
        assert!(
            dataset_has(&dataset, &claim_annotation, &asserted_at, &asserted_value),
            "assertedAt annotation must survive transpile"
        );

        // Explicit reifier for the directional-language name uses qObjectLiteral.
        let name_annotation = ox_named_node("https://example.org/alice-name-annotation");
        assert!(
            dataset_has(&dataset, &name_annotation, &rdf_type, &standpoint_claim),
            "name reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(&dataset, &name_annotation, &q_subject, &alice),
            "name gmeow:qSubject must point to alice"
        );
        assert!(
            dataset_has(&dataset, &name_annotation, &q_predicate, &name),
            "name gmeow:qPredicate must point to name"
        );
        assert!(
            dataset_has(&dataset, &name_annotation, &q_object_literal, &alice_name),
            "name gmeow:qObjectLiteral must point to the directional literal"
        );
    }

    /// A sample `@annotation` fragment shaped like serializer output must validate
    /// against the SHACL-derived JSON Schema `$defs/Annotation`.
    #[test]
    fn annotation_fragment_validates_against_json_schema() {
        use std::path::Path;

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("repo root");
        let schema_bytes = crate::fixture::authenticated_artifact(
            &root,
            "stage-export-json-schema",
            crate::stages::json_schema::JSON_SCHEMA_PATH,
        )
        .expect("load the producer-selected JSON Schema read-only");
        let mut schema: Value =
            serde_json::from_slice(&schema_bytes).expect("schema is valid JSON");

        // Validate a sample annotation object (the value inside `@annotation`) by
        // rooting the schema at `#/$defs/Annotation`.
        schema.as_object_mut().expect("schema is an object").insert(
            "$ref".to_string(),
            Value::String("#/$defs/Annotation".to_string()),
        );
        // Remove the anyOf at the root so the `$ref` is unambiguous.
        schema.as_object_mut().unwrap().remove("anyOf");
        schema.as_object_mut().unwrap().remove("properties");
        schema.as_object_mut().unwrap().remove("type");

        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(&schema)
            .expect("annotation subschema compiles");

        // Sample fragment mirroring the annotation objects the serializer emits:
        // typed-literal value objects (string `@value` + `@type`) and IRI objects.
        let fragment = serde_json::json!({
            "gmeow:confidence": {"@value": "0.9", "@type": "xsd:decimal"},
            "gmeow:accordingTo": {"@id": "http://example.org/source"},
            "gmeow:assertedAt": {"@value": "2026-06-05T00:00:00Z", "@type": "xsd:dateTime"}
        });

        let errors: Vec<String> = validator
            .iter_errors(&fragment)
            .map(|e| e.to_string())
            .collect();
        assert!(
            errors.is_empty(),
            "sample @annotation fragment must validate against the $defs/Annotation schema: {errors:?}"
        );
    }

    /// RDFC-1.0 canonical, sorted line representation of a carrier dataset, over the
    /// flattened (un-folded) star layer — the comparison key for the round-trip tests.
    fn canonical_nquads(dataset: &RdfDataset) -> String {
        canonical_lines(dataset).join("\n")
    }

    #[test]
    fn yaml_ld_star_ingest_rejects_anchors() {
        let anchored = "anchor: &a {x: 1}\nalias: *a\n";
        let err = yaml_ld_star_to_json(anchored.as_bytes())
            .expect_err("YAML anchors/aliases must hard-fail");
        assert!(
            err.is::<crate::error::Decode>(),
            "expected a Decode error, got {err:?}"
        );
    }

    #[test]
    fn roundtrip_isomorphic_accepts_emitted_jsonld() {
        let graph = minimal_graph();
        let dataset = synth_to_dataset(&graph);
        let json = serialize_graph(dataset.as_ref()).expect("serialize JSON-LD-star");
        let nquads = purrdf::serialize_dataset(
            dataset.as_ref(),
            "application/n-quads",
            purrdf::SerializeGraph::Dataset,
        )
        .expect("serialize fixture dataset to N-Quads");
        assert!(
            roundtrip_isomorphic(&nquads, json.as_bytes(), "jsonld")
                .expect("roundtrip_isomorphic for jsonld"),
            "emitted JSON-LD-star must round-trip isomorphic to the source N-Quads-star"
        );
    }

    #[test]
    fn roundtrip_isomorphic_accepts_emitted_yamlld() {
        let graph = minimal_graph();
        let dataset = synth_to_dataset(&graph);
        let yaml = serialize_graph_yaml(dataset.as_ref(), None).expect("serialize YAML-LD-star");
        let nquads = purrdf::serialize_dataset(
            dataset.as_ref(),
            "application/n-quads",
            purrdf::SerializeGraph::Dataset,
        )
        .expect("serialize fixture dataset to N-Quads");
        assert!(
            roundtrip_isomorphic(&nquads, yaml.as_bytes(), "yamlld")
                .expect("roundtrip_isomorphic for yamlld"),
            "emitted YAML-LD-star must round-trip isomorphic to the source N-Quads-star"
        );
    }

    /// The YAML-LD-star lift through
    /// `yaml_ld_star_to_gmeow_statement_metadata_nquads` produces a graph that is
    /// RDFC-1.0 canonically equal to the native Turtle (StatementMetadata) authoring of
    /// the same claim.
    ///
    /// Uses an explicit reifier `@id` so the downcast emits a stable IRI reifier (not a
    /// fresh blank node), allowing the Turtle counterpart to match exactly.
    #[test]
    fn yaml_ld_star_lift_equals_turtle_lift() {
        // ── 1. Minimal YAML-LD-star document ─────────────────────────────────
        // One base triple ex:s ex:p ex:o annotated on reifier ex:r with two
        // metadata predicates: gmeow:claimModality and gmeow:accordingTo.
        const YAML_DOC: &str = r#"
"@context":
  ex: "https://example.org/"
  gmeow: "https://blackcatinformatics.ca/gmeow/"
  xsd: "http://www.w3.org/2001/XMLSchema#"
"@graph":
  - "@id": "ex:s"
    "ex:p":
      "@id": "ex:o"
      "@annotation":
        "@id": "ex:r"
        "gmeow:claimModality":
          "@id": "gmeow:assertion"
        "gmeow:accordingTo":
          "@id": "ex:source1"
"#;

        // ── 2. Run the YAML-LD-star lift ──────────────────────────────────────
        let nquads = yaml_ld_star_to_gmeow_statement_metadata_nquads(YAML_DOC.as_bytes())
            .expect("YAML-LD-star lift must succeed");

        // ── 3. Guard: no RDF-1.2 quoted-triple terms in the output ────────────
        let yaml_lift = parse_nquads(&nquads);
        assert_no_triple_terms(&yaml_lift);

        // ── 4. Build the equivalent native Turtle (StatementMetadata) ─────────
        const TURTLE_DOC: &str = r#"
@prefix ex:    <https://example.org/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

ex:s ex:p ex:o .

ex:r a gmeow:StatementMetadata ;
     gmeow:qSubject   ex:s ;
     gmeow:qPredicate ex:p ;
     gmeow:qObject    ex:o ;
     gmeow:claimModality gmeow:assertion ;
     gmeow:accordingTo   ex:source1 .
"#;

        let turtle_lift = purrdf::parse_dataset(TURTLE_DOC.as_bytes(), "text/turtle", None)
            .expect("Turtle parse must succeed");

        // ── 5. RDFC-1.0 canonical equality: lift ≡ native Turtle ─────────────
        let yaml_lines = canonical_lines(&yaml_lift);
        let turtle_lines = canonical_lines(&turtle_lift);

        // Sanity: both graphs must be non-empty (guard against trivially-matching
        // empty datasets) and of the same size.
        assert!(
            !yaml_lines.is_empty(),
            "YAML-LD-star lift must produce at least one quad"
        );
        assert_eq!(
            yaml_lines.len(),
            turtle_lines.len(),
            "YAML-LD-star lift and native Turtle must have the same quad count"
        );

        assert_eq!(
            yaml_lines, turtle_lines,
            "YAML-LD-star lift must equal the native StatementMetadata Turtle authoring \
             (AC#5 lossless-into-GMEOW)"
        );
    }
}

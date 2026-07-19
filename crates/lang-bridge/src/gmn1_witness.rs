// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The per-claim inversion witness: the executed proof that `decode(encode(GMN-0))`
//! reproduces the canonical GMN-0 **per claim**, where a *claim* is a canonical-subject
//! group — every quad sharing one canonical subject in the RDFC-1.0 normal form.
//!
//! This STRENGTHENS (never replaces) [`crate::gmn1_codec::round_trip_check`], the
//! whole-model gate. On a canonically-equal round-trip every claim partition matches; the
//! added value is that on FAILURE this witness localizes the divergence to the offending
//! canonical subject, naming it in a typed [`Gmn1Error::PerClaimMismatch`].
//!
//! # The critical correctness rule (why the primary check never re-canonicalizes a claim)
//!
//! RDFC-1.0 blank-node labels (`_:c14nN`) are a GLOBAL function of the whole graph's
//! automorphisms. Encoding/decoding — or re-canonicalizing — a single claim's quads IN
//! ISOLATION would relabel its blanks and produce spurious mismatches. So the primary
//! witness ([`per_claim_round_trip_check`]):
//!
//! 1. Round-trips the WHOLE model ONCE (`gmn1_write` then `gmn1_read`).
//! 2. Canonicalizes BOTH the original and the reconstructed WHOLE model
//!    ([`Gmn0Model::canonical_nquads`](crate::gmn1_codec::Gmn0Model::canonical_nquads)).
//! 3. Partitions BOTH already-canonical N-Quads outputs by canonical subject
//!    ([`partition_by_subject`]) — the claim partition is TAKEN FROM the whole-model
//!    canonical output, never produced by re-canonicalizing a subject alone.
//! 4. blake3-digests each claim's partition and asserts original == reconstructed digests
//!    AND that the two claim-subject key-sets are equal.
//!
//! The ONE place an isolated round-trip is legitimate is the MSG-standalone leg
//! ([`per_claim_standalone_check`]), and only for claims that are ground or *blank-closed*
//! (their blanks appear in no other claim); claims whose blanks are shared across subjects
//! are honestly SKIPPED, never falsely failed.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

use crate::gmn1_codec::{
    Gmn0Model, Gmn1Document, Gmn1Error, GmnDictionary, gmn0_canonically_equal, gmn1_read,
    gmn1_write,
};

/// The `blake3:` algorithm tag every per-claim digest carries, matching the codec's digest
/// layer ([`crate::gmn1_digest`]) so this witness never introduces a second algorithm tag.
const ALGO_PREFIX: &str = "blake3:";

// ── The primary witness: whole-model canonicalize, then partition ──────────────────────

/// The per-claim inversion witness. Round-trips the WHOLE model once, canonicalizes both
/// sides, partitions each canonical N-Quads output by canonical subject, and asserts every
/// claim's partition digest (and the claim-subject key-set) agrees. Also discharges the
/// idempotence leg ([`idempotence_check`]): `encode(decode(gmn1)) == gmn1` byte-exact.
///
/// `Ok(())` iff every claim round-trips AND the encoding is idempotent; `Err` is a typed
/// [`Gmn1Error::PerClaimMismatch`] naming the offending canonical subject (or a typed
/// [`Gmn1Error::NonDecodableGrammar`] for an idempotence break).
///
/// # Errors
///
/// Propagates any [`gmn1_write`]/[`gmn1_read`] failure, returns
/// [`Gmn1Error::PerClaimMismatch`] on a claim divergence, and [`Gmn1Error::NonDecodableGrammar`]
/// on an idempotence break.
pub fn per_claim_round_trip_check(
    model: &Gmn0Model,
    dict: &GmnDictionary,
) -> Result<(), Gmn1Error> {
    // 1–2. Round-trip the WHOLE model once, then canonicalize both sides.
    let doc = gmn1_write(model, dict)?;
    let reconstructed = gmn1_read(&doc, dict)?;
    let original_nquads = model.canonical_nquads();
    let reconstructed_nquads = reconstructed.canonical_nquads();

    // 3–4. Partition the ALREADY-canonical whole-model outputs and compare per claim.
    compare_claim_partitions(&original_nquads, &reconstructed_nquads)?;

    // The idempotence leg: every value has exactly one spelling.
    idempotence_check(&doc, dict)?;

    Ok(())
}

/// The idempotence leg: `encode(decode(gmn1)) == gmn1` byte-exact (text AND reference
/// table, compared through [`Gmn1Document`]'s derived equality). A document the writer
/// produced must read back to a model that re-encodes to the SAME document — "every value
/// has exactly one spelling."
///
/// # Errors
///
/// Propagates the [`gmn1_read`]/[`gmn1_write`] failure, or returns
/// [`Gmn1Error::NonDecodableGrammar`] naming the idempotence break when the re-encoding
/// differs from the input document.
pub fn idempotence_check(doc: &Gmn1Document, dict: &GmnDictionary) -> Result<(), Gmn1Error> {
    let model = gmn1_read(doc, dict)?;
    let re_encoded = gmn1_write(&model, dict)?;
    if re_encoded == *doc {
        Ok(())
    } else {
        Err(Gmn1Error::NonDecodableGrammar {
            detail: format!(
                "idempotence break: encode(decode(gmn1)) != gmn1\n--- input ---\n{}\n--- re-encoded ---\n{}",
                doc.text, re_encoded.text
            ),
        })
    }
}

/// Compare two WHOLE-MODEL canonical N-Quads outputs claim-by-claim: partition each by
/// canonical subject, assert key-set equality, and assert per-claim digest equality. This
/// is the partition/compare primitive [`per_claim_round_trip_check`] drives with the real
/// codec's outputs; exposed so a witness harness (and this module's corruption test) can
/// drive it with any two canonical N-Quads strings.
///
/// The inputs MUST already be whole-model RDFC-1.0 canonical N-Quads — this function never
/// re-canonicalizes, so it never relabels a claim's blanks in isolation.
///
/// # Errors
///
/// Returns [`Gmn1Error::PerClaimMismatch`] naming the first canonical subject that is
/// present in one side but not the other, or whose partition digest disagrees.
pub fn compare_claim_partitions(
    original_nquads: &str,
    reconstructed_nquads: &str,
) -> Result<(), Gmn1Error> {
    let original = partition_by_subject(original_nquads);
    let reconstructed = partition_by_subject(reconstructed_nquads);

    // Key-set equality: a subject present on exactly one side is a mismatch that NAMES it.
    for subject in original.keys() {
        if !reconstructed.contains_key(subject) {
            return Err(Gmn1Error::PerClaimMismatch {
                subject: subject.clone(),
            });
        }
    }
    for subject in reconstructed.keys() {
        if !original.contains_key(subject) {
            return Err(Gmn1Error::PerClaimMismatch {
                subject: subject.clone(),
            });
        }
    }

    // Per-claim digest equality over the (canonically-ordered) partition lines.
    for (subject, lines) in &original {
        let other = &reconstructed[subject];
        if claim_digest(lines) != claim_digest(other) {
            return Err(Gmn1Error::PerClaimMismatch {
                subject: subject.clone(),
            });
        }
    }

    Ok(())
}

/// Partition whole-model canonical N-Quads into claims keyed by canonical subject: parse
/// each non-empty line's leading subject term (`<iri>`, `_:c14nN`, or an RDF-1.2 triple
/// term `<<( … )>>`) and group the lines by that subject string. The lines within a claim
/// are sorted (they arrive globally sorted, but sorting keeps the partition canonical
/// regardless of caller).
#[must_use]
pub fn partition_by_subject(nquads: &str) -> BTreeMap<String, Vec<String>> {
    let mut partition: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in nquads.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let subject = leading_subject(line).unwrap_or_else(|| {
            // A canonical N-Quads line always has a parseable subject; keep the codec
            // TOTAL (never a silent drop) by bucketing an unparsable line under its own
            // verbatim key, so a downstream digest mismatch still surfaces it.
            line.to_owned()
        });
        partition.entry(subject).or_default().push(line.to_owned());
    }
    for lines in partition.values_mut() {
        lines.sort();
    }
    partition
}

/// The leading subject term of a canonical N-Quads line, as its exact substring. Subjects
/// in N-Quads are IRIs (`<…>`, no unescaped `>` inside), blank nodes (`_:label` up to the
/// next whitespace), or RDF-1.2 triple terms (`<<( … )>>`, read balanced so nested spaces
/// do not truncate the subject). Returns `None` for a line whose first token is none of
/// these (never a valid canonical subject).
fn leading_subject(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    if line.starts_with("<<(") {
        // A balanced triple term: track `<<(` / `)>>` nesting depth.
        let mut depth = 0usize;
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i..].starts_with(b"<<(") {
                depth += 1;
                i += 3;
            } else if bytes[i..].starts_with(b")>>") {
                depth -= 1;
                i += 3;
                if depth == 0 {
                    return Some(line[..i].to_owned());
                }
            } else {
                i += 1;
            }
        }
        None
    } else if line.starts_with("_:") {
        let end = line.find(char::is_whitespace)?;
        Some(line[..end].to_owned())
    } else if line.starts_with('<') {
        // An IRIREF: no unescaped `>` inside, so the first `>` terminates it.
        let end = line.find('>')?;
        Some(line[..=end].to_owned())
    } else {
        None
    }
}

/// blake3 over a claim's canonically-ordered partition lines (each `'\n'`-joined),
/// returning `"blake3:<64-hex>"` — the same algorithm tag and lowercase-hex convention the
/// codec's [`content_digest`](crate::gmn1_digest::content_digest) uses.
#[must_use]
fn claim_digest(lines: &[String]) -> String {
    let joined = lines.join("\n");
    format!("{ALGO_PREFIX}{}", blake3::hash(joined.as_bytes()).to_hex())
}

// ── The MSG-standalone leg (the ONE legitimate isolated round-trip) ────────────────────

/// The report of [`per_claim_standalone_check`]: which claims were round-tripped ALONE
/// (ground or blank-closed) and which were SKIPPED (their blanks are shared across
/// subjects, so an isolated round-trip would relabel them and spuriously mismatch). Both
/// are keyed by the model-subject rendering (`<iri>`, `_:label`, or a triple term).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StandaloneReport {
    /// The claims whose quads were encoded/decoded IN ISOLATION and verified canonically
    /// equal, in sorted subject order.
    pub checked: Vec<String>,
    /// The claims SKIPPED because their blank nodes appear in another claim, in sorted
    /// subject order. Honestly scoped: the standalone leg cannot round-trip these alone
    /// without relabeling shared blanks, so it does not assert them.
    pub skipped: Vec<String>,
}

/// The MSG-standalone stronger check. For each claim that is **ground** (no blank nodes) or
/// **blank-closed** (its blank nodes appear in NO other claim), encode/decode THAT claim's
/// quads alone and assert [`gmn0_canonically_equal`]. Claims whose blanks are shared across
/// subjects are SKIPPED — the ONE place an isolated round-trip is legitimate is a
/// blank-closed component. Returns a [`StandaloneReport`] naming which claims were checked
/// versus skipped.
///
/// # Errors
///
/// Propagates any [`gmn1_write`]/[`gmn1_read`] failure for a checked claim, or returns
/// [`Gmn1Error::PerClaimMismatch`] naming a blank-closed claim whose isolated round-trip is
/// not canonically equal.
pub fn per_claim_standalone_check(
    model: &Gmn0Model,
    dict: &GmnDictionary,
) -> Result<StandaloneReport, Gmn1Error> {
    // Group the MODEL's quads by their (original) subject rendering.
    let mut claims: BTreeMap<String, Vec<RdfQuad>> = BTreeMap::new();
    for quad in &model.quads {
        claims
            .entry(term_key(&quad.subject))
            .or_default()
            .push(quad.clone());
    }

    // The set of blank labels each claim touches (subject / object / graph, recursing into
    // triple terms). The predicate is always an IRI, never a blank.
    let blanks_per_claim: BTreeMap<String, BTreeSet<String>> = claims
        .iter()
        .map(|(subject, quads)| {
            let mut blanks = BTreeSet::new();
            for quad in quads {
                collect_quad_blanks(quad, &mut blanks);
            }
            (subject.clone(), blanks)
        })
        .collect();

    let mut report = StandaloneReport::default();
    for (subject, quads) in &claims {
        let own_blanks = &blanks_per_claim[subject];
        // Shared iff some OTHER claim's blank set intersects this claim's. A ground claim
        // (empty blank set) is disjoint from everything, so it is never shared.
        let blanks_shared = blanks_per_claim
            .iter()
            .any(|(other, blanks)| other != subject && !blanks.is_disjoint(own_blanks));
        if blanks_shared {
            report.skipped.push(subject.clone());
            continue;
        }

        // Ground or blank-closed: round-trip this claim's quads ALONE and compare.
        let standalone = model_from_quads(quads);
        let doc = gmn1_write(&standalone, dict)?;
        let reconstructed = gmn1_read(&doc, dict)?;
        if !gmn0_canonically_equal(&standalone, &reconstructed) {
            return Err(Gmn1Error::PerClaimMismatch {
                subject: subject.clone(),
            });
        }
        report.checked.push(subject.clone());
    }

    Ok(report)
}

/// A canonical, sorted [`Gmn0Model`] over exactly `quads` — re-interned through a fresh
/// dataset so the model carries the SAME `from_dataset` sort/dedup discipline the whole
/// model does.
fn model_from_quads(quads: &[RdfQuad]) -> Gmn0Model {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        builder.push_owned_quad(quad);
    }
    let dataset = builder
        .freeze()
        .expect("a claim's valid RdfQuads freeze cleanly");
    Gmn0Model::from_dataset(&dataset)
}

/// The grouping key for a subject term: its canonical rendering (`<iri>`, `_:label`, or the
/// triple-term shorthand). Uses the term's own `Display`, the single source of truth in
/// `purrdf`.
fn term_key(term: &RdfTerm) -> String {
    term.to_string()
}

/// Collect every blank-node label a quad touches (subject, object, graph name), recursing
/// into RDF-1.2 triple terms.
fn collect_quad_blanks(quad: &RdfQuad, out: &mut BTreeSet<String>) {
    collect_term_blanks(&quad.subject, out);
    collect_term_blanks(&quad.object, out);
    if let Some(graph) = &quad.graph_name {
        collect_term_blanks(graph, out);
    }
}

/// Collect the blank-node labels reachable from a term (itself if blank, or the subject /
/// object of a nested triple term). A predicate is always an IRI, so a triple term's
/// predicate contributes no blank.
fn collect_term_blanks(term: &RdfTerm, out: &mut BTreeSet<String>) {
    match term {
        RdfTerm::BlankNode(label) => {
            out.insert(label.clone());
        }
        RdfTerm::Triple(triple) => {
            collect_term_blanks(&triple.subject, out);
            collect_term_blanks(&triple.object, out);
        }
        RdfTerm::Iri(_) | RdfTerm::Literal(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use purrdf::{RdfDataset, RdfLiteral, RdfQuad, RdfTerm, RdfTriple, parse_dataset};

    use super::*;
    use crate::gmn1_codec::GmnDictionary;

    const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

    fn empty_dict() -> GmnDictionary {
        GmnDictionary::default()
    }

    fn lang_module_dataset() -> Arc<RdfDataset> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../slices/grounding/lang/module.ttl"
        );
        let bytes = std::fs::read(path).expect("lang module.ttl is readable");
        parse_dataset(&bytes, "text/turtle", None).expect("lang module.ttl parses")
    }

    fn real_dict() -> GmnDictionary {
        GmnDictionary::from_dataset(&lang_module_dataset()).expect("dictionary loads")
    }

    fn iri(local: &str) -> RdfTerm {
        RdfTerm::Iri(format!("{GMEOW_NS}{local}"))
    }

    /// A multi-claim model with (a) two ground IRI claims, (b) a claim whose blank subject
    /// is closed within itself, and (c) two claims that SHARE a blank (an n-ary-reification
    /// idiom: a reifier blank referenced by two distinct subjects).
    fn multi_claim_model() -> Gmn0Model {
        let quads = vec![
            // Ground claim `gate1`.
            RdfQuad::new(iri("gate1"), format!("{GMEOW_NS}hasState"), iri("open")),
            // Ground claim `gate2`.
            RdfQuad::new(iri("gate2"), format!("{GMEOW_NS}hasState"), iri("closed")),
            // Blank-closed claim: `_:selfClosed` names only ground objects, referenced by
            // no other subject.
            RdfQuad::new(
                RdfTerm::blank_node("selfClosed"),
                format!("{GMEOW_NS}hasState"),
                iri("open"),
            ),
            // Two claims that SHARE the blank `_:shared`: `subjA` and `subjB` both point at
            // it, so neither is blank-closed.
            RdfQuad::new(
                iri("subjA"),
                format!("{GMEOW_NS}relatesTo"),
                RdfTerm::blank_node("shared"),
            ),
            RdfQuad::new(
                iri("subjB"),
                format!("{GMEOW_NS}relatesTo"),
                RdfTerm::blank_node("shared"),
            ),
            // The shared blank is itself a subject with a ground object.
            RdfQuad::new(
                RdfTerm::blank_node("shared"),
                format!("{GMEOW_NS}hasState"),
                iri("open"),
            ),
        ];
        model_from_quads(&quads)
    }

    #[test]
    fn per_claim_equality_holds_on_a_multi_claim_blank_bearing_model() {
        let model = multi_claim_model();
        let dict = empty_dict();
        // Sanity: the model genuinely carries a blank shared across subjects.
        assert!(
            model
                .quads
                .iter()
                .filter(|q| matches!(&q.object, RdfTerm::BlankNode(l) if l == "shared"))
                .count()
                >= 2,
            "fixture must exercise a blank shared across subjects"
        );
        per_claim_round_trip_check(&model, &dict)
            .expect("every claim round-trips, including the blank-bearing and blank-shared claims");
    }

    #[test]
    fn corrupted_single_claim_reds_only_that_claim() {
        // Two hand-built canonical N-Quads partitions differing in EXACTLY one subject's
        // object. This drives the partition/compare primitive directly (no real codec
        // mismatch is forcible — the codec round-trips), isolating the localization.
        let original = "\
<https://blackcatinformatics.ca/gmeow/gate1> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/open> .
<https://blackcatinformatics.ca/gmeow/gate2> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/closed> .
";
        // gate2's object is perturbed; gate1 is byte-identical.
        let reconstructed = "\
<https://blackcatinformatics.ca/gmeow/gate1> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/open> .
<https://blackcatinformatics.ca/gmeow/gate2> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/ajar> .
";
        let error = compare_claim_partitions(original, reconstructed)
            .expect_err("a perturbed claim must red the witness");
        assert_eq!(
            error,
            Gmn1Error::PerClaimMismatch {
                subject: "<https://blackcatinformatics.ca/gmeow/gate2>".to_owned(),
            },
            "the mismatch must NAME the offending canonical subject, and only it"
        );
        assert_eq!(
            error.failure_class(),
            "https://blackcatinformatics.ca/lang/GmnNonDecodableGrammar",
            "a per-claim mismatch reuses the whole-model round-trip class"
        );
    }

    #[test]
    fn missing_claim_subject_is_named() {
        // A subject present on one side only is a key-set mismatch that names it.
        let original = "\
<https://blackcatinformatics.ca/gmeow/gate1> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/open> .
<https://blackcatinformatics.ca/gmeow/gate2> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/closed> .
";
        let reconstructed = "\
<https://blackcatinformatics.ca/gmeow/gate1> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/open> .
";
        let error = compare_claim_partitions(original, reconstructed)
            .expect_err("a dropped claim must red the witness");
        assert_eq!(
            error,
            Gmn1Error::PerClaimMismatch {
                subject: "<https://blackcatinformatics.ca/gmeow/gate2>".to_owned(),
            }
        );
    }

    #[test]
    fn partition_reads_iri_blank_and_triple_term_subjects() {
        let nquads = "\
<https://blackcatinformatics.ca/gmeow/gate1> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/open> .
_:c14n0 <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/open> .
<<( <https://blackcatinformatics.ca/gmeow/s> <https://blackcatinformatics.ca/gmeow/p> <https://blackcatinformatics.ca/gmeow/o> )>> <https://blackcatinformatics.ca/gmeow/note> <https://blackcatinformatics.ca/gmeow/n1> .
";
        let partition = partition_by_subject(nquads);
        assert_eq!(partition.len(), 3, "three distinct canonical subjects");
        assert!(partition.contains_key("<https://blackcatinformatics.ca/gmeow/gate1>"));
        assert!(partition.contains_key("_:c14n0"));
        assert!(partition.contains_key(
            "<<( <https://blackcatinformatics.ca/gmeow/s> <https://blackcatinformatics.ca/gmeow/p> <https://blackcatinformatics.ca/gmeow/o> )>>"
        ));
    }

    #[test]
    fn idempotence_holds_on_a_by_reference_literal_document() {
        // A language-tagged literal rides BY REFERENCE (an `r_<hash>` token + a refs-table
        // payload), exercising the reference table in the idempotence comparison.
        let quads = vec![RdfQuad::new(
            iri("gate1"),
            format!("{GMEOW_NS}label"),
            RdfTerm::Literal(RdfLiteral::language_tagged("porte", "fr")),
        )];
        let model = model_from_quads(&quads);
        let dict = empty_dict();

        let doc = gmn1_write(&model, &dict).expect("by-reference document writes");
        assert!(
            doc.text.contains("r_"),
            "the langString must ride by reference (r_<hash> token): {}",
            doc.text
        );
        idempotence_check(&doc, &dict).expect("a writer-produced document re-encodes identically");
        // And the whole witness (which includes the idempotence leg) is green.
        per_claim_round_trip_check(&model, &dict)
            .expect("the by-reference model passes the full witness");
    }

    #[test]
    fn standalone_checks_ground_and_blank_closed_but_skips_shared() {
        let model = multi_claim_model();
        let dict = empty_dict();
        let report =
            per_claim_standalone_check(&model, &dict).expect("no blank-closed claim mis-inverts");

        // The ground claims and the self-closed blank claim are checked standalone.
        assert!(
            report
                .checked
                .contains(&"<https://blackcatinformatics.ca/gmeow/gate1>".to_owned()),
            "a ground claim round-trips standalone: {report:?}"
        );
        assert!(
            report.checked.contains(&"_:selfClosed".to_owned()),
            "a blank-closed claim round-trips standalone: {report:?}"
        );

        // The two subjects sharing `_:shared`, and the shared blank's own claim, are
        // SKIPPED — never falsely failed.
        for shared in [
            "<https://blackcatinformatics.ca/gmeow/subjA>",
            "<https://blackcatinformatics.ca/gmeow/subjB>",
            "_:shared",
        ] {
            assert!(
                report.skipped.contains(&shared.to_owned()),
                "a blank-shared claim must be SKIPPED, not checked: {shared} in {report:?}"
            );
            assert!(
                !report.checked.contains(&shared.to_owned()),
                "a blank-shared claim must NOT be checked standalone: {shared} in {report:?}"
            );
        }
    }

    #[test]
    fn standalone_round_trips_a_ground_reification_over_the_real_dictionary() {
        // A ground n-ary reification: a reifier IRI carrying an rdf:reifies triple term plus
        // a note. No blanks → every claim is ground → all checked standalone.
        let statement = RdfTriple::new(iri("gate1"), format!("{GMEOW_NS}hasState"), iri("open"));
        let quads = vec![
            RdfQuad::new(
                iri("reifier1"),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies",
                RdfTerm::triple(statement),
            ),
            RdfQuad::new(iri("reifier1"), format!("{GMEOW_NS}hasState"), iri("open")),
        ];
        let model = model_from_quads(&quads);
        let dict = real_dict();

        per_claim_round_trip_check(&model, &dict).expect("ground reification passes the witness");
        let report = per_claim_standalone_check(&model, &dict)
            .expect("ground reification inverts standalone");
        assert!(
            report.skipped.is_empty(),
            "no claim is blank-shared: {report:?}"
        );
        assert!(
            report
                .checked
                .contains(&"<https://blackcatinformatics.ca/gmeow/reifier1>".to_owned())
        );
    }

    #[test]
    fn partition_is_insensitive_to_input_line_order() {
        // Two byte-permutations of the same claim's lines partition and digest identically,
        // because the partition sorts each claim's lines into canonical order.
        let forward = partition_by_subject("_:x <p> <o1> .\n_:x <p> <o2> .\n");
        let reversed = partition_by_subject("_:x <p> <o2> .\n_:x <p> <o1> .\n");
        assert_eq!(forward, reversed, "partition is insensitive to line order");
        let (subject, lines) = forward.iter().next().expect("one claim");
        assert_eq!(subject, "_:x");
        assert_eq!(claim_digest(lines), claim_digest(&reversed[subject]));
    }
}

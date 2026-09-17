// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// A slice that ships no files at all — the right map for a graph-only axis test
/// (nothing here reads a slice-local file, so an empty map is the honest input,
/// not a stand-in for one).
fn no_files() -> BTreeMap<String, Vec<u8>> {
    BTreeMap::new()
}

/// The repo scoring environment anchored at `slice_dir` — the checkout path the
/// repo-scoped arms walk up from.
fn repo_env(slice_dir: &str) -> ScoringEnv {
    ScoringEnv::Repo {
        slice_dir: std::path::PathBuf::from(slice_dir),
    }
}

#[test]
fn resolve_maps_group_a_producers() {
    for key in IMPLEMENTED {
        assert!(resolve(key).is_some(), "{key} resolves to a primitive");
    }
    assert!(
        resolve("no_such_producer").is_none(),
        "unknown producer → None (hard fail upstream)"
    );
}

#[test]
fn advisory_constraint_terms_reads_info_constraints_only() {
    let ds = purrdf::parse_dataset(
            b"@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
              @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
              gmeow:FooAdvice a logic:Constraint ; logic:severity \"Info\" ; logic:formalizes gmeow:Foo .\n\
              gmeow:BarHard a logic:Constraint ; logic:severity \"Violation\" ; logic:formalizes gmeow:Bar .\n\
              gmeow:BazDefault a logic:Constraint ; logic:formalizes gmeow:Baz .\n",
            "text/turtle",
            None,
        )
        .expect("parse");
    let terms = advisory_constraint_terms(&ds);
    assert_eq!(
        terms.len(),
        1,
        "only the Info (advisory) constraint counts: {terms:?}"
    );
    assert!(terms.contains("https://blackcatinformatics.ca/gmeow/Foo"));
    assert!(
        !terms
            .iter()
            .any(|t| t.ends_with("Bar") || t.ends_with("Baz")),
        "a Violation (hard) or default-severity constraint is not advisory: {terms:?}"
    );
}

#[test]
fn advice_slice_prose_pins_the_source_language() {
    let slice = "https://blackcatinformatics.ca/gmeow/slices/testslice";
    let ttl = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             gmeow:Foo a owl:Class ; rdfs:isDefinedBy <{slice}> ; gmeow:avoidWhen \"avoid Foo\"@x-gmeow-english .\n\
             gmeow:Bar a owl:Class ; rdfs:isDefinedBy <{slice}> ; gmeow:useWhen \"use Bar\"@x-gmeow-english .\n\
             gmeow:Baz a owl:Class ; rdfs:isDefinedBy <{slice}> ; gmeow:avoidWhen \"avoid Baz\"@en .\n"
    );
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse");
    let files = no_files();
    let ctx = ScoreContext::new(slice.to_owned(), &files, &ds, repo_env("/tmp/testslice"));
    let prose = slice_advice_prose(&ctx);
    assert_eq!(
        prose.len(),
        2,
        "only the two @x-gmeow-english cells: {prose:?}"
    );
    assert!(prose.contains_key(&(
        "https://blackcatinformatics.ca/gmeow/Foo".to_owned(),
        ADVICE_AVOID_WHEN.to_owned()
    )));
    assert!(prose.contains_key(&(
        "https://blackcatinformatics.ca/gmeow/Bar".to_owned(),
        ADVICE_USE_WHEN.to_owned()
    )));
    assert!(
        !prose.keys().any(|(t, _)| t.ends_with("Baz")),
        "a non-source-language (@en) advisory literal must not enter the denominator"
    );
}

/// Repo-mode integration: proves the load-bearing visibility premise — the CENTRAL
/// logic-slice advisory constraints ARE seen when scoring a DOMAIN slice
/// (`ScoreContext.graph` is slice-local, so the axis MUST read the logic module off
/// the repo; if that read were broken the score would be a silent 0). Kernel authors
/// gmeow:Entity (avoidWhen + useWhen), which BareEntitySortalAdviceConstraint
/// formalizes, alongside other unharvested advice-prose terms — a real sub-1.0
/// fraction with an advisory for each uncovered cell.
#[test]
fn advice_coverage_axis_repo_sees_advisory_constraints() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root");
    let kernel_dir = repo.join("slices/core/kernel");
    let module = kernel_dir.join("module.ttl");
    let ds = crate::dataset_from_paths(&[module.as_path()]).expect("kernel module parses");
    let files = crate::report::slice_files_from_dir(&kernel_dir).expect("kernel slice files");
    let ctx = ScoreContext::new(
        "https://blackcatinformatics.ca/gmeow/slices/kernel".to_owned(),
        &files,
        &ds,
        ScoringEnv::Repo {
            slice_dir: kernel_dir,
        },
    );
    let result = advice_coverage_axis(&ctx);
    assert!(
        result.score > 0.0,
        "the central advisory constraints MUST be visible via the repo read — a 0.0 score \
             means the cross-slice constraint read is broken (silent-wrong), got {}",
        result.score
    );
    assert!(
        result.score < 1.0,
        "kernel still carries advice prose with no advisory constraint, so coverage is a \
             strict fraction, got {}",
        result.score
    );
    assert!(
        result
            .findings
            .iter()
            .any(|f| f.code == "slice-quality.advice-coverage.unharvested"),
        "each uncovered cell must surface an advisory to author the constraint"
    );
}

/// Per-cell coverage (Bundle mode, self-contained): an `avoidWhen` cell is covered ONLY by a
/// data-matching advisory `logic:Constraint`, a `useWhen` cell ONLY by a `logic:AdviceGuidance`
/// carrier. A term with only an avoidWhen constraint leaves its useWhen cell uncovered (and
/// vice versa), so a term needs BOTH to reach full coverage — the metric-coherence fix.
#[test]
fn advice_coverage_axis_is_per_cell_avoidwhen_vs_usewhen() {
    let slice = "https://blackcatinformatics.ca/gmeow/slices/testslice";
    // gmeow:Foo authors BOTH avoidWhen + useWhen and carries BOTH carriers → both cells covered.
    // gmeow:Bar authors BOTH but only an avoidWhen constraint → its useWhen cell is uncovered.
    let ttl = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             gmeow:Foo a owl:Class ; rdfs:isDefinedBy <{slice}> ;\n\
               gmeow:avoidWhen \"avoid Foo\"@x-gmeow-english ; gmeow:useWhen \"use Foo\"@x-gmeow-english .\n\
             gmeow:Bar a owl:Class ; rdfs:isDefinedBy <{slice}> ;\n\
               gmeow:avoidWhen \"avoid Bar\"@x-gmeow-english ; gmeow:useWhen \"use Bar\"@x-gmeow-english .\n\
             gmeow:FooAvoid a logic:Constraint ; logic:severity \"Info\" ; logic:formalizes gmeow:Foo .\n\
             gmeow:FooUse a logic:AdviceGuidance ; logic:formalizes gmeow:Foo .\n\
             gmeow:BarAvoid a logic:Constraint ; logic:severity \"Info\" ; logic:formalizes gmeow:Bar .\n"
    );
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse");
    // Bundle mode reads ctx.graph for BOTH per-field sets (the dict is unused here),
    // and carries NO directory at all — the point of the map-shaped context.
    let files = no_files();
    let ctx = ScoreContext::new(
        slice.to_owned(),
        &files,
        &ds,
        ScoringEnv::Bundle(std::sync::Arc::new(
            gmeow_lang_bridge::GmnDictionary::default(),
        )),
    );
    let result = advice_coverage_axis(&ctx);
    // 4 cells (Foo/Bar × avoidWhen/useWhen); covered: Foo.avoidWhen, Foo.useWhen, Bar.avoidWhen
    // = 3/4. Bar.useWhen is uncovered because no AdviceGuidance formalizes gmeow:Bar.
    assert!(
        (result.score - 0.75).abs() < 1e-9,
        "expected 3/4 per-cell coverage; got {}",
        result.score
    );
    assert!(
        result.findings.iter().any(|f| f
            .message
            .contains("gmeow:useWhen prose with no realized logic:AdviceGuidance")
            && f.message.contains("Bar")),
        "the uncovered Bar.useWhen cell must surface a logic:AdviceGuidance advisory: {:?}",
        result.findings
    );
}

/// The definitional harvest counts an enforcing realization and refuses a stub.
///
/// `Foo` is realized by a `Violation` constraint, `Baz` by a bare `logic:Formula`;
/// `Bar` carries only a reviewed-looking constraint with NO severity, which is an
/// unstated commitment and must not count. 2/3.
#[test]
fn harvest_coverage_counts_enforcing_realizations_only() {
    let slice = "https://blackcatinformatics.ca/gmeow/slices/testslice";
    let ttl = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             gmeow:Foo a owl:Class ; rdfs:isDefinedBy <{slice}> ; skos:definition \"a Foo\"@x-gmeow-english .\n\
             gmeow:Bar a owl:Class ; rdfs:isDefinedBy <{slice}> ; skos:definition \"a Bar\"@x-gmeow-english .\n\
             gmeow:Baz a owl:Class ; rdfs:isDefinedBy <{slice}> ; skos:definition \"a Baz\"@x-gmeow-english .\n\
             gmeow:FooLaw a logic:Constraint ; logic:severity \"Violation\" ; logic:formalizes gmeow:Foo .\n\
             gmeow:BarLaw a logic:Constraint ; logic:formalizes gmeow:Bar .\n\
             gmeow:BazLaw a logic:Formula ; logic:formalizes gmeow:Baz .\n"
    );
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse");
    let files = no_files();
    let ctx = ScoreContext::new(
        slice.to_owned(),
        &files,
        &ds,
        ScoringEnv::Bundle(std::sync::Arc::new(
            gmeow_lang_bridge::GmnDictionary::default(),
        )),
    );
    let result = harvest_coverage_axis(&ctx);
    assert!(
        (result.score - 2.0 / 3.0).abs() < 1e-9,
        "expected 2/3 (Foo enforcing, Baz formula, Bar severity-less); got {}",
        result.score
    );
    assert!(
        result
            .findings
            .iter()
            .any(|f| f.message.contains("Bar") && f.message.contains("no realized logic:")),
        "the severity-less Bar constraint must leave Bar surfaced as unharvested: {:?}",
        result.findings
    );
}

/// The two coverage axes must be independently reachable: an advisory harvest may
/// never credit the definitional one. A term whose ONLY realizations are an `Info`
/// constraint and a `logic:AdviceGuidance` scores 1.0 on advice coverage and 0.0
/// here — if this ever ties, one axis is inflating the other.
#[test]
fn harvest_coverage_is_disjoint_from_advice_coverage() {
    let slice = "https://blackcatinformatics.ca/gmeow/slices/testslice";
    let ttl = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             gmeow:Foo a owl:Class ; rdfs:isDefinedBy <{slice}> ;\n\
               skos:definition \"a Foo\"@x-gmeow-english ;\n\
               gmeow:avoidWhen \"avoid Foo\"@x-gmeow-english ; gmeow:useWhen \"use Foo\"@x-gmeow-english .\n\
             gmeow:FooAvoid a logic:Constraint ; logic:severity \"Info\" ; logic:formalizes gmeow:Foo .\n\
             gmeow:FooUse a logic:AdviceGuidance ; logic:formalizes gmeow:Foo .\n"
    );
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse");
    let files = no_files();
    let ctx = ScoreContext::new(
        slice.to_owned(),
        &files,
        &ds,
        ScoringEnv::Bundle(std::sync::Arc::new(
            gmeow_lang_bridge::GmnDictionary::default(),
        )),
    );
    assert!(
        (advice_coverage_axis(&ctx).score - 1.0).abs() < 1e-9,
        "both advisory cells carry their realized carriers"
    );
    assert!(
        harvest_coverage_axis(&ctx).score.abs() < 1e-9,
        "an Info constraint and an AdviceGuidance are advice, never a definitional harvest"
    );
}

/// A slice defining no terms is vacuously harvested, matching every other axis's
/// empty-population convention — an empty denominator is not a zero score.
#[test]
fn harvest_coverage_is_vacuous_without_definitions() {
    let slice = "https://blackcatinformatics.ca/gmeow/slices/testslice";
    let ttl = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             gmeow:Foo a owl:Class ; rdfs:isDefinedBy <{slice}> ; rdfs:label \"Foo\"@x-gmeow-english .\n"
    );
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse");
    let files = no_files();
    let ctx = ScoreContext::new(
        slice.to_owned(),
        &files,
        &ds,
        ScoringEnv::Bundle(std::sync::Arc::new(
            gmeow_lang_bridge::GmnDictionary::default(),
        )),
    );
    assert!((harvest_coverage_axis(&ctx).score - 1.0).abs() < 1e-9);
}

/// A reviewed, ACCEPTED deliberate non-assertion on `logic:ProseFieldDefinition` leaves the
/// denominator; a non-assertion on a DIFFERENT prose field does not.
///
/// `Foo` is realized. `Bar` is an accepted definition-field non-assertion → excluded, so the
/// score is 1/1, not 1/2. `Baz` carries a non-assertion on `ProseFieldAvoidWhen` — that is
/// the ADVICE axis's business and must NOT excuse its definition, so it stays an uncovered
/// cell. `Qux`'s non-assertion is still `CandidateProposed`, an unreviewed proposal that must
/// not buy a score. Final population {Foo, Baz, Qux}, covered {Foo} = 1/3.
#[test]
fn harvest_coverage_excuses_only_accepted_definition_non_assertions() {
    let slice = "https://blackcatinformatics.ca/gmeow/slices/testslice";
    let ttl = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             gmeow:Foo a owl:Class ; rdfs:isDefinedBy <{slice}> ; skos:definition \"Foo\"@x-gmeow-english .\n\
             gmeow:Bar a owl:Class ; rdfs:isDefinedBy <{slice}> ; skos:definition \"Bar\"@x-gmeow-english .\n\
             gmeow:Baz a owl:Class ; rdfs:isDefinedBy <{slice}> ; skos:definition \"Baz\"@x-gmeow-english .\n\
             gmeow:Qux a owl:Class ; rdfs:isDefinedBy <{slice}> ; skos:definition \"Qux\"@x-gmeow-english .\n\
             gmeow:FooLaw a logic:Formula ; logic:formalizes gmeow:Foo .\n\
             gmeow:BarNonAssertion a logic:FormalizationCandidate ;\n\
               logic:candidateDeliberateNonAssertion true ;\n\
               logic:candidateLifecycle logic:CandidateAccepted ;\n\
               logic:candidateSourceField logic:ProseFieldDefinition ;\n\
               logic:candidateFormalizes gmeow:Bar .\n\
             gmeow:BazNonAssertion a logic:FormalizationCandidate ;\n\
               logic:candidateDeliberateNonAssertion true ;\n\
               logic:candidateLifecycle logic:CandidateAccepted ;\n\
               logic:candidateSourceField logic:ProseFieldAvoidWhen ;\n\
               logic:candidateFormalizes gmeow:Baz .\n\
             gmeow:QuxNonAssertion a logic:FormalizationCandidate ;\n\
               logic:candidateDeliberateNonAssertion true ;\n\
               logic:candidateLifecycle logic:CandidateProposed ;\n\
               logic:candidateSourceField logic:ProseFieldDefinition ;\n\
               logic:candidateFormalizes gmeow:Qux .\n"
    );
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse");
    let files = no_files();
    let ctx = ScoreContext::new(
        slice.to_owned(),
        &files,
        &ds,
        ScoringEnv::Bundle(std::sync::Arc::new(
            gmeow_lang_bridge::GmnDictionary::default(),
        )),
    );
    let result = harvest_coverage_axis(&ctx);
    assert!(
        (result.score - 1.0 / 3.0).abs() < 1e-9,
        "only the accepted definition-field non-assertion leaves the denominator; expected \
             1/3, got {}",
        result.score
    );
    assert!(
        result.findings.iter().any(|f| f.code
            == "slice-quality.harvest-coverage.deliberate-non-assertion"
            && f.message.contains("Bar")),
        "the exclusion must be reported by name, never applied silently: {:?}",
        result.findings
    );
    for uncovered in ["Baz", "Qux"] {
        assert!(
            result
                .findings
                .iter()
                .any(|f| f.code == "slice-quality.harvest-coverage.unharvested"
                    && f.message.contains(uncovered)),
            "{uncovered} must stay an uncovered cell: {:?}",
            result.findings
        );
    }
}

/// Score a real on-disk slice through the `ScoringEnv::Repo` arm.
fn score_repo_slice(rel: &str, slice_iri: &str) -> AxisScore {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root");
    let slice_dir = repo.join(rel);
    let module = slice_dir.join("module.ttl");
    let ds = crate::dataset_from_paths(&[module.as_path()]).expect("slice module parses");
    let files = crate::report::slice_files_from_dir(&slice_dir).expect("slice files");
    let ctx = ScoreContext::new(
        slice_iri.to_owned(),
        &files,
        &ds,
        ScoringEnv::Repo { slice_dir },
    );
    harvest_coverage_axis(&ctx)
}

/// Repo-mode integration, the peer of `advice_coverage_axis_repo_sees_advisory_constraints`:
/// it exercises the `ScoringEnv::Repo` arm against the REAL logic module, so a wrong
/// `LOGIC_MODULE_REL` path or a `semantic_realization_terms` read that behaves differently
/// against real data surfaces here rather than only in an end-to-end CLI test.
///
/// It deliberately does NOT mirror the advice sibling's `score > 0.0` assertion on a domain
/// slice. The enforcing realizations in the central module formalize the logic slice's own
/// shape/schema terms (`logic:FreshnessGuardShape`, `logic:ActionSchema`,
/// `gmeow:GmeowClassShape`, …), so `slices/core/kernel` genuinely scores 0.0 today — the
/// corpus is unharvested, which is precisely what this axis exists to make visible. Asserting
/// a positive kernel score would pin a fiction. Instead the two arms are split: the grounding
/// slice proves realizations ARE found through the real read, and the domain slice proves the
/// read HAPPENED (no fail-closed escape) even though its honest answer is currently zero.
#[test]
fn harvest_coverage_axis_repo_reads_the_real_axiom_authority() {
    let grounding = score_repo_slice(
        "slices/grounding/logic",
        "https://blackcatinformatics.ca/gmeow/slices/logic",
    );
    assert!(
        grounding.score > 0.0,
        "the real logic module's enforcing logic:Constraint / logic:Formula realizations MUST \
             be found through the Repo read — a 0.0 here means the axiom read is broken \
             (silent-wrong), got {}",
        grounding.score
    );

    let kernel = score_repo_slice(
        "slices/core/kernel",
        "https://blackcatinformatics.ca/gmeow/slices/kernel",
    );
    assert!(
        kernel
            .findings
            .iter()
            .all(|f| f.code != "slice-quality.harvest-coverage.no-repo-root"
                && f.code != "slice-quality.harvest-coverage.no-axiom-source"),
        "scoring a real on-disk slice must genuinely MEASURE, never take a fail-closed \
             escape: {:?}",
        kernel.findings
    );
    assert!(
        kernel.score < 1.0,
        "kernel still carries defined terms with no realized axiom, so coverage is a strict \
             fraction, got {}",
        kernel.score
    );
    assert!(
        kernel
            .findings
            .iter()
            .any(|f| f.code == "slice-quality.harvest-coverage.unharvested"),
        "each uncovered term must surface an advisory naming the axiom to author"
    );
}

#[test]
fn boundary_detection() {
    assert!(states_boundary("A widget. It is NOT a gadget."));
    assert!(states_boundary("A relator, never a mere pair."));
    assert!(states_boundary("A process, as opposed to an endurant."));
    assert!(states_boundary("A quality, distinct from its bearer."));
    assert!(!states_boundary("A widget of the system."));
    // Incidental substrings must NOT pass through the ratchet: "whenever" is not
    // "never", "denote"/"NOTE" are not "not", "cannon" is not "cannot".
    assert!(!states_boundary(
        "Applies whenever a bearer exists; denote it clearly."
    ));
    assert!(!states_boundary(
        "A note about the cannon on the annotation."
    ));
    assert!(!states_boundary(
        "A widget. It is not an interchangeable alias for a broader, narrower, or merely related construct."
    ));
}

#[test]
fn worked_triple_detection() {
    assert!(is_worked_triple("ex:x a gmeow:Foo ."));
    assert!(is_worked_triple("ex:s ex:p ex:o ;"));
    assert!(!is_worked_triple("a plain sentence with no triple"));
    // A bare prose colon and a period is NOT a worked triple (old false positive).
    assert!(!is_worked_triple("See section 3: this is important."));
    // A full-IRI scheme colon is not a CURIE either.
    assert!(!is_worked_triple("visit http://example.org/ for details."));
    // Ownership metadata is not a worked use of the subject term.
    assert!(!is_worked_triple(
        "logic:Widget rdfs:isDefinedBy <https://example.org/slice> ."
    ));
}

#[test]
fn testing_corpus_excludes_comments_and_values_inventories() {
    let corpus = r#"
# logic:CommentOnly
ASK {
    VALUES ?term { logic:InventoryOnly logic:AlsoInventoryOnly }
    logic:ActuallyExercised rdfs:subClassOf ?term .
}
"#;
    let semantic = strip_non_executing_test_mentions(corpus);
    assert!(!word_at_boundary(&semantic, "CommentOnly"));
    assert!(!word_at_boundary(&semantic, "InventoryOnly"));
    assert!(!word_at_boundary(&semantic, "AlsoInventoryOnly"));
    assert!(word_at_boundary(&semantic, "ActuallyExercised"));
}

#[test]
fn generic_usage_coats_are_not_substantive_information() {
    assert!(is_generic_usage_coat(
        "Use logic:Widget when the modeled statement satisfies the scope and necessary conditions stated in this term's definition."
    ));
    assert!(is_generic_usage_coat(
        "Assert logic:Widget with its declared OWL kind and preserve its domain, range, standpoint, and provenance constraints."
    ));
    assert!(!is_generic_usage_coat(
        "Use logic:Widget for a rigid identity-bearing type whose instances remain Widgets in every accessible world."
    ));
}

#[test]
fn test_artifact_regex_is_valid() {
    // Forces the LazyLock initializer to run: proves the regex literal compiles
    // (so the `.expect` can never fire at runtime) and pins its intended matches.
    assert!(TEST_ARTIFACT.is_match("see test_foo_bar for evidence"));
    assert!(TEST_ARTIFACT.is_match("crates/foo.rs::bar"));
    assert!(TEST_ARTIFACT.is_match("tests/thing.py behaviour"));
    assert!(TEST_ARTIFACT.is_match("Mirrors the fixture"));
    assert!(!TEST_ARTIFACT.is_match("a genuine ontological rationale"));
}

#[test]
fn testing_axis_excludes_property_characteristic_assertion_carriers() {
    // A slice with two domain terms (one exercised by a test cell, one not) plus
    // two meta-level `logic:PropertyCharacteristicAssertion` carrier records. The
    // carriers must NOT count in the testing-axis denominator: with
    // them the score would be 1/4, without them it is 1/2.
    let ttl = "\
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
@prefix slice: <https://blackcatinformatics.ca/gmeow/slices/> .\n\
@prefix ex: <https://blackcatinformatics.ca/ex/> .\n\
ex:ExercisedTerm a owl:Class ; rdfs:isDefinedBy slice:demo .\n\
ex:UntestedTerm a owl:Class ; rdfs:isDefinedBy slice:demo .\n\
ex:CarrierOne a logic:PropertyCharacteristicAssertion ; rdfs:isDefinedBy slice:demo .\n\
ex:CarrierTwo a logic:PropertyCharacteristicAssertion ; rdfs:isDefinedBy slice:demo .\n";
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .expect("carrier-exclusion fixture parses as valid Turtle");

    // A test corpus that names only the exercised domain term (and, adversarially,
    // one carrier — which must still be excluded regardless of being "reached").
    // The cell is an ordinary map entry: the testing axis reads its corpus straight
    // out of `ctx.files`, so no temp directory is needed to exercise it at all —
    // which is also why the RAII `tempfile` guard `main` added here is gone rather
    // than kept: there is no scratch path left for a gate run to leak.
    let mut files = no_files();
    files.insert(
        "tests/competency.rq".to_owned(),
        b"ASK { ex:ExercisedTerm rdfs:subClassOf ex:CarrierOne . }\n".to_vec(),
    );

    let ctx = ScoreContext::new(
        "https://blackcatinformatics.ca/gmeow/slices/demo".to_owned(),
        &files,
        &ds,
        repo_env("/nonexistent/slices/demo"),
    );
    // slice_terms is untouched: it still sees all four typed, owned subjects.
    assert_eq!(ctx.terms.len(), 4, "slice_terms counts carriers globally");

    let score = testing_axis(&ctx);

    // Denominator excludes the two carriers → 2 scoreable domain terms, 1 reached.
    assert!(
        (score.score - 0.5).abs() < 1e-9,
        "carriers excluded from denominator: expected 1/2, got {}",
        score.score
    );
    // The one untested finding is the domain term, never a carrier.
    let msgs: Vec<&str> = score.findings.iter().map(|f| f.message.as_str()).collect();
    assert!(
        msgs.iter().any(|m| m.contains("UntestedTerm")),
        "the untested domain term is still flagged"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("Carrier")),
        "carriers are never flagged as untested"
    );
}

#[test]
fn word_boundary_rejects_incidental_substrings() {
    assert!(word_at_boundary("ex:Foo a owl:Class .", "Foo"));
    assert!(!word_at_boundary("ex:FooBar a owl:Class .", "Foo"));
    assert!(!word_at_boundary("prefixFoo", "Foo"));
    // Phrase cue spans a word gap but is boundary-checked at its ends.
    assert!(word_at_boundary("a, rather than b", "rather than"));
}

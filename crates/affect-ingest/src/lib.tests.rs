// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const TINY_CAPTURE: &str = concat!(
    "{\"model_identifier\":\"tiny\",\"model_revision\":\"r1\",",
    "\"model_framework\":\"transformers\",\"model_task\":\"text-classification\",",
    "\"function_to_apply\":\"sigmoid\",\"return_all_scores\":true,\"label_set_id\":\"GoEmotions\",",
    "\"score_semantics\":\"sigmoid\",\"threshold_policy\":{\"kind\":\"global\",\"value\":0.5},",
    "\"targets\":[{\"target_iri\":\"https://example.org/affect/chunk-1\",",
    "\"scores\":[{\"label\":\"joy\",\"score\":0.84},{\"label\":\"neutral\",\"score\":0.1}]}]}",
);

fn decode_capture(text: &str) -> Result<ClassifierRunCapture, DecodeError> {
    purrdf_lex::json::record::from_slice(text.as_bytes())
}

#[test]
fn json_capture_preserves_literal_bytes_and_real_put_get_round_trip() {
    let decoded = decode_capture(TINY_CAPTURE).unwrap();
    assert_eq!(
        purrdf_lex::json::write_compact(&decoded.to_json()),
        TINY_CAPTURE
    );
    assert_eq!(
        purrdf_lex::json::write_pretty(&decoded.to_json()),
        r#"{
  "model_identifier": "tiny",
  "model_revision": "r1",
  "model_framework": "transformers",
  "model_task": "text-classification",
  "function_to_apply": "sigmoid",
  "return_all_scores": true,
  "label_set_id": "GoEmotions",
  "score_semantics": "sigmoid",
  "threshold_policy": {
    "kind": "global",
    "value": 0.5
  },
  "targets": [
    {
      "target_iri": "https://example.org/affect/chunk-1",
      "scores": [
        {
          "label": "joy",
          "score": 0.84
        },
        {
          "label": "neutral",
          "score": 0.1
        }
      ]
    }
  ]
}"#
    );
    let cfg = config();
    let turtle = produce(&decoded, &cfg).unwrap();
    let recovered = recover(&turtle, &cfg).unwrap();
    assert_eq!(recovered, canonicalize(&decoded, &cfg));
    let encoded = purrdf_lex::json::write_pretty(&recovered.to_json());
    assert_eq!(decode_capture(&encoded).unwrap(), recovered);
    assert_eq!(
        produce(&decode_capture(&encoded).unwrap(), &cfg).unwrap(),
        produce(&recovered, &cfg).unwrap()
    );
}

#[test]
fn json_recovery_refuses_nonfinite_scores_and_thresholds_before_export() {
    let capture = decode_capture(TINY_CAPTURE).unwrap();
    let cfg = config();
    let turtle = produce(&capture, &cfg).unwrap();
    let index = index_turtle(turtle.as_bytes()).unwrap();
    for (finite, field) in [("0.84", "classifierScore"), ("0.5", "thresholdApplied")] {
        let predicate = g(field);
        let expected_subjects: Vec<_> = subjects(&index)
            .filter(|subject| first_literal(&index, subject, &predicate).as_deref() == Some(finite))
            .cloned()
            .collect();
        assert!(!expected_subjects.is_empty(), "missing source {predicate}");
        let quoted = format!("\"{finite}\"");
        assert!(turtle.contains(&quoted), "missing finite source {finite}");
        for invalid in ["NaN", "inf", "-inf", "1e400", "-1e400"] {
            let changed = turtle.replace(&quoted, &format!("\"{invalid}\""));
            let error = recover(&changed, &cfg).unwrap_err().to_string();
            assert!(
                error.contains("finite decimal"),
                "{finite} -> {invalid}: {error}"
            );
            assert!(error.contains(&predicate), "missing predicate: {error}");
            assert!(
                expected_subjects
                    .iter()
                    .any(|subject| error.contains(subject)),
                "missing subject: {error}"
            );
        }
    }
}

#[test]
fn json_unknown_members_are_ignored_but_known_duplicates_are_refused() {
    let baseline = decode_capture(TINY_CAPTURE).unwrap();
    let unknown = TINY_CAPTURE
        .replacen("{", "{\"unknown\":1,\"unknown\":2,", 1)
        .replace(
            "\"kind\":\"global\"",
            "\"kind\":\"global\",\"unknown\":null",
        )
        .replace(
            "\"score\":0.84",
            "\"score\":0.84,\"unknown\":[],\"unknown\":false",
        );
    assert_eq!(decode_capture(&unknown).unwrap(), baseline);
    for (old, new, pointer) in [
        (
            "\"model_revision\":\"r1\"",
            "\"model_revision\":\"r1\",\"model_revision\":\"r2\"",
            "/model_revision",
        ),
        (
            "\"kind\":\"global\"",
            "\"kind\":\"global\",\"kind\":\"global\"",
            "/threshold_policy/kind",
        ),
        (
            "\"value\":0.5",
            "\"value\":0.5,\"value\":0.6",
            "/threshold_policy/value",
        ),
        (
            "\"score\":0.84",
            "\"score\":0.84,\"score\":0.9",
            "/targets/0/scores/0/score",
        ),
        (
            "\"target_iri\":",
            "\"target_iri\":\"other\",\"target_iri\":",
            "/targets/0/target_iri",
        ),
    ] {
        let error = decode_capture(&TINY_CAPTURE.replacen(old, new, 1))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("duplicate field") && error.contains(pointer),
            "{error}"
        );
    }
    let duplicate_optional = TINY_CAPTURE.replacen(
        "{",
        "{\"tokenizer_revision\":null,\"tokenizer_revision\":null,",
        1,
    );
    assert!(
        decode_capture(&duplicate_optional)
            .unwrap_err()
            .to_string()
            .contains("duplicate field")
    );
}

#[test]
fn json_optional_members_accept_absent_or_null_and_omit_none() {
    let absent = decode_capture(TINY_CAPTURE).unwrap();
    let nulls = TINY_CAPTURE.replacen(
        "{",
        concat!(
            "{\"score_calibration\":null,\"tokenizer_revision\":null,\"label_set_revision\":null,",
            "\"hypothesis_template\":null,\"candidate_labels\":null,"
        ),
        1,
    );
    assert_eq!(decode_capture(&nulls).unwrap(), absent);
    assert_eq!(
        purrdf_lex::json::write_compact(&decode_capture(&nulls).unwrap().to_json()),
        TINY_CAPTURE
    );
    let mut present = absent;
    present.score_calibration = Some("profile".to_owned());
    present.tokenizer_revision = Some("tok".to_owned());
    present.label_set_revision = Some("labels".to_owned());
    present.hypothesis_template = Some("This expresses {}.".to_owned());
    present.candidate_labels = Some(vec!["joy".to_owned()]);
    let encoded = purrdf_lex::json::write_compact(&present.to_json());
    assert!(encoded.ends_with(concat!(
        ",\"score_calibration\":\"profile\",\"tokenizer_revision\":\"tok\",",
        "\"label_set_revision\":\"labels\",\"hypothesis_template\":\"This expresses {}.\",\"candidate_labels\":[\"joy\"]}"
    )));
    assert_eq!(decode_capture(&encoded).unwrap(), present);
    let invalid = TINY_CAPTURE.replacen("{", "{\"candidate_labels\":false,", 1);
    assert!(
        decode_capture(&invalid)
            .unwrap_err()
            .to_string()
            .contains("/candidate_labels")
    );
}

#[test]
fn json_threshold_tags_maps_and_score_vocabulary_preserve_their_wire_forms() {
    for (text, expected) in [
        ("sigmoid", ScoreSemantics::Sigmoid),
        ("softmax", ScoreSemantics::Softmax),
        (
            "calibrated_probability",
            ScoreSemantics::CalibratedProbability,
        ),
        ("entailment", ScoreSemantics::Entailment),
        ("logit", ScoreSemantics::Logit),
        ("margin", ScoreSemantics::Margin),
    ] {
        assert_eq!(
            ScoreSemantics::from_json(&Value::from(text)).unwrap(),
            expected
        );
        assert_eq!(expected.to_json(), Value::from(text));
    }
    let text =
        br#"{"kind":"per_label","thresholds":{"neutral":0.1,"joy":0.2,"joy":0.5},"ignored":true}"#;
    let policy: ThresholdPolicy = purrdf_lex::json::record::from_slice(text).unwrap();
    assert_eq!(
        purrdf_lex::json::write_compact(&policy.to_json()),
        r#"{"kind":"per_label","thresholds":{"joy":0.5,"neutral":0.1}}"#
    );
    let mut decoded = decode_capture(TINY_CAPTURE).unwrap();
    decoded.threshold_policy = policy;
    let cfg = config();
    let recovered = recover(&produce(&decoded, &cfg).unwrap(), &cfg).unwrap();
    assert_eq!(recovered, canonicalize(&decoded, &cfg));
    assert_eq!(
        decode_capture(&purrdf_lex::json::write_compact(&recovered.to_json())).unwrap(),
        recovered
    );
    for invalid in [
        r#"{"kind":"other"}"#,
        r#"{"value":0.5}"#,
        r#"{"kind":"global","value":null}"#,
        r#"{"kind":"per_label","thresholds":[]}"#,
    ] {
        assert!(
            purrdf_lex::json::record::from_slice::<ThresholdPolicy>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn json_native_record_and_enum_admission_refuses_incidental_serde_forms() {
    for text in ["[]", "null", "false"] {
        let error = decode_capture(text).unwrap_err().to_string();
        assert!(error.contains("struct ClassifierRunCapture"), "{error}");
    }
    let error = purrdf_lex::json::record::from_slice::<LabelScore>(br#"["joy",0.5]"#)
        .unwrap_err()
        .to_string();
    assert!(error.contains("struct LabelScore"), "{error}");
    assert!(
        purrdf_lex::json::record::from_slice::<TargetInput>(br#"["https://example.org/x",[]]"#)
            .unwrap_err()
            .to_string()
            .contains("struct TargetInput")
    );
    assert!(
        purrdf_lex::json::record::from_slice::<ThresholdPolicy>(br#"["global",0.5]"#)
            .unwrap_err()
            .to_string()
            .contains("struct ThresholdPolicy")
    );
    let singleton_map = TINY_CAPTURE.replace(
        "\"score_semantics\":\"sigmoid\"",
        "\"score_semantics\":{\"sigmoid\":null}",
    );
    let error = decode_capture(&singleton_map).unwrap_err().to_string();
    assert!(
        error.contains("ScoreSemantics") && error.contains("/score_semantics"),
        "{error}"
    );
}

#[test]
fn json_numeric_admission_is_finite_and_typed_with_canonical_binary64_output() {
    for replacement in ["1e400", "null", "\"0.84\""] {
        let text = TINY_CAPTURE.replace("\"score\":0.84", &format!("\"score\":{replacement}"));
        let error = decode_capture(&text).unwrap_err().to_string();
        assert!(error.contains("/targets/0/scores/0/score"), "{error}");
        let text = TINY_CAPTURE.replace("\"value\":0.5", &format!("\"value\":{replacement}"));
        assert!(
            decode_capture(&text)
                .unwrap_err()
                .to_string()
                .contains("/threshold_policy/value")
        );
    }
    let error = purrdf_lex::json::record::from_slice::<ThresholdPolicy>(
        br#"{"kind":"per_label","thresholds":{"joy":1e400}}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("/thresholds/joy"), "{error}");
    for (input, output) in [
        ("1e0", "1.0"),
        ("-0", "-0.0"),
        ("1e-7", "1e-7"),
        ("0.00001", "0.00001"),
        ("10000000000000000", "1e+16"),
    ] {
        let score: LabelScore = purrdf_lex::json::record::from_slice(
            format!(r#"{{"label":"é\n\"","score":{input}}}"#).as_bytes(),
        )
        .unwrap();
        let expected = format!(r#"{{"label":"é\n\"","score":{output}}}"#);
        assert_eq!(purrdf_lex::json::write_compact(&score.to_json()), expected);
    }
}

#[test]
fn json_all_committed_captures_use_native_object_and_string_forms() {
    for text in [
        include_str!("../fixtures/goemotions-sample.json"),
        include_str!("../fixtures/sst2-sample.json"),
        include_str!("../fixtures/cardiff-sample.json"),
        include_str!("../fixtures/ekman7-sample.json"),
        include_str!("../fixtures/zeroshot-sample.json"),
    ] {
        let authored = purrdf_lex::json::read(text).unwrap();
        assert!(authored.as_object().is_some());
        assert!(authored["score_semantics"].as_str().is_some());
        assert!(authored["threshold_policy"].as_object().is_some());
        for target in authored["targets"].as_array().unwrap() {
            assert!(target.as_object().is_some());
            assert!(
                target["scores"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|score| score.as_object().is_some())
            );
        }
        let capture = decode_capture(text).unwrap();
        let value = capture.to_json();
        assert_eq!(
            decode_capture(&purrdf_lex::json::write_compact(&value)).unwrap(),
            capture
        );
    }
}

fn produce_refusal(capture: &ClassifierRunCapture, config: &IngestConfig) -> IngestError {
    let failure = produce(capture, config).expect_err("the capture must be refused");
    failure
        .downcast_ref::<IngestError>()
        .expect("original typed capture refusal")
        .clone()
}

/// A minimal ontology fixture: a two-label GoEmotions set (`joy` mapped to an
/// EmotionType, `neutral` unmapped) with the closeMatch cell authored the way
/// `mappings/equivalences.ttl` authors it.
const ONTO: &str = concat!(
    "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
    "@prefix gmeow-goemotions: <https://blackcatinformatics.ca/gmeow-registry/goemotions/> .\n",
    "@prefix gmeow-labelset: <https://blackcatinformatics.ca/gmeow-registry/labelset/> .\n",
    "@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n",
    "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n",
    "gmeow-labelset:GoEmotions gmeow:labelSetDecision gmeow:decisionIndependentThreshold .\n",
    "gmeow-goemotions:joy a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n",
    "gmeow-goemotions:neutral a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n",
    "gmeow:emotionJoy a gmeow:EmotionType ; rdfs:label \"joy\"@x-gmeow-english .\n",
    "gmeow-goemotions:joy skos:closeMatch gmeow:emotionJoy {| gmeow:sssomFile \"gmeow-affect.sssom.tsv\" |} .\n",
);

fn config() -> IngestConfig {
    let index = index_turtle(ONTO.as_bytes()).expect("index onto fixture");
    IngestConfig::config_for_label_set(&index, "GoEmotions").expect("goemotions config")
}

fn capture() -> ClassifierRunCapture {
    ClassifierRunCapture {
        model_identifier: "SamLowe/roberta-base-go_emotions".to_owned(),
        model_revision: "a1b2c3d4e5f6".to_owned(),
        model_framework: "transformers".to_owned(),
        model_task: "text-classification".to_owned(),
        function_to_apply: "sigmoid".to_owned(),
        return_all_scores: true,
        label_set_id: "GoEmotions".to_owned(),
        score_semantics: ScoreSemantics::Sigmoid,
        threshold_policy: ThresholdPolicy::Global { value: 0.5 },
        targets: vec![TargetInput {
            target_iri: "https://example.org/affect/chunk-1".to_owned(),
            scores: vec![
                LabelScore {
                    label: "joy".to_owned(),
                    score: 0.84,
                },
                LabelScore {
                    label: "neutral".to_owned(),
                    score: 0.10,
                },
            ],
        }],
        score_calibration: None,
        tokenizer_revision: Some("tok-9".to_owned()),
        label_set_revision: None,
        hypothesis_template: None,
        candidate_labels: None,
    }
}

#[test]
fn sample_fixture_deserializes_with_28_labels() {
    let json = include_str!("../fixtures/goemotions-sample.json");
    let cap: ClassifierRunCapture =
        purrdf_lex::json::record::from_slice(json.as_bytes()).expect("valid fixture");
    assert_eq!(
        cap.model_revision,
        "d75048347613a25d77de8cf6412eaae9fa7b26be"
    );
    assert_eq!(cap.score_semantics, ScoreSemantics::Sigmoid);
    assert!(cap.return_all_scores);
    assert!(!cap.targets.is_empty(), "at least one captured target");
    for target in &cap.targets {
        assert_eq!(target.scores.len(), 28, "GoEmotions emits 28 labels");
    }
}

/// Labels + canonical EmotionType typing but NO in-graph closeMatch — the
/// shape of the compiled bundle, where the mapping arrives via SSSOM.
const ONTO_NO_MAPPING: &str = concat!(
    "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
    "@prefix gmeow-goemotions: <https://blackcatinformatics.ca/gmeow-registry/goemotions/> .\n",
    "@prefix gmeow-labelset: <https://blackcatinformatics.ca/gmeow-registry/labelset/> .\n",
    "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n",
    "gmeow-labelset:GoEmotions gmeow:labelSetDecision gmeow:decisionIndependentThreshold .\n",
    "gmeow-goemotions:joy a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n",
    "gmeow-goemotions:grief a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n",
    "gmeow-goemotions:desire a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n",
    "gmeow:emotionJoy a gmeow:EmotionType ; rdfs:label \"joy\"@x-gmeow-english .\n",
    "gmeow:emotionSadness a gmeow:EmotionType ; rdfs:label \"sadness\"@x-gmeow-english .\n",
    "gmeow:Desire a gmeow:Kind .\n",
);

/// An SSSOM surface shaped like `generated/mappings/gmeow-affect.sssom.tsv`:
/// a curie_map header, a column header, and rows — including the broadMatch
/// (grief) and closeMatch-to-non-EmotionType (desire) rows that must NOT route.
const SSSOM: &str = concat!(
    "# mapping_tool: gmeow-dev sync --mode update --outputs generated (mappings)\n",
    "# curie_map:\n",
    "#   gmeow: https://blackcatinformatics.ca/gmeow/\n",
    "#   gmeow-goemotions: https://blackcatinformatics.ca/gmeow-registry/goemotions/\n",
    "#   skos: http://www.w3.org/2004/02/skos/core#\n",
    "#   semapv: https://w3id.org/semapv/vocab/\n",
    "subject_id\tpredicate_id\tobject_id\tmapping_justification\tconfidence\tcomment\n",
    "gmeow-goemotions:joy\tskos:closeMatch\tgmeow:emotionJoy\tsemapv:ManualMappingCuration\t0.9\t\n",
    "gmeow-goemotions:grief\tskos:broadMatch\tgmeow:emotionSadness\tsemapv:ManualMappingCuration\t0.8\tnarrower\n",
    "gmeow-goemotions:desire\tskos:closeMatch\tgmeow:Desire\tsemapv:ManualMappingCuration\t0.85\tconative\n",
);

#[test]
fn sssom_correspondences_route_only_closematch_emotiontypes() {
    let index = index_turtle(ONTO_NO_MAPPING.as_bytes()).expect("index onto");
    let mut cfg =
        IngestConfig::config_for_label_set(&index, "GoEmotions").expect("goemotions config");
    // the bundle-shaped graph carries NO in-graph closeMatch.
    assert!(cfg.emotion_close_match.is_empty());

    cfg.add_sssom_correspondences(SSSOM, &index)
        .expect("well-formed sssom");
    // joy: closeMatch → EmotionType → routed, glossed by the CANONICAL term.
    assert_eq!(cfg.emotion_close_match[&cfg.label_iri("joy")].word, "joy");
    // grief: broadMatch (not closeMatch) → excluded.
    assert!(
        !cfg.emotion_close_match
            .contains_key(&cfg.label_iri("grief"))
    );
    // desire: closeMatch but object is teleology's gmeow:Desire, not an
    // EmotionType → excluded.
    assert!(
        !cfg.emotion_close_match
            .contains_key(&cfg.label_iri("desire"))
    );
}

#[test]
fn config_loads_labels_and_only_closematch_emotiontypes() {
    let c = config();
    let mut labels: Vec<&str> = c.registered_labels().collect();
    labels.sort_unstable();
    assert_eq!(labels, vec!["joy", "neutral"]);
    // joy reviews onto an EmotionType; neutral does not.
    assert!(c.emotion_close_match.contains_key(&c.label_iri("joy")));
    assert!(!c.emotion_close_match.contains_key(&c.label_iri("neutral")));
    assert_eq!(c.emotion_close_match[&c.label_iri("joy")].word, "joy");
}

#[test]
fn produce_emits_run_all_labels_and_a_mapped_claim() {
    let ttl = produce(&capture(), &config()).expect("produce");
    assert!(ttl.contains("ModelInferenceRun"));
    assert!(ttl.contains("SamLowe/roberta-base-go_emotions"));
    // lossless: an output for BOTH labels, incl. the below-threshold neutral.
    assert!(ttl.contains("gmeow-registry/goemotions/joy"));
    assert!(ttl.contains("gmeow-registry/goemotions/neutral"));
    // the claim references the CANONICAL word, not the raw label string source.
    assert!(ttl.contains("the text expresses joy"));
    assert!(ttl.contains("AffectiveClaim"));
    // rule 5: the output never carries emotionType directly.
    assert!(!ttl.contains("gmeow:emotionType"));
}

#[test]
fn produce_is_idempotent() {
    let (cap, cfg) = (capture(), config());
    assert_eq!(produce(&cap, &cfg).unwrap(), produce(&cap, &cfg).unwrap());
}

#[test]
fn round_trip_recover_produce_is_identity() {
    let (cap, cfg) = (capture(), config());
    let ttl = produce(&cap, &cfg).unwrap();
    // recover is authored independently of produce; the round-trip is the
    // losslessness proof, compared against the canonical normal form.
    assert_eq!(recover(&ttl, &cfg).unwrap(), canonicalize(&cap, &cfg));
}

#[test]
fn round_trip_multi_target() {
    let cfg = config();
    let mut cap = capture();
    cap.threshold_policy = ThresholdPolicy::PerLabel {
        thresholds: BTreeMap::from([("joy".to_owned(), 0.4), ("neutral".to_owned(), 0.6)]),
    };
    cap.targets.push(TargetInput {
        target_iri: "https://example.org/affect/chunk-2".to_owned(),
        scores: vec![
            LabelScore {
                label: "neutral".to_owned(),
                score: 0.72,
            },
            LabelScore {
                label: "joy".to_owned(),
                score: 0.20,
            },
        ],
    });
    let ttl = produce(&cap, &cfg).unwrap();
    assert_eq!(recover(&ttl, &cfg).unwrap(), canonicalize(&cap, &cfg));
}

#[test]
fn recover_hard_fails_on_missing_required_triple() {
    let (cap, cfg) = (capture(), config());
    let ttl = produce(&cap, &cfg).unwrap();
    // drop the mandatory modelRevision line: the graph is no longer a
    // well-formed run and the get leg must refuse it.
    let broken: String = ttl
        .lines()
        .filter(|l| !l.contains("modelRevision"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(matches!(
        recover(&broken, &cfg),
        Err(IngestError::MalformedGraph { .. })
    ));
}

#[test]
fn concluded_when_nothing_crosses_threshold() {
    let mut cap = capture();
    for cell in &mut cap.targets[0].scores {
        cell.score = 0.10; // everything sub-threshold
    }
    let ttl = produce(&cap, &config()).unwrap();
    assert!(ttl.contains("AffectEvaluationConcluded"));
    assert!(!ttl.contains("AffectiveClaim"));
}

#[test]
fn hard_fail_missing_revision() {
    let mut cap = capture();
    cap.model_revision = "  ".to_owned();
    assert_eq!(
        produce_refusal(&cap, &config()),
        IngestError::MissingModelRevision
    );
}

#[test]
fn hard_fail_unregistered_label() {
    let mut cap = capture();
    cap.return_all_scores = false;
    cap.targets[0].scores = vec![LabelScore {
        label: "notALabel".to_owned(),
        score: 0.9,
    }];
    assert_eq!(
        produce_refusal(&cap, &config()),
        IngestError::UnregisteredLabel {
            label: "notALabel".to_owned()
        }
    );
}

#[test]
fn hard_fail_calibrated_without_calibration() {
    let mut cap = capture();
    cap.score_semantics = ScoreSemantics::CalibratedProbability;
    cap.function_to_apply = "sigmoid".to_owned(); // n/a for calibrated → no activation check
    assert_eq!(
        produce_refusal(&cap, &config()),
        IngestError::MissingScoreCalibration
    );
}

#[test]
fn hard_fail_score_out_of_range_and_nan() {
    let mut cap = capture();
    cap.targets[0].scores[0].score = 1.5;
    assert!(matches!(
        produce_refusal(&cap, &config()),
        IngestError::ScoreOutOfRange { .. }
    ));
    cap.targets[0].scores[0].score = f64::NAN;
    assert!(matches!(
        produce_refusal(&cap, &config()),
        IngestError::ScoreOutOfRange { .. }
    ));
}

#[test]
fn hard_fail_duplicate_label() {
    let mut cap = capture();
    cap.return_all_scores = false;
    cap.targets[0].scores = vec![
        LabelScore {
            label: "joy".to_owned(),
            score: 0.8,
        },
        LabelScore {
            label: "joy".to_owned(),
            score: 0.7,
        },
    ];
    assert!(matches!(
        produce_refusal(&cap, &config()),
        IngestError::DuplicateLabel { .. }
    ));
}

#[test]
fn hard_fail_invalid_and_duplicate_target_iri() {
    let mut cap = capture();
    cap.targets[0].target_iri = "not an iri".to_owned();
    assert!(matches!(
        produce_refusal(&cap, &config()),
        IngestError::InvalidTargetIri { .. }
    ));

    let mut cap = capture();
    let dup = cap.targets[0].clone();
    cap.targets.push(dup);
    assert!(matches!(
        produce_refusal(&cap, &config()),
        IngestError::DuplicateTargetIri { .. }
    ));
}

#[test]
fn hard_fail_incomplete_when_return_all_scores() {
    let mut cap = capture();
    // drop neutral while claiming completeness
    cap.targets[0].scores = vec![LabelScore {
        label: "joy".to_owned(),
        score: 0.8,
    }];
    assert!(matches!(
        produce_refusal(&cap, &config()),
        IngestError::IncompleteScores { .. }
    ));
}

#[test]
fn hard_fail_activation_mismatch() {
    let mut cap = capture();
    cap.function_to_apply = "softmax".to_owned();
    assert!(matches!(
        produce_refusal(&cap, &config()),
        IngestError::ActivationMismatch { .. }
    ));
}

/// A minimal zero-shot (run-scoped) capture: NLI entailment over a candidate set
/// declared per run, not read from a static label set.
fn zeroshot_capture() -> ClassifierRunCapture {
    ClassifierRunCapture {
        model_identifier: "facebook/bart-large-mnli".to_owned(),
        model_revision: "deadbeef".to_owned(),
        model_framework: "transformers".to_owned(),
        model_task: "zero-shot-classification".to_owned(),
        function_to_apply: "entailment".to_owned(),
        return_all_scores: true,
        label_set_id: "ZeroShotEmotion2".to_owned(),
        score_semantics: ScoreSemantics::Entailment,
        threshold_policy: ThresholdPolicy::Global { value: 0.5 },
        targets: vec![TargetInput {
            target_iri: "https://example.org/affect/passage-1".to_owned(),
            scores: vec![
                LabelScore {
                    label: "joy".to_owned(),
                    score: 0.97,
                },
                LabelScore {
                    label: "fear".to_owned(),
                    score: 0.02,
                },
            ],
        }],
        score_calibration: None,
        tokenizer_revision: None,
        label_set_revision: Some("candidates:fear,joy".to_owned()),
        hypothesis_template: Some("This text expresses {}.".to_owned()),
        candidate_labels: Some(vec!["joy".to_owned(), "fear".to_owned()]),
    }
}

#[test]
fn zeroshot_run_scoped_round_trips_and_routes_no_claim() {
    let cap = zeroshot_capture();
    let cfg = IngestConfig::run_scoped_from_capture(&cap).expect("run-scoped config");
    let ttl = produce(&cap, &cfg).expect("produce zeroshot");
    // Entailment semantics + run-scoped provenance are emitted; the candidate
    // set is minted in-graph (not a static registry).
    assert!(ttl.contains("scoreEntailment"));
    assert!(ttl.contains("hypothesisTemplate"));
    assert!(ttl.contains("AffectLabelSet"));
    // Evidence only — a run-scoped prompt candidate routes NO expresses-claim.
    assert!(!ttl.contains("the text expresses"));
    // recover reads the candidate set + template back from the graph.
    assert_eq!(recover(&ttl, &cfg).unwrap(), canonicalize(&cap, &cfg));
}

#[test]
fn zeroshot_hard_fails_without_template_or_candidates() {
    let cfg = IngestConfig::run_scoped_from_capture(&zeroshot_capture()).unwrap();
    let mut no_template = zeroshot_capture();
    no_template.hypothesis_template = None;
    assert_eq!(
        produce_refusal(&no_template, &cfg),
        IngestError::MissingHypothesisTemplate
    );
}

/// Score lexical forms stay decimal-compatible for whole, tiny and signed values.
#[test]
fn f64_decimal_lexical_is_always_a_valid_xsd_decimal() {
    // xsd:decimal forbids scientific notation. A tiny in-range score must
    // still serialize as a plain decimal (and round-trip through the lexical).
    for v in [0.0, 1.0, 0.84, 0.0000001, 1e-12, 0.5_f64] {
        let s = f64_decimal_lexical(v);
        assert!(
            !s.contains(['e', 'E']),
            "f64_decimal_lexical({v}) = {s:?} must not use scientific notation"
        );
        assert!(
            s.contains('.'),
            "f64_decimal_lexical({v}) = {s:?} must carry a point"
        );
        assert_eq!(renormalize_decimal(v), s.parse::<f64>().unwrap());
    }
}

// ── label-set exclusivity ──────────────────────────────────────────────

/// A single-label (argmax) set: a 3-label Ekman-style softmax set with one
/// reviewed closeMatch (joy → emotionJoy), declared gmeow:decisionArgmax.
const ARGMAX_ONTO: &str = concat!(
    "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
    "@prefix gmeow-hf: <https://blackcatinformatics.ca/gmeow-registry/hf/> .\n",
    "@prefix gmeow-labelset: <https://blackcatinformatics.ca/gmeow-registry/labelset/> .\n",
    "@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n",
    "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n",
    "gmeow-labelset:Ekman7 gmeow:labelSetDecision gmeow:decisionArgmax .\n",
    "gmeow-hf:joy a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:Ekman7 .\n",
    "gmeow-hf:anger a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:Ekman7 .\n",
    "gmeow-hf:neutral a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:Ekman7 .\n",
    "gmeow:emotionJoy a gmeow:EmotionType ; rdfs:label \"joy\"@x-gmeow-english .\n",
    "gmeow-hf:joy skos:closeMatch gmeow:emotionJoy {| gmeow:sssomFile \"gmeow-affect.sssom.tsv\" |} .\n",
);

fn argmax_config() -> IngestConfig {
    let index = index_turtle(ARGMAX_ONTO.as_bytes()).expect("index argmax onto");
    IngestConfig::config_for_label_set(&index, "Ekman7").expect("ekman7 config")
}

/// A softmax capture whose argmax (neutral, 0.45) falls BELOW the 0.5 threshold.
fn argmax_capture() -> ClassifierRunCapture {
    ClassifierRunCapture {
        model_identifier: "j-hartmann/emotion-english-distilroberta-base".to_owned(),
        model_revision: "rev-ek".to_owned(),
        model_framework: "transformers".to_owned(),
        model_task: "text-classification".to_owned(),
        function_to_apply: "softmax".to_owned(),
        return_all_scores: true,
        label_set_id: "Ekman7".to_owned(),
        score_semantics: ScoreSemantics::Softmax,
        threshold_policy: ThresholdPolicy::Global { value: 0.5 },
        targets: vec![TargetInput {
            target_iri: "https://example.org/affect/chunk-e".to_owned(),
            scores: vec![
                LabelScore {
                    label: "joy".to_owned(),
                    score: 0.20,
                },
                LabelScore {
                    label: "anger".to_owned(),
                    score: 0.35,
                },
                LabelScore {
                    label: "neutral".to_owned(),
                    score: 0.45,
                },
            ],
        }],
        score_calibration: None,
        tokenizer_revision: None,
        label_set_revision: None,
        hypothesis_template: None,
        candidate_labels: None,
    }
}

fn sole_decision(ttl: &str) -> (String, TripleIndex) {
    let idx = index_turtle(ttl.as_bytes()).expect("index evidence");
    let decision = subjects(&idx)
        .find(|s| has_type(&idx, s, &g("AffectDecision")))
        .expect("an AffectDecision node")
        .clone();
    (decision, idx)
}

#[test]
fn argmax_decision_recorded_faithfully_below_threshold() {
    let ttl = produce(&argmax_capture(), &argmax_config()).unwrap();
    let (decision, idx) = sole_decision(&ttl);
    // the argmax winner (neutral) is recorded even though nothing crossed.
    assert_eq!(
        first_iri(&idx, &decision, &g("decidedLabel")).as_deref(),
        Some("https://blackcatinformatics.ca/gmeow-registry/hf/neutral")
    );
    assert_eq!(
        first_literal(&idx, &decision, &g("decisionCrossedThreshold")).as_deref(),
        Some("false")
    );
    // margin = 0.45 − 0.35 = 0.10, recorded as the argmax confidence.
    let margin: f64 = first_literal(&idx, &decision, &g("decisionMargin"))
        .expect("a margin (≥2 scores)")
        .parse()
        .expect("decimal margin");
    assert!((margin - 0.10).abs() < 1e-9, "margin {margin}");
    assert_eq!(
        first_iri(&idx, &decision, &g("derivedByFunction")).as_deref(),
        Some(g("fnArgmax").as_str())
    );
    // faithful evidence, NOT a forced claim; and NOT the multi-label concluded node.
    assert!(!ttl.contains("AffectiveClaim"));
    assert!(!ttl.contains("AffectEvaluationConcluded"));
}

#[test]
fn single_score_argmax_decision_has_no_margin() {
    let mut cap = argmax_capture();
    cap.return_all_scores = false;
    cap.targets[0].scores = vec![LabelScore {
        label: "joy".to_owned(),
        score: 0.90,
    }];
    let ttl = produce(&cap, &argmax_config()).unwrap();
    let (decision, idx) = sole_decision(&ttl);
    // decision minted from a single top-1 score, with no runner-up → no margin,
    // and NEVER a hard fail on a declared partial capture.
    assert!(first_iri(&idx, &decision, &g("decidedLabel")).is_some());
    assert_eq!(
        first_literal(&idx, &decision, &g("decisionCrossedThreshold")).as_deref(),
        Some("true")
    );
    assert!(first_literal(&idx, &decision, &g("decisionMargin")).is_none());
}

#[test]
fn exclusivity_violation_on_two_crossings() {
    let mut cap = argmax_capture();
    // low per-label thresholds so both anger (0.35) and neutral (0.45) cross.
    cap.threshold_policy = ThresholdPolicy::PerLabel {
        thresholds: BTreeMap::from([
            ("joy".to_owned(), 0.5),
            ("anger".to_owned(), 0.3),
            ("neutral".to_owned(), 0.3),
        ]),
    };
    assert!(matches!(
        produce_refusal(&cap, &argmax_config()),
        IngestError::ExclusivityViolation { .. }
    ));
}

#[test]
fn ambiguous_argmax_on_exact_tie() {
    let mut cap = argmax_capture();
    cap.targets[0].scores = vec![
        LabelScore {
            label: "joy".to_owned(),
            score: 0.40,
        },
        LabelScore {
            label: "anger".to_owned(),
            score: 0.40,
        },
        LabelScore {
            label: "neutral".to_owned(),
            score: 0.20,
        },
    ];
    assert!(matches!(
        produce_refusal(&cap, &argmax_config()),
        IngestError::AmbiguousArgmax { .. }
    ));
}

#[test]
fn non_normalized_softmax_over_exclusive_set_hard_fails() {
    let mut cap = argmax_capture();
    // sums to 0.6 — off the probability simplex.
    cap.targets[0].scores = vec![
        LabelScore {
            label: "joy".to_owned(),
            score: 0.20,
        },
        LabelScore {
            label: "anger".to_owned(),
            score: 0.20,
        },
        LabelScore {
            label: "neutral".to_owned(),
            score: 0.20,
        },
    ];
    assert!(matches!(
        produce_refusal(&cap, &argmax_config()),
        IngestError::NonNormalizedExclusiveScores { .. }
    ));
}

#[test]
fn score_semantics_decision_rule_mismatch_hard_fails() {
    let mut cap = argmax_capture();
    // a sigmoid (multi-label) semantics over an exclusive (argmax) set.
    cap.score_semantics = ScoreSemantics::Sigmoid;
    cap.function_to_apply = "sigmoid".to_owned();
    assert!(matches!(
        produce_refusal(&cap, &argmax_config()),
        IngestError::ScoreSemanticsDecisionMismatch { .. }
    ));
}

#[test]
fn multilabel_two_crossings_no_guard_no_decision() {
    let mut cap = capture(); // GoEmotions, sigmoid, multi-label
    cap.targets[0].scores = vec![
        LabelScore {
            label: "joy".to_owned(),
            score: 0.84,
        },
        LabelScore {
            label: "neutral".to_owned(),
            score: 0.72,
        },
    ];
    // two crossings are legitimate for a multi-label set — no hard fail.
    let ttl = produce(&cap, &config()).unwrap();
    assert!(!ttl.contains("AffectDecision"));
    assert!(ttl.contains("AffectiveClaim")); // joy still routes its claim
}

#[test]
fn missing_label_set_decision_hard_fails() {
    const NO_RULE: &str = concat!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
        "@prefix gmeow-hf: <https://blackcatinformatics.ca/gmeow-registry/hf/> .\n",
        "@prefix gmeow-labelset: <https://blackcatinformatics.ca/gmeow-registry/labelset/> .\n",
        "gmeow-hf:pos a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:SST2 .\n",
    );
    let index = index_turtle(NO_RULE.as_bytes()).expect("index");
    assert!(matches!(
        IngestConfig::config_for_label_set(&index, "SST2"),
        Err(IngestError::MissingLabelSetDecision { .. })
    ));
}

#[test]
fn argmax_round_trip_with_decision_is_identity() {
    let (cap, cfg) = (argmax_capture(), argmax_config());
    let ttl = produce(&cap, &cfg).unwrap();
    // the decision node is present in the graph…
    assert!(ttl.contains("AffectDecision"));
    // …but recover ignores it (derived interpretation), so losslessness holds.
    assert_eq!(recover(&ttl, &cfg).unwrap(), canonicalize(&cap, &cfg));
}

#[test]
fn run_scoped_mints_no_decision() {
    let cap = zeroshot_capture();
    let cfg = IngestConfig::run_scoped_from_capture(&cap).unwrap();
    let ttl = produce(&cap, &cfg).unwrap();
    assert!(!ttl.contains("AffectDecision"));
}

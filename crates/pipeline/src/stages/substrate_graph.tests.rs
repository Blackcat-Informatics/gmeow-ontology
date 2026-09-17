// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn claim(comp: &str, site: &'static str, dim: &'static str, value: &str) -> Claim {
    Claim {
        component_slug: comp.into(),
        site,
        dimension: dim,
        value: value.into(),
        witness: None,
    }
}

fn sample() -> (Vec<Component>, Vec<Claim>, Vec<Embed>) {
    let components = vec![
        Component {
            slug: "purrdf".into(),
            name: "purrdf".into(),
            expected_sites: vec![SITE_LOCKFILE, SITE_SHIPPED_ARTIFACT],
        },
        Component {
            slug: "gmn-engine".into(),
            name: "gmn-engine".into(),
            expected_sites: vec![],
        },
    ];
    let claims = vec![
        claim("purrdf", SITE_LOCKFILE, DIM_CRATE_VERSION, "0.12.0"),
        claim("purrdf", SITE_SHIPPED_ARTIFACT, DIM_CRATE_VERSION, "0.12.0"),
    ];
    let embeds = vec![Embed {
        engine_slug: "gmn-engine".into(),
        embedded_slug: "purrdf".into(),
    }];
    (components, claims, embeds)
}

#[test]
fn projection_is_byte_deterministic() {
    let (c, cl, e) = sample();
    let a = project_substrate_graph(&c, &cl, &e);
    let mut c2 = c.clone();
    c2.reverse();
    let mut cl2 = cl.clone();
    cl2.reverse();
    let b = project_substrate_graph(&c2, &cl2, &e);
    assert_eq!(a, b, "projection must be byte-stable across input order");
}

#[test]
fn projection_carries_no_runtime_ids() {
    // Every substrate node IRI is built from a PUBLIC slug (component name, site,
    // dimension) — never an opaque runtime id. So no IRI in the gmeow substrate
    // namespace carries a `#` fragment (RDF predicate IRIs like `…-ns#type`
    // legitimately do, so the check is scoped to the substrate namespace) and none
    // carries a synthetic `unit#`/`artifact#`/`origin-set#` id.
    let (c, cl, e) = sample();
    let nt = project_substrate_graph(&c, &cl, &e);
    for id in ["unit#", "artifact#", "origin-set#"] {
        assert!(
            !nt.contains(id),
            "no runtime {id} id may leak into the graph"
        );
    }
    for token in nt.split_whitespace() {
        if let Some(rest) = token.strip_prefix("<https://blackcatinformatics.ca/gmeow/substrate/") {
            let iri = rest.trim_end_matches('>');
            assert!(
                !iri.contains('#'),
                "substrate IRI must be built from a public slug, not an opaque id: {token}"
            );
        }
    }
}

#[test]
fn substrate_inputs_are_all_build_inputs_never_generated() {
    // The non-fixpoint property: every claim value
    // derives from a repo INPUT, never a render-produced digest under generated/.
    let root = Path::new("/repo");
    for p in substrate_input_paths(root) {
        let s = p.to_string_lossy();
        assert!(
            !s.contains("/generated/"),
            "substrate input {s} must be a build input, not a generated artifact"
        );
    }
}

#[test]
fn agreeing_sites_reconcile_disagreeing_do_not() {
    let agree = vec![
        claim("p", SITE_LOCKFILE, DIM_CRATE_VERSION, "0.12.0"),
        claim("p", SITE_PROSE, DIM_CRATE_VERSION, "0.12.0"),
    ];
    assert_eq!(
        reconcile(&agree).len(),
        1,
        "agreeing sites reconcile to one value"
    );
    let disagree = vec![
        claim("p", SITE_LOCKFILE, DIM_CRATE_VERSION, "0.12.0"),
        claim("p", SITE_PROSE, DIM_CRATE_VERSION, "0.13.0"),
    ];
    assert!(
        reconcile(&disagree).is_empty(),
        "disagreeing sites leave no reconciled pin (drift)"
    );
}

#[test]
fn requirements_and_resolutions_are_separate_claim_dimensions() {
    let claims = vec![
        claim(
            "purrdf",
            SITE_WORKSPACE_MANIFEST,
            DIM_VERSION_REQUIREMENT,
            "^2",
        ),
        claim("purrdf", SITE_FUZZ_MANIFEST, DIM_VERSION_REQUIREMENT, "^2"),
        claim("purrdf", SITE_LOCKFILE, DIM_CRATE_VERSION, "2.0.0"),
        claim("purrdf", SITE_SHIPPED_ARTIFACT, DIM_CRATE_VERSION, "2.0.0"),
    ];
    let reconciled = reconcile(&claims);
    assert!(reconciled.contains(&("purrdf".into(), DIM_VERSION_REQUIREMENT, "^2".into())));
    assert!(reconciled.contains(&("purrdf".into(), DIM_CRATE_VERSION, "2.0.0".into())));
    let mut drift = claims;
    drift.push(claim(
        "purrdf",
        SITE_SHIPPED_ARTIFACT,
        DIM_CRATE_VERSION,
        "2.0.1",
    ));
    assert_eq!(
        reconcile(&drift),
        vec![("purrdf".into(), DIM_VERSION_REQUIREMENT, "^2".into())]
    );
}

#[test]
fn parses_stamp_and_prose() {
    let stamp =
        parse_substrate_stamp("purrdf 0.12.0; wasm-bindgen 0.2.125; binaryen version_130\n")
            .expect("well-formed stamp parses");
    assert_eq!(stamp[0], ("purrdf".into(), "0.12.0".into()));
    assert_eq!(stamp[2], ("binaryen".into(), "version_130".into()));
    // A malformed stamp part (missing version, or extra token) is a hard fail.
    assert!(
        parse_substrate_stamp("purrdf 0.12.0; binaryen").is_err(),
        "a part without a version must be rejected, not silently skipped"
    );
    assert!(
        parse_substrate_stamp("purrdf 0.12.0 extra").is_err(),
        "a part with an extra token must be rejected"
    );
    assert_eq!(
        parse_prose_purrdf_version("projected by the Rust **purrdf 0.12.0 engine").as_deref(),
        Some("0.12.0")
    );
}

#[test]
fn distinct_engines_with_different_versions_emit_distinct_claims() {
    // H1: two engines stamping DIFFERENT purrdf versions must produce two distinct
    // gmeow:PinClaim IRIs (via the engine witness), so PinAgreementConstraint sees
    // both and reports drift — rather than collapsing to one claim node.
    let claims = vec![
        Claim {
            component_slug: "purrdf".into(),
            site: SITE_SHIPPED_ARTIFACT,
            dimension: DIM_CRATE_VERSION,
            value: "0.12.0".into(),
            witness: Some("gmn".into()),
        },
        Claim {
            component_slug: "purrdf".into(),
            site: SITE_SHIPPED_ARTIFACT,
            dimension: DIM_CRATE_VERSION,
            value: "0.13.0".into(),
            witness: Some("query".into()),
        },
    ];
    let nt = project_substrate_graph(&[], &claims, &[]);
    assert!(
        nt.contains("substrate/claim/purrdf-ShippedArtifact-CrateVersion-gmn"),
        "the gmn engine's stamp is a distinct claim IRI: {nt}"
    );
    assert!(
        nt.contains("substrate/claim/purrdf-ShippedArtifact-CrateVersion-query"),
        "the query engine's stamp is a distinct claim IRI: {nt}"
    );
    // Keyed by (component, dimension), the two differing values do NOT reconcile.
    assert!(
        reconcile(&claims).is_empty(),
        "disagreeing engine stamps leave no ReconciledPin (drift)"
    );
}

#[test]
fn covers_all_six_claim_sites_and_reconciles_purrdf() {
    use purrdf::{DatasetView, GraphMatch, TermValue};

    // The producer owns the repository scan and reconciliation. Consume its exact
    // admitted source-load product; a missing receipt is terminal.
    let fixture = crate::fixture::stage_fixture(&crate_repo_root(), 1, "stage-source-load")
        .expect("load authenticated source-load product without rebuilding corpus");
    let dataset = fixture.outcome.product.dataset();
    let graph = dataset
        .term_id_by_value(&TermValue::iri(GRAPH_PROVENANCE))
        .expect("provenance graph is present");
    let rdf_type = dataset
        .term_id_by_value(&TermValue::iri(RDF_TYPE))
        .expect("rdf:type is interned");
    let contains = |iri: String| {
        let object = dataset
            .term_id_by_value(&TermValue::iri(iri))
            .expect("expected substrate term is interned");
        dataset
            .quads_for_pattern(None, Some(rdf_type), Some(object), GraphMatch::Named(graph))
            .next()
            .is_some()
    };
    for site in [
        SITE_WORKSPACE_MANIFEST,
        SITE_FUZZ_MANIFEST,
        SITE_LOCKFILE,
        SITE_LINKED_CONSTANT,
        SITE_SHIPPED_ARTIFACT,
        SITE_PROSE,
    ] {
        let site = dataset
            .term_id_by_value(&TermValue::iri(format!("{GMEOW}{site}")))
            .unwrap_or_else(|| panic!("the substrate graph must carry claim site {site}"));
        assert!(
            dataset
                .quads_for_pattern(None, None, Some(site), GraphMatch::Named(graph))
                .next()
                .is_some(),
            "the substrate graph must carry a claim at every site"
        );
    }
    assert!(contains(format!("{GMEOW}SubstrateComponent")));
    assert!(contains(format!("{GMEOW}ReconciledPin")));
    let embeds = dataset
        .term_id_by_value(&TermValue::iri(format!("{GMEOW}embeds")))
        .expect("gmeow:embeds is interned");
    assert!(
        dataset
            .quads_for_pattern(None, Some(embeds), None, GraphMatch::Named(graph))
            .next()
            .is_some(),
        "≥1 embeds edge (SBOM contains)"
    );
}

#[test]
fn spdx_sbom_projection_carries_a_package_per_engine_and_contains_edges() {
    // The substrate reconciliation A-Box, projected through the
    // COMPILED `spdx.rq` (the same projection authority a consumer view runs), yields
    // a first-class SBOM — one `spdx:Package` per shipped engine and embedded library,
    // `spdx:versionInfo` from the reconciled pin, and an SPDX `contains` relationship
    // for every `gmeow:embeds` edge. This is the production producer folded into
    // gmeow.gts so `gmeow project --profile spdx` returns substrate packages.
    use purrdf::{DatasetView, GraphMatch, TermValue};

    let fixture = crate::fixture::stage_fixture(&crate_repo_root(), 1, "stage-mappings")
        .expect("load authenticated mappings product without rebuilding corpus");
    let dataset = fixture.outcome.product.dataset();
    let graph_iri = crate::stages::carrier::GRAPH_SUBSTRATE_SBOM;
    let graph = dataset
        .term_id_by_value(&TermValue::iri(graph_iri))
        .expect("substrate SBOM graph is present");
    let id = |iri: &str| {
        dataset
            .term_id_by_value(&TermValue::iri(iri))
            .unwrap_or_else(|| panic!("expected SBOM term {iri}"))
    };
    let rdf_type = id(RDF_TYPE);
    let package = id("http://spdx.org/rdf/terms#Package");
    let version = id("http://spdx.org/rdf/terms#versionInfo");
    let relationship = id("http://spdx.org/rdf/terms#relationship");

    // Every embedded library reconciles a crate version, so each carries an
    // spdx:versionInfo — purrdf (0.12.0) plus the toolchain libraries.
    for name in ["purrdf", "binaryen", "wasm-bindgen"] {
        let comp = iri("component", name);
        assert!(
            dataset
                .quads_for_pattern(
                    Some(id(&comp)),
                    Some(rdf_type),
                    Some(package),
                    GraphMatch::Named(graph),
                )
                .next()
                .is_some(),
            "{name} must project as an spdx:Package"
        );
        assert!(
            dataset
                .quads_for_pattern(
                    Some(id(&comp)),
                    Some(version),
                    None,
                    GraphMatch::Named(graph),
                )
                .next()
                .is_some(),
            "{name} must carry an spdx:versionInfo from its reconciled pin"
        );
    }
    // Every shipped engine is an spdx:Package that contains its embeds.
    for asset in ALL_ASSETS {
        let engine = asset.name;
        let engine_iri = iri("component", &format!("{engine}-engine"));
        assert!(
            dataset
                .quads_for_pattern(
                    Some(id(&engine_iri)),
                    Some(rdf_type),
                    Some(package),
                    GraphMatch::Named(graph),
                )
                .next()
                .is_some(),
            "the {engine} engine must project as an spdx:Package"
        );
        assert!(
            dataset
                .quads_for_pattern(
                    Some(id(&engine_iri)),
                    Some(relationship),
                    None,
                    GraphMatch::Named(graph),
                )
                .next()
                .is_some(),
            "the {engine} engine must carry an spdx:relationship (contains)"
        );
    }
    // Directional & lossy: the internal gmeow substrate vocabulary never leaks into
    // the pure-SPDX projection.
    assert!(
        dataset
            .term_id_by_value(&TermValue::iri(format!("{GMEOW}claimedValue")))
            .is_none_or(|predicate| dataset
                .quads_for_pattern(None, Some(predicate), None, GraphMatch::Named(graph),)
                .next()
                .is_none()),
        "internal gmeow substrate predicate leaked into the SBOM"
    );
}

fn crate_repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

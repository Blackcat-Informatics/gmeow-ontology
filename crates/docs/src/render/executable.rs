// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Interactive asset packaging and executable documentation pages.

use std::collections::BTreeMap;

use crate::exec::ExecutableDocsData;
use crate::model::{DocTerm, DocTermCategory, DocsModel};

use super::{Page, blank, fenced, heading, line, push_line, rel, root_href, slice_slug, term_slug};

/// The site-relative path the offline SPARQL playground's bundled RDF asset (TriG)
/// is emitted to. Language-neutral: the RDF is language-independent.
pub(super) const PLAYGROUND_TRIG_PATH: &str = "assets/playground.trig";

/// The site-relative path of the **conjecture demo library** — the curated
/// `logic:Conjecture` corpus as Turtle ([`ExecutableDocsData::conjectures_ttl`]). The W4
/// conjecture playground fetches + byte-verifies it (via [`BUNDLE_MANIFEST_PATH`]) and
/// presents its curated conjectures alongside the live wasm symmetric engine.
pub(super) const CONJECTURES_PATH: &str = "assets/conjectures.ttl";

/// The site-relative path of the FULL `gmeow.gts` bundle
/// ([`ExecutableDocsData::full_bundle_gts`]) — the in-browser Tier-1 validate surface
/// reads its `shapes-archive`. An EXTERNAL site asset (never re-embedded in the bundle).
pub(super) const FULL_BUNDLE_GTS_PATH: &str = "assets/gmeow.gts";

/// The site-relative path of the browser-bundle integrity manifest: a JSON map from
/// each bundle asset path to its `blake3:<hex>` content address and byte length, so
/// the client loader can record/verify which exact bytes it fetched. A deterministic
/// function of the emitted asset bytes.
pub(super) const BUNDLE_MANIFEST_PATH: &str = "assets/bundle-manifest.json";

/// The site-relative path of the docs controller module (SPARQL playground query
/// execution + result transcoding). A self-contained ES module.
pub(super) const DOCS_JS_PATH: &str = "assets/gmeow-docs.js";

/// The embedded docs controller module, emitted to [`DOCS_JS_PATH`] when the
/// playground is present.
pub(super) const DOCS_JS: &str = include_str!("../../assets/gmeow-docs.js");

/// The site-relative path of the shared MCP transport — the engine boot, the JSON-RPC
/// frame shape and the reasoning-segment demand loading, in one place.
///
/// The standalone console's engine worker imports it. The documentation site does NOT:
/// the site dispatches per capability to the four in-repo wasm engines, and this is the
/// console's protocol seam, emitted here because both surfaces pack from one asset set.
const MCP_TRANSPORT_PATH: &str = "assets/mcp-transport.mjs";

/// The embedded shared MCP transport, emitted to [`MCP_TRANSPORT_PATH`].
const MCP_TRANSPORT: &str = include_str!("../../assets/mcp-transport.mjs");

const REASONING_ARTIFACT_PATH: &str = "assets/reasoning-artifact.mjs";
const REASONING_ARTIFACT: &str = include_str!("../../assets/reasoning-artifact.mjs");

/// The site-relative path of the client-side BLAKE3 the transport verifies every
/// integrity-pinned asset with. The browser has no BLAKE3 primitive and `crypto.subtle`
/// offers none, so the project's own content-address function is reimplemented rather
/// than a weaker check substituted.
const BLAKE3_PATH: &str = "assets/blake3.mjs";

/// The embedded client-side BLAKE3, emitted to [`BLAKE3_PATH`].
const BLAKE3_MODULE: &str = include_str!("../../assets/blake3.mjs");

/// The wasm engines emitted under `assets/<name>/` when the playground is present: the
/// offline SPARQL/bundle-explorer runtime, the repo-free Tier-1 validator, the
/// structured-DL reasoner and the GMN codec. Every one is built in this repository; one
/// descriptor per asset lives in [`crate::vendored_asset`], and this is an alias for that
/// module's single registry rather than a second copy of the list.
use crate::vendored_asset::ALL_ASSETS as VENDORED_WASM_ASSETS;

/// The browser-bundle integrity manifest JSON — a stable, sorted, byte-deterministic
/// map from each bundle asset path to its `blake3:<hex>` content address and length.
/// A pure function of the emitted bundle bytes; the client loader records/verifies it.
fn bundle_manifest_json(exec: &ExecutableDocsData) -> String {
    // One entry per shipped bundle asset, in a fixed deterministic order (full bundle,
    // then the playground corpus, then the optional conjecture demo library). EVERY
    // client-fetched RDF asset gets an entry: the browser loader verifies each fetch
    // against its recorded content address, and an asset with no entry could only be
    // accepted unverified — the bypass the manifest exists to make impossible.
    let entry = |path: &str, bytes: &[u8]| {
        format!(
            "  \"{path}\": {{ \"blake3\": \"blake3:{d}\", \"bytes\": {n} }}",
            d = purrdf_hash::hex::encode(purrdf_hash::blake3::hash(bytes).as_bytes()),
            n = bytes.len(),
        )
    };
    let mut entries = vec![entry(FULL_BUNDLE_GTS_PATH, &exec.full_bundle_gts)];
    if !exec.playground_trig.is_empty() {
        entries.push(entry(PLAYGROUND_TRIG_PATH, &exec.playground_trig));
    }
    // The conjecture demo library ships iff the W4 playground surface is rendered.
    if exec.has_conjectures() {
        entries.push(entry(CONJECTURES_PATH, &exec.conjectures_ttl));
    }
    format!("{{\n{}\n}}\n", entries.join(",\n"))
}

/// The interactive asset FILES (no pages) an exec-backed render ships, keyed by their
/// site-relative path (`assets/…`): the shared controller module, the vendored wasm
/// engines, and — when present — the SPARQL playground data and the browser bundle +
/// its integrity manifest.
///
/// This is the SINGLE authority for "which interactive assets exist": the static site
/// emits them at the site root and the mdbook packer ([`crate::mdbook`]) copies the
/// same map under the book `src/` tree, so both surfaces carry byte-identical engines —
/// the ones the native↔wasm witness lanes prove. Empty when the render is neither
/// playground- nor bundle-backed.
pub fn interactive_asset_files(exec: &ExecutableDocsData) -> BTreeMap<String, Vec<u8>> {
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    if !exec.has_playground() && !exec.has_bundle() && !exec.has_conjectures() {
        return files;
    }
    files.insert(DOCS_JS_PATH.to_string(), DOCS_JS.as_bytes().to_vec());
    // The console's protocol seam and the client-side content-address function. Emitted
    // with the interactive set rather than from `console_files`, because `console_files`
    // folds THIS map in and derives the service worker's shell from the result — an asset
    // the console fetches but the shell never lists is the offline hole that derivation
    // exists to close.
    files.insert(
        MCP_TRANSPORT_PATH.to_string(),
        MCP_TRANSPORT.as_bytes().to_vec(),
    );
    files.insert(BLAKE3_PATH.to_string(), BLAKE3_MODULE.as_bytes().to_vec());
    files.insert(
        REASONING_ARTIFACT_PATH.to_string(),
        REASONING_ARTIFACT.as_bytes().to_vec(),
    );
    for asset in VENDORED_WASM_ASSETS {
        asset.emit_into(&mut files);
    }
    if exec.has_playground() {
        files.insert(
            PLAYGROUND_TRIG_PATH.to_string(),
            exec.playground_trig.clone(),
        );
    }
    if exec.has_bundle() {
        files.insert(
            FULL_BUNDLE_GTS_PATH.to_string(),
            exec.full_bundle_gts.clone(),
        );
        files.insert(
            BUNDLE_MANIFEST_PATH.to_string(),
            bundle_manifest_json(exec).into_bytes(),
        );
    }
    // The conjecture demo library: an UNCONDITIONAL site sub-asset whenever the W4
    // playground surface renders (the release path hard-fails on an empty declared
    // sub-asset, so it must always be present when interactive). Its integrity entry
    // rides in the bundle manifest emitted just above (`has_conjectures()` ⟹ `has_bundle()`).
    if exec.has_conjectures() {
        files.insert(CONJECTURES_PATH.to_string(), exec.conjectures_ttl.clone());
    }
    files
}

/// The DOM hooks the documentation controller binds to.
///
/// This is the single inventory of "a page carries an interactive control", read by the
/// script-injection gate and by the console producer. It mirrors the elements
/// `assets/gmeow-docs.js` binds a handler to; a widget that added a hook without adding
/// it here would render a control with no controller, which is exactly the defect the
/// derived gate exists to prevent.
///
/// The mirror is itself gated: `controller_hooks_match_the_shipped_controller_selectors`
/// scrapes the shipped controller module for every element it looks up
/// (`getElementById` / `querySelectorAll`) AND registers a listener on, and asserts
/// set-EQUALITY with this list in both directions. A hook added to the controller and not
/// to this list — or removed from the controller and left here — fails that test, so this
/// list cannot drift out of agreement with the module it mirrors.
pub(super) const CONTROLLER_HOOKS: &[&str] = &[
    "gmeow-run-validation",
    "id=\"gmeow-explorer-form\"",
    "id=\"gmeow-reason-form\"",
    "id=\"gmeow-conjecture-form\"",
    "id=\"gmeow-gmn-form\"",
    "id=\"gmeow-sparql\"",
];

/// Whether a rendered body carries any control the documentation controller drives.
#[must_use]
pub fn body_carries_interactive_control(body: &str) -> bool {
    CONTROLLER_HOOKS.iter().any(|hook| body.contains(hook))
}

// ── Executable-docs surfaces (rendered only when `exec` supplies data) ─────────

/// Append the per-slice export affordance (a `DESCRIBE` in the playground) and the
/// reasoner "try it" inference blocks to a slice page's Markdown. No-op when `exec`
/// carries no data.
pub(super) fn append_slice_executable_sections(
    out: &mut String,
    model: &DocsModel,
    slug: &str,
    exec: &ExecutableDocsData,
) {
    let Some(slice) = model.slices.iter().find(|s| slice_slug(s) == slug) else {
        return;
    };
    let root = root_href(&Page::Slice(slug.to_string()).dir());

    // Export: query this slice's vocabulary in the offline playground and copy the
    // result in any RDF format (client-side, no static per-format files).
    if exec.has_playground() {
        heading(out, 2, "Export");
        let query = format!("DESCRIBE <{}>", slice.iri);
        let encoded = url_query_encode(&query);
        line(
            out,
            &format!(
                "[Explore this slice in the SPARQL playground]({root}sparql/index.html?q={encoded}) \
                 — query its vocabulary offline and copy the result as Turtle / N-Triples / \
                 N-Quads / TriG / RDF-XML / JSON-LD."
            ),
        );
        // OKF is a structural per-concept Markdown bundle with no per-slice document;
        // point at its root index (the entry point to every term's OKF projection).
        line(
            out,
            "Each of this slice's terms also ships an OKF Markdown projection under the \
             `gmeow-okf/` bundle (see `gmeow-okf/index.md`).",
        );
    }

    // Try it: the reasoner's inferences over each worked example.
    let mut wrote_heading = false;
    for example in model.examples.iter().filter(|e| e.slice == slice.iri) {
        let Some(diff) = exec.inference_for(&slice.iri, &example.logical_path) else {
            continue;
        };
        if diff.inferred.is_empty() {
            continue;
        }
        if !wrote_heading {
            heading(out, 2, "Try it — reasoner inferences");
            line(
                out,
                "Fed through the native reasoner, each worked example yields these \
                 additional triples beyond what it asserts:",
            );
            wrote_heading = true;
        }
        heading(out, 3, &example.title);
        // Show both columns of the diff — the example's asserted ABox and, beneath it,
        // what the reasoner derived on top of it — so the surface is genuinely
        // asserted-vs-inferred rather than inferred-only.
        if !diff.asserted.is_empty() {
            line(out, "**Asserted**");
            fenced(out, "turtle", &diff.asserted.join("\n"));
        }
        line(out, "**Inferred**");
        fenced(out, "turtle", &diff.inferred.join("\n"));
    }
}

/// The `gmeow-okf/` bundle-relative path of a term's OKF (Ontology Knowledge
/// Format) Markdown document, or `None` for a category the OKF bundle does not
/// emit a per-concept document for (datatypes / other). The `{category-dir}/
/// {local-name}.md` scheme MUST match the OKF projection in the pipeline
/// (`crates/pipeline/src/stages/okf.rs` — `category_dir` + `slug`); a mismatch
/// would produce a dangling reference, so only the three covered categories emit
/// one and the datatype/other arms deliberately return `None`.
pub fn okf_doc_reference(term: &DocTerm) -> Option<String> {
    let dir = match term.category {
        DocTermCategory::Class => "classes",
        DocTermCategory::Property => "properties",
        DocTermCategory::Individual => "individuals",
        DocTermCategory::Datatype | DocTermCategory::Other => return None,
    };
    // The OKF projection is derived from the COMPOSED ontology (the carrier term surface),
    // whose term collector covers only the `gmeow:` core vocabulary (see
    // `crate::stages::export::collect_terms` — namespace-gated). A term in another
    // namespace — a grounding-language term (`lang:`/`logic:`/`math:`), an external IRI, or
    // a docs-site-only nested-example term whose curie is a full IRI — is NOT in the OKF
    // export universe, so it emits no OKF link (and the OKF-coverage gate, which pairs this
    // reference against the emitted OKF docs, correctly skips it rather than flagging a
    // dangling link the OKF bundle never promised to render).
    let (prefix, local) = term.curie.split_once(':')?;
    if prefix != "gmeow" || local.contains(['/', '#']) {
        return None;
    }
    Some(format!("gmeow-okf/{dir}/{local}.md"))
}

/// Append the per-term export affordance (a `DESCRIBE` in the playground + the
/// prompt-ready card + the OKF projection reference) to a term page's Markdown.
/// No-op without a playground.
pub(super) fn append_term_export_section(
    out: &mut String,
    model: &DocsModel,
    slug: &str,
    exec: &ExecutableDocsData,
) {
    if !exec.has_playground() {
        return;
    }
    let Some(term) = model.terms.iter().find(|t| term_slug(t) == slug) else {
        return;
    };
    let root = root_href(&Page::Term(slug.to_string()).dir());
    heading(out, 2, "Export");
    // A DESCRIBE prefilled into the offline playground: the reader runs it in-browser
    // and copies the result in any RDF format. The card is the prompt-ready projection.
    let query = format!("DESCRIBE <{}>", term.iri);
    let encoded = url_query_encode(&query);
    line(
        out,
        &format!(
            "[Describe this term in the SPARQL playground]({root}sparql/index.html?q={encoded}) \
             (run offline, copy as Turtle / N-Triples / N-Quads / TriG / RDF-XML / JSON-LD) · \
             [prompt-ready card]({root}terms/{slug}/card.md)"
        ),
    );
    // OKF is a structural multi-file Markdown bundle (not a serialize codec the
    // playground can transcode), so its per-concept document is referenced by its
    // path in the sibling `gmeow-okf/` bundle rather than transcoded inline.
    if let Some(okf) = okf_doc_reference(term) {
        line(
            out,
            &format!("The OKF Markdown projection of this term ships at `{okf}`."),
        );
    }
}

/// The offline SPARQL playground page.
/// The bundle explorer page: browser `gmeow info`/`describe` over the object-level
/// core bundle. The controller (`gmeow-docs.js`) loads the FULL `gmeow.gts` bundle
/// via the shared loader (`Dataset.fromGts`, byte-verified against the manifest),
/// shows the bundle's `info` summary on boot, and runs a client-side `DESCRIBE` for
/// the entered term IRI — the same describe the native `gmeow describe` produces,
/// proven byte-identical by the F2 witness lane.
pub(super) fn md_bundle_explorer(model: &DocsModel, _exec: &ExecutableDocsData) -> String {
    let mut out = String::new();
    heading(&mut out, 1, "Bundle explorer");
    line(
        &mut out,
        "Explore the shipped ontology **entirely in your browser** — no server, no \
         network. This loads the full `gmeow.gts` bundle — every named graph and the \
         RDF 1.2 statement layer — and answers `info` (a summary of the loaded \
         dataset) and `describe <iri>` (every triple mentioning a term) via the \
         repository's own query engine compiled to WebAssembly — the same answers \
         the `gmeow` CLI gives.",
    );
    // Raw HTML passes through the Markdown → HTML step; the controller script is
    // injected per page by the HTML shell (gated on `has_bundle()`).
    out.push_str(
        "<div id=\"gmeow-explorer\" class=\"gmeow-explorer\">\n\
         <p id=\"gmeow-explorer-info\" class=\"gmeow-explorer-info\">Loading the bundle…</p>\n\
         <form id=\"gmeow-explorer-form\">\n\
         <label for=\"gmeow-explorer-iri\">Describe a term (IRI or CURIE)</label>\n\
         <input id=\"gmeow-explorer-iri\" type=\"text\" spellcheck=\"false\" \
         placeholder=\"https://blackcatinformatics.ca/gmeow/Cat\">\n\
         <button type=\"submit\">Describe</button>\n\
         </form>\n\
         <div id=\"gmeow-explorer-results\" class=\"gmeow-explorer-results\"></div>\n\
         </div>\n",
    );
    // ── Live entailment panel (W4b) ─────────────────────────────────────────
    heading(&mut out, 2, "Live entailment");
    line(
        &mut out,
        "Paste RDF (Turtle) and run the **native GMEOW structured-DL reasoner** over \
         it in your browser — the SAME chase the on-gate authority runs (serial on \
         wasm, byte-identical to the parallel native path). The panel shows the \
         inference diff: the entailed triples the reasoner derived.",
    );
    out.push_str(
        "<div id=\"gmeow-reason\" class=\"gmeow-reason\">\n\
         <form id=\"gmeow-reason-form\">\n\
         <label for=\"gmeow-reason-input\">RDF (Turtle)</label>\n\
         <textarea id=\"gmeow-reason-input\" rows=\"6\" spellcheck=\"false\">\
@prefix rdfs: &lt;http://www.w3.org/2000/01/rdf-schema#&gt; .\n\
@prefix rdf: &lt;http://www.w3.org/1999/02/22-rdf-syntax-ns#&gt; .\n\
@prefix ex: &lt;https://example.org/&gt; .\n\
ex:Cat rdfs:subClassOf ex:Animal .\n\
ex:felix rdf:type ex:Cat .</textarea>\n\
         <button type=\"submit\">Reason</button>\n\
         </form>\n\
         <div id=\"gmeow-reason-results\" class=\"gmeow-reason-results\"></div>\n\
         </div>\n",
    );
    // ── GMN transcode widget (W4c) ──────────────────────────────────────────
    heading(&mut out, 2, "GMN transcode");
    line(
        &mut out,
        "Transcode authored RDF into the token-compact **GMN-1** surface — and back — \
         in your browser, using the SAME codec + glyph symbology the on-gate authority \
         ships. GMN-1 is a source code over the LLM token channel: reference-position \
         terms resolve through the shipped `lang:` codebook, so a codebook-covered \
         surface reads back to the identical RDF. The panel shows the round-trip: your \
         RDF → GMN-1 → the canonical N-Quads it reads back to. Hover a glyph in the \
         legend to see its real token cost.",
    );
    out.push_str(
        "<div id=\"gmeow-gmn\" class=\"gmeow-gmn\">\n\
         <form id=\"gmeow-gmn-form\">\n\
         <label for=\"gmeow-gmn-input\">RDF (Turtle)</label>\n\
         <textarea id=\"gmeow-gmn-input\" rows=\"6\" spellcheck=\"false\">\
@prefix gmeow: &lt;https://blackcatinformatics.ca/gmeow/&gt; .\n\
gmeow:gate1 gmeow:hasState gmeow:doorGate1 .\n\
gmeow:gate1 gmeow:statusLabel &quot;open&quot; .</textarea>\n\
         <button type=\"submit\">Transcode</button>\n\
         </form>\n\
         <div id=\"gmeow-gmn-legend\" class=\"gmeow-gmn-legend\"></div>\n\
         <div id=\"gmeow-gmn-results\" class=\"gmeow-gmn-results\"></div>\n\
         </div>\n",
    );
    let _ = model;
    out
}

/// The conjecture playground page (the WASM-interactive docs W4 deliverable): the browser SYMMETRIC conjecture /
/// anti-conjecture engine. The controller (`gmeow-docs.js`) fetches + byte-verifies the
/// curated demo library, loads the core bundle as the KB, and — on submit — runs the
/// vendored wasm `conjecture` export (the SAME native `logic:` reasoner, proven
/// byte-identical by the W4 conjecture witness lane), then renders BOTH legs of the test:
/// the proof leg (`KB ⊨ φ`), the counterproof leg (`KB ∪ {φ} ⊨ ⊥`) with its contradiction
/// witness, and the Belnap classification.
pub(super) fn md_conjecture_playground(model: &DocsModel, exec: &ExecutableDocsData) -> String {
    let mut out = String::new();
    heading(&mut out, 1, "Conjecture playground");
    line(
        &mut out,
        "Test a candidate `logic:` formula against a knowledge base with the **native \
         GMEOW symmetric conjecture engine** — entirely in your browser, no server, no \
         network. The engine runs TWO independent legs: a **proof** leg (does the KB \
         *entail* the formula? `KB ⊨ φ`) and a **counterproof** leg (does asserting the \
         formula make the KB *inconsistent*? `KB ∪ {φ} ⊨ ⊥`, yielding a concrete \
         contradiction witness). The Belnap classification of the two legs decides the \
         conjecture's epistemic lifecycle: corroborated, refuted-in-standpoint, or open. \
         This is the SAME chase the on-gate authority runs (serial on wasm, byte-identical \
         to native — proven by the conjecture witness lane).",
    );
    // The interactive form (raw HTML passes through the Markdown → HTML step). The
    // controller script is injected per page by the HTML shell (gated on
    // `has_conjectures()`); it populates the selector from the curated demo library and
    // renders the symmetric verdict.
    out.push_str(
        "<div id=\"gmeow-conjecture\" class=\"gmeow-conjecture\">\n\
         <p id=\"gmeow-conjecture-status\" class=\"gmeow-conjecture-status\">Loading the \
         conjecture engine…</p>\n\
         <form id=\"gmeow-conjecture-form\">\n\
         <label for=\"gmeow-conjecture-select\">Curated conjecture demo</label>\n\
         <select id=\"gmeow-conjecture-select\"></select>\n\
         <button type=\"submit\">Test</button>\n\
         </form>\n\
         <div id=\"gmeow-conjecture-results\" class=\"gmeow-conjecture-results\"></div>\n\
         </div>\n",
    );
    // The curated conjecture demo library, shipped verbatim as a site sub-asset and
    // rendered here for reference: six `logic:Conjecture`s exercising every branch of the
    // Belnap-to-lifecycle projection (open, corroborated, refuted-in-standpoint with a
    // contradiction witness + symmetric anti-conjecture leg, a Lakatos refinement
    // successor, and a phi-entails-psi propagation pair). The controller fetches this exact
    // asset (`assets/conjectures.ttl`) and byte-verifies it against the bundle manifest.
    heading(&mut out, 2, "The curated conjecture library");
    line(
        &mut out,
        "The playground ships this curated `logic:Conjecture` corpus as an integrity-pinned \
         site asset. Each conjecture carries its formula, its reified standpoint, its \
         engine-produced verdict lifecycle, and — for a refutation — a concrete \
         `logic:ContradictionWitness` and the symmetric anti-conjecture \
         `logic:NonEntailmentObligation` it proposes.",
    );
    let library = String::from_utf8_lossy(&exec.conjectures_ttl);
    fenced(&mut out, "turtle", library.trim_end());
    let _ = model;
    out
}

pub(super) fn md_playground(model: &DocsModel, exec: &ExecutableDocsData) -> String {
    let mut out = String::new();
    heading(&mut out, 1, "SPARQL playground");
    line(
        &mut out,
        "Query the bundled ontology and its documentation **entirely in your browser** — \
         no server, no network. The query runs against a self-contained RDF asset via the \
         native `purrdf` engine compiled to WebAssembly.",
    );
    // The interactive form (raw HTML passes through the Markdown → HTML step). The
    // controller script is injected per page by the HTML shell, not embedded here.
    out.push_str(
        "<form id=\"gmeow-sparql\" class=\"gmeow-sparql\">\n\
         <label for=\"gmeow-sparql-query\">SPARQL query</label>\n\
         <textarea id=\"gmeow-sparql-query\" rows=\"8\" spellcheck=\"false\">\
         SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 20</textarea>\n\
         <div class=\"gmeow-sparql-controls\">\n\
         <button type=\"submit\">Run</button>\n\
         <span id=\"gmeow-sparql-status\" role=\"status\"></span>\n\
         </div>\n\
         </form>\n\
         <div id=\"gmeow-sparql-results\"></div>\n\n",
    );
    line(
        &mut out,
        "SELECT and ASK return a results table; CONSTRUCT and DESCRIBE return a graph you \
         can copy in any RDF serialization. A `SERVICE` or `LOAD` clause fails offline — \
         there is no remote endpoint to reach.",
    );

    // Surface any reasoner inferences that could not be attributed to a single worked
    // example (shared / Skolem witnesses) — never silently dropped.
    if !exec.cross_example.is_empty() {
        heading(&mut out, 2, "Cross-example inferences");
        line(
            &mut out,
            "The reasoner derived these triples from the union of all worked examples; they \
             are not attributable to any single example:",
        );
        fenced(&mut out, "turtle", &exec.cross_example.join("\n"));
    }

    // Explain a chase-invented existential null (Skolem witness). The reason stage
    // projects each invented null into `graph/reasoning` as a `gmeow:InventedWitness`
    // typed node plus a standard-RDF-reification head quad, so the playground can
    // decompose it — with NO new vocabulary — back into the rule that fired, its
    // existential ordinal, and the frontier binding that satisfied the antecedent.
    heading(&mut out, 2, "Explain a chase-invented witness");
    line(
        &mut out,
        "When the reasoner satisfies an existential obligation it *invents* a fresh null — a \
         Skolem witness with a content-addressed IRI. That witness ships in the queryable asset \
         alongside the closure that references it, so you can decompose any null into its firing \
         rule, existential ordinal, and frontier binding entirely in your browser:",
    );
    let witness_query = "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n\
         PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>\n\
         SELECT ?witness ?rule ?ordinal ?frontier WHERE {\n\
         \x20 ?witness a gmeow:InventedWitness ;\n\
         \x20          gmeow:existentialOrdinal ?ordinal .\n\
         \x20 ?r rdf:object ?witness ;\n\
         \x20    rdf:subject ?frontier ;\n\
         \x20    gmeow:viaRule ?rule .\n\
         }";
    let root = root_href(&Page::SparqlPlayground.dir());
    let encoded = url_query_encode(witness_query);
    line(
        &mut out,
        &format!(
            "[Explain a chase-invented witness in the SPARQL playground]\
             ({root}sparql/index.html?q={encoded}) (runs offline, no server)."
        ),
    );
    fenced(&mut out, "sparql", witness_query);
    line(
        &mut out,
        "To decompose one specific null, pin it by its Skolem IRI: add \
         `FILTER(?witness = <skolem-iri>)` to the query above, or run `DESCRIBE <skolem-iri>` \
         to dump every triple that mentions it.",
    );

    let _ = model;
    out
}

/// Percent-encode a string for use in a URL query value (RFC 3986 unreserved kept).
fn url_query_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                purrdf_hash::hex::encode_upper_into(&[b], &mut out);
            }
        }
    }
    out
}

/// Link the interactive surfaces from the landing page, for the ones this render carries.
///
/// The explorer and the conjecture playground were RENDERED and unreachable: only the SPARQL
/// playground was ever linked, and only from slice and term pages. A page nothing navigates to
/// is a page nobody opens, and the shipped site is the deliverable — so the nav is derived from
/// the same `exec` predicates that decide whether the page is emitted at all, rather than from a
/// hand-kept list that can fall out of step with what was built.
pub(super) fn append_landing_interactive_nav(
    out: &mut String,
    model: &DocsModel,
    exec: &ExecutableDocsData,
) {
    let surfaces: Vec<(String, &str, &str)> = [(
        Page::SparqlPlayground.dir(),
        "SPARQL playground",
        "query the shipped bundle in your browser, offline.",
    )]
    .into_iter()
    .filter(|_| exec.has_playground())
    .chain(
        [(
            Page::BundleExplorer.dir(),
            "Bundle explorer",
            "read any term's full description straight out of `gmeow.gts`.",
        )]
        .into_iter()
        .filter(|_| exec.has_bundle()),
    )
    .chain(
        [(
            Page::ConjecturePlayground.dir(),
            "Conjecture playground",
            "put a conjecture to the reasoner and read the verdict.",
        )]
        .into_iter()
        .filter(|_| exec.has_conjectures()),
    )
    .chain(
        [(
            crate::console::CONSOLE_PREFIX
                .trim_end_matches('/')
                .to_string(),
            "Standalone console",
            "the same engine in its own shell, installable and offline-first.",
        )]
        .into_iter()
        .filter(|_| exec.has_bundle()),
    )
    .collect();
    if surfaces.is_empty() {
        return;
    }
    let from = Page::Landing.dir();
    heading(out, 2, model.ui("body_interactive"));
    for (dir, title, blurb) in surfaces {
        push_line(
            out,
            &format!("- [{title}]({}index.md) — {blurb}", rel(&from, &dir)),
        );
    }
    blank(out);
}

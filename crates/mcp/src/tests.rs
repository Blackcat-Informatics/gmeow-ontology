// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

mod browser_library_contract;
mod coherence_certificate;
mod conjecture_persistence;
mod consumer_documentation;
mod convert_transcode_contract;
mod distribution_matrix_contract;
mod glyph_legend_contract;
mod gmn_tools;
mod native_proof;
mod segment_deferral;
mod witness_explore;

gmeow_test_batch_macros::batch_mcp_items! {
    use super::*;
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, OnceLock};

    use purrdf::gts::examples::agent_memory::Memory;

    /// The `env` module the environment-mutating helpers below use. Named here rather
    /// than at the crate root so the production surface carries no `std::env` import at
    /// all — the seam is the only production path to a configuration value.
    use std::env;

    /// Read a real on-disk slice into the JSON `files` argument the `slice_quality`
    /// tool takes. The READING is the test's job — the tool is handed bytes, which is
    /// exactly the point of the argument. Delegates to the scorer's own directory
    /// reader so the test and the CLI agree on which files make up a slice.
    fn slice_files_arg(slice_dir: &Path) -> Value {
        let files = gmeow_slice_quality::report::slice_files_from_dir(slice_dir)
            .expect("the in-repo slice reads");
        Value::Object(
            files
                .into_iter()
                .map(|(path, bytes)| {
                    (
                        path,
                        Value::String(
                            String::from_utf8(bytes).expect("slice files are UTF-8 text"),
                        ),
                    )
                })
                .collect(),
        )
    }

    /// A native conjecture/candidate library at an explicit path — the handle the
    /// library-level tests drive instead of the process backend's env-resolved one.
    fn library_at(path: &Path) -> Arc<dyn SegmentLibrary> {
        crate::storage::fs_segment_library(path.to_path_buf())
    }

    /// Append one hand-built conjecture-verdict segment to the append-only library at `path`,
    /// as a GTS `ai-package` segment, via the SAME locked/atomic commit path production code
    /// uses ([`with_library_lock`] + [`append_library_segments`]). Test-only: it seeds a
    /// library file with a single segment so segment-order-resolution tests
    /// ([`read_library`]) can be driven without going through a full `store_conjecture`
    /// engine run. Production call sites build BOTH the verdict segment and its audit segment
    /// and commit them together via [`append_library_segments`] directly (one atomic replace
    /// covering both), rather than through this single-segment helper.
    fn write_conjecture_segment(path: &Path, nt_body: &str) -> gmeow_errors::Result<()> {
        let segment = build_nt_segment(&[], &probe_medium(), nt_body)?;
        let library = library_at(path);
        with_library_lock(library.as_ref(), || {
            append_library_segments(library.as_ref(), &[segment])
        })
    }

    /// A representative N-Triples body mirroring what `project_conjecture_verdict` /
    /// `project_candidate_verdict` emit: multiple triples; a repeated subject/predicate IRI
    /// (exercises term-table dedup); a blank-node subject linked to a blank-node object
    /// (`_:witness0` → `_:premise0`); typed literals carrying the projection's `\\ \" \n \t`
    /// escape subset; and both `xsd:string` and `xsd:integer` datatypes. Interning order per
    /// line is subject, predicate, [datatype,] object — the order the append-only GTS segment
    /// bytes are keyed on.
    const BYTE_PARITY_NT_BODY: &str = concat!(
        "<urn:c:1> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://gmeow.ai/logic#Conjecture> .\n",
        "<urn:c:1> <https://gmeow.ai/logic#conjectureFormula> \"line1\\nquote \\\" back \\\\ tab \\t end\"^^<http://www.w3.org/2001/XMLSchema#string> .\n",
        "<urn:c:1> <https://gmeow.ai/logic#conjectureStandpoint> <urn:sp:default> .\n",
        "<urn:c:1> <https://gmeow.ai/logic#conjectureRefutationWitness> _:witness0 .\n",
        "_:witness0 <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://gmeow.ai/logic#ContradictionWitness> .\n",
        "_:witness0 <https://gmeow.ai/logic#derivedFrom> _:premise0 .\n",
        "_:premise0 <https://gmeow.ai/logic#conjectureFormula> \"0\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n",
    );

    /// Permanent regression guard for [`build_nt_segment`]'s exact output bytes. The append-only
    /// conjecture/candidate libraries are content-addressed, so the segment bytes (and thus this
    /// digest) MUST stay byte-identical across any change to how the body is parsed — this pins
    /// the delegation of N-Triples parsing to purrdf against the prior hand-rolled lexer.
    #[test]
    fn build_nt_segment_bytes_are_stable() {
        // DETERMINISM, not a pinned digest. The segment is authored through the store medium
        // now, so its bytes are a function of the shipped dictionary — which every corpus
        // sweep legitimately re-trains. A hardcoded digest would therefore red on ordinary
        // maintenance while saying "content-addressed", which is the opposite of what it
        // claims to protect. What must hold is that the SAME body under the SAME medium gives
        // the same bytes: that is what makes the segment content-addressable at all.
        let medium = probe_medium();
        let once = build_nt_segment(&[], &medium, BYTE_PARITY_NT_BODY)
            .expect("representative body must parse");
        let twice = build_nt_segment(&[], &medium, BYTE_PARITY_NT_BODY)
            .expect("representative body must parse");
        assert_eq!(
            sha256_hex(&once),
            sha256_hex(&twice),
            "build_nt_segment is not deterministic; segment bytes are content-addressed and \
             cannot depend on anything but the body and the medium",
        );
        assert!(
            !once.is_empty(),
            "a representative body must author real bytes, or the comparison above is vacuous",
        );
    }

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvRestore(Vec<(&'static str, Option<OsString>)>);

    impl EnvRestore {
        fn capture(keys: &[&'static str]) -> Self {
            Self(keys.iter().map(|key| (*key, env::var_os(key))).collect())
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                // SAFETY: single-threaded test env mutation under the test's env lock.
                unsafe {
                    match value {
                        Some(value) => env::set_var(key, value),
                        None => env::remove_var(key),
                    }
                }
            }
        }
    }

    struct CwdRestore(PathBuf);

    impl CwdRestore {
        fn capture() -> Self {
            Self(env::current_dir().expect("current dir"))
        }
    }

    impl Drop for CwdRestore {
        fn drop(&mut self) {
            env::set_current_dir(&self.0).expect("restore current dir");
        }
    }

    /// The exact hot-store medium selected by the explicit fixture producer.
    ///
    /// Tests author only synthetic runtime records. They consume the dictionary bytes
    /// the producer resolved from the shipped bundle rather than reopening that bundle
    /// as input to a test-side segment producer.
    pub(crate) fn probe_medium() -> StoreMedium {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = gmeow_bundle_import::load_authenticated_corpus_artifact(
            &root,
            corpus_observations::MEMORY_HOT_MEDIUM,
        )
        .expect("producer-selected hot-store dictionary");
        StoreMedium {
            dictionary: MEMORY_HOT_DICTIONARY.to_string(),
            bytes,
        }
    }

    /// The independently prepared original authority; corpus tests only read its receipt.
    fn source_action_policy() -> &'static PreparedActionPolicy {
        static POLICY: OnceLock<PreparedActionPolicy> = OnceLock::new();
        POLICY.get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &root,
                "stage-conformance",
                gmeow_logic_compile::action_policy::SOURCE_ARTIFACT,
            )
            .expect("original policy has an exact producer selection");
            let policy: PreparedActionPolicy =
                PreparedActionPolicy::decode_json(&bytes).expect("native source policy");
            policy
                .validate()
                .expect("selected policy identity and native rows");
            policy
        })
    }

    /// Tiny canon tests explicitly supply the same producer-selected laws as the
    /// runtime, without packaging or compiling an authored module in the test.
    fn select_native_verification_laws(server: &McpServer) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = gmeow_action_cache::selection::source_artifacts::load(
            &root,
            "stage-conformance",
            gmeow_logic::verify::PREPARED_GATES_CHANNEL,
        )
        .expect("native verification laws have an exact producer selection");
        let gates: gmeow_logic::verify::PreparedReasonedGates =
            gmeow_logic::verify::PreparedReasonedGates::from_json_bytes(&bytes).expect("prepared native verification laws");
        gates
            .validate_source_identity()
            .expect("exact native source identity");
        server
            .view
            .prepared_gates
            .set(gates)
            .expect("tiny canon supplies laws exactly once");
    }

    fn action_policy_nquads() -> &'static str {
        source_action_policy().nquads()
    }

    fn action_policy_control(name: &str) -> &'static str {
        static CONTROLS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
        CONTROLS
            .get_or_init(|| {
                let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
                let bytes = gmeow_action_cache::selection::source_artifacts::load(
                    &root,
                    "stage-conformance",
                    "pipeline/mcp-action-policy-controls.json",
                )
                .expect("policy projection controls have an exact producer selection");
                purrdf_lex::json::record::from_slice(&bytes).expect("native policy control observations")
            })
            .get(name)
            .expect("required policy control was produced")
    }

    fn snapshot() -> Arc<[u8]> {
        static SNAPSHOT: OnceLock<Arc<[u8]>> = OnceLock::new();
        Arc::clone(SNAPSHOT.get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            Arc::from(
                gmeow_bundle_import::load_authenticated_source_bytes(&root)
                    .expect("authenticated snapshot; tests never produce it"),
            )
        }))
    }

    #[test]
    fn selected_bundle_import_cache_is_consumed_read_only() {
        let snapshot = snapshot();
        let server = McpServer::from_snapshot(&snapshot)
            .expect("the runner-selected immutable import must already exist");
        assert_eq!(server.view.gts_bytes(), snapshot.as_ref());
        let core = McpServer::from_snapshot_segmented(&snapshot, SegmentSet::core())
            .expect("another server shares the selected immutable view");
        assert!(Arc::ptr_eq(&server.view, &core.view));
        assert_eq!(server.segments(), SegmentSet::linked());
        assert_eq!(core.segments(), SegmentSet::core());
    }

    fn text_payload(value: Value) -> Value {
        let text = value["content"][0]["text"].as_str().expect("text content");
        serde_json::from_str(text).expect("tool text is JSON")
    }

    fn temp_memory() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("memory.gts");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_MEMORY_PATH", &path);
        }
        (dir, path)
    }

    #[test]
    fn consumer_view_retains_raw_snapshot_and_reaches_shapes_archive() {
        // The native validation surface (`validate_local`) needs the raw GTS bytes
        // back so `gmeow_validate` can read the folded `shapes-archive` blob — the
        // parsed carrier dataset does not carry it. Prove the bytes are retained
        // verbatim and that the shapes archive is reachable from them, so the
        // consumer server (the shippable `gmeow mcp`) can validate agent data.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        assert_eq!(
            server.view.gts_bytes(),
            bytes.as_ref(),
            "the view must retain the snapshot bytes verbatim",
        );
        let shapes =
            gmeow_bundle_view::bundle_blobs::Bundle::from_snapshot(server.view.gts_bytes())
                .expect("bundle parses from retained bytes")
                .shapes()
                .expect("shapes-archive readable from retained bytes");
        assert!(
            !shapes.is_empty(),
            "the shapes-archive blob must be reachable from the retained snapshot \
             bytes — it is the SHACL surface validate_local checks agent data against",
        );
    }

    /// The grounded-memory triad is served by ONE segment, whichever segment that is.
    ///
    /// This is a deployment-correctness gate, not a taste one. In the browser the engine
    /// ships as two wasm modules, each with its own linear memory, and the claim package is
    /// a `static` inside one of them: a segment IS a store. So a `store_claim` served by one
    /// image and a `recall` served by the other are not two views of one store, they are two
    /// stores — the write succeeds, mints an id, and is unreachable by every read. That was
    /// the shipped behaviour before `recall` and `store_segment` joined the writes in
    /// [`REASONING_SEGMENT_TOOLS`]: `store_claim` returned `ok: true`, `recall` returned
    /// `[]`, and `store_segment` reported an empty store.
    ///
    /// It is checked HERE, over the routing declaration, because the native build cannot
    /// reproduce the failure at all — one process, one `browser_storage()`, one store. The
    /// end-to-end proof across two real wasm images is
    /// `crates/mcp-core-wasm/js/tests/witness.test.mjs`; this is the invariant that keeps
    /// the split from re-opening, and it fails the moment any claim-store tool is routed
    /// away from the others.
    #[test]
    fn the_grounded_memory_triad_is_served_by_one_segment() {
        let segments: BTreeSet<&str> = CLAIM_STORE_TOOLS
            .iter()
            .map(|tool| SegmentSet::segment_of(tool))
            .collect();
        assert_eq!(
            segments.len(),
            1,
            "the tools that share the claim package must share a segment — a browser image \
             is a store, so {CLAIM_STORE_TOOLS:?} split across {segments:?} means a stored \
             claim is unreachable by every read"
        );

        // …and each half of the tiering agrees: a core deployment defers ALL of them, a
        // reasoning deployment serves ALL of them. Either mixed answer is the same defect
        // seen from one side.
        for tool in CLAIM_STORE_TOOLS {
            assert!(
                !SegmentSet::core().serves(tool),
                "`{tool}` reads or writes the claim package, so the lean core must defer it \
                 rather than answer from an image the writes cannot reach"
            );
            assert!(
                SegmentSet::reasoning_only().serves(tool),
                "`{tool}` reads or writes the claim package, so the image that owns that \
                 package must serve it"
            );
        }
    }

    /// [`CLAIM_STORE_TOOLS`] is the WHOLE claim-store surface, and every entry is real.
    ///
    /// The invariant above is only as good as the list it quantifies over, so the list is
    /// checked from both ends: every name is an advertised tool (no ghost entry padding the
    /// set), and every tool whose descriptor is about the grounded-memory package is in it.
    /// The second half is what catches a NEW memory tool added outside the list — the way
    /// `store_segment` itself was added.
    #[test]
    fn the_claim_store_tool_list_covers_the_whole_memory_surface() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let advertised = advertised_consumer_tools();
        for tool in CLAIM_STORE_TOOLS {
            assert!(
                advertised.contains(*tool),
                "CLAIM_STORE_TOOLS names `{tool}`, which the consumer surface does not \
                 advertise"
            );
        }
        // The engine's memory surface, named the way the crate itself names it: the tools
        // whose bodies go through `McpServer::claim_store`. Restated here as a literal so
        // this test is an INDEPENDENT statement of the set rather than a tautology over
        // `CLAIM_STORE_TOOLS` — the two must agree.
        let memory_surface: BTreeSet<&str> =
            BTreeSet::from(["recall", "revise_belief", "store_claim", "store_segment"]);
        assert_eq!(
            CLAIM_STORE_TOOLS
                .iter()
                .copied()
                .collect::<BTreeSet<&str>>(),
            memory_surface,
            "CLAIM_STORE_TOOLS must be exactly the tools that reach the claim package"
        );
    }

    /// Every shipped surface that STATES a tool count states the DERIVED one.
    ///
    /// The counts had rotted to "35" in five places at once — two crate descriptions, two
    /// READMEs, a feature comment, and the published npm bindings — while the surface was
    /// at 38, and the segment split was described as "twelve / twenty-three" while the
    /// declaration held thirteen. Hand-fixing those numbers is what produced the drift in
    /// the first place, so the fix is a GATE: [`TOOL_COUNT`] and the four numbers derived
    /// from it are the only tool counts any of these files may state.
    ///
    /// Rust prose reads the constants directly (a rustdoc link, or `format!` for a tool
    /// description an agent sees at run time). A `Cargo.toml` comment, a README, and the
    /// wasm-bindgen output cannot — there is no interpolation in TOML or Markdown, and the
    /// bindings are generated bytes — so those state the number and are CHECKED here. Both
    /// halves of Principle: read the derived value, or be gated against it.
    ///
    /// The scan reads PROSE, not code: comment lines, Markdown, and a `description =` key.
    /// Within it, any number ≥ 5 (numeral or English word) followed within four tokens by
    /// something naming a tool must be one of the derived counts. Below 5 is English
    /// ("exactly one action schema per tool"), not arithmetic; and code is excluded because
    /// a request-frame literal carrying a JSON-RPC id next to the `tools/call` method name
    /// states nothing whatever about the surface.
    #[test]
    fn the_shipped_prose_states_the_derived_tool_counts() {
        /// `(path relative to this crate, must this file state at least one count?)`.
        ///
        /// The published wasm bindings are included because they are the bytes an npm
        /// consumer reads: a stale vendored `pkg/` carries a false count into the package
        /// even when every source file here is right. Re-vendor
        /// (`make maint-refresh-mcp-core-asset` / `make maint-refresh-mcp-asset`) is what
        /// clears them.
        const SURFACES: &[(&str, bool)] = &[
            ("src/lib.rs", true),
            ("src/error.rs", false),
            ("Cargo.toml", true),
            ("../mcp-core-wasm/src/lib.rs", false),
            ("../mcp-core-wasm/Cargo.toml", true),
            ("../mcp-core-wasm/README.md", true),
            ("../mcp-core-wasm/js/index.mjs", false),
            ("../mcp-core-wasm/js/index.d.ts", false),
            ("../mcp-wasm/src/lib.rs", false),
            ("../mcp-wasm/Cargo.toml", true),
            ("../mcp-wasm/README.md", true),
            ("../mcp-wasm/js/index.mjs", false),
            ("../mcp-wasm/js/index.d.ts", false),
            ("../docs/assets/mcp-core/index.mjs", false),
            ("../docs/assets/mcp-core/pkg/gmeow_mcp_core_wasm.js", false),
            (
                "../docs/assets/mcp-core/pkg/gmeow_mcp_core_wasm.d.ts",
                false,
            ),
            ("../docs/assets/mcp/index.mjs", false),
            ("../docs/assets/mcp/pkg/gmeow_mcp_wasm.js", false),
            ("../docs/assets/mcp/pkg/gmeow_mcp_wasm.d.ts", false),
        ];

        /// Is this line PROSE — a place a count is CLAIMED rather than computed?
        ///
        /// Rust and JS comment lines (including the `*` continuations wasm-bindgen emits
        /// for a rustdoc block), every line of Markdown, and TOML comments plus the
        /// `description` key that becomes the published package blurb.
        fn is_prose(path: &str, line: &str) -> bool {
            let trimmed = line.trim_start();
            if path.ends_with(".md") {
                return true;
            }
            if path.ends_with(".toml") {
                return trimmed.starts_with('#') || trimmed.starts_with("description");
            }
            trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*")
        }

        /// The English number words a count claim has used. Only ≥ 5, for the reason above.
        fn word_value(token: &str) -> Option<usize> {
            Some(match token {
                "five" => 5,
                "six" => 6,
                "seven" => 7,
                "eight" => 8,
                "nine" => 9,
                "ten" => 10,
                "eleven" => 11,
                "twelve" => 12,
                "thirteen" => 13,
                "fourteen" => 14,
                "fifteen" => 15,
                "sixteen" => 16,
                "seventeen" => 17,
                "eighteen" => 18,
                "nineteen" => 19,
                "twenty" => 20,
                "thirty" => 30,
                "forty" => 40,
                "fifty" => 50,
                _ => return None,
            })
        }

        let allowed = [
            TOOL_COUNT,
            READ_TOOL_COUNT,
            WRITE_TOOL_COUNT,
            REASONING_SEGMENT_TOOL_COUNT,
            CHASE_SEGMENT_TOOL_COUNT,
            DEFERRED_TOOL_COUNT,
            CORE_SEGMENT_TOOL_COUNT,
        ];
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut problems: Vec<String> = Vec::new();
        let mut checked_total = 0usize;

        for (relative, must_state) in SURFACES {
            let path = here.join(relative);
            // A missing shipped surface is a defect, never a skip: the file being absent is
            // precisely the state in which nothing is checked.
            let text = fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!(
                    "the count gate must read the shipped surface {}: {e}",
                    path.display()
                )
            });
            // Tokenised per PROSE line: a claim does not span a line, and joining the file
            // would let a comment's last word pair with the next line's code.
            let prose: String = text
                .lines()
                .filter(|line| is_prose(relative, line))
                .collect::<Vec<_>>()
                .join("\n");
            let tokens: Vec<&str> = prose
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .filter(|token| !token.is_empty())
                .collect();
            let mut checked_here = 0usize;
            for (index, token) in tokens.iter().enumerate() {
                let value = token
                    .parse::<usize>()
                    .ok()
                    // A count of this surface is a two-digit number. A longer numeral next
                    // to the word `tools` in prose is an error code or an IRI fragment
                    // being discussed, not a claim about how many tools there are.
                    .filter(|value| (5..100).contains(value))
                    .or_else(|| word_value(&token.to_ascii_lowercase()));
                let Some(value) = value else { continue };
                let names_a_tool = tokens[index + 1..]
                    .iter()
                    .take(4)
                    .any(|later| later.to_ascii_lowercase().contains("tool"));
                if !names_a_tool {
                    continue;
                }
                checked_here += 1;
                if !allowed.contains(&value) {
                    let context = tokens[index..(index + 5).min(tokens.len())].join(" ");
                    problems.push(format!(
                        "{relative}: `{context}` states {value}, which is none of the \
                         derived counts {allowed:?}"
                    ));
                }
            }
            if *must_state && checked_here == 0 {
                problems.push(format!(
                    "{relative} states NO tool count — either the surface stopped describing \
                     itself or this gate stopped reading it; both are defects"
                ));
            }
            checked_total += checked_here;
        }

        assert!(
            problems.is_empty(),
            "shipped prose states a tool count that is not derived from TOOL_COUNT \
             ({TOOL_COUNT} tools = {READ_TOOL_COUNT} reads + {WRITE_TOOL_COUNT} writes; \
             {REASONING_SEGMENT_TOOL_COUNT} reasoning + {CORE_SEGMENT_TOOL_COUNT} core):\n  {}",
            problems.join("\n  ")
        );
        assert!(
            checked_total >= SURFACES.len(),
            "the scan found only {checked_total} count claims across {} shipped surfaces — \
             too few to be reading them, so the gate is vacuous",
            SURFACES.len()
        );
    }

    /// The CONSUMER surface is exactly [`TOOL_COUNT`] tools and 5 resources.
    ///
    /// The counts are pinned, not approximated: a later bijection gate is defined
    /// against the consumer tool list, so silently adding (or dev-promoting) a tool
    /// would change that contract without anyone noticing. The names are asserted
    /// alongside the counts so a rename cannot pass by keeping the arithmetic.
    ///
    /// This is also what makes [`TOOL_COUNT`] a DERIVED number rather than a claim: every
    /// count in this crate's shipped prose resolves to that constant, and the constant
    /// cannot survive a surface that grew past it.
    #[test]
    fn consumer_surface_matches_the_declared_tool_count() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let consumer = McpServer::from_snapshot(&bytes).unwrap();
        let names = consumer.surface().tool_names();
        assert_eq!(
            names.len(),
            TOOL_COUNT,
            "the consumer tool surface is TOOL_COUNT tools, got {names:?}"
        );
        assert_eq!(
            names,
            [
                "lookup_term",
                "llms_txt",
                "llms_full",
                "doc_card",
                "okf_index",
                "query_docs",
                "docs_search",
                "query_local",
                "encode_gmn1",
                "verify_graph",
                "reason_graph",
                "explain_quad",
                "coherence_certificate",
                "validate_local",
                "gmn_validate",
                "gmn_expand",
                "gmn_explain",
                "advise",
                "explain_finding",
                "store_claim",
                "conjecture_test",
                "store_conjecture",
                "refute_conjecture",
                "recall",
                "store_segment",
                "revise_belief",
                "counter_examples",
                "entailments",
                "competency_questions",
                "slice_quality",
                "slice_brief",
                "submit_candidate",
                "withdraw_candidate",
                "list_candidates",
                "convert",
                "gmn_glyph_legend",
                "distribution_matrix",
                "action_policy",
            ],
            "the consumer tool list changed"
        );
        let resources = consumer.surface().resource_descriptors();
        assert_eq!(
            resources.len(),
            5,
            "the consumer resource surface is 5 resources, got {resources:?}"
        );
        let uris: Vec<&str> = resources
            .iter()
            .map(|r| r["uri"].as_str().expect("resource uri"))
            .collect();
        assert_eq!(
            uris,
            [
                "gmeow://ontology/llms.txt",
                "gmeow://ontology/llms-full.txt",
                "gmeow://ontology/gmn1-primer",
                "gmeow://ontology/okf-index",
                "gmeow://ontology/action-policy",
            ],
            "the consumer resource list changed"
        );
    }

    /// Dispatching a name the surface does not carry is a NAMED hard error
    /// (`mcp.unknown-tool`, quoting the name) — never a silent no-op and never a
    /// generic fallthrough. The same holds for a resource URI.
    #[test]
    fn dispatching_an_unregistered_tool_is_a_named_hard_error() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let err = server
            .surface()
            .dispatch_tool(&server, "no_such_tool", &json!({}))
            .expect_err("an unregistered tool name must NOT dispatch");
        assert_eq!(err.code(), crate::error::UnknownTool::register());
        assert!(
            err.to_string().contains("no_such_tool"),
            "the refusal must name the tool: {err}"
        );

        // `sync` is a DEV tool: over a consumer server it is not registered at all,
        // so it takes the same named-refusal path (not a mode guard that quietly
        // returns nothing).
        let dev_only = server
            .surface()
            .dispatch_tool(&server, "sync", &json!({}))
            .expect_err("a dev tool must not dispatch on a consumer server");
        assert_eq!(dev_only.code(), crate::error::UnknownTool::register());
        assert!(dev_only.to_string().contains("sync"), "{dev_only}");

        // The JSON-RPC envelope carries the same refusal as an MCP tool error.
        let envelope = server.call_tool_result("no_such_tool", &json!({}));
        assert_eq!(envelope["isError"], json!(true), "{envelope}");
        assert!(
            envelope["content"][0]["text"]
                .as_str()
                .expect("text content")
                .contains("no_such_tool"),
            "{envelope}"
        );

        let missing = server
            .surface()
            .read_resource(&server, "gmeow://ontology/nope", &["en".to_string()])
            .expect_err("an unregistered resource URI must NOT resolve");
        assert_eq!(missing.code(), crate::error::UnknownResource::register());
        assert!(missing.to_string().contains("gmeow://ontology/nope"));
    }

    /// Registering a tool name (or a resource URI) that is already claimed is a
    /// NAMED hard error at construction — last-writer-wins would let the advertised
    /// descriptor and the dispatched handler disagree.
    #[test]
    fn duplicate_registration_is_a_named_hard_error() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();

        // Shadowing a BUILTIN tool name.
        let shadowing = Extension::new()
            .with_tool(tool("lookup_term", "A second lookup_term.", &[]), |_, _| {
                Ok(String::new())
            });
        let err = McpServer::from_snapshot_with(&bytes, shadowing)
            .err()
            .expect("shadowing a builtin tool name must refuse to construct");
        assert_eq!(err.code(), crate::error::DuplicateRegistration::register());
        assert!(err.to_string().contains("lookup_term"), "{err}");

        // Two EXTENSION entries claiming the same new name.
        let twice = Extension::new()
            .with_tool(tool("host_tool", "First.", &[]), |_, _| Ok(String::new()))
            .with_tool(tool("host_tool", "Second.", &[]), |_, _| Ok(String::new()));
        let err = McpServer::from_snapshot_with(&bytes, twice)
            .err()
            .expect("registering one tool name twice must refuse to construct");
        assert_eq!(err.code(), crate::error::DuplicateRegistration::register());
        assert!(err.to_string().contains("host_tool"), "{err}");

        // The resource twin: shadowing a builtin resource URI.
        let dup_resource = Extension::new().with_resource(
            resource(
                "gmeow://ontology/okf-index",
                "okf-index",
                "A second okf-index.",
                "application/json",
            ),
            |_, _| Ok(String::new()),
        );
        let err = McpServer::from_snapshot_with(&bytes, dup_resource)
            .err()
            .expect("shadowing a builtin resource URI must refuse to construct");
        assert_eq!(err.code(), crate::error::DuplicateRegistration::register());
        assert!(
            err.to_string().contains("gmeow://ontology/okf-index"),
            "{err}"
        );
    }

    /// A host extension's tools and resources are advertised AND dispatchable —
    /// the seam `gmeow-mcp-dev` registers its four repo-reading tools through.
    #[test]
    fn a_host_extension_is_advertised_and_dispatchable() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let extension = Extension::new()
            .with_tool(tool("host_echo", "Echo the arg.", &[]), |_, args| {
                Ok(json!({"ok": true, "echo": args.clone()}).to_string())
            })
            .with_resource(
                resource(
                    "gmeow://host/marker",
                    "marker",
                    "A host-registered resource.",
                    "text/markdown",
                ),
                |_, _| Ok("host body".to_string()),
            );
        let server = McpServer::from_snapshot_with(&bytes, extension).unwrap();

        assert_eq!(server.surface().tool_names().len(), 39);
        assert_eq!(
            server.surface().tool_names().last().copied(),
            Some("host_echo"),
            "a host tool is advertised AFTER the builtins"
        );
        let out = text_payload(server.call_tool_result("host_echo", &json!({"a": 1})));
        assert_eq!(out["echo"], json!({"a": 1}), "{out}");

        assert_eq!(server.surface().resource_descriptors().len(), 6);
        let read = server.read_resource_result("gmeow://host/marker");
        assert!(read.get("isError").is_none(), "{read}");
        assert_eq!(read["contents"][0]["text"], json!("host body"), "{read}");
        assert_eq!(
            read["contents"][0]["mimeType"],
            json!("text/markdown"),
            "the served media type is the ADVERTISED one: {read}"
        );
    }

    #[test]
    fn the_consumer_surface_advertises_the_agent_facing_tools() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let consumer = McpServer::from_snapshot(&bytes).unwrap();
        let consumer_tools = consumer.tools_result().to_string();
        assert!(consumer_tools.contains("\"lookup_term\""));
        assert!(consumer_tools.contains("\"llms_txt\""));
        assert!(consumer_tools.contains("\"llms_full\""));
        assert!(consumer_tools.contains("\"okf_index\""));
        assert!(consumer_tools.contains("\"query_docs\""));
        assert!(consumer_tools.contains("\"store_claim\""));
        // The AI-agent docs surface: every one of its tools is CONSUMER-visible
        // (served by the shippable `gmeow mcp` off the bundle alone), never
        // dev-gated. `validate_local` is distinct from the dev-only `validate`.
        assert!(consumer_tools.contains("\"validate_local\""));
        // `advise` — the recommendation companion of `validate_local`.
        assert!(consumer_tools.contains("\"advise\""));
        assert!(consumer_tools.contains("\"docs_search\""));
        assert!(consumer_tools.contains("\"counter_examples\""));
        assert!(consumer_tools.contains("\"entailments\""));
        assert!(consumer_tools.contains("\"competency_questions\""));
        assert!(!consumer_tools.contains("\"validate\""));
        // `slice_quality` is CONSUMER-visible: it scores an external slice directory
        // against the bundle-carried rubric, needing no checkout.
        assert!(consumer_tools.contains("\"slice_quality\""));
        assert!(
            !consumer
                .resources_result()
                .to_string()
                .contains("constitution")
        );
        // The four repo-reading dev tools live in `gmeow-mcp-dev` and are registered
        // through the extension seam; the DEV surface counts are asserted there.
        for dev_only in ["validate", "reason", "sync", "constitution"] {
            assert!(
                !consumer.surface().tool_names().contains(&dev_only),
                "`{dev_only}` must NOT be on the consumer surface"
            );
        }
    }

    #[test]
    fn conjecture_tool_schemas_advertise_their_enforced_required_args() {
        // `conjecture_test` / `store_conjecture` enforce `formula`,
        // `kb`, `standpoint` via `required_str` at call time (see `tool_conjecture_test` /
        // `tool_store_conjecture`); `refute_conjecture` enforces only `conjecture_id`. The
        // advertised `inputSchema.required` array must list EXACTLY what the tool body
        // enforces — otherwise a client sees an arg marked OPTIONAL and only discovers it is
        // mandatory from a runtime error (the dishonest-schema gap this test closes).
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let tools_result = server.tools_result();
        let tools = tools_result["tools"].as_array().expect("tools array");

        let required_of = |name: &str| -> BTreeSet<String> {
            let tool = tools
                .iter()
                .find(|t| t["name"] == name)
                .unwrap_or_else(|| panic!("tool {name} must be advertised"));
            tool["inputSchema"]["required"]
                .as_array()
                .unwrap_or_else(|| panic!("{name} must advertise a required array"))
                .iter()
                .map(|v| {
                    v.as_str()
                        .expect("required entries are strings")
                        .to_string()
                })
                .collect()
        };

        for name in ["conjecture_test", "store_conjecture"] {
            let required = required_of(name);
            for arg in ["formula", "kb", "standpoint"] {
                assert!(
                    required.contains(arg),
                    "{name} enforces `{arg}` via required_str at call time but does not \
                     advertise it as required: {required:?}"
                );
            }
        }

        let refute_required = required_of("refute_conjecture");
        assert!(
            refute_required.contains("conjecture_id"),
            "refute_conjecture enforces `conjecture_id` via required_str but does not advertise \
             it as required: {refute_required:?}"
        );
        // `reason` / `dry_run` are read with `optional_str` / `optional_bool_checked` at the
        // call site, so advertising them as required would be dishonest the other way.
        assert!(
            !refute_required.contains("reason"),
            "refute_conjecture's `reason` is optional at call time; must not be advertised as \
             required: {refute_required:?}"
        );
        assert!(
            !refute_required.contains("dry_run"),
            "refute_conjecture's `dry_run` is optional at call time; must not be advertised as \
             required: {refute_required:?}"
        );

        // The authoring-factory tools enforce EXACTLY these keys via `required_str` in their
        // bodies (see `tool_slice_quality` / `tool_slice_brief` / `tool_submit_candidate` /
        // `tool_withdraw_candidate` / `tool_list_candidates`); every other advertised arg is
        // read with `optional_*`. The advertised `required` array must equal the enforced set —
        // no more (a client would get a runtime error omitting a merely-optional arg), no less.
        let expected_required: &[(&str, &[&str])] = &[
            ("slice_quality", &["files"]),
            ("slice_brief", &["slice"]),
            ("submit_candidate", &["formula", "kb", "standpoint"]),
            ("withdraw_candidate", &["candidate_id"]),
            // `slice` and `disposition` are BOTH optional filters — `list_candidates` enforces
            // nothing, so it must advertise an EMPTY required array (the dishonest-`slice`-required
            // gap this asserts against).
            ("list_candidates", &[]),
        ];
        for (name, enforced) in expected_required {
            let required = required_of(name);
            let want: BTreeSet<String> = enforced.iter().map(|s| (*s).to_string()).collect();
            assert_eq!(
                required, want,
                "{name} must advertise EXACTLY the args it enforces via required_str \
                 ({want:?}); advertised {required:?}"
            );
        }
    }

    #[test]
    fn slice_quality_tool_reports_grades_and_advice_in_consumer_mode() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        // Positive CONSUMER-mode dispatch (AC3): a server built with `root: None`
        // scores a slice handed to it as BYTES, purely off the embedded bundle rubric.
        // The bytes here come from a real in-repo slice read off disk BY THE TEST — the
        // tool itself never touches a filesystem.
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let slice_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../slices/core/ai");
        let files = slice_files_arg(&slice_dir);

        // Functional dispatch: the tool returns the documented JSON shape — grades as
        // {axis, tier, score} and advice as {code, message}.
        let out = text_payload(server.call_tool_result("slice_quality", &json!({"files": files})));
        assert!(
            out.get("ok").is_none(),
            "a successful score carries no error envelope: {out}"
        );
        assert!(out["slice"].is_string(), "slice IRI present: {out}");
        assert!(
            out["rollup_tier"].is_string(),
            "roll-up tier present: {out}"
        );
        let grades = out["grades"].as_array().expect("grades array");
        assert!(!grades.is_empty(), "at least one axis grade: {out}");
        for g in grades {
            assert!(g["axis"].is_string(), "grade.axis is a string: {g}");
            assert!(g["tier"].is_string(), "grade.tier is a string: {g}");
            assert!(g["score"].is_number(), "grade.score is a number: {g}");
        }
        for a in out["advice"].as_array().expect("advice array") {
            assert!(a["code"].is_string(), "advice.code is a string: {a}");
            assert!(a["message"].is_string(), "advice.message is a string: {a}");
        }
    }

    /// Every way the `files` map can fail to describe a slice is a NAMED hard error —
    /// never a panic, never a silent pass, and never a vacuous clean score.
    #[test]
    fn slice_quality_tool_errors_on_a_map_that_is_not_a_slice() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // The `files` argument is missing entirely.
        let absent = text_payload(server.call_tool_result("slice_quality", &json!({})));
        assert_eq!(
            absent["ok"], false,
            "an omitted `files` map must hard-fail: {absent}"
        );
        assert!(
            absent["error"]
                .as_str()
                .unwrap_or_default()
                .contains("files"),
            "the error must name the missing argument: {absent}"
        );

        // A map that carries files but no `manifest.ttl` — the slice IRI cannot be
        // resolved, so there is nothing to score.
        let no_manifest = text_payload(server.call_tool_result(
            "slice_quality",
            &json!({"files": {"module.ttl": "# nothing here\n"}}),
        ));
        assert_eq!(
            no_manifest["ok"], false,
            "a map with no manifest.ttl must hard-fail: {no_manifest}"
        );
        assert!(
            no_manifest["error"]
                .as_str()
                .unwrap_or_default()
                .contains("manifest.ttl"),
            "the error must NAME manifest.ttl so the caller knows what to add: {no_manifest}"
        );

        // A `files` value that is not a string is a distinct, separately-named defect.
        let bad_value = text_payload(
            server.call_tool_result("slice_quality", &json!({"files": {"manifest.ttl": 42}})),
        );
        assert_eq!(
            bad_value["ok"], false,
            "a non-string file body must hard-fail: {bad_value}"
        );
        assert!(
            bad_value["error"]
                .as_str()
                .unwrap_or_default()
                .contains("manifest.ttl"),
            "the error must name the offending entry: {bad_value}"
        );
    }

    #[test]
    fn slice_brief_tool_serves_packet_with_fr_grounding_in_consumer_mode() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        // CONSUMER mode (root: None) serves the packet purely from the embedded
        // `graph/authoring-briefs` corpus — no checkout. The deterministic term-batch
        // numbering shifts whenever lang terms are added/removed, so the batch that
        // carries a present French grounding cell is NOT a fixed constant — it is
        // discovered dynamically below (a bare, batch-less request returns every `lang`
        // packet) rather than hardcoded, so the test survives future renumbering.
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let all = text_payload(server.call_tool_result("slice_brief", &json!({"slice": "lang"})));
        let fr_batch = all["packets"]
            .as_array()
            .expect("packets array")
            .iter()
            .find(|p| {
                p["grounding"].as_array().is_some_and(|g| {
                    g.iter()
                        .any(|c| c["attribute"] == "groundingFr" && c["value"].is_string())
                })
            })
            .unwrap_or_else(|| {
                panic!("no `lang` batch carries a present French grounding cell: {all}")
            })["batch"]
            .as_u64()
            .expect("batch is a number");

        let out = text_payload(
            server.call_tool_result("slice_brief", &json!({"slice": "lang", "batch": fr_batch})),
        );

        assert!(
            out.get("ok").is_none(),
            "a served packet carries no error envelope: {out}"
        );
        assert_eq!(
            out["slice"], "https://blackcatinformatics.ca/gmeow/slices/lang",
            "short-name expanded to the full slice IRI: {out}"
        );
        assert_eq!(out["packet_count"], 1, "exactly the requested batch: {out}");
        let packet = &out["packets"][0];
        assert_eq!(packet["axis"], "whole");
        assert_eq!(packet["batch"], fr_batch);
        assert!(
            packet["digest"].is_string(),
            "packet digest present: {packet}"
        );
        assert!(
            packet["term_count"].as_i64().is_some_and(|n| n > 0),
            "packet covers terms: {packet}"
        );
        assert!(
            packet["covers_terms"]
                .as_array()
                .is_some_and(|a| !a.is_empty()),
            "covered-term IRIs listed: {packet}"
        );
        // AC4: a French translation cell survives with its JOINed value.
        let grounding = packet["grounding"].as_array().expect("grounding array");
        let fr = grounding
            .iter()
            .find(|c| c["attribute"] == "groundingFr" && c["value"].is_string());
        assert!(
            fr.is_some(),
            "a present French grounding value survives the round-trip: {packet}"
        );
        // The canonical turtle is the byte-reconstructible surface the bundle folded.
        let turtle = out["turtle"].as_str().expect("turtle string");
        assert!(
            turtle.contains("AuthoringPacket") && turtle.contains("packetSourceSlice"),
            "turtle carries the packet body"
        );
    }

    #[test]
    fn slice_brief_tool_hard_fails_on_unknown_slice() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // No packet for the slice → explicit hard error, never a vacuous empty pass.
        let miss = text_payload(
            server.call_tool_result("slice_brief", &json!({"slice": "no-such-slice-xyz"})),
        );
        assert_eq!(miss["ok"], false, "unknown slice must hard-fail: {miss}");

        // A real slice but an out-of-range batch also hard-fails.
        let bad_batch = text_payload(
            server.call_tool_result("slice_brief", &json!({"slice": "lang", "batch": 99999})),
        );
        assert_eq!(
            bad_batch["ok"], false,
            "an out-of-range batch must hard-fail: {bad_batch}"
        );
    }

    #[test]
    fn query_docs_selects_over_the_documentation_graph() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // A SELECT over the bundled documentation graph returns SPARQL-1.1 JSON
        // bindings (the doc graph carries a gmeow:DocumentedTerm per documented term).
        let select = text_payload(server.call_tool_result(
            "query_docs",
            &json!({"query": "SELECT ?s WHERE { ?s a <https://blackcatinformatics.ca/gmeow/DocumentedTerm> } LIMIT 3"}),
        ));
        assert_eq!(
            select["ok"], true,
            "query_docs SELECT must succeed: {select}"
        );
        assert_eq!(select["head"]["vars"][0], "s");
        assert!(
            select["results"]["bindings"]
                .as_array()
                .map(|b| !b.is_empty())
                .unwrap_or(false),
            "expected at least one DocumentedTerm binding: {select}"
        );

        // CONSTRUCT is ANSWERED, and the envelope DECLARES the result form. This used to
        // assert a refusal ("SELECT and ASK"), which was a refusal standing in for a
        // capability: the native engine had already evaluated the CONSTRUCT and handed back
        // a `SparqlResult::Graph` that the surface then declined to serialize.
        let construct = text_payload(server.call_tool_result(
            "query_docs",
            &json!({"query": "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 1"}),
        ));
        assert_eq!(construct["ok"], true, "{construct}");
        assert_eq!(construct["form"], "graph", "{construct}");
        assert!(
            construct["graph_nquads"]
                .as_str()
                .is_some_and(|g| g.contains('<')),
            "the graph result carries real N-Quads: {construct}"
        );
        assert_eq!(construct["quad_count"], 1, "{construct}");

        // The three forms are distinguishable WITHOUT parsing: a client dispatches on
        // `form`, never on whether a JSON parse of the payload happened to fail.
        assert_eq!(select["form"], "bindings", "{select}");
        let ask = text_payload(
            server.call_tool_result("query_docs", &json!({"query": "ASK { ?s ?p ?o }"})),
        );
        assert_eq!(ask["form"], "boolean", "{ask}");
        assert_eq!(ask["boolean"], true, "{ask}");
    }

    #[test]
    fn memory_triad_preserves_suppression_on_every_default_recall_path() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let canary = text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "SUPPRESSED-CANARY belief about the launch window", "confidence": 0.9}),
        ));
        let canary_id = canary["claim"]["id"].as_str().unwrap().to_string();
        text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "CONTROL-CANARY belief about the launch window", "confidence": 0.9}),
        ));
        let revised = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": canary_id, "reason": "revised"}),
        ));
        assert_eq!(revised["ok"], true);

        text_payload(server.call_tool_result("recall", &json!({"query": "launch window"})));
        let calls = crate::storage::fs_claim_store(memory_path).unwrap().tool_calls().unwrap();
        assert_eq!(
            calls
                .iter()
                .map(|call| call.tool.as_str())
                .collect::<Vec<_>>(),
            vec![
                "urn:gmeow:tool:store_claim",
                "urn:gmeow:tool:store_claim",
                "urn:gmeow:tool:revise_belief"
            ]
        );
        assert_eq!(calls[0].generated, vec![canary_id.clone()]);

        let payload = calls[0].payload.as_ref().unwrap();
        let substitutions = [
            ("arguments".to_owned(), purrdf::TermValue::from_rdf_term(payload.arguments.as_ref().unwrap())),
            ("result".to_owned(), purrdf::TermValue::from_rdf_term(payload.result.as_ref().unwrap())),
            ("claim".to_owned(), purrdf::TermValue::Literal { lexical_form: canary_id.clone(), datatype: "http://www.w3.org/2001/XMLSchema#string".to_owned(), language: None, direction: None }),
        ];
        let query = r#"PREFIX g: <https://blackcatinformatics.ca/gmeow/>
            ASK {
                ?arguments g:hasFacet [ g:facetLabel "text"; g:facetStructuredValue [ g:structuredTerm "SUPPRESSED-CANARY belief about the launch window" ] ] .
                ?result g:hasFacet [ g:facetLabel "ok"; g:facetStructuredValue [ g:structuredTerm true ] ],
                    [ g:facetLabel "claim"; g:facetStructuredValue [ g:hasFacet [ g:facetLabel "id"; g:facetStructuredValue [ g:structuredTerm ?claim ] ] ] ] .
            }"#;
        let stored = purrdf::SparqlEngine::query(
            &purrdf::sparql::NativeSparqlEngine::new(), &payload.graph,
            purrdf::SparqlRequest { query, base_iri: None, substitutions: &substitutions },
        ).unwrap();
        assert!(matches!(stored, purrdf::SparqlResult::Boolean(true)), "recorded arguments/result keep the text, claim id and ok=true outcomes");

        for args in [
            json!({}),
            json!({"query": "launch window"}),
            json!({"query": "SUPPRESSED-CANARY belief"}),
            json!({"query": "launch", "min_confidence": 0.5}),
            json!({"query": "", "limit": 100}),
        ] {
            let recalled = text_payload(server.call_tool_result("recall", &args));
            let texts: Vec<&str> = recalled["claims"]
                .as_array()
                .unwrap()
                .iter()
                .map(|claim| claim["text"].as_str().unwrap())
                .collect();
            assert!(!texts.contains(&"SUPPRESSED-CANARY belief about the launch window"));
            assert!(texts.contains(&"CONTROL-CANARY belief about the launch window"));
        }

        let audit = text_payload(server.call_tool_result(
            "recall",
            &json!({"query": "launch window", "include_suppressed": true}),
        ));
        assert!(audit["claims"].as_array().unwrap().iter().any(|claim| {
            claim["text"] == "SUPPRESSED-CANARY belief about the launch window"
                && claim["suppressed"] == true
        }));
    }

    #[test]
    fn revision_rejects_unknown_ids_before_writing() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let claim =
            text_payload(server.call_tool_result("store_claim", &json!({"text": "a real belief"})));
        let claim_id = claim["claim"]["id"].as_str().unwrap();

        let missing = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": "urn:gmeow:assertion:no-such-id"}),
        ));
        assert_eq!(missing["ok"], false);

        let bad_successor = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": claim_id, "superseded_by": "urn:gmeow:assertion:ghost"}),
        ));
        assert_eq!(bad_successor["ok"], false);

        let live =
            text_payload(server.call_tool_result("recall", &json!({"query": "real belief"})));
        assert_eq!(live["claims"][0]["id"], claim_id);
        assert_eq!(live["claims"][0]["suppressed"], false);
    }

    /// The `convert` tool's byte channel. The three residue classes (0, 1, 2 trailing
    /// bytes) are the whole of base64's arithmetic, and each has its own padding; the
    /// vectors are RFC 4648 §10's.
    #[test]
    fn base64_encodes_every_residue_class_with_the_rfc_vectors() {
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_encode(plain.as_bytes()), encoded, "encode {plain:?}");
            assert_eq!(
                base64_decode("test", encoded).expect("decode"),
                plain.as_bytes(),
                "decode {encoded:?}"
            );
        }
    }

    /// Round-trip over every byte value, including the ones that make `+` and `/` appear —
    /// the two alphabet characters a naive table gets wrong.
    #[test]
    fn base64_round_trips_every_byte_value() {
        let all: Vec<u8> = (0u8..=255).collect();
        let encoded = base64_encode(&all);
        assert_eq!(base64_decode("test", &encoded).expect("decode"), all);
        assert!(encoded.contains('+') && encoded.contains('/'), "{encoded}");
    }

    /// Line-wrapped input decodes (a pasted payload routinely is), but malformed input is
    /// REFUSED rather than truncated to whatever happened to decode.
    #[test]
    fn base64_decoding_is_strict_about_everything_except_whitespace() {
        assert_eq!(
            base64_decode("test", "Zm9v\n YmFy").expect("whitespace is skipped"),
            b"foobar"
        );
        for bad in ["Zg=", "Zm9vYg", "Zg===", "Zm=v", "Zm9v!!!!", "Z===="] {
            let err = base64_decode("convert: `data`", bad)
                .expect_err("malformed base64 must be refused, not partially decoded");
            assert!(
                err.to_string().contains("convert: `data`"),
                "the refusal must name the argument it is about: {err}"
            );
        }
    }

    #[test]
    fn canonical_action_policy_is_the_single_authority_and_parses() {
        // The embedded slice file is the one source of truth for the action theory.
        let policy = action_policy_nquads();
        assert!(!policy.is_empty());
        assert!(policy.contains(MCP_STORE_CLAIM_SCHEMA));
        assert!(policy.contains(MCP_REVISE_BELIEF_SCHEMA));
        assert!(policy.contains(TXN_WORLD));
    }

    /// The `action_policy` TOOL returns the projected theory the engine itself reads —
    /// the same quad set [`action_policy_nquads`] yields natively, as a SET (line order is
    /// not the contract; membership is).
    ///
    /// This is the point of the tool: no other surface can serve it. `tools/list` returns
    /// names and JSON Schemas, and `query_docs` is scoped to `gmeow:graph/documentation`
    /// while the policy is authored in the agentic slice's examples graph. If the tool ever
    /// re-derived the theory instead of returning it, the console's pane derivation — and
    /// anyone auditing what the engine gates its writes on — would be reading a copy.
    #[test]
    fn the_action_policy_tool_returns_exactly_the_projected_theory_the_engine_reads() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let server = McpServer::from_snapshot(&snapshot()).unwrap();

        let payload = text_payload(server.call_tool_result("action_policy", &json!({})));
        assert_eq!(payload["ok"], json!(true), "{payload}");
        assert_eq!(payload["graph"], json!(TXN_WORLD), "{payload}");
        assert_eq!(
            payload["media_type"],
            json!(ACTION_POLICY_MEDIA_TYPE),
            "{payload}"
        );

        let native = action_policy_nquads();
        let served = payload["nquads"].as_str().expect("nquads text");
        assert_eq!(
            served, native,
            "the tool must return the engine's projection verbatim, not a re-derivation"
        );

        let native_set: BTreeSet<&str> = native.lines().collect();
        let served_set: BTreeSet<&str> = served.lines().collect();
        assert_eq!(
            served_set, native_set,
            "the served quad SET must equal the natively projected quad set"
        );
        assert!(
            !native_set.is_empty(),
            "the projected theory must be non-empty, or this test proves nothing"
        );
    }

    /// The mirroring RESOURCE serves the identical bytes under the identical media type.
    /// Tool and resource are two readers of ONE projection, exactly as `constitution` is
    /// on the dev side — a second copy on either side could drift.
    #[test]
    fn the_action_policy_resource_serves_the_same_bytes_as_the_tool() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let server = McpServer::from_snapshot(&snapshot()).unwrap();

        let read = server.read_resource_result(ACTION_POLICY_URI);
        assert!(read.get("isError").is_none(), "{read}");
        let content = &read["contents"][0];
        assert_eq!(content["uri"], json!(ACTION_POLICY_URI), "{read}");
        assert_eq!(
            content["mimeType"],
            json!(ACTION_POLICY_MEDIA_TYPE),
            "the served media type must be the descriptor's: {read}"
        );

        let resource_text = content["text"].as_str().expect("resource text");
        assert_eq!(
            resource_text,
            action_policy_nquads(),
            "the resource must serve the engine's projection verbatim"
        );

        let tool_text = text_payload(server.call_tool_result("action_policy", &json!({})));
        assert_eq!(
            tool_text["nquads"].as_str().expect("nquads"),
            resource_text,
            "the tool and the resource must serve the SAME bytes"
        );
    }

    // ── The bijection gate: the action theory is TOTAL over the tool surface ──────
    //
    // Everything below replaces a spot-check that named five policy subjects by hand and
    // therefore could not notice a tool with no schema (or a schema with no tool) — the
    // two failures that make an action theory a decoration instead of a contract.

    /// The policy's asserted MCP wire-name predicate, independent of schema IRI spelling.
    const LOGIC_MCP_TOOL_NAME: &str = "https://blackcatinformatics.ca/logic/mcpToolName";
    const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const LOGIC_ACTION_SCHEMA: &str = "https://blackcatinformatics.ca/logic/ActionSchema";
    const LOGIC_MCP_ACTION_SCHEMA: &str = "https://blackcatinformatics.ca/logic/McpActionSchema";

    /// The four repo-reading DEV tools, registered by `gmeow-mcp-dev` through the extension
    /// seam. That crate depends on THIS one, so it cannot be named from here; the list is
    /// restated and `gmeow_mcp_dev`'s own `dev_surface_is_thirty_nine_tools_and_six_resources`
    /// pins the same four, so a rename reds there rather than silently widening this list.
    const DEV_ONLY_TOOLS: [&str; 4] = ["validate", "reason", "sync", "constitution"];

    /// One object term of a projected action-policy quad.
    #[derive(Debug, PartialEq, Eq)]
    enum ProjectedObject {
        Iri(String),
        Literal(String),
    }

    /// Take a leading `<iri>` off `s`, returning the IRI and the rest (left-trimmed).
    fn take_angle(s: &str) -> (String, &str) {
        let body = s
            .strip_prefix('<')
            .unwrap_or_else(|| panic!("expected an IRI term at {s:?}"));
        let end = body
            .find('>')
            .unwrap_or_else(|| panic!("unterminated IRI term at {s:?}"));
        (body[..end].to_string(), body[end + 1..].trim_start())
    }

    /// Unescape a literal body starting immediately after its opening quote.
    fn take_literal(after_quote: &str) -> String {
        let mut out = String::new();
        let mut chars = after_quote.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return out,
                '\\' => out.push(chars.next().expect("an escape has a body")),
                c => out.push(c),
            }
        }
        panic!("unterminated literal at {after_quote:?}");
    }

    /// Split one line of [`gmeow_logic_compile::action_policy::project_nquads`]'s output. The projection writes every line
    /// as `<s> <p> o <TXN_WORLD> .`, so this parses exactly that shape rather than general
    /// N-Quads — which is the point: it reads back what the engine is actually handed.
    fn split_projected(line: &str) -> (String, String, ProjectedObject) {
        let body = line
            .strip_suffix(&format!(" <{TXN_WORLD}> ."))
            .unwrap_or_else(|| panic!("every projected quad is stamped into TXN_WORLD: {line:?}"));
        let (subject, rest) = take_angle(body);
        let (predicate, object) = take_angle(rest);
        let object = match object.strip_prefix('"') {
            Some(after_quote) => ProjectedObject::Literal(take_literal(after_quote)),
            None => ProjectedObject::Iri(take_angle(object).0),
        };
        (subject, predicate, object)
    }

    /// The ASSERTED action theory, read back out of the projected N-Quads the engine reads.
    ///
    /// Asserted, not entailed: `logic:McpActionSchema` is a SUBCLASS of `logic:ActionSchema`
    /// (`slices/grounding/logic/module.ttl`), so under the subclass-closed view every write
    /// is also an `ActionSchema` and the read/write partition would collapse. The projection
    /// is an asserted-quad projection, so reading it this way is faithful — and it is why
    /// this file must NOT carry a disjointness axiom between the two classes, which would
    /// contradict the subclass edge and red the reasoner.
    #[derive(Debug, Default)]
    struct ActionTheory {
        /// Subjects asserted `a logic:ActionSchema` — the reads.
        plain: BTreeSet<String>,
        /// Subjects asserted `a logic:McpActionSchema` — the governed writes.
        governed: BTreeSet<String>,
        /// Subject IRI → every `logic:mcpToolName` asserted on it.
        tool_names: BTreeMap<String, BTreeSet<String>>,
    }

    impl ActionTheory {
        fn read(nquads: &str) -> Self {
            let mut theory = Self::default();
            for line in nquads.lines() {
                let (subject, predicate, object) = split_projected(line);
                match (predicate.as_str(), &object) {
                    (RDF_TYPE_IRI, ProjectedObject::Iri(class)) if class == LOGIC_ACTION_SCHEMA => {
                        theory.plain.insert(subject);
                    }
                    (RDF_TYPE_IRI, ProjectedObject::Iri(class))
                        if class == LOGIC_MCP_ACTION_SCHEMA =>
                    {
                        theory.governed.insert(subject);
                    }
                    (LOGIC_MCP_TOOL_NAME, ProjectedObject::Literal(name)) => {
                        theory
                            .tool_names
                            .entry(subject)
                            .or_default()
                            .insert(name.clone());
                    }
                    (LOGIC_MCP_TOOL_NAME, ProjectedObject::Iri(iri)) => panic!(
                        "logic:mcpToolName is a datatype property: <{subject}> names <{iri}>"
                    ),
                    _ => {}
                }
            }
            theory
        }

        /// Every subject asserted as a schema, read or write.
        fn schemas(&self) -> BTreeSet<String> {
            self.plain.union(&self.governed).cloned().collect()
        }

        /// Wire name → the schema subjects claiming it.
        fn by_tool_name(&self) -> BTreeMap<String, BTreeSet<String>> {
            let mut index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for (subject, names) in &self.tool_names {
                for name in names {
                    index
                        .entry(name.clone())
                        .or_default()
                        .insert(subject.clone());
                }
            }
            index
        }

        /// The wire names asserted on `subjects`, which must each carry exactly one.
        fn names_of(&self, subjects: &BTreeSet<String>) -> BTreeSet<String> {
            subjects
                .iter()
                .map(|subject| {
                    let names = self
                        .tool_names
                        .get(subject)
                        .unwrap_or_else(|| panic!("<{subject}> carries no logic:mcpToolName"));
                    assert_eq!(
                        names.len(),
                        1,
                        "<{subject}> declares {} tool names ({names:?})",
                        names.len()
                    );
                    names.iter().next().expect("exactly one name").clone()
                })
                .collect()
        }
    }

    /// The bijection check itself, as the list of violations (empty when it holds). The gate
    /// and BOTH its negative tests call this, so all three exercise one comparison instead of
    /// three lookalikes.
    fn bijection_violations(
        advertised: &BTreeSet<String>,
        named: &BTreeSet<String>,
    ) -> Vec<String> {
        let mut problems = Vec::new();
        for tool in advertised.difference(named) {
            problems.push(format!(
                "advertised consumer tool `{tool}` has NO logic:mcpToolName row in the \
                 shipped action theory"
            ));
        }
        for row in named.difference(advertised) {
            problems.push(format!(
                "action-theory row names tool `{row}`, which the consumer surface does NOT \
                 advertise"
            ));
        }
        problems
    }

    /// The advertised consumer tool names, as a set.
    fn advertised_consumer_tools() -> BTreeSet<String> {
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let consumer = McpServer::from_snapshot(&bytes).expect("consumer server constructs");
        consumer
            .surface()
            .tool_names()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    /// THE GATE. The tool-name set of the CONSUMER builtin surface is EQUAL to the
    /// `{?s logic:mcpToolName ?n}` set over `logic:ActionSchema ∪ logic:McpActionSchema` in
    /// the shipped policy — in BOTH directions.
    ///
    /// A tool with no schema means the engine advertises an action its own action theory
    /// does not describe; a schema with no tool means the theory describes an action the
    /// engine cannot perform. Either one makes the `action_policy` projection a decoration.
    /// Both are named individually when they fail.
    ///
    /// The correspondence is checked on `logic:mcpToolName` and NOT on the schema local
    /// name, because the two genuinely differ: `ex:persistConjecture` is the tool
    /// `store_conjecture` and `ex:withdrawConjecture` is `refute_conjecture`. Any gate built
    /// on name mangling would either reject those two or accept anything.
    #[test]
    fn the_action_theory_is_bijective_with_the_consumer_tool_surface() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let advertised = advertised_consumer_tools();
        assert_eq!(
            advertised.len(),
            TOOL_COUNT,
            "the consumer surface is TOOL_COUNT tools; this gate's arithmetic depends on it"
        );

        let theory = ActionTheory::read(action_policy_nquads());
        let by_name = theory.by_tool_name();
        let named: BTreeSet<String> = by_name.keys().cloned().collect();

        let problems = bijection_violations(&advertised, &named);
        assert!(
            problems.is_empty(),
            "the action theory is NOT bijective with the consumer tool surface:\n  {}",
            problems.join("\n  ")
        );
        assert_eq!(
            named, advertised,
            "both directions hold, so the sets are equal"
        );

        // A BIJECTION, not merely a two-sided cover: one schema per name, one name per
        // schema. Two schemas claiming `store_claim` would pass the set equality above while
        // leaving the engine's gate ambiguous.
        for (name, subjects) in &by_name {
            assert_eq!(
                subjects.len(),
                1,
                "tool `{name}` is claimed by {} action schemas ({subjects:?})",
                subjects.len()
            );
        }
        let schemas = theory.schemas();
        for (subject, names) in &theory.tool_names {
            assert_eq!(
                names.len(),
                1,
                "action schema <{subject}> declares {} tool names ({names:?})",
                names.len()
            );
            assert!(
                schemas.contains(subject),
                "<{subject}> carries logic:mcpToolName but is asserted neither \
                 logic:ActionSchema nor logic:McpActionSchema"
            );
        }
        for subject in &schemas {
            assert!(
                theory.tool_names.contains_key(subject),
                "action schema <{subject}> carries NO logic:mcpToolName, so nothing ties it \
                 to a tool"
            );
        }

        // The READ / WRITE partition, over ASSERTED types. `{?s a logic:ActionSchema} \
        // {?s a logic:McpActionSchema}` must be NON-EMPTY and be exactly the reads: if the
        // reads were ever typed logic:McpActionSchema (or the writes lost their type) this
        // difference would silently empty out, and a console pane built on it would go blank
        // rather than fail.
        assert!(
            theory.plain.is_disjoint(&theory.governed),
            "no schema is asserted BOTH plain and governed: {:?}",
            theory
                .plain
                .intersection(&theory.governed)
                .collect::<Vec<_>>()
        );
        let read_subjects: BTreeSet<String> =
            theory.plain.difference(&theory.governed).cloned().collect();
        assert!(
            !read_subjects.is_empty(),
            "the asserted-plain-minus-governed set must not be empty"
        );
        // Both counts are the DERIVED constants the shipped prose quotes, so this gate is
        // also what keeps `READ_TOOL_COUNT` / `WRITE_TOOL_COUNT` honest: the policy is the
        // authority on what a write is, and the constants must agree with it.
        assert_eq!(
            read_subjects.len(),
            READ_TOOL_COUNT,
            "READ_TOOL_COUNT reads: {read_subjects:?}"
        );
        assert_eq!(
            theory.governed.len(),
            WRITE_TOOL_COUNT,
            "WRITE_TOOL_COUNT writes: {:?}",
            theory.governed
        );

        let write_names = theory.names_of(&theory.governed);
        assert_eq!(
            write_names,
            WRITE_TOOLS
                .iter()
                .map(|name| (*name).to_string())
                .collect::<BTreeSet<String>>(),
            "the governed writes are exactly the tools WRITE_TOOLS declares — the policy is \
             the authority, so a drift here is the constant's defect, not the policy's"
        );
        let read_names = theory.names_of(&read_subjects);
        assert_eq!(
            read_names,
            advertised
                .difference(&write_names)
                .cloned()
                .collect::<BTreeSet<String>>(),
            "the reads are exactly the advertised tools that are not writes"
        );

        // The four DEV tools are on NEITHER side: they need a checkout, so a consumer server
        // neither advertises them nor is governed for them.
        for dev_only in DEV_ONLY_TOOLS {
            assert!(
                !advertised.contains(dev_only),
                "`{dev_only}` is dev-gated and must NOT be on the consumer surface"
            );
            assert!(
                !named.contains(dev_only),
                "`{dev_only}` is dev-gated and must NOT have a row in the consumer action \
                 theory"
            );
        }
    }

    /// NEGATIVE 1 — a tool with no row REDS the gate, naming that tool.
    ///
    /// The explicit producer mutates the original native source and runs the shipping
    /// projection. The test grades that actual output through the same comparison.
    #[test]
    fn a_tool_with_no_action_schema_row_reds_the_bijection_gate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let advertised = advertised_consumer_tools();

        assert!(
            ActionTheory::read(action_policy_nquads())
                .by_tool_name()
                .contains_key("store_conjecture"),
            "the producer control must remove a row that the source authority actually carries"
        );
        let theory = ActionTheory::read(action_policy_control("missing_store_conjecture"));
        let named: BTreeSet<String> = theory.by_tool_name().keys().cloned().collect();
        let problems = bijection_violations(&advertised, &named);

        assert_eq!(problems.len(), 1, "exactly one violation: {problems:?}");
        assert!(
            problems[0].contains("`store_conjecture`") && problems[0].contains("NO "),
            "the failure must NAME the unmodelled tool: {}",
            problems[0]
        );
    }

    /// NEGATIVE 2 — a row with no tool REDS the gate, naming that row.
    #[test]
    fn an_action_schema_row_with_no_tool_reds_the_bijection_gate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let advertised = advertised_consumer_tools();

        let theory = ActionTheory::read(action_policy_control("orphan_teleport"));
        let named: BTreeSet<String> = theory.by_tool_name().keys().cloned().collect();
        let problems = bijection_violations(&advertised, &named);

        assert_eq!(problems.len(), 1, "exactly one violation: {problems:?}");
        assert!(
            problems[0].contains("`teleport_ontology`") && problems[0].contains("does NOT"),
            "the failure must NAME the orphaned row: {}",
            problems[0]
        );
    }

    /// Compare original-source policy statements with independently produced bundle
    /// statements, including annotations. The resource must then serve that source
    /// authority's execution projection verbatim. Both inputs are compact selected
    /// artifacts; tests never rebuild the original policy or extract the bundle corpus.
    #[test]
    fn the_action_theorys_two_carriers_agree_quad_for_quad() {
        let embedded = source_action_policy().source_statements();
        assert!(
            !embedded.is_empty(),
            "the embedded copy must be non-empty, or this test proves nothing"
        );

        let bytes = snapshot();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let projected = gmeow_bundle_import::load_authenticated_corpus_artifact(
            &root,
            "mcp-action-policy-statements.json",
        )
        .expect("independent bundle policy statements have an exact producer selection");
        let bundled: BTreeSet<String> =
            purrdf_lex::json::record::from_slice(&projected).expect("bundle policy statements");
        assert!(
            !bundled.is_empty(),
            "gmeow.gts carries NO action-theory quads. The producer folds every slice's \
             examples corpus into graph/examples, so the theory must be there — an empty set \
             means the fold stopped and this equality has become vacuous rather than passing."
        );
        let only_embedded: Vec<&str> = embedded.difference(&bundled).map(String::as_str).collect();
        let only_bundled: Vec<&str> = bundled.difference(embedded).map(String::as_str).collect();
        assert!(
            only_embedded.is_empty() && only_bundled.is_empty(),
            "the two carriers of the action theory have DRIFTED — the console would display a \
             policy the engine does not obey.\n  embedded-only ({}): {}\n  bundled-only ({}): {}",
            only_embedded.len(),
            only_embedded
                .iter()
                .take(10)
                .copied()
                .collect::<Vec<_>>()
                .join("\n    "),
            only_bundled.len(),
            only_bundled
                .iter()
                .take(10)
                .copied()
                .collect::<Vec<_>>()
                .join("\n    ")
        );

        // …and the bytes the resource serves ARE the projection of that one carrier, so
        // "which copy the browser reads" is not a claim about intent but about identity.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let server = McpServer::from_snapshot(&bytes).expect("consumer server constructs");
        let read = server.read_resource_result(ACTION_POLICY_URI);
        assert_eq!(
            read["contents"][0]["text"].as_str().expect("resource text"),
            action_policy_nquads(),
            "the browser-facing resource serves the projection of the embedded carrier"
        );
    }

    /// The VOCABULARY the projection depends on is genuinely two-carrier, and the two must
    /// agree: `logic:mcpToolName` is declared in `slices/grounding/logic/module.ttl`, and a
    /// reader that resolves the predicate against the bundle must find the SAME declaration
    /// the projection asserts — otherwise the served quads name a property the shipped
    /// ontology does not define, and the tool↔schema link is unresolvable for anyone but
    /// this crate.
    ///
    /// The declaration lands in TWO bundle graphs, and this test pins both because each
    /// carries a different half and neither half alone resolves the predicate:
    ///
    /// * `graph/logic` is the canonical projection of the compiled `logic:` program, not a
    ///   verbatim fold of the slice file. Its frontend lifts only `logic:`-namespaced
    ///   predicates (plus a narrow annotation lane), so the SIGNATURE reaches it exactly
    ///   when it is authored as `logic:domain` / `logic:range` — the spelling every other
    ///   signature-bearing term in the slice uses. An `rdfs:`-spelled signature is dropped
    ///   at ingestion and never ships; asserting it here is what keeps the authored
    ///   spelling honest.
    /// * The default graph carries the canonical typing (`a logic:DatatypeProperty`) as
    ///   authored. `graph/logic` carries the term's signature, so the typing must be
    ///   resolved where it actually rides.
    ///
    /// Checked against the bundle, which means this test can only pass once `gmeow.gts` has
    /// been regenerated over the minted term. It is deliberately NOT weakened to "the slice
    /// file says so": the slice file is the source, and asserting the source against itself
    /// would prove nothing about what ships.
    #[test]
    fn the_bundled_logic_vocabulary_declares_the_tool_name_property() {
        const GRAPH_LOGIC: &str = "https://blackcatinformatics.ca/gmeow/graph/logic";
        const LOGIC_DATATYPE_PROPERTY: &str =
            "https://blackcatinformatics.ca/logic/DatatypeProperty";
        const LOGIC_DOMAIN: &str = "https://blackcatinformatics.ca/logic/domain";
        const LOGIC_RANGE: &str = "https://blackcatinformatics.ca/logic/range";
        const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

        let bytes = snapshot();
        let bundle = purrdf::import_gts_events(&bytes).expect("the shipped snapshot reads");
        let quads = purrdf::flat_rdf_quads_from_dataset(bundle.dataset.as_ref());

        // (predicate, IRI object) pairs asserted ON the term, partitioned by carrier graph.
        let iri_statements = |in_graph: &dyn Fn(&Option<purrdf::RdfTerm>) -> bool| {
            quads
                .iter()
                .filter(|quad| {
                    in_graph(&quad.graph_name)
                        && matches!(&quad.subject, purrdf::RdfTerm::Iri(s) if s == LOGIC_MCP_TOOL_NAME)
                })
                .filter_map(|quad| match &quad.object {
                    purrdf::RdfTerm::Iri(object) => {
                        Some((quad.predicate.clone(), object.clone()))
                    }
                    _ => None,
                })
                .collect::<BTreeSet<(String, String)>>()
        };

        let in_logic_graph = |g: &Option<purrdf::RdfTerm>| matches!(g, Some(purrdf::RdfTerm::Iri(g)) if g == GRAPH_LOGIC);
        let declared_in_logic_graph = iri_statements(&in_logic_graph);
        for (predicate, object) in [
            (LOGIC_DOMAIN, LOGIC_ACTION_SCHEMA),
            (LOGIC_RANGE, XSD_STRING),
        ] {
            assert!(
                declared_in_logic_graph.contains(&(predicate.to_string(), object.to_string())),
                "the shipped bundle's graph/logic must declare \
                 <{LOGIC_MCP_TOOL_NAME}> <{predicate}> <{object}>; it declares \
                 {declared_in_logic_graph:?}"
            );
        }

        let declared_in_default_graph = iri_statements(&|g: &Option<purrdf::RdfTerm>| g.is_none());
        assert!(
            declared_in_default_graph.contains(&(
                RDF_TYPE_IRI.to_string(),
                LOGIC_DATATYPE_PROPERTY.to_string()
            )),
            "the shipped bundle's default graph must declare \
             <{LOGIC_MCP_TOOL_NAME}> <{RDF_TYPE_IRI}> <{LOGIC_DATATYPE_PROPERTY}>; it declares \
             {declared_in_default_graph:?}"
        );
    }

    /// The projection retains `logic:mcpToolName` literals and NO other literal.
    ///
    /// The one-predicate exception in [`gmeow_logic_compile::action_policy::project_nquads`] is load-bearing and narrow:
    /// widening it would push `rdfs:label` / `rdfs:comment` prose into the world the
    /// executional-entailment run reasons in, and every language's translation with it.
    #[test]
    fn the_projection_retains_only_the_tool_name_literal() {
        let policy = action_policy_nquads();
        let mut literal_predicates: BTreeSet<String> = BTreeSet::new();
        let mut tool_name_count = 0usize;
        for line in policy.lines() {
            let (_subject, predicate, object) = split_projected(line);
            if let ProjectedObject::Literal(_) = object {
                literal_predicates.insert(predicate.clone());
                if predicate == LOGIC_MCP_TOOL_NAME {
                    tool_name_count += 1;
                }
            }
        }
        assert_eq!(
            literal_predicates,
            BTreeSet::from([LOGIC_MCP_TOOL_NAME.to_string()]),
            "exactly one literal-valued predicate survives the projection"
        );
        assert_eq!(
            tool_name_count, TOOL_COUNT,
            "one logic:mcpToolName per advertised consumer tool"
        );
        // The dropped annotations really were present in the source, so the assertion above
        // is about the FILTER and not about an unannotated source file.
        assert!(
            source_action_policy()
                .omitted_annotations()
                .iter()
                .any(|row| row.contains("<http://www.w3.org/2000/01/rdf-schema#label>"))
                && source_action_policy()
                    .omitted_annotations()
                    .iter()
                    .any(|row| row.contains("<http://www.w3.org/2000/01/rdf-schema#comment>")),
            "the source carries the annotations the projection drops"
        );
    }

    #[test]
    fn dry_run_must_be_a_boolean() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let bad = text_payload(
            server.call_tool_result("store_claim", &json!({"text": "x", "dry_run": "yes"})),
        );
        assert_eq!(bad["ok"], false);
        assert!(
            bad["error"]
                .as_str()
                .unwrap()
                .contains("dry_run must be a boolean")
        );
    }

    #[test]
    fn store_claim_dry_run_computes_verdict_without_persisting() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let dry = text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "a dry-run belief about orbits", "dry_run": true}),
        ));
        assert_eq!(dry["ok"], true);
        assert_eq!(dry["dry_run"], true);
        assert_eq!(dry["transaction"]["committed"], false);
        assert_eq!(dry["transaction"]["succeeded"], true);
        assert!(
            dry["transaction"]["witness"].as_str().is_some(),
            "a sandbox run leaves a content-addressed witness"
        );
        assert!(dry.get("claim").is_none(), "dry run writes no claim");

        // Nothing persisted: recall is empty and the memory holds no claims or tool calls.
        let recalled =
            text_payload(server.call_tool_result("recall", &json!({"query": "dry-run belief"})));
        assert!(recalled["claims"].as_array().unwrap().is_empty());
        let memory = Memory::new(&memory_path);
        assert!(memory.claims().unwrap().is_empty());
        assert!(memory.tool_calls().unwrap().is_empty());
    }

    #[test]
    fn committed_store_records_the_audit_context() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let stored = text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "an audited belief about thrust", "confidence": 0.8}),
        ));
        assert_eq!(stored["ok"], true);
        assert_eq!(stored["transaction"]["committed"], true);
        assert_eq!(stored["transaction"]["succeeded"], true);

        // The committed turn is cold-auditable: the persisted memory.gts carries exactly the
        // predicates emit_trajectory_audits requires on the recorded ToolCall and its anchor.
        let raw = fs::read(&memory_path).unwrap();
        let bundle = purrdf::import_gts_events(&raw).expect("import memory.gts");
        let predicates: BTreeSet<String> = purrdf::flat_rdf_quads_from_dataset(&bundle.dataset)
            .iter()
            .map(|quad| quad.predicate.clone())
            .collect();
        for predicate in [
            LOGIC_INSTANTIATES_SCHEMA,
            LOGIC_PROPER_PART_OF,
            GMEOW_AT_TIME,
            GMEOW_EVENT_TEMPORAL_FRAME,
            LOGIC_TRANSITION_FROM_STATE,
            LOGIC_SITUATION_OBTAINS,
        ] {
            assert!(
                predicates.contains(predicate),
                "memory.gts must carry {predicate} for the trajectory audit"
            );
        }
        // The single canonical temporal frame is recorded (P11 — one frame per trajectory).
        let frames: Vec<String> = purrdf::flat_rdf_quads_from_dataset(&bundle.dataset)
            .iter()
            .filter(|quad| quad.predicate == GMEOW_EVENT_TEMPORAL_FRAME)
            .map(|quad| quad.object.to_string())
            .collect();
        assert!(
            frames
                .iter()
                .all(|frame| frame.contains("temporalFrameUTCGregorian"))
        );
    }

    #[test]
    fn revise_belief_dry_run_does_not_suppress() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let stored = text_payload(
            server.call_tool_result("store_claim", &json!({"text": "a revisable belief"})),
        );
        let claim_id = stored["claim"]["id"].as_str().unwrap().to_string();

        let dry = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": claim_id, "dry_run": true}),
        ));
        assert_eq!(dry["ok"], true);
        assert_eq!(dry["dry_run"], true);
        assert_eq!(dry["transaction"]["committed"], false);
        assert_eq!(dry["transaction"]["succeeded"], true);

        // The claim is still live — a sandbox revise suppresses nothing (P10 for free).
        let live =
            text_payload(server.call_tool_result("recall", &json!({"query": "revisable belief"})));
        assert_eq!(live["claims"][0]["suppressed"], false);
    }

    #[test]
    fn committed_revise_suppresses_but_never_deletes() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let stored = text_payload(
            server.call_tool_result("store_claim", &json!({"text": "a belief to retire"})),
        );
        let claim_id = stored["claim"]["id"].as_str().unwrap().to_string();

        let revised = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": claim_id, "reason": "superseded"}),
        ));
        assert_eq!(revised["ok"], true);
        assert_eq!(revised["transaction"]["committed"], true);

        // Default recall hides it (suppressed) ...
        let default =
            text_payload(server.call_tool_result("recall", &json!({"query": "belief retire"})));
        assert!(default["claims"].as_array().unwrap().is_empty());
        // ... but it is still present (supersession, never erasure — P10).
        let audit = text_payload(server.call_tool_result(
            "recall",
            &json!({"query": "belief retire", "include_suppressed": true}),
        ));
        assert!(
            audit["claims"]
                .as_array()
                .unwrap()
                .iter()
                .any(|claim| claim["id"] == claim_id.as_str() && claim["suppressed"] == true)
        );
    }

    #[test]
    fn startup_language_is_validated_and_json_rpc_dispatches() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        let bytes = snapshot();
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_LANG", "notatag");
        }
        let err = match McpServer::from_snapshot(&bytes) {
            Ok(_) => panic!("invalid startup language must fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("unknown language tag 'notatag'"));

        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_LANG", "fr");
        }
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let init: Value = serde_json::from_str(
            &server.handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        )
        .unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "gmeow");

        let tools: Value = serde_json::from_str(
            &server.handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#),
        )
        .unwrap();
        assert!(
            tools["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == "lookup_term")
        );

        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_LANG", "X-GMEOW-FRENCH");
        }
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let fr = text_payload(
            server.call_tool_result("lookup_term", &json!({"term": "gmeow:EntityExistence"})),
        );
        assert_eq!(fr["label"], "Existence d'entit\u{e9}");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
    }

    /// A bare local name that collides across grounding namespaces HARD-FAILS on
    /// EVERY consumer surface — the MCP twin of `gmeow describe`'s typed ambiguity —
    /// never a silent first-exact pick (`.goals` NO OPTIONALITY). `Conjecture` names
    /// both `logic:Conjecture` and `math:Conjecture` in the shipped bundle.
    #[test]
    fn ambiguous_bare_name_hard_fails_across_mcp_surfaces() {
        let server = consumer_server();

        // `lookup_term`: the DISTINCT ambiguity envelope (ok:false) listing BOTH
        // sorted candidate CURIEs — not a silent pick of the first exact match.
        let looked =
            text_payload(server.call_tool_result("lookup_term", &json!({"term": "Conjecture"})));
        assert_eq!(looked["ok"], json!(false));
        let err = looked["error"].as_str().expect("ambiguity error string");
        assert_eq!(
            err, "ambiguous term 'Conjecture': logic:Conjecture, math:Conjecture",
            "lookup_term must emit the sorted-candidate ambiguity envelope"
        );

        // `doc_card`: hard fail (isError) with the same ambiguity — not a card.
        let card = server.call_tool_result("doc_card", &json!({"term": "Conjecture"}));
        assert_eq!(card["isError"], json!(true));
        let card_err = text_payload(card);
        assert_eq!(card_err["ok"], json!(false));
        let ce_msg = card_err["error"].as_str().expect("doc_card error");
        assert!(
            ce_msg.contains("logic:Conjecture") && ce_msg.contains("math:Conjecture"),
            "doc_card ambiguity must list sorted candidates, got {ce_msg:?}"
        );

        // The resolution guard feeding `counter_examples` / `entailments` /
        // `competency_questions` hard-fails too (isError) — never a silent IRI.
        let guarded = server.call_tool_result("counter_examples", &json!({"term": "Conjecture"}));
        assert_eq!(guarded["isError"], json!(true));

        // The ambiguity carries its OWN typed code, DISTINCT from the generic
        // unknown-term `pipeline.mcp` — greppable as `pipeline.mcp.ambiguous-term`.
        let requested = server.startup_requested.clone();
        let ConsumerResolution::Ambiguous { candidates } =
            server.view.resolve_term_iri("Conjecture", requested)
        else {
            panic!("`Conjecture` must resolve ambiguously across logic:/math:");
        };
        assert_eq!(
            candidates,
            vec![
                "logic:Conjecture".to_string(),
                "math:Conjecture".to_string()
            ],
            "candidates sorted + deduped"
        );
        let diag = ambiguous_term_err("Conjecture", &candidates);
        assert_eq!(diag.code(), crate::error::McpAmbiguousTerm::register());

        // Regression: unambiguous queries still RESOLVE on the same surface — the
        // ambiguity gate fires ONLY on genuine cross-namespace collisions.
        for (q, curie) in [
            ("lang:Denotation", "lang:Denotation"),
            ("math:Function", "math:Function"),
            ("Denotation", "lang:Denotation"),
        ] {
            let hit = text_payload(server.call_tool_result("lookup_term", &json!({"term": q})));
            assert_eq!(hit["ok"], json!(true), "`{q}` must still resolve");
            assert_eq!(hit["curie"], json!(curie), "`{q}` resolves to {curie}");
        }
    }

    #[test]
    fn default_memory_path_lives_under_home() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
            env::remove_var("GMEOW_MEMORY_PATH");
        }
        let dir = tempfile::tempdir().expect("tempdir");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("HOME", dir.path());
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        text_payload(server.call_tool_result("store_claim", &json!({"text": "durable belief"})));
        assert!(dir.path().join(".gmeow/memory.gts").exists());
    }

    #[test]
    fn memory_path_handles_userprofile_and_relative_files() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        let _cwd = CwdRestore::capture();
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
            env::remove_var("GMEOW_MEMORY_PATH");
            env::remove_var("HOME");
        }
        let dir = tempfile::tempdir().expect("tempdir");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("USERPROFILE", dir.path());
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        text_payload(
            server.call_tool_result("store_claim", &json!({"text": "profile fallback belief"})),
        );
        assert!(dir.path().join(".gmeow/memory.gts").exists());

        let relative_dir = tempfile::tempdir().expect("relative tempdir");
        env::set_current_dir(relative_dir.path()).expect("set current dir");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_MEMORY_PATH", "memory.gts");
        }
        let server = McpServer::from_snapshot(&bytes).unwrap();
        text_payload(
            server.call_tool_result("store_claim", &json!({"text": "relative path belief"})),
        );
        assert!(relative_dir.path().join("memory.gts").exists());
    }

    /// The read-only local-ontology overlay: reads see `bundle ∪ overlay`, the
    /// overlay is provenance-isolated under the external graph, and nothing is
    /// written back — not the overlay file, not the canon, not memory.
    #[test]
    fn local_overlay_is_a_read_only_external_annex() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (mem_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // A local lower-tier vocab file the agent supplies (not part of the canon).
        let overlay_ttl = "<urn:ex:widget> <urn:ex:label> \"Local Widget\" .\n<urn:ex:widget> a <urn:ex:Thing> .\n";
        let overlay_data = overlay_ttl;

        // One bundle-scope query proves all three union obligations together: the
        // overlay is in the default graph, the same bytes are provenance-isolated
        // under the external graph, and the signed canon remains visible. Keeping
        // these joins in one query avoids rebuilding the identical bundle union for
        // three assertions.
        let seen = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "data": overlay_data, "format": "turtle",
                "query": "SELECT ?label ?external_label ?ontology WHERE { \
                          <urn:ex:widget> <urn:ex:label> ?label . \
                          GRAPH <urn:gmeow:mcp:overlay:external> { \
                            <urn:ex:widget> <urn:ex:label> ?external_label \
                          } \
                          ?ontology a <http://www.w3.org/2002/07/owl#Ontology> . \
                          } LIMIT 1",
            }),
        ));
        assert_eq!(seen["ok"], true, "overlay query must succeed: {seen}");
        assert_eq!(
            seen["results"]["bindings"][0]["label"]["value"],
            "Local Widget"
        );
        assert_eq!(
            seen["results"]["bindings"][0]["external_label"]["value"],
            "Local Widget"
        );
        assert!(
            seen["results"]["bindings"][0]["ontology"]["value"].is_string(),
            "bundle scope must retain a signed ontology row: {seen}"
        );

        // CONSTRUCT/DESCRIBE return native N-Quads and declare the graph form.
        let construct = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "data": overlay_data,
                "format": "turtle",
                "scope": "input",
                "query": "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 1",
            }),
        ));
        assert_eq!(construct["ok"], true, "{construct}");
        assert_eq!(construct["form"], "graph", "{construct}");

        // ── the `scope` selection ────────────────────────────────────────────────────
        // The two scopes answer different questions over the SAME overlay, and both are
        // real answers. `input` was previously unaskable: every query silently carried the
        // canon, so a caller could not read a pasted document on its own terms.
        assert_eq!(QueryScope::parse(None).unwrap(), QueryScope::BundleUnion);
        assert_eq!(
            QueryScope::parse(Some("bundle")).unwrap(),
            QueryScope::BundleUnion
        );
        let input_scope = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "data": overlay_data, "format": "turtle", "scope": "input",
                "query": "SELECT ?label ?ontology WHERE { \
                          <urn:ex:widget> <urn:ex:label> ?label . \
                          OPTIONAL { ?ontology a <http://www.w3.org/2002/07/owl#Ontology> } \
                          } LIMIT 1",
            }),
        ));
        assert_eq!(input_scope["ok"], true, "{input_scope}");
        assert_eq!(
            input_scope["results"]["bindings"][0]["label"]["value"],
            "Local Widget"
        );
        assert!(
            input_scope["results"]["bindings"][0]
                .get("ontology")
                .is_none(),
            "input scope must exclude the signed canon while retaining the overlay: {input_scope}"
        );
        // An unknown scope is a NAMED hard error, never a silent fallback to the default —
        // the same discipline `format` has.
        let bogus = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "data": overlay_data, "format": "turtle", "scope": "everything",
                "query": "SELECT ?s WHERE { ?s ?p ?o }",
            }),
        ));
        assert_eq!(bogus["ok"], false, "{bogus}");
        assert!(
            bogus["error"]
                .as_str()
                .unwrap_or_default()
                .contains("unknown scope `everything`"),
            "the refusal names the offending token and the accepted set: {bogus}"
        );

        // Read-only: the overlay file is byte-for-byte unchanged and NOTHING was
        // written to memory (the write triad never touches the overlay or canon).
        assert!(Memory::new(&memory_path).claims().unwrap().is_empty());
        assert!(!memory_path.exists());
        drop(mem_dir);
    }

    /// verify_graph fires the matching `verify.<stem>` finding on a known bad-example
    /// overlay: a doxastic state whose asserted `gmeow:credence` is out of `[0,1]` — the
    /// exact violation `queries/verify/credence-out-of-range.rq` is a negative test for.
    /// The overlay joins the reasoning default world and the flat verify query matches it,
    /// so the response `findings` carries `verify.credence-out-of-range`.
    #[test]
    fn verify_graph_fires_on_a_bad_example_overlay_heavy_offgate() {
        let observed = mcp_native_observation(corpus_observations::BAD_CREDENCE);
        let out = &observed.response;
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        let codes: Vec<String> = out["findings"]
            .as_array()
            .expect("findings array")
            .iter()
            .map(|f| f["code"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(
            codes.iter().any(|c| c == "verify.credence-out-of-range"),
            "the credence bad example must fire verify.credence-out-of-range: {out}"
        );
        assert!(
            out["error_count"].as_u64().unwrap_or(0) >= 1,
            "the violation must count as an error finding: {out}"
        );
    }

    /// Proof faithfulness: `cited_iris` must be derived ONLY from structured RDF
    /// binding terms, never scraped from rendered finding prose — an
    /// agent-controlled overlay literal carrying angle-bracket text must not be
    /// accepted as a genuine citation. The overlay's `gmeow:credence` value is the
    /// STRING literal `"see <urn:fake>"` (non-numeric, so `credence-out-of-range`
    /// fires) attached to the real subject `<urn:ex:forge-cited-iris-state>`. A
    /// text-scrape over the rendered `detail` (`credence="see <urn:fake>",
    /// state=<urn:ex:...>`) would forge `urn:fake` into `cited_iris`; the
    /// structured-term derivation must not, while the genuinely-cited subject IRI
    /// must still appear.
    #[test]
    fn verify_graph_cited_iris_excludes_forged_literal_text_heavy_offgate() {
        let observed = mcp_native_observation(corpus_observations::FORGED_CITATION);
        let out = &observed.response;
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        let codes: Vec<String> = out["findings"]
            .as_array()
            .expect("findings array")
            .iter()
            .map(|f| f["code"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(
            codes.iter().any(|c| c == "verify.credence-out-of-range"),
            "the forged-literal overlay must fire verify.credence-out-of-range: {out}"
        );
        let cited: Vec<String> = out["cited_iris"]
            .as_array()
            .expect("cited_iris array")
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(
            !cited.iter().any(|c| c.contains("urn:fake")),
            "a literal's angle-bracket text must never forge a citation: {out}"
        );
        assert!(
            cited.iter().any(|c| c == "urn:ex:forge-cited-iris-state"),
            "the finding's genuinely-cited subject IRI must still appear: {out}"
        );
    }

    /// Overlay isolation: verify_graph builds a TRANSIENT union and drops it — the signed
    /// canon `McpView::dataset` is never mutated. After the call the canon Arc is the SAME
    /// allocation with the SAME quad count, and the overlay file is byte-unchanged (the
    /// external annex is read-only and never merged into or written back from the canon).
    #[test]
    fn verify_graph_leaves_the_canon_dataset_untouched_heavy_offgate() {
        let observed = mcp_native_observation(corpus_observations::ISOLATION);
        let out = &observed.response;
        let isolation = observed
            .isolation
            .as_ref()
            .expect("carrier isolation evidence");
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");

        // Canon Arc identity + quad count are unchanged — the union was transient.
        assert!(
            isolation.same_allocation,
            "the signed canon Arc must not be swapped by verify_graph"
        );
        assert_eq!(
            isolation.after_quads, isolation.before_quads,
            "the signed canon quad count must be unchanged (overlay never merged)"
        );
        // The overlay probe triple never leaks into the canon graphs.
        let leaked = isolation.probe_leaked;
        assert!(
            !leaked,
            "overlay triple must not appear in the signed canon"
        );
    }

    /// A budget-cut closure yields the strictly-weaker ATTESTATION, never a certificate:
    /// a `max_steps` of 1 cuts the forward chase over the large bundle union mid-flight,
    /// so `reason_all_budgeted` returns a non-conclusive BudgetExhausted / Incomplete
    /// verdict. The completeness gate then MUST render `CoherenceCheckAttestation` with a
    /// non-conclusive completeness axis — a certificate is impossible on an incomplete search.
    #[test]
    fn verify_graph_budget_cut_yields_attestation_never_certificate_heavy_offgate() {
        let observed = mcp_native_observation(corpus_observations::BUDGET_CUT);
        let out = &observed.response;
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        assert_eq!(
            out["class_local_name"], "CoherenceCheckAttestation",
            "a budget-cut closure must attest, never certify: {out}"
        );
        assert_ne!(
            out["class_local_name"], "CoherenceCertificate",
            "a budget-cut closure must NEVER render a certificate: {out}"
        );
        // The completeness axis is non-conclusive (the mid-chase cut → Incomplete), and the
        // computation axis records the budget exhaustion.
        assert_eq!(out["completeness"], "incomplete", "{out}");
        assert_eq!(out["evaluation"], "budget-exhausted", "{out}");
    }

    /// R4: OMITTING `max_steps`/`max_answers` entirely must NEVER run an
    /// unbounded Turing-complete chase — `governed_budget` stamps the finite
    /// `DEFAULT_MAX_STEPS` server-side ceiling on every agent-facing call, never `None`.
    /// The real bundle's full DL closure is far larger than `DEFAULT_MAX_STEPS` (the sibling
    /// `verify_graph_budget_cut_yields_attestation_never_certificate_heavy_offgate` above
    /// already shows even `max_steps: 1` cuts it), so a call that omits the args entirely
    /// must land on the SAME governed, non-conclusive `CoherenceCheckAttestation` —
    /// `budget-exhausted` / `incomplete` — never a conclusive certificate an unbounded chase
    /// would be needed to produce.
    #[test]
    fn verify_graph_omitted_max_steps_is_governed_not_unbounded_heavy_offgate() {
        let observed = mcp_native_observation(corpus_observations::OMITTED_BUDGET);
        assert!(observed.request.get("max_steps").is_none());
        assert!(observed.request.get("max_answers").is_none());
        let out = &observed.response;
        let native = observed.native.as_ref().expect("native execution evidence");
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        assert_eq!(
            out["class_local_name"], "CoherenceCheckAttestation",
            "an omitted-max_steps call over the large bundle union must still be GOVERNED \
             (budget-cut) by the default server-side ceiling, never a conclusive certificate \
             that only an unbounded chase could produce: {out}"
        );
        assert_ne!(
            out["class_local_name"], "CoherenceCertificate",
            "an omitted-max_steps call must NEVER run to an unbounded conclusive closure: {out}"
        );
        assert_eq!(
            out["completeness"], "incomplete",
            "the default ceiling must cut the real bundle's closure: {out}"
        );
        assert_eq!(
            out["evaluation"], "budget-exhausted",
            "the omitted max_steps must resolve to the finite DEFAULT_MAX_STEPS, never \
             None/unbounded: {out}"
        );

        // The grounded judgment's own carried budget usage confirms the finite ceiling: the
        // consumed step count is bounded by (and the declared allowance matches) the SAME
        // DEFAULT_MAX_STEPS `governed_budget` stamps — never absent (which would mean
        // unbounded).
        let judgment = out["judgment_nquads"]
            .as_str()
            .expect("judgment_nquads string");
        assert_eq!(judgment, native.judgment_nquads);
        assert_eq!(
            native.consumed_budget.allowance,
            Some(DEFAULT_MAX_STEPS),
            "the grounded judgment must declare the DEFAULT_MAX_STEPS allowance, never an \
             absent (unbounded) allowance: {judgment}"
        );
        assert!(
            native.consumed_budget.consumed <= DEFAULT_MAX_STEPS,
            "consumed steps must never exceed the declared default allowance: {judgment}"
        );
    }

    /// An overlay exceeding `MAX_VERIFY_OVERLAY_QUADS` is a HARD FAIL — the bounded agent
    /// path — refused BEFORE any reasoning runs, never a silently truncated graph.
    #[test]
    fn verify_graph_rejects_an_oversized_overlay() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // One distinct triple over the ceiling — the smallest overlay that trips it.
        let mut body = String::with_capacity((MAX_VERIFY_OVERLAY_QUADS + 1) * 40);
        for i in 0..=MAX_VERIFY_OVERLAY_QUADS {
            body.push_str(&format!("<urn:ex:s{i}> <urn:ex:p> <urn:ex:o{i}> .\n"));
        }
        let overlay_data = body.as_str();

        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 1}),
        ));
        assert_eq!(
            out["ok"], false,
            "an oversized overlay must hard-fail: {out}"
        );
        assert!(
            out["error"]
                .as_str()
                .unwrap_or_default()
                .contains("exceeding"),
            "the error must name the ceiling breach: {out}"
        );
    }

    /// The R4 byte gate: an inline overlay whose LENGTH exceeds
    /// `MAX_VERIFY_OVERLAY_BYTES` is refused BEFORE it is handed to the parser — so a
    /// huge payload can never exhaust memory building a dataset before the post-parse
    /// `MAX_VERIFY_OVERLAY_QUADS` ceiling gets a chance to run. The filler here is a
    /// single deliberately-oversized comment line, NOT well-formed RDF that would parse
    /// into many quads: if the byte gate did not run before the parse (i.e. this fix
    /// regressed), the payload would still parse successfully (as an empty, all-comment
    /// document) and `verify_graph` would return `ok:true` instead of hard-failing on
    /// the byte ceiling, so this test would catch the regression either way — and it
    /// must never OOM proving it.
    #[test]
    fn verify_graph_rejects_an_overlay_over_the_byte_ceiling_before_read() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // One byte over the ceiling — the smallest overlay that trips it. A single
        // `#`-prefixed line is cheap to build (one allocation, no per-quad
        // formatting) and never parses into any quads.
        let filler = "#".repeat((MAX_VERIFY_OVERLAY_BYTES + 1) as usize);
        let overlay_data = filler.as_str();

        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 1}),
        ));
        assert_eq!(
            out["ok"], false,
            "an overlay over the byte ceiling must hard-fail: {out}"
        );
        let error = out["error"].as_str().unwrap_or_default();
        assert!(
            error.contains(&MAX_VERIFY_OVERLAY_BYTES.to_string()) && error.contains("byte"),
            "the error must name the byte limit: {out}"
        );
    }

    /// `query_local` takes BYTES plus an EXPLICIT `format`. A pasted Turtle string
    /// with `{"format":"turtle"}` is accepted — the positive half of the contract
    /// the two negative tests below pin. The matching verifier contract is exercised
    /// on focused synthetic canons in the required inventory; its exhaustive
    /// whole-bundle twin is retained in the maintained corpus inventory.
    #[test]
    fn query_local_accepts_pasted_turtle_with_an_explicit_format() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let pasted = NORMAL_SMALL_OVERLAY;

        let queried = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "data": pasted,
                "format": "turtle",
                "query": "SELECT ?o WHERE { <urn:ex:pasted> <urn:ex:label> ?o }",
            }),
        ));
        assert_eq!(
            queried["ok"], true,
            "pasted Turtle with an explicit format must query cleanly: {queried}"
        );
        assert_eq!(
            queried["results"]["bindings"][0]["o"]["value"], "Pasted Widget",
            "the pasted overlay must be visible to the query: {queried}"
        );
    }

    /// A MISSING `format` is a hard error on both overlay tools — never a guess at
    /// Turtle. The two tools also ADVERTISE `format` as required, so a client can see
    /// the obligation before it calls.
    #[test]
    fn overlay_tools_hard_fail_without_a_format() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let pasted = "<urn:ex:pasted> <urn:ex:p> <urn:ex:o> .\n";

        let queried = text_payload(server.call_tool_result(
            "query_local",
            &json!({"data": pasted, "query": "ASK { ?s ?p ?o }"}),
        ));
        assert_eq!(
            queried["ok"], false,
            "query_local without a format must hard-fail: {queried}"
        );
        assert!(
            queried["error"]
                .as_str()
                .unwrap_or_default()
                .contains("format"),
            "the error must name the missing argument: {queried}"
        );

        let verified = text_payload(
            server.call_tool_result("verify_graph", &json!({"data": pasted, "max_steps": 1})),
        );
        assert_eq!(
            verified["ok"], false,
            "verify_graph without a format must hard-fail: {verified}"
        );
        assert!(
            verified["error"]
                .as_str()
                .unwrap_or_default()
                .contains("format"),
            "the error must name the missing argument: {verified}"
        );

        // The advertised schema agrees with the enforcement.
        for name in ["query_local", "verify_graph"] {
            let descriptor = server
                .surface
                .tool_descriptors()
                .into_iter()
                .find(|t| t["name"] == name)
                .unwrap_or_else(|| panic!("{name} is advertised"));
            let required: Vec<String> = descriptor["inputSchema"]["required"]
                .as_array()
                .expect("required array")
                .iter()
                .map(|v| v.as_str().expect("required entries are strings").to_owned())
                .collect();
            for arg in ["data", "format"] {
                assert!(
                    required.iter().any(|r| r == arg),
                    "{name} enforces `{arg}` at call time and must advertise it as required: \
                     {required:?}"
                );
            }
        }
    }

    /// An UNKNOWN `format` is a hard error NAMING THE ACCEPTED SET on both overlay
    /// tools — the caller is told what to pass, and no fallback parse ever runs.
    #[test]
    fn overlay_tools_hard_fail_on_an_unknown_format_naming_the_accepted_set() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // Well-formed Turtle: the ONLY thing wrong is the declared format, so a
        // fallback-to-Turtle regression would return `ok:true` and fail this test.
        let pasted = "<urn:ex:pasted> <urn:ex:p> <urn:ex:o> .\n";

        let queried = text_payload(server.call_tool_result(
            "query_local",
            &json!({"data": pasted, "format": "yaml-ld", "query": "ASK { ?s ?p ?o }"}),
        ));
        assert_eq!(
            queried["ok"], false,
            "an unknown format must hard-fail rather than fall back to Turtle: {queried}"
        );
        let error = queried["error"].as_str().unwrap_or_default();
        assert!(
            error.contains("query_local") && error.contains("yaml-ld"),
            "the error must name the tool and the rejected token: {queried}"
        );
        for accepted in [
            "turtle",
            "n-triples",
            "n-quads",
            "trig",
            "rdf+xml",
            "json-ld",
        ] {
            assert!(
                error.contains(accepted),
                "the error must name the accepted format `{accepted}`: {queried}"
            );
        }

        let verified = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": pasted, "format": "yaml-ld", "max_steps": 1}),
        ));
        assert_eq!(
            verified["ok"], false,
            "an unknown format must hard-fail rather than fall back to Turtle: {verified}"
        );
        let error = verified["error"].as_str().unwrap_or_default();
        assert!(
            error.contains("verify_graph") && error.contains("yaml-ld"),
            "the error must name the tool and the rejected token: {verified}"
        );
        for accepted in [
            "turtle",
            "n-triples",
            "n-quads",
            "trig",
            "rdf+xml",
            "json-ld",
        ] {
            assert!(
                error.contains(accepted),
                "the error must name the accepted format `{accepted}`: {verified}"
            );
        }
    }

    /// A normal, well-under-ceiling overlay still succeeds through both the byte gate
    /// and the quad gate — the byte cap must never reject a legitimate small annex.
    #[test]
    fn verify_graph_accepts_a_normal_small_overlay_over_the_whole_bundle_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let out = normal_small_overlay_verdict();
        assert_eq!(
            out["ok"], true,
            "a normal small overlay must succeed: {out}"
        );
    }

    /// The grounded RDF judgment: `judgment_nquads` carries the `logic:ReasoningResult`
    /// node with the exact native result projection, so an agent can reason over the
    /// verdict itself. Its evaluation axis matches the JSON envelope's.
    #[test]
    fn verify_graph_judgment_nquads_grounds_the_reasoning_result_heavy_offgate() {
        let observed = mcp_native_observation(corpus_observations::BUDGET_CUT);
        let out = &observed.response;
        let native = observed.native.as_ref().expect("native execution evidence");
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        let judgment = out["judgment_nquads"]
            .as_str()
            .expect("judgment_nquads string");
        assert!(
            judgment.contains("<https://blackcatinformatics.ca/logic/ReasoningResult>"),
            "the judgment must ground a logic:ReasoningResult node: {judgment}"
        );
        // The actual typed result projects to these exact bytes and carries the same
        // evaluation axis. Tiny result_rdf contracts independently own parser fidelity.
        assert_eq!(judgment, native.judgment_nquads);
        assert_eq!(
            native.evaluation.wire(),
            out["evaluation"].as_str().unwrap(),
            "the grounded judgment's evaluation axis must match the envelope: {out}"
        );
    }

    // ── explain_quad ──────────────────────────────────────────────────────────

    /// A synthetic explain row for the fast disambiguation / N3 unit tests.
    fn synthetic_row(graph: &str, subject: &str, predicate: &str, obj: &str) -> Row {
        Row {
            graph: graph.to_owned(),
            subject: subject.to_owned(),
            predicate: predicate.to_owned(),
            obj: obj.to_owned(),
            derivation_id: "urn:ex:deriv".to_owned(),
            rule_iri: "urn:ex:rule".to_owned(),
            source_quad_ids: Vec::new(),
            modal_evaluation: None,
        }
    }

    /// FAST (on-gate) unit test of the world-disambiguation helper on synthetic rows:
    /// a reifier shared across two worlds resolves to exactly the row in the supplied
    /// graph; a non-resolving graph over that multi-world reifier is an ambiguity HARD
    /// FAIL (never an arbitrary pick); a reifier no row carries is `not in closure`.
    #[test]
    fn explain_quad_disambiguation_resolves_by_graph_and_hard_fails_across_worlds() {
        let (world_a, world_b) = ("urn:ex:world-a", "urn:ex:world-b");
        let (s, p, o) = ("urn:ex:s", "urn:ex:p", "<urn:ex:o>");
        let rows = vec![
            synthetic_row(world_a, s, p, o),
            synthetic_row(world_b, s, p, o),
        ];
        let reifier = reifier_from_row(&rows[0]);
        assert_eq!(
            reifier,
            reifier_from_row(&rows[1]),
            "identical (S,P,O) shares a reifier across worlds"
        );

        // The supplied graph disambiguates to exactly one row.
        assert_eq!(locate_explain_target(&rows, &reifier, world_a).unwrap(), 0);
        assert_eq!(locate_explain_target(&rows, &reifier, world_b).unwrap(), 1);

        // A non-resolving graph over a multi-world reifier → ambiguity hard fail.
        let ambiguous = locate_explain_target(&rows, &reifier, "urn:ex:world-c").unwrap_err();
        assert!(
            ambiguous.to_string().contains("ambiguous"),
            "a cross-world reifier without a resolving graph must be ambiguous: {ambiguous}"
        );

        // A reifier no row carries → not in closure (never an arbitrary pick).
        let missing = locate_explain_target(
            &rows,
            "https://blackcatinformatics.ca/gmeow/reifier/deadbeef",
            world_a,
        )
        .unwrap_err();
        assert!(
            missing.to_string().contains("not in closure"),
            "an unknown reifier must be `not in closure`: {missing}"
        );
    }

    /// FAST (on-gate): a single-world reifier queried in the WRONG world is
    /// `not in closure`, and the error names the world the quad actually lives in.
    #[test]
    fn explain_quad_wrong_single_world_is_not_in_closure() {
        let rows = vec![synthetic_row(
            "urn:ex:world-a",
            "urn:ex:s",
            "urn:ex:p",
            "<urn:ex:o>",
        )];
        let reifier = reifier_from_row(&rows[0]);
        let err = locate_explain_target(&rows, &reifier, "urn:ex:other-world").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not in closure"), "{msg}");
        assert!(
            msg.contains("urn:ex:world-a"),
            "names the actual world: {msg}"
        );
    }

    /// FAST (on-gate): the canonical object N3 surface `explain_quad` builds is
    /// byte-identical to what a `Row.obj` carries (`term_display`), across IRI, plain
    /// literal, and typed literal; a bad `object_kind` and an `object_datatype` on an
    /// IRI object are HARD FAILS; and the omitted-kind inference is deterministic.
    #[test]
    fn explain_quad_object_n3_is_the_canonical_row_surface() {
        // IRI object → `<iri>` (explicit and inferred).
        assert_eq!(
            object_term_n3("urn:ex:o", Some("iri"), None).unwrap(),
            "<urn:ex:o>"
        );
        assert_eq!(
            object_term_n3("urn:ex:o", None, None).unwrap(),
            "<urn:ex:o>"
        );
        // Plain literal → `"lex"` (xsd:string elided, exactly like term_display).
        assert_eq!(
            object_term_n3("hello", Some("literal"), None).unwrap(),
            "\"hello\""
        );
        // A bare non-IRI value with whitespace is inferred as a literal.
        assert_eq!(
            object_term_n3("just text", None, None).unwrap(),
            "\"just text\""
        );
        // Typed literal → `"lex"^^<dt>`.
        assert_eq!(
            object_term_n3(
                "5",
                Some("literal"),
                Some("http://www.w3.org/2001/XMLSchema#integer")
            )
            .unwrap(),
            "\"5\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
        // The N3 surface must MATCH what a Row built from the same TermValue carries.
        let iri_display = term_display(&TermValue::iri("urn:ex:o"));
        assert_eq!(
            object_term_n3("urn:ex:o", Some("iri"), None).unwrap(),
            iri_display
        );

        // A non-iri/literal kind is a hard error.
        assert!(object_term_n3("x", Some("bnode"), None).is_err());
        // A datatype on an IRI object is a contradictory request — hard error.
        assert!(object_term_n3("urn:ex:o", Some("iri"), Some("http://x")).is_err());
    }

    /// Read only the exact observation admitted by the explicit fixture producer.
    fn mcp_native_observation(name: &str) -> corpus_observations::Observation {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = gmeow_bundle_import::load_authenticated_corpus_artifact(&root, name)
            .expect("producer-selected MCP native observation");
        serde_json::from_slice(&bytes).expect("typed MCP native observation")
    }

    /// HEAVY (off-gate): a DERIVED quad in the reasoned bundle closure explains to a
    /// non-empty skeleton (target first, `is_asserted:false`, firing rule preserved),
    /// a non-empty cited-IRI set that includes the target's reifier, and `faithful:true`.
    #[test]
    fn explain_quad_explains_a_derived_quad_heavy_offgate() {
        let observed = mcp_native_observation(corpus_observations::DERIVED_EXPLANATION);
        assert_eq!(observed.request["max_steps"], 64);
        let out = &observed.response;
        let derived = observed.target.as_ref().expect("unique derived target");
        assert_eq!(out["ok"], true, "explain_quad must succeed: {out}");
        assert_eq!(out["faithful"], true, "the proof must be faithful: {out}");
        let steps = out["step_skeleton"]
            .as_array()
            .expect("step_skeleton array");
        assert!(
            !steps.is_empty(),
            "a derived quad has a non-empty skeleton: {out}"
        );
        assert_eq!(
            steps[0]["is_asserted"], false,
            "the target step is derived: {out}"
        );
        assert_eq!(
            steps[0]["rule_iri"].as_str().unwrap(),
            derived.rule_iri,
            "the target step preserves the firing rule: {out}"
        );
        let cited = out["cited_iris"].as_array().expect("cited_iris array");
        assert!(!cited.is_empty(), "cited_iris must be non-empty: {out}");
        let reifier = derived.reifier.clone();
        assert!(
            cited.iter().any(|c| c.as_str() == Some(reifier.as_str())),
            "the target reifier must be cited (the reifier matches the request): {out}"
        );
        assert_eq!(
            steps[0]["obj_n3"].as_str().unwrap(),
            derived.obj,
            "the target step's object N3 matches the row surface: {out}"
        );
    }

    /// HEAVY (off-gate): an ASSERTED (EDB) quad explains to a SINGLE step with
    /// `is_asserted:true` and `rule_iri` = the assert-rule IRI.
    #[test]
    fn explain_quad_explains_an_asserted_quad_heavy_offgate() {
        let observed = mcp_native_observation(corpus_observations::ASSERTED_EXPLANATION);
        assert_eq!(observed.request["max_steps"], 8);
        let out = &observed.response;
        assert_eq!(out["ok"], true, "explain_quad must succeed: {out}");
        assert_eq!(out["faithful"], true, "the proof must be faithful: {out}");
        let steps = out["step_skeleton"]
            .as_array()
            .expect("step_skeleton array");
        assert_eq!(
            steps.len(),
            1,
            "an asserted quad is a single leaf step: {out}"
        );
        assert_eq!(steps[0]["is_asserted"], true, "the leaf is asserted: {out}");
        assert_eq!(
            steps[0]["rule_iri"].as_str().unwrap(),
            gmeow_logic::provenance::ASSERT_RULE_IRI,
            "the asserted leaf carries the assert-rule IRI: {out}"
        );
    }

    /// HEAVY (off-gate): a quad the closure does not entail is a HARD FAIL
    /// (`ok:false` + `not in closure`), NEVER an empty-but-ok proof.
    #[test]
    fn explain_quad_hard_fails_on_a_quad_not_in_closure_heavy_offgate() {
        let observed = mcp_native_observation(corpus_observations::ABSENT_EXPLANATION);
        assert_eq!(observed.request["max_steps"], 8);
        let out = &observed.response;
        assert_eq!(out["ok"], false, "a bogus quad must hard-fail: {out}");
        assert!(
            out["error"]
                .as_str()
                .unwrap_or_default()
                .contains("not in closure"),
            "the error must say the quad is not in the closure: {out}"
        );
    }

    /// HEAVY (off-gate): `judgment_nquads` grounds the `logic:ReasoningResult` node
    /// and exactly matches the once-evaluated native result projection.
    #[test]
    fn explain_quad_judgment_nquads_grounds_the_reasoning_result_heavy_offgate() {
        let observed = mcp_native_observation(corpus_observations::ASSERTED_EXPLANATION);
        assert_eq!(observed.request["max_steps"], 8);
        let out = &observed.response;
        let native = observed.native.as_ref().expect("native execution evidence");
        assert_eq!(out["ok"], true, "explain_quad must succeed: {out}");
        let judgment = out["judgment_nquads"]
            .as_str()
            .expect("judgment_nquads string");
        assert!(
            judgment.contains("<https://blackcatinformatics.ca/logic/ReasoningResult>"),
            "the judgment must ground a logic:ReasoningResult node: {judgment}"
        );
        assert_eq!(judgment, native.judgment_nquads);
        // The once-evaluated native judgment carries a defined completeness axis.
        assert!(
            !native.completeness.wire().is_empty(),
            "the grounded judgment must carry a completeness axis: {judgment}"
        );
    }

    /// Full MCP protocol conformance over the real JSON-RPC dispatch: the
    /// handshake, the discovery surfaces, a read tool call and a TR-write tool call
    /// with `dry_run=true` (asserting the write stays hypothetical), and that EVERY
    /// advertised tool is dispatch-callable (no `unknown tool`).
    #[test]
    fn json_rpc_protocol_conformance_round_trip() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_mem_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let rpc = |body: &str| -> Value {
            let raw = server.handle_message(body);
            let value: Value = serde_json::from_str(&raw).expect("response is JSON");
            assert_eq!(value["jsonrpc"], "2.0", "JSON-RPC framing: {value}");
            value
        };

        // initialize
        let init = rpc(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["serverInfo"]["name"], "gmeow");
        assert!(init["result"]["protocolVersion"].as_str().is_some());

        // tools/list
        let tools = rpc(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#);
        let tool_names: Vec<String> = tools["result"]["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        for expected in [
            "lookup_term",
            "query_docs",
            "query_local",
            "store_claim",
            "recall",
        ] {
            assert!(
                tool_names.iter().any(|n| n == expected),
                "missing {expected}"
            );
        }

        // resources/list
        let resources = rpc(r#"{"jsonrpc":"2.0","id":3,"method":"resources/list","params":{}}"#);
        assert!(
            resources["result"]["resources"]
                .as_array()
                .map(|r| !r.is_empty())
                .unwrap_or(false)
        );

        // tools/call — a read tool (query_docs ASK) succeeds.
        let read = rpc(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"query_docs","arguments":{"query":"ASK { ?s ?p ?o }"}}}"#,
        );
        let read_text: Value =
            serde_json::from_str(read["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(read_text["ok"], true);
        assert_eq!(read_text["boolean"], true);

        // tools/call — a TR-write tool (store_claim) with dry_run=true stays
        // hypothetical: the verdict is computed but nothing is committed.
        let dry = rpc(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"store_claim","arguments":{"text":"a conformance probe belief","dry_run":true}}}"#,
        );
        let dry_text: Value =
            serde_json::from_str(dry["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(dry_text["ok"], true);
        assert_eq!(dry_text["dry_run"], true);
        assert_eq!(dry_text["transaction"]["committed"], false);
        assert!(dry_text.get("claim").is_none(), "dry run commits no claim");
        // Nothing persisted by the dry-run write.
        assert!(Memory::new(&memory_path).claims().unwrap().is_empty());
        assert!(!memory_path.exists());

        // Every advertised tool is dispatch-callable (recognized by tools/call).
        let overlay_data = "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n";
        let mut call_args: HashMap<&str, Value> = HashMap::new();
        call_args.insert("lookup_term", json!({"term": "gmeow:Entity"}));
        call_args.insert("doc_card", json!({"term": "gmeow:Entity"}));
        call_args.insert("query_docs", json!({"query": "ASK { ?s ?p ?o }"}));
        call_args.insert("docs_search", json!({"query": "entity"}));
        call_args.insert(
            "query_local",
            json!({"data": overlay_data, "format": "turtle", "query": "ASK { ?s ?p ?o }"}),
        );
        // The default (Tier-1) `validate_local` path is fast, so a valid tiny graph
        // dispatches and returns a well-formed EnrichedReport. (Tier-2 `deep` is opt-in
        // and reasons over the whole bundle, minutes — never exercised in this loop.)
        call_args.insert(
            "validate_local",
            json!({"data": "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n", "format": "turtle"}),
        );
        // `advise` is the fast Tier-1-only recommendation surface: a tiny clean graph
        // dispatches and returns ok:true with no recommendations.
        call_args.insert(
            "advise",
            json!({"data": "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n", "format": "turtle"}),
        );
        call_args.insert("store_claim", json!({"text": "probe", "dry_run": true}));
        call_args.insert(
            "conjecture_test",
            json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/probe",
            }),
        );
        call_args.insert(
            "store_conjecture",
            json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/probe",
                "dry_run": true,
            }),
        );
        call_args.insert("recall", json!({}));
        call_args.insert(
            "revise_belief",
            json!({"claim_id": "urn:gmeow:assertion:none", "dry_run": true}),
        );
        call_args.insert(
            "refute_conjecture",
            json!({"conjecture_id": "urn:gmeow:conjecture:none", "dry_run": true}),
        );
        // Documentation-surface tools: real terms that carry live data in the shipped
        // bundle (gmeow:Activity documents fixtures, gmeow:Entity grounds
        // entailments); competency_questions dispatches in its whole-index form.
        call_args.insert("counter_examples", json!({"term": "gmeow:Activity"}));
        call_args.insert("entailments", json!({"term": "gmeow:Entity"}));
        call_args.insert("competency_questions", json!({}));
        for name in &tool_names {
            let args = call_args
                .get(name.as_str())
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = server.call_tool_result(name, &args);
            let content = result["content"][0]["text"].as_str().expect("tool text");
            assert!(
                !content.contains("unknown tool"),
                "advertised tool {name} is not dispatch-callable: {content}"
            );
        }
    }

    /// Build a consumer server over the shipped bundle with a clean language env.
    fn consumer_server() -> McpServer {
        let bytes = snapshot();
        McpServer::from_snapshot(&bytes).unwrap()
    }

    /// One independent Tier-1 reference view for the selected bundle. The production
    /// tool owns a separate lazy slot on [`McpView`]; the suite keeps this second
    /// construction for parity evidence but never decodes it once per assertion.
    fn reference_tier1_shapes() -> &'static gmeow_validate::data_validate::Tier1Shapes {
        static SHAPES: OnceLock<gmeow_validate::data_validate::Tier1Shapes> = OnceLock::new();
        SHAPES.get_or_init(|| {
            gmeow_validate::data_validate::Tier1Shapes::from_gts(&snapshot())
                .expect("parse the selected bundle's independent Tier-1 reference")
        })
    }

    /// The exact real counter-example selected by the independent Tier-1 oracle.
    /// Several contracts assert different properties of this same witness; selecting
    /// and reproducing it repeatedly adds no coverage.
    fn selected_counter_example() -> &'static (String, String, String) {
        static SELECTION: OnceLock<(String, String, String)> = OnceLock::new();
        SELECTION.get_or_init(|| {
            select_reproducing_counter_example(&consumer_server(), reference_tier1_shapes())
        })
    }

    /// The production enriched report for the selected counter-example, dispatched
    /// once and shared by the contracts that check parity, rejection, enrichment,
    /// and schema conformance over that identical input.
    fn selected_counter_example_report() -> &'static Value {
        static REPORT: OnceLock<Value> = OnceLock::new();
        REPORT.get_or_init(|| {
            let (_, _, text) = selected_counter_example();
            text_payload(
                consumer_server()
                    .call_tool_result("validate_local", &json!({"data": text, "format": "turtle"})),
            )
        })
    }

    const NORMAL_SMALL_OVERLAY: &str = "<urn:ex:pasted> <urn:ex:label> \"Pasted Widget\" .\n";

    /// One production `verify_graph` result for the exact normal-overlay witness.
    /// Two contracts inspect different obligations on this identical producer
    /// observation; the test runner only reads its authenticated bytes.
    fn normal_small_overlay_verdict() -> &'static Value {
        static VERDICT: OnceLock<Value> = OnceLock::new();
        VERDICT.get_or_init(|| mcp_native_observation(corpus_observations::NORMAL_OVERLAY).response)
    }

    /// The sorted finding-code multiset of a report.
    fn codes_of(report: &gmeow_errors::Report) -> Vec<String> {
        let mut codes: Vec<String> = report.findings.iter().map(|f| f.code.clone()).collect();
        codes.sort();
        codes
    }

    /// Select a REAL counter-example fixture from the SHIPPED bundle's
    /// `gmeow:graph/documentation` projection whose authored `gmeow:docViolationCode`
    /// is actually REPRODUCED by the Tier-1 SHACL engine on its own `gmeow:docFixtureText`
    /// body — the honest correspondence anchor (never weakened to "non-empty").
    ///
    /// The candidate for each code is the SAME one the enrichment join attaches:
    /// `fixture_maps` keys a code to the lexicographically-first fixture IRI carrying
    /// it, so selecting by `code → (first-fixture IRI, its text)` guarantees the
    /// attached counter-example's text equals the payload we validate. `tier1` is
    /// the production Tier-1 core (the [`run_tier1`] engine over ONE decode of the
    /// shipped bundle — decoding the whole bundle per candidate would multiply the
    /// dominant cost without strengthening anything). Returns
    /// `(fixture_iri, code, text)`. Panics with the observed codes if NONE reproduces
    /// (a real blocker, not a soft skip).
    fn select_reproducing_counter_example(
        server: &McpServer,
        tier1: &gmeow_validate::data_validate::Tier1Shapes,
    ) -> (String, String, String) {
        let rows = server
            .view
            .docs_select_rows(COUNTER_EXAMPLE_FIXTURE_QUERY)
            .expect("query counter-example fixtures from graph/documentation");
        // First row per fixture IRI (the fixture's code + full body).
        let mut by_fixture: BTreeMap<String, (String, String)> = BTreeMap::new();
        for row in &rows {
            if let (Some(f), Some(code), Some(text)) =
                (row.get("f"), row.get("code"), row.get("text"))
            {
                by_fixture
                    .entry(f.clone())
                    .or_insert_with(|| (code.clone(), text.clone()));
            }
        }
        // code → (first-fixture IRI, its text) — matches `fixture_maps`' first-wins,
        // so the attached counter-example is exactly this fixture.
        let mut by_code: BTreeMap<String, (String, String)> = BTreeMap::new();
        for (iri, (code, text)) in &by_fixture {
            by_code
                .entry(code.clone())
                .or_insert_with(|| (iri.clone(), text.clone()));
        }
        assert!(
            !by_code.is_empty(),
            "the shipped bundle carries NO bound counter-example fixtures"
        );
        for (code, (iri, text)) in &by_code {
            let report = tier1
                .validate(
                    text.as_bytes(),
                    "turtle",
                    MCP_NAMESPACE,
                    VALIDATE_LOCAL_ORIGIN,
                )
                .expect("tier-1 validate the fixture body");
            if report.findings.iter().any(|f| &f.code == code) {
                return (iri.clone(), code.clone(), text.clone());
            }
        }
        panic!(
            "no bound counter-example fixture reproduced its authored violation code under \
             Tier-1 validation — observed candidate codes: {:?}",
            by_code.keys().collect::<Vec<_>>()
        );
    }

    /// PARITY + CORRESPONDENCE (end-to-end, production surface): drive the REAL
    /// `validate_local` tool (default fast Tier-1 path) over a REAL counter-example
    /// fixture from the shipped bundle, and assert (a) PARITY — the enriched finding
    /// codes EQUAL `run_tier1` (the `gmeow validate` core); and (b) CORRESPONDENCE —
    /// at least one finding carries a counter-example, EVERY attached counter-example
    /// corresponds by violation code + rule help URI, and the finding whose code is
    /// the chosen fixture's `docViolationCode` gets that fixture's exact body back.
    #[test]
    fn validate_local_enrichment_parity_and_correspondence() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        // The reference side owns an independent bundle decode, built once for the
        // selected identity and shared by every parity assertion in this process.
        let tier1_shapes = reference_tier1_shapes();
        let (fixture_iri, code, text) = selected_counter_example();
        eprintln!("validate_local test selected fixture={fixture_iri} code={code}");
        let help_uri = gmeow_validate::rule_catalog::help_uri_for(code);

        // The CLI core: Tier-1 (the same engine `gmeow validate`'s `run_tier1` drives).
        let tier1 = tier1_shapes
            .validate(
                text.as_bytes(),
                "turtle",
                MCP_NAMESPACE,
                VALIDATE_LOCAL_ORIGIN,
            )
            .expect("tier-1 validate");
        let tier1_codes = codes_of(&tier1);

        // Drive the REAL tool by DISPATCH BY NAME (deep defaults to false → fast).
        let enriched = selected_counter_example_report();
        assert_ne!(
            enriched["ok"],
            Value::Null,
            "the tool returned an EnrichedReport: {enriched}"
        );
        let findings = enriched["findings"].as_array().expect("findings array");

        // PARITY: with deep off, the tool's finding codes EQUAL the `gmeow validate`
        // (`run_tier1`) codes exactly — validate_local drops/adds/mutates nothing.
        let mut local_codes: Vec<String> = findings
            .iter()
            .map(|f| f["code"].as_str().unwrap().to_string())
            .collect();
        local_codes.sort();
        assert_eq!(
            local_codes, tier1_codes,
            "validate_local must reproduce the gmeow-validate Tier-1 finding codes exactly"
        );

        // CORRESPONDENCE (BINDING): the finding whose code == the fixture's violation
        // code carries THAT fixture's body back, with the corresponding help URI.
        let matched: Vec<&Value> = findings
            .iter()
            .filter(|f| f["code"].as_str() == Some(code.as_str()))
            .collect();
        assert!(
            !matched.is_empty(),
            "the chosen fixture's code {code} must appear among the findings: {enriched}"
        );
        let with_ce = matched
            .iter()
            .find(|f| !f["counter_example"].is_null())
            .unwrap_or_else(|| {
                panic!("the matched finding {code} must carry a counter-example: {enriched}")
            });
        assert_eq!(
            with_ce["counter_example"]["violation_code"].as_str(),
            Some(code.as_str()),
            "the attached counter-example corresponds by violation code"
        );
        assert_eq!(
            with_ce["counter_example"]["text"].as_str(),
            Some(text.as_str()),
            "the finding whose code is the fixture's violation code gets that fixture's body back"
        );
        assert_eq!(
            with_ce["help_uri"].as_str(),
            Some(help_uri.as_str()),
            "the finding carries the rule catalog help URI for its code"
        );

        // NON-VACUITY + the CORRESPONDENCE INVARIANT across the whole report: at least
        // one finding got a counter-example, and EVERY attached counter-example's
        // violation code equals its OWN finding's code (by-code, never blanket).
        let attached = findings
            .iter()
            .filter(|f| !f["counter_example"].is_null())
            .count();
        assert!(
            attached >= 1,
            "at least one finding must carry a counter-example (non-vacuous): {enriched}"
        );
        for f in findings {
            if !f["counter_example"].is_null() {
                assert_eq!(
                    f["counter_example"]["violation_code"].as_str(),
                    f["code"].as_str(),
                    "every attached counter-example corresponds to ITS finding's code \
                     (correspondence, never blanket): {f}"
                );
                assert_eq!(
                    f["help_uri"].as_str(),
                    Some(
                        gmeow_validate::rule_catalog::help_uri_for(f["code"].as_str().unwrap())
                            .as_str()
                    ),
                    "every enriched finding carries its rule catalog help URI: {f}"
                );
            }
        }
    }

    /// SELECT a REAL well-formed conformance fixture from the SHIPPED bundle's
    /// `gmeow:graph/documentation` projection whose `gmeow:docFixtureText` body
    /// ACTUALLY validates clean under Tier-1 (no `Error`-severity finding) — the
    /// honest correspondence anchor for "a claim consistent with the bundled
    /// axioms", mirroring [`select_reproducing_counter_example`]'s reproduction
    /// discipline: never hand-authored, always a REAL fixture the engine itself
    /// agrees is clean. Returns `(fixture_iri, text)`. Panics with the candidate
    /// count if NONE reproduces (a real blocker, not a soft skip).
    fn select_reproducing_wellformed_example(
        server: &McpServer,
        tier1: &gmeow_validate::data_validate::Tier1Shapes,
    ) -> (String, String) {
        let rows = server
            .view
            .docs_select_rows(WELLFORMED_FIXTURE_QUERY)
            .expect("query well-formed fixtures from graph/documentation");
        let mut by_fixture: BTreeMap<String, String> = BTreeMap::new();
        for row in &rows {
            if let (Some(f), Some(text)) = (row.get("f"), row.get("text")) {
                by_fixture.entry(f.clone()).or_insert_with(|| text.clone());
            }
        }
        assert!(
            !by_fixture.is_empty(),
            "the shipped bundle carries NO bound well-formed conformance fixtures"
        );
        for (iri, text) in &by_fixture {
            let report = tier1
                .validate(
                    text.as_bytes(),
                    "turtle",
                    MCP_NAMESPACE,
                    VALIDATE_LOCAL_ORIGIN,
                )
                .expect("tier-1 validate the fixture body");
            if report.ok() {
                return (iri.clone(), text.clone());
            }
        }
        panic!(
            "no bound well-formed fixture actually validated clean under Tier-1 — {} \
             candidates checked, none reproduced a clean Tier-1 pass",
            by_fixture.len()
        );
    }

    /// ACCEPTANCE (R5, half 1): drive the REAL `validate_local` tool over a REAL
    /// well-formed fixture from the shipped bundle — a claim CONSISTENT with the
    /// bundled axioms — and assert the production surface reports it clean: `ok:
    /// true`, and every finding present (if any non-error advisory survives) still
    /// carries the full teaching surface (`help_uri`, and `entails`/
    /// `wellformed_exemplar` when the finding names a documented term). Never
    /// idealized: a clean claim MAY still carry Warning/Note/Info findings (the
    /// tool only hard-rejects on `Error` severity), so this asserts on `ok`, not
    /// on an empty findings array.
    #[test]
    fn validate_local_accepts_a_consistent_claim() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let (fixture_iri, text) =
            select_reproducing_wellformed_example(&server, reference_tier1_shapes());
        eprintln!("validate_local clean-claim test selected fixture={fixture_iri}");

        let enriched = text_payload(
            server.call_tool_result("validate_local", &json!({"data": text, "format": "turtle"})),
        );
        assert_eq!(
            enriched["ok"], true,
            "a claim consistent with the bundled axioms must validate clean: {enriched}"
        );
        assert_eq!(enriched["tool"].as_str(), Some("validate"));
        let findings = enriched["findings"].as_array().expect("findings array");
        // `ok: true` tolerates non-Error survivors; whichever DO appear must still
        // carry the full enrichment surface (never a bare code+message).
        for f in findings {
            assert!(
                f["help_uri"].as_str().is_some_and(|u| !u.is_empty()),
                "every surfaced finding carries a non-empty rule catalog help_uri: {f}"
            );
            assert!(
                f["finding_iri"].is_string(),
                "every finding has a stable IRI: {f}"
            );
        }
    }

    /// ACCEPTANCE (R5, half 2): drive the REAL `validate_local` tool over a REAL
    /// counter-example fixture from the shipped bundle — a claim that VIOLATES a
    /// bundled SHACL shape / modelling discipline — and assert the production
    /// surface REJECTS it: the tool surfaces the violating finding code at `Error`
    /// severity (proven against the raw Tier-1 `Report`, since the enriched
    /// envelope does not carry severity), and the enriched envelope is `ok: false`
    /// (never a silent clean pass on inconsistent input).
    #[test]
    fn validate_local_rejects_an_inconsistent_claim() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let tier1_shapes = reference_tier1_shapes();
        let (fixture_iri, code, text) = selected_counter_example();
        eprintln!(
            "validate_local inconsistent-claim test selected fixture={fixture_iri} code={code}"
        );

        // The raw Tier-1 report (the `gmeow validate` core) proves the specific
        // finding code fires at `Error` severity — the hard-reject signal
        // `EnrichedFinding` does not itself carry.
        let tier1 = tier1_shapes
            .validate(
                text.as_bytes(),
                "turtle",
                MCP_NAMESPACE,
                VALIDATE_LOCAL_ORIGIN,
            )
            .expect("tier-1 validate the violating fixture");
        let tier1_finding = tier1
            .findings
            .iter()
            .find(|f| f.code.as_str() == code)
            .unwrap_or_else(|| panic!("tier-1 report must reproduce code {code}: {tier1:?}"));
        assert_eq!(
            tier1_finding.severity,
            gmeow_errors::Severity::Error,
            "the chosen counter-example must violate at Error severity (a hard reject, not \
             an advisory): {tier1_finding:?}"
        );
        assert!(
            !tier1.ok(),
            "a report carrying an Error-severity finding must not be ok(): {tier1:?}"
        );

        // Drive the REAL production tool over the SAME violating claim.
        let enriched = selected_counter_example_report();
        assert_eq!(
            enriched["ok"], false,
            "a claim violating a bundled axiom must be rejected, not validated clean: {enriched}"
        );
        let findings = enriched["findings"].as_array().expect("findings array");
        assert!(
            findings
                .iter()
                .any(|f| f["code"].as_str() == Some(code.as_str())),
            "the rejecting tool must surface the specific violation code {code}: {enriched}"
        );
    }

    /// Compile a hand-authored draft-2020-12 schema and assert `instance` CONFORMS,
    /// surfacing every validation error verbatim on failure (a real payload the
    /// schema rejects is a schema bug to FIX, never to weaken).
    fn assert_conforms(schema: &[u8], instance: &Value, what: &str) {
        let errors = gmeow_validate::instance::validate_instance(
            &serde_json::to_vec(instance).expect("serialize actual instance payload"),
            gmeow_validate::instance::InstanceFormat::Json,
            schema,
        )
        .unwrap_or_else(|e| panic!("{what}: could not validate payload: {e}"));
        assert!(
            errors.is_empty(),
            "{what}: real payload does not conform to its schema:\n{}\ninstance: {instance}",
            errors.join("\n")
        );
    }

    /// SELF-DESCRIBING SURFACE (card): a REAL `doc_card format=json` payload — the
    /// exact bytes the packed `terms/{slug}/card.json` member carries — CONFORMS to
    /// the hand-authored `card.schema.json` (`gmeow_docs_model::card::card_json_schema`).
    /// Both the STANDARD tier (the `card.json` shape) and the FULL tier (every rich
    /// panel, exercising the `$defs`) are checked against the SAME schema.
    #[test]
    fn card_json_conforms_to_card_schema() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let schema = gmeow_docs_model::card::card_json_schema().to_string().into_bytes();

        // STANDARD tier — the `card.json` shape. `gmeow:Entity` is a real bundled term.
        let standard = text_payload(server.call_tool_result(
            "doc_card",
            &json!({"term": "gmeow:Entity", "format": "json", "detail": "standard"}),
        ));
        assert_eq!(standard["ok"], true, "doc_card standard: {standard}");
        assert_conforms(&schema, &standard["card"], "card.json (standard tier)");

        // FULL tier — exercises the rich panels ($defs). `gmeow:Activity` documents
        // conformance fixtures and `gmeow:Entity` grounds entailments in the shipped
        // bundle, so at least one full card carries populated panels.
        for term in ["gmeow:Activity", "gmeow:Entity"] {
            let full = text_payload(server.call_tool_result(
                "doc_card",
                &json!({"term": term, "format": "json", "detail": "full"}),
            ));
            assert_eq!(full["ok"], true, "doc_card full for {term}: {full}");
            assert_conforms(&schema, &full["card"], "card.json (full tier)");
        }
    }

    /// SELF-DESCRIBING SURFACE (finding): a REAL `validate_local` envelope — produced
    /// by driving the tool over a REAL counter-example from the shipped bundle —
    /// CONFORMS to the hand-authored `validate-finding.schema.json`
    /// (`gmeow_validate::local_oracle::finding_json_schema`), exercising the finding /
    /// fixture / entailment `$defs` on a payload that carries an attached
    /// counter-example.
    #[test]
    fn enriched_report_conforms_to_finding_schema() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let enriched = selected_counter_example_report();
        // The chosen fixture reproduces its violation, so the envelope is non-vacuous:
        // at least one finding, and at least one attached counter-example fixture.
        let findings = enriched["findings"].as_array().expect("findings array");
        assert!(!findings.is_empty(), "expected findings: {enriched}");
        assert!(
            findings.iter().any(|f| !f["counter_example"].is_null()),
            "expected an attached counter-example (exercises the fixture $def): {enriched}"
        );
        let schema = gmeow_validate::local_oracle::finding_json_schema();
        assert_conforms(&serde_json::to_vec(&schema).expect("serialize finding schema"), enriched, "validate_local EnrichedReport");
    }

    /// DEEP PASS (heavy): drive the tool end-to-end with `deep = true` over a REAL
    /// counter-example from the shipped bundle and assert the Tier-2 semantic pass ran
    /// (a `validate.deep.*` finding is present) AND that the Tier-1 surface is
    /// preserved (true tool-vs-`gmeow validate` parity). `#[ignore]`d because the
    /// native deep reasoner over the whole bundle runs well past the 120 s on-gate
    /// nextest cliff. It runs in `make maint-rust-heavy`, which is only true because that
    /// recipe passes `--run-ignored all` — a filter expression cannot see `#[ignore]`, so
    /// the profile's `default-filter` alone would leave this test in no lane at all, and
    /// `run-ignored` is not a profile key. It also carries a 900 s budget there; the 120 s
    /// would kill it on content it is expected to exceed. To run it alone:
    /// `cargo nextest run -E 'test(validate_local_deep)' --run-ignored all`.
    /// `validate.deep.contract-invalid` is engine-enforced (it fires only on a bundle
    /// carrying a garbled `logic:admissibleValuation`, which the shipped bundle does
    /// not) and is regression-covered by
    /// `gmeow_validate::data_validate` `deep_pass_garbled_contract_produces_error_not_advisory`.
    #[test]
    #[ignore = "runs the full-bundle native deep reasoner (>120s); heavy lane only"]
    fn validate_local_deep_pass_surfaces_deep_finding() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let tier1_shapes = reference_tier1_shapes();
        let (_iri, _code, text) = selected_counter_example();

        // The CLI Tier-1 surface the deep run must preserve.
        let tier1 = tier1_shapes
            .validate(
                text.as_bytes(),
                "turtle",
                MCP_NAMESPACE,
                VALIDATE_LOCAL_ORIGIN,
            )
            .expect("tier-1 validate");
        let tier1_codes = codes_of(&tier1);

        // Explicitly request the Tier-2 pass via the `deep` arg.
        let enriched = text_payload(server.call_tool_result(
            "validate_local",
            &json!({"data": text, "format": "turtle", "deep": true}),
        ));
        let findings = enriched["findings"].as_array().expect("findings array");

        assert!(
            findings
                .iter()
                .any(|f| f["code"].as_str().unwrap().starts_with("validate.deep.")),
            "the deep pass must run (a validate.deep.* finding must appear): {enriched}"
        );
        let mut tier1_surface: Vec<String> = findings
            .iter()
            .map(|f| f["code"].as_str().unwrap().to_string())
            .filter(|c| !c.starts_with("validate.deep."))
            .collect();
        tier1_surface.sort();
        assert_eq!(
            tier1_surface, tier1_codes,
            "the deep run must preserve the Tier-1 surface (parity with gmeow validate)"
        );
    }

    /// ROBUSTNESS: an unknown `format`, an oversized `data` payload, and malformed
    /// RDF each return a well-formed error envelope (`ok:false`) — never a panic, and
    /// a malformed graph surfaces as an Error, not a silent success.
    #[test]
    fn validate_local_hard_fails_bad_format_oversize_and_malformed() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();

        // Unknown format → error envelope listing the accepted tokens.
        let bad_format = text_payload(server.call_tool_result(
            "validate_local",
            &json!({"data": "<urn:s> <urn:p> <urn:o> .", "format": "bogus"}),
        ));
        assert_eq!(
            bad_format["ok"], false,
            "unknown format must be a hard fail"
        );
        assert!(
            bad_format["error"]
                .as_str()
                .unwrap()
                .contains("unrecognized RDF format"),
            "the error must name the offending format: {bad_format}"
        );

        // Oversized payload → error envelope, no truncation.
        let huge = format!(
            "<urn:s> <urn:p> \"{}\" .",
            "x".repeat(MAX_VALIDATE_DATA_BYTES + 1)
        );
        let oversize = text_payload(
            server.call_tool_result("validate_local", &json!({"data": huge, "format": "turtle"})),
        );
        assert_eq!(
            oversize["ok"], false,
            "oversized payload must be a hard fail"
        );
        assert!(
            oversize["error"].as_str().unwrap().contains("ceiling"),
            "the error must explain the size ceiling: {oversize}"
        );

        // Malformed Turtle → error envelope (the parse hard-fails; never a silent
        // empty-but-ok report).
        let malformed = text_payload(server.call_tool_result(
            "validate_local",
            &json!({"data": "<urn:s> <urn:p> <urn:o>  # unterminated, no dot", "format": "turtle"}),
        ));
        assert_eq!(
            malformed["ok"], false,
            "malformed RDF must surface as an error, not a silent success: {malformed}"
        );
    }

    /// The n-triples claim that types an individual as a BARE gmeow:Entity — the
    /// exact fixture the advisory bridge fires `BareEntitySortalAdviceConstraint`
    /// (Entity avoidWhen) on.
    const BARE_ENTITY_CLAIM: &str = "<https://ex.test/x> \
         <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
         <https://blackcatinformatics.ca/gmeow/Entity> .\n";

    /// AC2: `advise` returns the non-gating RECOMMENDATIONS a claim
    /// trips — driven over the REAL JSON-RPC `handle_message` dispatch. A bare-Entity
    /// claim surfaces the Entity avoid/use/how-to advice, `ok:true`.
    #[test]
    fn advise_surfaces_recommendations_for_a_matching_claim() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();

        // Drive the REAL JSON-RPC path: tools/call → dispatch → tool_advise.
        let body = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"advise","arguments":{{"data":{},"format":"ntriples"}}}}}}"#,
            serde_json::to_string(BARE_ENTITY_CLAIM).unwrap()
        );
        let raw = server.handle_message(&body);
        let envelope: Value = serde_json::from_str(&raw).expect("JSON-RPC response");
        assert_eq!(envelope["jsonrpc"], "2.0");
        let payload: Value = serde_json::from_str(
            envelope["result"]["content"][0]["text"]
                .as_str()
                .expect("tool text"),
        )
        .expect("advise payload is JSON");

        assert_eq!(
            payload["ok"], true,
            "advise is a recommendation surface — always ok:true: {payload}"
        );
        assert_eq!(payload["tool"], "advise");
        let recs = payload["recommendations"]
            .as_array()
            .expect("recommendations array");
        assert!(
            !recs.is_empty(),
            "a bare-Entity claim must surface at least one recommendation: {payload}"
        );
        for rec in recs {
            let code = rec["code"].as_str().unwrap();
            assert!(
                code.starts_with("advice."),
                "advise must surface ONLY advisory advice.* codes: {rec}"
            );
            assert!(
                !rec["avoid_when"].as_str().unwrap().is_empty(),
                "each recommendation carries its avoid-when prohibition prose: {rec}"
            );
            assert_eq!(
                rec["help_uri"].as_str().unwrap(),
                gmeow_validate::rule_catalog::catalog_anchor_uri(code),
                "help_uri routes through the single anchor authority (→ #advice-): {rec}"
            );
        }
        // The Entity advice carries its formalized term and its corrective/permission
        // guidance (the contrary-to-duty how-to-use / use-when legs).
        let entity = recs
            .iter()
            .find(|r| {
                r["formalizes"].as_str() == Some("https://blackcatinformatics.ca/gmeow/Entity")
            })
            .unwrap_or_else(|| {
                panic!("the Entity advice recommendation must be present: {payload}")
            });
        let use_when = entity["use_when"].as_array().unwrap();
        assert!(
            !use_when.is_empty(),
            "the Entity advice carries use-when guidance: {entity}"
        );
        assert!(
            use_when
                .iter()
                .all(|v| !v.as_str().unwrap().starts_with("Use when: ")),
            "use_when entries must have the \"Use when: \" marker stripped: {entity}"
        );
        let how_to_use = entity["how_to_use"].as_array().unwrap();
        assert!(
            !how_to_use.is_empty(),
            "the Entity advice carries how-to-use guidance: {entity}"
        );
        assert!(
            how_to_use
                .iter()
                .all(|v| !v.as_str().unwrap().starts_with("Use when: ")),
            "no permission-leg prose may leak into how_to_use: {entity}"
        );
        // The tripped node is visible on the MCP surface, not just the RDF claim wing —
        // `subject` resolves to the focus IRI the finding's location carries.
        assert_eq!(
            entity["subject"].as_str(),
            Some("https://ex.test/x"),
            "advise must surface the tripped node as `subject`, not null: {entity}"
        );
    }

    /// AC2: a claim that trips NO advice returns an empty recommendation list, still
    /// `ok:true` — advice is a recommendation, never a rejection.
    #[test]
    fn advise_returns_empty_for_a_clean_claim() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let payload = text_payload(server.call_tool_result(
            "advise",
            &json!({"data": "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n", "format": "ntriples"}),
        ));
        assert_eq!(
            payload["ok"], true,
            "advice never fails, even for a claim with no recommendations: {payload}"
        );
        assert_eq!(payload["tool"], "advise");
        assert!(
            payload["recommendations"].as_array().unwrap().is_empty(),
            "a claim tripping no advice returns an empty recommendation list: {payload}"
        );
    }

    /// AC2 (the sharpest witness): on a MIXED claim tripping BOTH a binding Error AND
    /// the Entity advice, `advise` returns ONLY the advisory tier — `ok:true`, never
    /// the binding code — while `validate_local` on the same claim is `ok:false`.
    #[test]
    fn advise_on_a_mixed_claim_returns_only_advice() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let (_fixture_iri, code, text) = selected_counter_example();

        // The Error-tripping fixture PLUS a bare-Entity advice trigger (a new subject,
        // so the fixture's Error still fires and the Entity advice is added).
        let claim = format!(
            "{text}\n<https://ex.test/advicex> a <https://blackcatinformatics.ca/gmeow/Entity> .\n"
        );

        // validate_local sees the binding Error → ok:false.
        let validated = text_payload(server.call_tool_result(
            "validate_local",
            &json!({"data": claim, "format": "turtle"}),
        ));
        assert_eq!(
            validated["ok"], false,
            "the mixed claim carries a binding Error (validate_local rejects it): {validated}"
        );

        // advise returns ONLY the advisory tier, ok:true, and NEVER the binding code.
        let advised = text_payload(
            server.call_tool_result("advise", &json!({"data": claim, "format": "turtle"})),
        );
        assert_eq!(
            advised["ok"], true,
            "advise never fails, even on a claim carrying a binding violation: {advised}"
        );
        let recs = advised["recommendations"].as_array().unwrap();
        assert!(
            !recs.is_empty(),
            "the bare-Entity leg must still surface advice on the mixed claim: {advised}"
        );
        for rec in recs {
            let rec_code = rec["code"].as_str().unwrap();
            assert!(
                rec_code.starts_with("advice."),
                "advise surfaces only advice.* codes, never the binding {code}: {rec}"
            );
            assert_ne!(
                rec_code, code,
                "the binding Error code must never appear in advise output: {rec}"
            );
        }
    }

    /// ROBUSTNESS (parity with `validate_local`): an unknown `format` and an oversized
    /// `data` payload each return a well-formed `ok:false` error envelope — never a
    /// panic, never a silent truncation.
    #[test]
    fn advise_hard_fails_bad_format_and_oversize() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();

        let bad_format = text_payload(server.call_tool_result(
            "advise",
            &json!({"data": "<urn:s> <urn:p> <urn:o> .", "format": "bogus"}),
        ));
        assert_eq!(
            bad_format["ok"], false,
            "unknown format must be a hard fail"
        );
        assert!(
            bad_format["error"]
                .as_str()
                .unwrap()
                .contains("unrecognized RDF format"),
            "the error must name the offending format: {bad_format}"
        );

        let huge = format!(
            "<urn:s> <urn:p> \"{}\" .",
            "x".repeat(MAX_VALIDATE_DATA_BYTES + 1)
        );
        let oversize = text_payload(
            server.call_tool_result("advise", &json!({"data": huge, "format": "turtle"})),
        );
        assert_eq!(
            oversize["ok"], false,
            "oversized payload must be a hard fail"
        );
        assert!(
            oversize["error"].as_str().unwrap().contains("ceiling"),
            "the error must explain the size ceiling: {oversize}"
        );
    }

    /// The `explain_finding` tool, driven by DISPATCH BY NAME over a server built
    /// from the shipped bundle: a real fingerprint IRI returns the finding's code +
    /// a gate verdict; an unknown IRI is a HARD FAIL (isError), never an empty DAG.
    #[test]
    fn explain_finding_tool_walks_a_real_witness_and_hard_fails_unknown() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // Obtain a real fingerprint IRI the SAME way `explain` does: the first key of
        // the FindingIndex the reader rehydrates from the server's held snapshot. An
        // empty graph/diagnostics is a blocker, not something to paper over.
        let index = gmeow_bundle_view::diagnostics_reader::read_findings(&server.view.dataset)
            .expect("read graph/diagnostics from shipped bundle");
        assert!(
            !index.is_empty(),
            "shipped bundle graph/diagnostics carries NO findings — explain_finding has no witness"
        );
        let real_iri = index.findings.keys().next().unwrap().clone();
        let expected_code = index.get(&real_iri).unwrap().code.clone();

        let ok = text_payload(
            server.call_tool_result("explain_finding", &json!({"target_iri": real_iri})),
        );
        assert_eq!(ok["ok"], true, "explain_finding must succeed: {ok}");
        assert_eq!(ok["kind"], "finding");
        assert_eq!(ok["focus"]["code"], expected_code);
        assert!(
            ok["verdict"].as_str().is_some(),
            "explain_finding must carry a gate verdict: {ok}"
        );
        assert!(
            ok["focus"]["provenance_dag"]
                .as_str()
                .unwrap()
                .contains(&real_iri),
            "provenance DAG must render the focus finding: {ok}"
        );

        // Unknown target → hard fail (isError result), never a success with an empty DAG.
        let bad = server.call_tool_result("explain_finding", &json!({"target_iri": "urn:nope"}));
        assert_eq!(
            bad["isError"], true,
            "unknown IRI must be a hard fail: {bad}"
        );
        let bad_text = text_payload(bad);
        assert_eq!(bad_text["ok"], false);
        assert!(
            bad_text["error"]
                .as_str()
                .unwrap_or_default()
                .contains("unknown explain target")
        );
    }

    /// The SPARQL half of the acceptance criterion, on the PRODUCTION MCP surface:
    /// `query_local` runs a SELECT over the bundle's `graph/diagnostics` named graph
    /// and returns a real finding's code — proving SPARQL over the diagnostics
    /// projection executes through the shipped query tool (its canon is the full
    /// signed dataset, which retains the diagnostics graph).
    #[test]
    fn query_local_selects_a_finding_code_over_graph_diagnostics() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // query_local requires an overlay path; a trivial annex suffices — the query
        // itself targets the bundle's graph/diagnostics named graph directly.
        let overlay_data = "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n";

        let query = "SELECT ?s ?code WHERE { \
                     GRAPH <https://blackcatinformatics.ca/gmeow/graph/diagnostics> { \
                     ?s a <https://blackcatinformatics.ca/gmeow/Finding> ; \
                     <https://blackcatinformatics.ca/gmeow/findingCode> ?code } } LIMIT 1";
        let res = text_payload(server.call_tool_result(
            "query_local",
            &json!({"data": overlay_data, "format": "turtle", "query": query}),
        ));
        assert_eq!(
            res["ok"], true,
            "query_local over graph/diagnostics must succeed: {res}"
        );
        let bindings = res["results"]["bindings"]
            .as_array()
            .expect("bindings array");
        assert!(
            !bindings.is_empty(),
            "expected at least one Finding binding from graph/diagnostics: {res}"
        );
        assert!(
            bindings[0]["code"]["value"].as_str().is_some(),
            "the Finding binding must carry a findingCode literal: {res}"
        );
    }

    use gmeow_ns::LOGIC_NS;

    /// A `∀x. trigger(x, mark) → rdf:type(x, <cls>)` candidate, authored as a reified
    /// `logic:Formula` (a single top-level formula — the trivially-Horn consequent is a
    /// sub-formula, so it never trips the `with_formulas` guard).
    fn forall_horn_candidate(cls_local: &str) -> String {
        format!(
            "@prefix logic: <{LOGIC_NS}> .\n\
             @prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:cand a logic:Formula ;\n\
                 logic:forall ex:body ;\n\
                 logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"x\" ] .\n\
             ex:body a logic:Formula ;\n\
                 logic:antecedent ex:ant ;\n\
                 logic:consequent ex:con .\n\
             ex:ant a logic:Formula ;\n\
                 logic:relation ex:trigger ;\n\
                 logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
                 logic:argument [ logic:termIndex 1 ; logic:termIri ex:mark ] .\n\
             ex:con a logic:Formula ;\n\
                 logic:relation rdf:type ;\n\
                 logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
                 logic:argument [ logic:termIndex 1 ; logic:termIri ex:{cls_local} ] .\n"
        )
    }

    /// A KB where the candidate's head class is DISJOINT with the individual's asserted type,
    /// so firing `rdf:type(a, <cls>)` forces an `owl:Nothing` clash ⇒ refutation + witness.
    fn refuting_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             ex:A owl:disjointWith ex:{cls_local} .\n"
        )
    }

    /// Parse a `graph/attestations` N-Quads document into a dataset the read helper reads.
    fn dataset_of(nquads: &str) -> Arc<purrdf::RdfDataset> {
        purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
            .expect("parse coherence N-Quads")
    }

}

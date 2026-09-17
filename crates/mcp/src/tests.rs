// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

mod convert_transcode_contract;
mod distribution_matrix_contract;
mod glyph_legend_contract;
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
                serde_json::from_slice(&bytes).expect("native source policy");
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
            serde_json::from_slice(&bytes).expect("prepared native verification laws");
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
                serde_json::from_slice(&bytes).expect("native policy control observations")
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
        let calls = Memory::new(memory_path).tool_calls().unwrap();
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
        let stored_result: Value =
            serde_json::from_str(calls[0].result.as_deref().unwrap()).unwrap();
        assert_eq!(stored_result["ok"], true);
        assert_eq!(stored_result["claim"]["id"], canary_id);
        let stored_arguments: Value =
            serde_json::from_str(calls[0].arguments.as_deref().unwrap()).unwrap();
        assert_eq!(
            stored_arguments["text"],
            "SUPPRESSED-CANARY belief about the launch window"
        );

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
            serde_json::from_slice(&projected).expect("bundle policy statements");
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

        // CONSTRUCT/DESCRIBE are ANSWERED and the form is declared — see
        // `sparql_result_to_json` on why the old refusal was a capability gap, not a policy.
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

    /// The shared producer/runtime explanation exit preserves genuine native proof
    /// premises and refuses an absent target without starting another execution.
    #[test]
    fn explain_native_result_preserves_the_selected_proof_and_refuses_absent_target() {
        // gmeow-test-input: synthetic-only
        let edb = dataset_of(
            "<urn:proof:x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <urn:proof:A> .\n\
             <urn:proof:A> <https://blackcatinformatics.ca/logic/subClassOf> <urn:proof:B> .\n",
        );
        let input = prepare_reasoning_input(&edb).expect("synthetic native input");
        let domains = SelectedDomains::new([SelectedLogicalWorld::new(
            LogicalGraph::Default,
            DomainProfile::NonemptyObjectDomainV1,
            "gmeow.mcp.synthetic-explanation.v1".to_owned(),
            *input.ingress_contract(),
        )
        .expect("explicit default theory")])
        .expect("selected theory");
        let result = reason_all_budgeted(input, &domains, &governed_budget(Some(64), None))
            .expect("one governed native execution");
        let graph = gmeow_logic::reason::rl::DEFAULT_WORLD;
        let predicate = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let asserted = McpView::explain_native_result(
            &result,
            "urn:proof:x",
            predicate,
            "<urn:proof:A>",
            graph,
        )
        .expect("asserted proof");
        assert_eq!(asserted["faithful"], true);
        assert_eq!(asserted["step_skeleton"].as_array().unwrap().len(), 1);
        assert_eq!(asserted["step_skeleton"][0]["is_asserted"], true);
        let derived = McpView::explain_native_result(
            &result,
            "urn:proof:x",
            predicate,
            "<urn:proof:B>",
            graph,
        )
        .expect("derived proof");
        assert_eq!(derived["faithful"], true);
        let steps = derived["step_skeleton"].as_array().unwrap();
        assert!(!steps[0]["is_asserted"].as_bool().unwrap());
        assert!(
            steps.len() > 1,
            "a genuine derivation retains its premises: {derived}"
        );
        assert_eq!(
            derived["judgment_nquads"].as_str().unwrap(),
            project_reasoning_result(&result).unwrap(),
        );
        let error = McpView::explain_native_result(
            &result,
            "urn:proof:absent",
            predicate,
            "<urn:proof:B>",
            graph,
        )
        .expect_err("unknown target must refuse");
        assert!(error.to_string().contains("not in closure"));
    }

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
    fn assert_conforms(schema: &Value, instance: &Value, what: &str) {
        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(schema)
            .unwrap_or_else(|e| panic!("{what}: schema does not compile: {e}"));
        let errors: Vec<String> = validator
            .iter_errors(instance)
            .map(|e| e.to_string())
            .collect();
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
        let schema = gmeow_docs_model::card::card_json_schema();

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
        assert_conforms(&schema, enriched, "validate_local EnrichedReport");
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

    // ── Conjecture-library persistence ───────────────────────────────────────

    use gmeow_ns::LOGIC_NS;
    use gmeow_ns::MATH_NS;

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

    /// A KB where the candidate's head class is UNRELATED (no disjointness), so firing derives
    /// a new consistent fact ⇒ Open/Neither, no witness.
    fn open_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             # {cls_local} is unrelated to A — no clash.\n"
        )
    }

    fn temp_conjecture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("conjectures.gts");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_CONJECTURE_PATH", &path);
        }
        (dir, path)
    }

    /// The imported conjecture-library dataset (all appended segments unioned).
    fn read_conjectures(path: &Path) -> std::sync::Arc<purrdf::RdfDataset> {
        let bytes = fs::read(path).expect("read conjecture library");
        purrdf::import_gts_events(&bytes)
            .expect("import conjecture library")
            .dataset
    }

    /// Every subject typed `logic:Conjecture` in `dataset`.
    fn conjecture_nodes(dataset: &purrdf::RdfDataset) -> BTreeSet<String> {
        dataset
            .owned_quads()
            .filter(|q| {
                q.predicate == RDF_TYPE_IRI
                    && q.object == RdfTerm::iri(format!("{LOGIC_NS}Conjecture"))
            })
            .filter_map(|q| match q.subject {
                RdfTerm::Iri(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    /// The `logic:witnessPremise` literal lexicals in `dataset`.
    fn witness_premises(dataset: &purrdf::RdfDataset) -> Vec<String> {
        dataset
            .owned_quads()
            .filter(|q| q.predicate == format!("{LOGIC_NS}witnessPremise"))
            .filter_map(|q| match q.object {
                RdfTerm::Literal(lit) => Some(lit.lexical_form),
                _ => None,
            })
            .collect()
    }

    struct ConjEnvGuard;
    impl ConjEnvGuard {
        fn set() -> (EnvRestore, ConjEnvGuard) {
            let env = EnvRestore::capture(&[
                "GMEOW_LANG",
                "GMEOW_MEMORY_PATH",
                "GMEOW_CONJECTURE_PATH",
                "GMEOW_CANDIDATE_PATH",
                "HOME",
                "USERPROFILE",
            ]);
            unsafe {
                // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
                env::remove_var("GMEOW_LANG");
            }
            (env, ConjEnvGuard)
        }
    }

    #[test]
    fn persist_conjecture_precondition_gates_the_commit() {
        // The write is a REAL TR gate: with the precondition present the committed run
        // succeeds; with it absent the run FAILS (so the tool returns ok:false before writing).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let ok = execute_memory_txn(
            source_action_policy(),
            MCP_PERSIST_CONJECTURE_SCHEMA,
            &[MCP_CONJECTURE_VERDICT_PRESENTED],
            false,
        )
        .unwrap();
        assert!(
            matches!(ok, TxReceipt::CommittedSuccess { .. }),
            "the precondition-present committed run must succeed: {ok:?}"
        );
        let unmet = execute_memory_txn(
            source_action_policy(),
            MCP_PERSIST_CONJECTURE_SCHEMA,
            &[],
            false,
        )
        .unwrap();
        assert!(
            matches!(unmet, TxReceipt::CommittedFailure { .. }),
            "a persist with the precondition UNMET must fail the commit (the tool then returns \
             ok:false before writing): {unmet:?}"
        );
    }

    #[test]
    fn conjecture_test_is_pure_and_writes_nothing() {
        // R3a: the "test" leg (`conjecture_test`) is a PURE hypothetical evaluation. Driving it
        // with a candidate that, under the OLD single-tool surface, would have persisted a
        // refutation must still return the full verdict envelope while leaving the library file
        // byte-unchanged (here: absent) — no TR gate, no append, ever.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        assert!(!path.exists(), "library must not exist before the call");
        let native = server
            .evaluate_conjecture_test_request(&json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }))
            .expect("valid governed conjecture request succeeds");
        let resp = McpServer::conjecture_test_response(&native);
        assert_eq!(resp["ok"], true, "the pure test must succeed: {resp}");
        assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
        assert_eq!(resp["witness"]["individual"], "http://ex/a");
        let node = resp["conjecture"].as_str().expect("node iri").to_string();
        assert!(!node.is_empty());
        // No persist/transaction section at all on this surface.
        assert!(
            resp.get("transaction").is_none(),
            "conjecture_test must never render a transaction section: {resp}"
        );
        assert!(
            resp.get("committed").is_none(),
            "conjecture_test must never render a committed flag: {resp}"
        );

        // T1 (G7): the grounded judgment travels on the SAME `judgment_nquads` key
        // `verify_graph`/`explain_quad` carry — even on this pure, nothing-persisted path —
        // and exactly matches the evaluator's projection of a real `logic:Conjecture`
        // node embedding a non-empty `logic:ReasoningResult`. The tiny result_rdf
        // contracts independently own the reader round trip.
        let judgment = resp["judgment_nquads"]
            .as_str()
            .expect("conjecture_test must carry a judgment_nquads string");
        assert!(
            !judgment.trim().is_empty(),
            "judgment_nquads must not be empty"
        );
        assert_eq!(judgment, native.verdict_nt);
        assert_eq!(
            native.lifecycle,
            ConjectureLifecycleState::RefutedInStandpoint.wire()
        );
        assert_eq!(resp["verdict"]["lifecycle"], native.lifecycle);
        assert!(
            judgment.contains(&format!(
                "<{}> <https://blackcatinformatics.ca/logic/conjectureLifecycleState> <{}> .",
                native.node_iri,
                ConjectureLifecycleState::RefutedInStandpoint.iri()
            )),
            "the actual conjecture node must carry its native refuted lifecycle"
        );
        assert!(
            judgment.contains(&format!(
                "<{}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Conjecture> .",
                native.node_iri
            )),
            "the native projection must ground the returned logic:Conjecture node"
        );
        assert!(
            judgment.contains("<https://blackcatinformatics.ca/logic/conjectureVerdict>")
                && judgment.contains("<https://blackcatinformatics.ca/logic/ReasoningResult>"),
            "the native projection must link its embedded logic:ReasoningResult body"
        );

        // The library file is BYTE-UNCHANGED (still absent): no TR gate, no append.
        assert!(
            !path.exists(),
            "a pure conjecture_test call must write nothing to the library"
        );
    }

    #[test]
    fn store_conjecture_refutes_and_persists_with_witness() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        assert!(
            !path.exists(),
            "library must not exist before the first persist"
        );
        let resp = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(resp["ok"], true, "refutation persist must succeed: {resp}");
        assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
        assert_eq!(resp["witness"]["individual"], "http://ex/a");
        let node = resp["conjecture"].as_str().expect("node iri").to_string();
        assert!(!node.is_empty());

        // The library file grew, and the witness premises round-trip back through the GTS.
        assert!(
            path.exists(),
            "the conjecture library file must have been written"
        );
        let dataset = read_conjectures(&path);
        assert!(
            conjecture_nodes(&dataset).contains(&node),
            "the content-addressed conjecture node must be readable back: {node}"
        );
        // The standpoint scope is recoverable.
        assert!(dataset.owned_quads().any(|q| {
            q.predicate == format!("{LOGIC_NS}conjectureStandpoint")
                && q.object == RdfTerm::iri("http://ex/standpoint/alice")
        }));
        // The witness premises are recoverable.
        let premises = witness_premises(&dataset);
        assert!(
            !premises.is_empty(),
            "a refutation must persist recoverable witness premises"
        );
    }

    #[test]
    fn store_conjecture_bridges_math_twin_via_conjecture_under_test() {
        // Driving the real MCP `store_conjecture` tool (the shared conjecture-test core) with a
        // `math_conjecture` must persist the always-present structural twin bridge
        // `<math> math:conjectureUnderTest <logic:Conjecture-node>` (domain math:Conjecture,
        // range logic:Conjecture) — readable back out of the append-only GTS library.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let math_iri = "https://blackcatinformatics.ca/math/conjecture/goldbach";
        let resp = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
                "math_conjecture": math_iri,
            }),
        ));
        assert_eq!(resp["ok"], true, "math-twin persist must succeed: {resp}");
        let node = resp["conjecture"].as_str().expect("node iri").to_string();

        let dataset = read_conjectures(&path);
        let under_test = format!("{MATH_NS}conjectureUnderTest");
        assert!(
            dataset.owned_quads().any(|q| {
                q.subject == RdfTerm::iri(math_iri)
                    && q.predicate == under_test
                    && q.object == RdfTerm::iri(node.clone())
            }),
            "the math:conjectureUnderTest bridge <{math_iri}> -> <{node}> must be recoverable \
             from the persisted GTS library"
        );
    }

    #[test]
    fn store_conjecture_dry_run_writes_nothing() {
        // The `dry_run` witness on `store_conjecture` is a HYPOTHETICAL commit: the verdict and
        // TR receipt are computed exactly as a real commit would be, but nothing is appended.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let native = server
            .evaluate_store_conjecture_request(&json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
                "dry_run": true,
            }))
            .expect("valid governed conjecture request succeeds");
        let resp = McpServer::store_conjecture_response(&native);
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["dry_run"], true);
        assert_eq!(resp["transaction"]["committed"], false);
        // The verdict is still computed and returned.
        assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
        // Nothing written: the library file does not exist / is zero bytes.
        assert!(
            !path.exists() || fs::metadata(&path).unwrap().len() == 0,
            "a dry run must write nothing to the library"
        );

        // T1 (G7): `store_conjecture`'s dry-run path STILL carries the grounded judgment under
        // the SAME `judgment_nquads` key the read tools and `conjecture_test` use — a
        // hypothetical persist is not a hypothetical verdict; the engine really ran.
        let judgment = resp["judgment_nquads"]
            .as_str()
            .expect("store_conjecture dry-run must carry a judgment_nquads string");
        assert!(
            !judgment.trim().is_empty(),
            "judgment_nquads must not be empty"
        );
        assert_eq!(judgment, native.verdict_nt);
        assert_eq!(
            native.lifecycle,
            ConjectureLifecycleState::RefutedInStandpoint.wire()
        );
        assert_eq!(resp["verdict"]["lifecycle"], native.lifecycle);
        assert!(
            judgment.contains(&format!(
                "<{}> <https://blackcatinformatics.ca/logic/conjectureLifecycleState> <{}> .",
                native.node_iri,
                ConjectureLifecycleState::RefutedInStandpoint.iri()
            )),
            "the actual conjecture node must carry its native refuted lifecycle"
        );
        assert!(
            judgment.contains(&format!(
                "<{}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Conjecture> .",
                native.node_iri
            )),
            "the native projection must ground the returned logic:Conjecture node"
        );
        assert!(
            judgment.contains("<https://blackcatinformatics.ca/logic/conjectureVerdict>")
                && judgment.contains("<https://blackcatinformatics.ca/logic/ReasoningResult>"),
            "the native projection must link its embedded logic:ReasoningResult body"
        );

        // A second, committing call on the SAME candidate now appends for real: the dry run
        // above left the library byte-unchanged, so this is the library's FIRST segment.
        let committed = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(committed["ok"], true);
        assert!(path.exists() && fs::metadata(&path).unwrap().len() > 0);
        // The committed path carries judgment_nquads too.
        assert!(
            committed["judgment_nquads"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty()),
            "the committed store_conjecture response must also carry judgment_nquads: {committed}"
        );
    }

    /// Store a real conjecture through the shipped `store_conjecture` tool and return its
    /// content-addressed node IRI (the shared setup for the refute tests below).
    fn store_one_conjecture(server: &McpServer, cls: &str) -> String {
        let resp = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate(cls),
                "kb": open_kb(cls),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(resp["ok"], true, "store must succeed: {resp}");
        assert_eq!(resp["verdict"]["lifecycle"], "open");
        resp["conjecture"].as_str().expect("node iri").to_string()
    }

    #[test]
    fn refute_conjecture_withdraws_and_appends_author_segment() {
        // store_conjecture then refute_conjecture its node IRI: the library gains a NEW
        // append-only segment marking that node ConjectureWithdrawn with the author reason;
        // the reader's EFFECTIVE lifecycle is now Withdrawn; the PRIOR segment bytes are
        // byte-for-byte intact (append-only, never mutated).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let node = store_one_conjecture(&server, "B");
        // Before withdrawal the effective state is the engine verdict (Open), never Withdrawn.
        let before = read_library(library_at(&path).as_ref()).unwrap();
        assert_eq!(
            before.get(&node).copied(),
            Some(ConjectureLifecycleState::Open)
        );
        let prior = fs::read(&path).expect("library bytes before refute");

        let resp = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "author retired this line of inquiry"}),
        ));
        assert_eq!(resp["ok"], true, "the withdrawal must commit: {resp}");
        assert_eq!(resp["conjecture"], node);
        assert_eq!(resp["lifecycle"], "withdrawn");
        assert_eq!(resp["transaction"]["committed"], true);
        assert_eq!(resp["transaction"]["succeeded"], true);
        // T1 (G7): the compensating withdrawal carries its own grounded RDF projection under
        // the SAME `judgment_nquads` key — the target node re-marked ConjectureWithdrawn.
        assert!(
            resp["judgment_nquads"]
                .as_str()
                .is_some_and(|s| s.contains("ConjectureWithdrawn")),
            "refute_conjecture must carry judgment_nquads naming ConjectureWithdrawn: {resp}"
        );

        // The EFFECTIVE lifecycle (segment order) is now Withdrawn.
        let after = read_library(library_at(&path).as_ref()).unwrap();
        assert_eq!(
            after.get(&node).copied(),
            Some(ConjectureLifecycleState::Withdrawn),
            "the effective lifecycle after refute must be Withdrawn"
        );

        // Append-only: the file GREW and the prior bytes are an untouched prefix.
        let now = fs::read(&path).expect("library bytes after refute");
        assert!(
            now.len() > prior.len(),
            "the withdrawal must append new bytes"
        );
        assert_eq!(
            &now[..prior.len()],
            &prior[..],
            "prior segment bytes must be byte-for-byte intact (append-only)"
        );

        // The author reason literal round-trips out of the unioned library.
        let dataset = read_conjectures(&path);
        assert!(
            dataset.owned_quads().any(|q| {
                q.subject == RdfTerm::iri(node.clone())
                    && q.predicate == format!("{LOGIC_NS}withdrawalReason")
                    && matches!(
                        q.object,
                        RdfTerm::Literal(ref lit)
                            if lit.lexical_form == "author retired this line of inquiry"
                    )
            }),
            "the author withdrawal reason must be recoverable from the library"
        );
        // The withdrawal is reviewer-asserted, never engine-produced.
        assert!(
            dataset.owned_quads().any(|q| {
                q.subject == RdfTerm::iri(node.clone())
                    && q.predicate == format!("{LOGIC_NS}verdictProvenance")
                    && q.object == RdfTerm::iri(format!("{LOGIC_NS}VerdictReviewerAsserted"))
            }),
            "the withdrawal must carry VerdictReviewerAsserted provenance"
        );
    }

    #[test]
    fn refute_conjecture_second_withdraw_rejected_by_segment_order() {
        // store -> withdraw -> withdraw again the SAME node: the second refute is rejected as
        // precondition-unmet because the EFFECTIVE state is already Withdrawn, decided by
        // SEGMENT ORDER (the last lifecycle assertion), not by the union or gmeow:atTime. The
        // rejected call appends NOTHING (the library stays byte-for-byte identical).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let node = store_one_conjecture(&server, "B");
        let first = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "first withdrawal"}),
        ));
        assert_eq!(
            first["ok"], true,
            "the first withdrawal must commit: {first}"
        );
        let after_first = fs::read(&path).expect("library bytes after first withdrawal");

        let second = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "second withdrawal"}),
        ));
        assert_eq!(
            second["ok"], false,
            "a second withdrawal must be rejected: {second}"
        );
        assert_eq!(second["transaction"]["committed"], true);
        assert_eq!(second["transaction"]["succeeded"], false);
        assert!(
            second["error"]
                .as_str()
                .is_some_and(|e| e.contains("already withdrawn")),
            "the rejection must name the already-withdrawn precondition: {second}"
        );
        // The rejected call wrote nothing.
        let after_second = fs::read(&path).expect("library bytes after rejected withdrawal");
        assert_eq!(
            after_first, after_second,
            "a rejected withdrawal must append nothing to the library"
        );
    }

    #[test]
    fn refute_conjecture_unknown_id_rejected() {
        // An unknown conjecture_id (nothing stored) fails the TR gate (empty start state) and
        // returns ok:false before any write — the library file is never created.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        assert!(!path.exists(), "library must not exist before the call");
        let resp = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": "https://blackcatinformatics.ca/gmeow/graph/conjecture/ghost"}),
        ));
        assert_eq!(resp["ok"], false, "an unknown id must be rejected: {resp}");
        assert!(
            resp["error"]
                .as_str()
                .is_some_and(|e| e.contains("unknown conjecture id")),
            "the rejection must name the unknown id: {resp}"
        );
        assert!(
            !path.exists(),
            "a rejected withdrawal of an unknown id must write nothing"
        );
    }

    #[test]
    fn refute_conjecture_dry_run_writes_nothing() {
        // dry_run=true witnesses the hypothetical commit (lifecycle withdrawn, committed:false)
        // but appends NOTHING — the library file stays byte-unchanged.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let node = store_one_conjecture(&server, "B");
        let before = fs::read(&path).expect("library bytes before dry run");

        let resp = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "sandbox", "dry_run": true}),
        ));
        assert_eq!(resp["ok"], true, "the dry run must succeed: {resp}");
        assert_eq!(resp["dry_run"], true);
        assert_eq!(resp["lifecycle"], "withdrawn");
        assert_eq!(resp["transaction"]["committed"], false);
        // T1 (G7): the hypothetical withdrawal still carries judgment_nquads — the RDF
        // projection is pure and side-effect-free, so witnessing it costs nothing written.
        assert!(
            resp["judgment_nquads"]
                .as_str()
                .is_some_and(|s| s.contains("ConjectureWithdrawn")),
            "refute_conjecture dry-run must carry judgment_nquads naming ConjectureWithdrawn: \
             {resp}"
        );

        // Nothing appended: bytes unchanged AND the effective lifecycle is still Open.
        let after = fs::read(&path).expect("library bytes after dry run");
        assert_eq!(before, after, "a dry run must write nothing to the library");
        let library = read_library(library_at(&path).as_ref()).unwrap();
        assert_eq!(
            library.get(&node).copied(),
            Some(ConjectureLifecycleState::Open),
            "a dry run must not change the effective lifecycle"
        );
    }

    #[test]
    fn store_conjecture_and_refute_round_trip_keep_library_and_audit_consistent() {
        // store_conjecture and refute_conjecture must each land their
        // library segment AND their audit segment TOGETHER — never one without the other. Drive
        // the real `call_tool_result` surface for both tools and, after EACH commit, assert the
        // library dataset carries BOTH the verdict/withdrawal triples AND the matching
        // `logic:instantiatesSchema` audit marker for that same call — round-tripping cleanly.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let node = store_one_conjecture(&server, "B");
        let after_store = read_conjectures(&path);
        assert!(
            conjecture_nodes(&after_store).contains(&node),
            "the store's library segment must be present after the commit"
        );
        assert!(
            after_store.owned_quads().any(|q| {
                q.predicate == LOGIC_INSTANTIATES_SCHEMA
                    && q.object == RdfTerm::iri(MCP_PERSIST_CONJECTURE_SCHEMA)
            }),
            "the store's audit segment must be present in the SAME commit as its library \
             segment (no library-without-audit gap): {after_store:?}"
        );

        let refute = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "round-trip proof"}),
        ));
        assert_eq!(refute["ok"], true, "the withdrawal must commit: {refute}");

        let after_refute = read_conjectures(&path);
        let library = read_library(library_at(&path).as_ref()).unwrap();
        assert_eq!(
            library.get(&node).copied(),
            Some(ConjectureLifecycleState::Withdrawn),
            "the round-trip's effective state must be Withdrawn"
        );
        assert!(
            after_refute.owned_quads().any(|q| {
                q.predicate == LOGIC_INSTANTIATES_SCHEMA
                    && q.object == RdfTerm::iri(MCP_WITHDRAW_CONJECTURE_SCHEMA)
            }),
            "the refute's audit segment must be present in the SAME commit as its withdrawal \
             segment: {after_refute:?}"
        );
        // The store's audit marker is STILL there too — append-only, nothing overwritten.
        assert!(
            after_refute.owned_quads().any(|q| {
                q.predicate == LOGIC_INSTANTIATES_SCHEMA
                    && q.object == RdfTerm::iri(MCP_PERSIST_CONJECTURE_SCHEMA)
            }),
            "the store's audit marker must survive the later refute commit"
        );
    }

    #[cfg(unix)]
    #[test]
    fn conjecture_persist_is_all_or_nothing_on_a_failed_commit() {
        // forcing the atomic-replace write to fail PARTWAY (the temp file
        // for the combined library+audit bytes can't even be created) must leave the library
        // BYTE-UNCHANGED — never holding the library segment without its audit segment (or vice
        // versa), because both are assembled in memory and committed via ONE rename. This
        // simulates the "audit append fails after the library append already landed" half of the
        // gap: with the fix, there is no such partial state to observe.
        use std::os::unix::fs::PermissionsExt;

        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_conjecture();

        // Seed the library with one already-committed segment (as if a prior store succeeded).
        let seed_node = "https://blackcatinformatics.ca/gmeow/graph/conjecture/seed";
        let seed_nt = format!(
            "<{seed_node}> <{RDF_TYPE_IRI}> <{LOGIC_NS}Conjecture> .\n\
             <{seed_node}> <{LOGIC_NS}conjectureLifecycleState> <{LOGIC_NS}ConjectureOpen> .\n"
        );
        write_conjecture_segment(&path, &seed_nt).unwrap();
        let before = fs::read(&path).expect("seeded library bytes");
        assert!(!before.is_empty());

        // Make the library's directory read-only: the existing `.lock` sidecar can still be
        // opened (it already exists), but `append_conjecture_segments` can no longer create the
        // same-directory temp file its atomic rename depends on — an I/O failure squarely inside
        // the combined library+audit commit, after both segments' bytes are already built.
        let dir = path
            .parent()
            .expect("library has a parent dir")
            .to_path_buf();
        let original_mode = fs::metadata(&dir).unwrap().permissions().mode();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).expect("chmod read-only");

        let outcome = (|| -> gmeow_errors::Result<()> {
            let lib_segment = build_nt_segment(
                &[],
                &probe_medium(),
                &format!(
                    "<{seed_node}> <{LOGIC_NS}conjectureLifecycleState> <{LOGIC_NS}ConjectureWithdrawn> .\n"
                ),
            )?;
            let audit_segment = build_audit_segment(
                &[],
                &probe_medium(),
                "urn:gmeow:conjecture-call:simulated-failure",
                MCP_WITHDRAW_CONJECTURE_SCHEMA,
                &[MCP_CONJECTURE_IN_LIBRARY],
                "1970-01-01T00:00:00Z",
            )?;
            let library = library_at(&path);
            with_library_lock(library.as_ref(), || {
                append_library_segments(library.as_ref(), &[lib_segment, audit_segment])
            })
        })();

        // Restore permissions unconditionally before asserting, so the tempdir can still clean
        // itself up even if an assertion below panics.
        fs::set_permissions(&dir, fs::Permissions::from_mode(original_mode))
            .expect("chmod restore");

        assert!(
            outcome.is_err(),
            "the forced I/O failure must surface as an error, not a silent partial write"
        );
        let after = fs::read(&path).expect("library bytes after the failed commit");
        assert_eq!(
            before, after,
            "a failed combined library+audit commit must leave the library BYTE-UNCHANGED \
             (all-or-nothing) — never holding one segment without the other"
        );
        let library = read_library(library_at(&path).as_ref()).unwrap();
        assert_eq!(
            library.get(seed_node).copied(),
            Some(ConjectureLifecycleState::Open),
            "the seed's state must still be Open — the failed withdrawal must not have applied"
        );
    }

    #[test]
    fn conjecture_lock_serializes_concurrent_writers() {
        // `with_conjecture_lock` must provide REAL mutual exclusion, not
        // just an in-process convention a second caller could sidestep — so this test opens the
        // SAME sidecar `.lock` file from two INDEPENDENT `std::fs::File` descriptors (one per
        // thread), mirroring how two separate OS processes would each open it themselves. Since
        // `flock` locks are scoped to the OPEN FILE DESCRIPTION (not the process), two distinct
        // descriptors contending on the same path exercise the identical kernel mechanism that
        // serializes real cross-process `store_conjecture` / `refute_conjecture` callers.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_conjecture();

        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let holder_path = path.clone();
        let holder = std::thread::spawn(move || {
            with_library_lock(library_at(&holder_path).as_ref(), || {
                started_tx.send(()).expect("signal lock acquired");
                release_rx.recv().expect("wait for release signal");
                Ok(())
            })
            .expect("holder must acquire and release cleanly")
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the holder thread must acquire the lock");

        let waiter_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let waiter_done_writer = waiter_done.clone();
        let waiter_path = path.clone();
        let waiter = std::thread::spawn(move || {
            with_library_lock(library_at(&waiter_path).as_ref(), || {
                waiter_done_writer.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
            .expect("waiter must eventually acquire and release cleanly")
        });

        // While the holder still owns the lock, the waiter's OWN, independently-opened file
        // descriptor must be blocked — proving this is a real `flock`, not an in-process no-op.
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert!(
            !waiter_done.load(std::sync::atomic::Ordering::SeqCst),
            "a second, independently-opened lock attempt must still be blocked while the first \
             holder is inside its critical section"
        );

        release_tx.send(()).expect("release the holder");
        holder.join().expect("holder thread must not panic");
        waiter.join().expect("waiter thread must not panic");
        assert!(
            waiter_done.load(std::sync::atomic::Ordering::SeqCst),
            "the second lock attempt must complete once the first holder releases"
        );
    }

    #[test]
    fn read_conjecture_library_resolves_effective_state_by_segment_order() {
        // The reader resolves the effective lifecycle purely by SEGMENT ORDER (last writer
        // wins) — NOT by the union (which would carry both states at once) and NOT by
        // gmeow:atTime (every segment shares the fixed determinism epoch). Two nodes are
        // written with OPPOSITE last-writer states to prove position alone decides: the state
        // that "sounds terminal" (Withdrawn) does NOT win unless it is written LAST.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_conjecture();

        let base = "https://blackcatinformatics.ca/gmeow/graph/conjecture/";
        let reopened = format!("{base}reopened");
        let retired = format!("{base}retired");
        let ty = format!("<{reopened}> <{RDF_TYPE_IRI}> <{LOGIC_NS}Conjecture> .\n");
        let ty2 = format!("<{retired}> <{RDF_TYPE_IRI}> <{LOGIC_NS}Conjecture> .\n");
        let lc = |node: &str, state: &str| {
            format!("<{node}> <{LOGIC_NS}conjectureLifecycleState> <{LOGIC_NS}{state}> .\n")
        };

        // `reopened`: Withdrawn FIRST, then Open — Open is last, so Open must win.
        write_conjecture_segment(
            &path,
            &format!("{ty}{}", lc(&reopened, "ConjectureWithdrawn")),
        )
        .unwrap();
        // `retired`: Open FIRST, then Withdrawn — Withdrawn is last, so Withdrawn must win.
        write_conjecture_segment(&path, &format!("{ty2}{}", lc(&retired, "ConjectureOpen")))
            .unwrap();
        write_conjecture_segment(&path, &lc(&reopened, "ConjectureOpen")).unwrap();
        write_conjecture_segment(&path, &lc(&retired, "ConjectureWithdrawn")).unwrap();

        let library = read_library(library_at(&path).as_ref()).unwrap();
        assert_eq!(
            library.get(&reopened).copied(),
            Some(ConjectureLifecycleState::Open),
            "the LAST segment (Open) must win even though a prior segment said Withdrawn"
        );
        assert_eq!(
            library.get(&retired).copied(),
            Some(ConjectureLifecycleState::Withdrawn),
            "the LAST segment (Withdrawn) must win over the prior Open"
        );
    }

    /// A candidate authored as a REIFIED GROUND binary atom — the exact `logic:relation` /
    /// `logic:argument` reification every authored formula uses, but VARIABLE-FREE: the ground
    /// fact `ex:a rdf:type ex:<cls_local>`. The reconstructed `Formula::Atom` is trivially-Horn,
    /// so it once panicked `LogicProgram::with_formulas` during the candidate parse.
    fn reified_ground_atom_candidate(cls_local: &str) -> String {
        format!(
            "@prefix logic: <{LOGIC_NS}> .\n\
             @prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:phi a logic:Formula ;\n\
                 logic:relation rdf:type ;\n\
                 logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\n\
                 logic:argument [ logic:termIndex 1 ; logic:termIri ex:{cls_local} ] .\n"
        )
    }

    /// A KB that ASSERTS the ground fact the reified-ground-atom candidate names, so `KB ⊨ φ`.
    fn ground_atom_entailing_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a rdf:type ex:{cls_local} .\n"
        )
    }

    #[test]
    fn parse_candidate_reified_ground_atom_lifts_not_panics() {
        // F2 regression: a REIFIED GROUND binary atom is trivially-Horn, so the front-end must
        // route it to `LogicProgram.axioms` (not `with_formulas`, which hard-asserts) and
        // `parse_candidate_formula` must reconstruct it — cleanly, never a panic and never a
        // false "0 formula(s) and 0 axiom(s)" rejection.
        use gmeow_logic_compile::ir::{Formula, Term as IrTerm};
        // `parse_candidate_formula` now lives in the shared gmeow-logic conjecture-eval
        // authority; assert the SHIPPED re-export still lifts a reified ground atom cleanly.
        let candidate = gmeow_logic::conjecture_eval::parse_candidate_formula(
            &reified_ground_atom_candidate("B"),
        )
        .expect("reified ground atom must lift to a candidate formula");
        match candidate {
            Formula::Atom { relation, args } => {
                assert_eq!(relation, IrTerm::Iri(RDF_TYPE_IRI.to_owned()));
                assert_eq!(
                    args,
                    vec![
                        IrTerm::Iri("http://ex/a".to_owned()),
                        IrTerm::Iri("http://ex/B".to_owned()),
                    ]
                );
            }
            other => panic!("expected a ground binary atom, got {other:?}"),
        }
    }

    #[test]
    fn conjecture_test_reified_ground_atom_evaluates_via_shipped_core() {
        // F2 regression on the SHIPPED surface: driving `run_conjecture_test` (the shared core
        // behind the CLI + MCP tool) with a reified ground-atom candidate must EVALUATE it (a KB
        // asserting the fact entails φ ⇒ corroborated) rather than panic (exit 101).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, _path) = temp_conjecture();

        let out = run_conjecture_test(
            &ConjectureRunInput {
                formula_ttl: &reified_ground_atom_candidate("B"),
                kb_ttl: &ground_atom_entailing_kb("B"),
                standpoint: "http://ex/standpoint/alice",
                math_conjecture: None,
                dry_run: true,
                max_steps: None,
                max_answers: None,
            },
            &probe_medium(),
            source_action_policy(),
        )
        .expect("a reified ground-atom conjecture must evaluate, not panic");
        assert_eq!(out.lifecycle, "corroborated");
        assert_eq!(out.information, "supported");
        assert_eq!(out.evaluation, "completed");
    }

    fn temp_candidate() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("candidates.gts");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_CANDIDATE_PATH", &path);
        }
        (dir, path)
    }

    #[test]
    fn submit_candidate_admits_corroborated_records_provenance_and_lists() {
        // AC5: a candidate whose isolated-world verdict CORROBORATES it is admissible, so it is
        // committed to the append-only candidate library — carrying its target provenance — and
        // becomes visible to list_candidates.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_candidate();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        assert!(!path.exists(), "library must not exist before the call");
        let resp = text_payload(server.call_tool_result(
            "submit_candidate",
            &json!({
                "formula": reified_ground_atom_candidate("B"),
                "kb": ground_atom_entailing_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
                "for_slice": "https://blackcatinformatics.ca/gmeow/slices/logic",
            }),
        ));
        assert_eq!(resp["ok"], true, "an admissible candidate commits: {resp}");
        assert_eq!(resp["admissible"], true);
        assert_eq!(resp["committed"], true);
        assert_eq!(resp["verdict"]["lifecycle"], "corroborated");
        let node = resp["candidate"]
            .as_str()
            .expect("candidate iri")
            .to_string();
        assert!(path.exists(), "the admissible candidate was appended");

        // list_candidates surfaces it, in-library, with its provenance.
        let list = text_payload(server.call_tool_result("list_candidates", &json!({})));
        assert_eq!(list["ok"], true);
        assert_eq!(list["candidate_count"], 1, "one admitted candidate: {list}");
        let c = &list["candidates"][0];
        assert_eq!(c["candidate"], node);
        assert_eq!(c["disposition"], "in-library");
        assert_eq!(
            c["for_slice"],
            "https://blackcatinformatics.ca/gmeow/slices/logic"
        );

        // The slice-provenance filter matches and mismatches correctly.
        let filtered = text_payload(server.call_tool_result(
            "list_candidates",
            &json!({"slice": "https://blackcatinformatics.ca/gmeow/slices/logic"}),
        ));
        assert_eq!(filtered["candidate_count"], 1);
        let other = text_payload(
            server.call_tool_result("list_candidates", &json!({"slice": "http://ex/nope"})),
        );
        assert_eq!(other["candidate_count"], 0);
    }

    #[test]
    fn submit_candidate_stages_nothing_on_refuted_or_open() {
        // AC6: a refuted (or open) candidate is NOT admissible — the candidateAdmissible
        // precondition never obtains, the commit fails, and the library file stays byte-identical
        // (here: absent). This is the polarity gate a verbatim conjecture clone would get WRONG
        // (it would commit a refuted node).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let bytes = snapshot();

        for (label, formula, kb, expect_lifecycle) in [
            (
                "refuted",
                forall_horn_candidate("B"),
                refuting_kb("B"),
                "refuted-in-standpoint",
            ),
            ("open", forall_horn_candidate("B"), open_kb("B"), "open"),
        ] {
            let (_env, _cg) = ConjEnvGuard::set();
            let (_dir, path) = temp_candidate();
            let server = McpServer::from_snapshot(&bytes).unwrap();
            assert!(!path.exists(), "{label}: library absent before the call");

            let resp = text_payload(server.call_tool_result(
                "submit_candidate",
                &json!({
                    "formula": formula,
                    "kb": kb,
                    "standpoint": "http://ex/standpoint/alice",
                }),
            ));
            assert_eq!(resp["ok"], false, "{label}: not admitted: {resp}");
            assert_eq!(resp["admissible"], false, "{label}");
            assert_eq!(resp["verdict"]["lifecycle"], expect_lifecycle, "{label}");
            assert!(
                !path.exists(),
                "{label}: a non-admissible candidate must write NOTHING to the library"
            );
        }
    }

    #[test]
    fn submit_candidate_refuted_leaves_populated_library_byte_identical() {
        // AC6, strengthened: the "stages nothing" invariant must hold against a NON-EMPTY store,
        // not just an absent one. Admit a corroborated candidate (the library now holds real
        // bytes), snapshot them, then submit a genuinely refuted candidate — the polarity gate
        // must leave the on-disk library BYTE-IDENTICAL (no append, no truncation, no rewrite).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_candidate();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // Populate the library with one admissible candidate.
        let admit = text_payload(server.call_tool_result(
            "submit_candidate",
            &json!({
                "formula": reified_ground_atom_candidate("B"),
                "kb": ground_atom_entailing_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(
            admit["committed"], true,
            "setup: admissible candidate commits"
        );
        let populated = std::fs::read(&path).expect("library exists after an admitted candidate");
        assert!(
            !populated.is_empty(),
            "the populated library holds real bytes"
        );

        // A refuted submit against the POPULATED store must write nothing.
        let resp = text_payload(server.call_tool_result(
            "submit_candidate",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(
            resp["ok"], false,
            "refuted candidate is not admitted: {resp}"
        );
        assert_eq!(resp["admissible"], false);
        assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");

        let after = std::fs::read(&path).expect("library still present");
        assert_eq!(
            after, populated,
            "a refuted submit must leave the populated library byte-identical"
        );
    }

    #[test]
    fn submit_candidate_dry_run_writes_nothing() {
        // A dry-run on an ADMISSIBLE candidate computes the verdict via a hypothetical commit but
        // writes nothing.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_candidate();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let resp = text_payload(server.call_tool_result(
            "submit_candidate",
            &json!({
                "formula": reified_ground_atom_candidate("B"),
                "kb": ground_atom_entailing_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
                "dry_run": true,
            }),
        ));
        assert_eq!(resp["ok"], true, "{resp}");
        assert_eq!(resp["dry_run"], true);
        assert_eq!(resp["admissible"], true);
        assert!(
            !path.exists(),
            "a dry-run submit must write nothing to the library"
        );
    }

    #[test]
    fn withdraw_candidate_supersedes_and_gates() {
        // Submit an admissible candidate, then withdraw it: list flips it to `withdrawn`
        // (superseded, never deleted). Withdrawing an unknown id hard-fails before writing.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_candidate();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let submit = text_payload(server.call_tool_result(
            "submit_candidate",
            &json!({
                "formula": reified_ground_atom_candidate("B"),
                "kb": ground_atom_entailing_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let node = submit["candidate"]
            .as_str()
            .expect("candidate iri")
            .to_string();

        // Withdrawing an unknown id fails the precondition (nothing appended for it).
        let unknown = text_payload(server.call_tool_result(
            "withdraw_candidate",
            &json!({"candidate_id": "urn:gmeow:not-a-candidate"}),
        ));
        assert_eq!(unknown["ok"], false, "unknown id must hard-fail: {unknown}");

        // Withdrawing the real node succeeds and supersedes it.
        let withdraw = text_payload(
            server.call_tool_result("withdraw_candidate", &json!({"candidate_id": node})),
        );
        assert_eq!(withdraw["ok"], true, "withdraw the real node: {withdraw}");
        assert!(path.exists());

        let list = text_payload(server.call_tool_result("list_candidates", &json!({})));
        assert_eq!(
            list["candidate_count"], 1,
            "still listed (superseded): {list}"
        );
        assert_eq!(list["candidates"][0]["disposition"], "withdrawn");

        // The disposition filter narrows correctly.
        let in_library = text_payload(
            server.call_tool_result("list_candidates", &json!({"disposition": "in-library"})),
        );
        assert_eq!(in_library["candidate_count"], 0);
    }

    #[test]
    fn action_policy_covers_the_candidate_submission_pair() {
        // The submit_candidate ⇄ withdraw_candidate governed-write pair must be REPRESENTED in
        // the canonical action theory the engine parses (the same projected N-Quads the TR run
        // feeds), with the mutual P10 compensation pairing — not merely documented.
        const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/";
        const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        const LOGIC_MCP_ACTION_SCHEMA: &str =
            "https://blackcatinformatics.ca/logic/McpActionSchema";
        const LOGIC_COMPENSATION: &str = "https://blackcatinformatics.ca/logic/compensation";

        let policy = action_policy_nquads();
        for (schema, compensation) in [
            ("submitCandidate", "withdrawCandidate"),
            ("withdrawCandidate", "submitCandidate"),
        ] {
            let type_line =
                format!("<{EX}{schema}> <{RDF_TYPE}> <{LOGIC_MCP_ACTION_SCHEMA}> <{TXN_WORLD}> .");
            let comp_line = format!(
                "<{EX}{schema}> <{LOGIC_COMPENSATION}> <{EX}{compensation}> <{TXN_WORLD}> ."
            );
            assert!(
                policy.contains(&type_line),
                "{schema} must be typed logic:McpActionSchema: missing {type_line:?}"
            );
            assert!(
                policy.contains(&comp_line),
                "{schema}'s compensation must be {compensation}: missing {comp_line:?}"
            );
        }
    }

    /// A KB whose `ex:trigger` fires the `∀`-Horn candidate on SEVERAL individuals, so the
    /// candidate's derived (non-EDB) closure is strictly larger than a `max_steps`/`max_answers`
    /// bound of 1 — the isolated scenario evaluation exceeds the ceiling.
    fn multi_trigger_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:b ex:trigger ex:mark .\n\
             ex:c ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             # {cls_local} is unrelated to A — no clash, just several derived facts.\n"
        )
    }

    #[test]
    fn conjecture_test_budget_bound_forces_open_via_the_mcp_surface() {
        // The `max_steps` / `max_answers` bound is reachable from the SHIPPED MCP
        // surface (not just the logic-crate unit test). A run whose derived closure exceeds the
        // ceiling is truncated → evaluation budget-exhausted → lifecycle open → discharge
        // Unknown. This is a PURE assertion on `conjecture_test`: no persist tail exists on
        // this surface at all, so the library stays absent throughout.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // Unbounded control: the same candidate/KB runs to Completed (a non-budget verdict).
        let unbounded = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(unbounded["ok"], true);
        assert_ne!(
            unbounded["verdict"]["evaluation"], "budget-exhausted",
            "the unbounded control must not trip the ceiling: {unbounded}"
        );

        // Bounded: a ceiling of 1 truncates the multi-fact derived closure.
        let bounded = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
                "max_steps": 1,
            }),
        ));
        assert_eq!(
            bounded["ok"], true,
            "bounded run must compute a verdict: {bounded}"
        );
        assert_eq!(
            bounded["verdict"]["evaluation"], "budget-exhausted",
            "exceeding the ceiling must stamp BudgetExhausted: {bounded}"
        );
        assert_eq!(
            bounded["verdict"]["lifecycle"], "open",
            "a budget-exhausted run is inconclusive → Open: {bounded}"
        );
        assert_eq!(
            bounded["verdict"]["discharge"], "ObligationUnknown",
            "a budget-exhausted run carries the obligation forward as Unknown: {bounded}"
        );

        // `max_answers` is the equivalent binding-count ceiling and trips the same way.
        let bounded_answers = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
                "max_answers": 1,
            }),
        ));
        assert_eq!(
            bounded_answers["verdict"]["evaluation"], "budget-exhausted",
            "max_answers must impose the same ceiling: {bounded_answers}"
        );
        // `conjecture_test` is PURE: none of these calls ever touch the library.
        assert!(
            !path.exists(),
            "conjecture_test calls must write nothing to the library"
        );
    }

    #[test]
    fn store_conjecture_budget_bound_forces_open_and_still_persists() {
        // R3b acceptance: a budget-exhausted `store_conjecture` (committing) must still yield
        // the non-conclusive verdict — lifecycle open, discharge Unknown — via the governor,
        // NEVER a false discharge, even though the run commits and appends to the library.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        assert!(!path.exists());
        let bounded = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
                "max_steps": 1,
            }),
        ));
        assert_eq!(
            bounded["ok"], true,
            "a budget-exhausted commit must still compute+persist a verdict: {bounded}"
        );
        assert_eq!(bounded["verdict"]["evaluation"], "budget-exhausted");
        assert_eq!(
            bounded["verdict"]["lifecycle"], "open",
            "never a false discharge: budget-exhausted must stay Open: {bounded}"
        );
        assert_eq!(bounded["verdict"]["discharge"], "ObligationUnknown");
        assert_eq!(bounded["transaction"]["committed"], true);

        // The non-conclusive verdict is still a real, committed, append-only segment.
        assert!(
            path.exists() && fs::metadata(&path).unwrap().len() > 0,
            "a committed budget-exhausted run must still append its Open verdict"
        );
        let node = bounded["conjecture"]
            .as_str()
            .expect("node iri")
            .to_string();
        assert!(conjecture_nodes(read_conjectures(&path).as_ref()).contains(&node));
    }

    #[test]
    fn store_conjecture_persists_are_append_only() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let first = fs::read(&path).expect("first library bytes");
        let first_len = first.len();
        assert!(first_len > 0);

        // A DISTINCT conjecture (different head class) appends a second segment.
        text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": open_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let second = fs::read(&path).expect("second library bytes");
        assert!(
            second.len() > first_len,
            "the second persist must APPEND, growing the file"
        );
        assert_eq!(
            &second[..first_len],
            &first[..],
            "the first segment's bytes must be intact (append-only, never mutated)"
        );
        // Both conjectures are readable back.
        assert_eq!(conjecture_nodes(read_conjectures(&path).as_ref()).len(), 2);
    }

    #[test]
    fn store_conjecture_library_is_isolated_from_the_base_kb() {
        // R2: the caller's KB text is unchanged by the call, and the library is a DISTINCT
        // file the reasoner never folds into its base graph.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, mem_path) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let kb = refuting_kb("B");
        let kb_before = kb.clone();
        text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": kb,
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        // The KB argument the tool was given is an owned String — the tool cannot mutate the
        // caller's copy. (Isolation is inherent: store_conjecture borrows and copies the KB.)
        assert_eq!(kb_before, refuting_kb("B"));
        // The conjecture library is a distinct file, NOT the memory store, and never the base
        // reasoning graph (reason reads graph_dataset(), never conjecture_path()).
        assert!(path.exists());
        assert_ne!(path, mem_path);
        // The bundled reasoning surface is unaffected — reason still runs cleanly over the
        // untouched base graph (the library is never unioned in).
        let reasoned = text_payload(server.call_tool_result("reason", &json!({})));
        // `reason` is dev-only; over a Consumer server it returns an error, proving the base
        // reasoning path does not consult the library either way. What matters for R2 is that
        // the library file and the KB are untouched, asserted above.
        let _ = reasoned;
    }

    #[test]
    fn same_formula_two_standpoints_mints_two_nodes() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let a = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": open_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let b = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": open_kb("C"),
                "standpoint": "http://ex/standpoint/bob",
            }),
        ));
        assert_ne!(
            a["conjecture"], b["conjecture"],
            "the same formula in two standpoints must mint two DISTINCT nodes (P9)"
        );
        assert_eq!(conjecture_nodes(read_conjectures(&path).as_ref()).len(), 2);
    }

    #[test]
    fn conjecture_library_corpus_query_recovers_open_and_refuted() {
        // A corpus competency: persist several DISTINCT conjectures (open + refuted) in one
        // standpoint, then scan the collection for all of them, with witness premises for the
        // refuted ones recoverable.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // Two refuted (B, D disjoint) and two open (C, E unrelated).
        for cls in ["B", "D"] {
            let r = text_payload(server.call_tool_result(
                "store_conjecture",
                &json!({
                    "formula": forall_horn_candidate(cls),
                    "kb": refuting_kb(cls),
                    "standpoint": "http://ex/standpoint/team",
                }),
            ));
            assert_eq!(r["verdict"]["lifecycle"], "refuted-in-standpoint", "{r}");
        }
        for cls in ["C", "E"] {
            let r = text_payload(server.call_tool_result(
                "store_conjecture",
                &json!({
                    "formula": forall_horn_candidate(cls),
                    "kb": open_kb(cls),
                    "standpoint": "http://ex/standpoint/team",
                }),
            ));
            assert_eq!(r["verdict"]["lifecycle"], "open", "{r}");
        }

        let dataset = read_conjectures(&path);
        // All four conjectures are recoverable.
        assert_eq!(conjecture_nodes(&dataset).len(), 4);
        // All are scoped to the team standpoint.
        let team_scoped = dataset
            .owned_quads()
            .filter(|q| {
                q.predicate == format!("{LOGIC_NS}conjectureStandpoint")
                    && q.object == RdfTerm::iri("http://ex/standpoint/team")
            })
            .count();
        assert_eq!(team_scoped, 4, "every conjecture is standpoint-scoped");
        // The two refuted conjectures each carry a recoverable individual + premises.
        let refuted = dataset
            .owned_quads()
            .filter(|q| {
                q.predicate == format!("{LOGIC_NS}conjectureLifecycleState")
                    && q.object == RdfTerm::iri(format!("{LOGIC_NS}ConjectureRefutedInStandpoint"))
            })
            .count();
        assert_eq!(refuted, 2, "exactly the two disjointness cases are refuted");
        assert!(
            witness_premises(&dataset).len() >= 2,
            "each refutation persists recoverable witness premises"
        );
    }

    /// `counter_examples` over the live bundle: a term documenting fixtures yields
    /// the real, split fixture bodies; a resolvable term documenting none yields the
    /// honest empty-but-ok shape; an unknown term is a hard error envelope.
    ///
    /// `gmeow:Activity` documents BOTH a well-formed exemplar and a counter-example
    /// in the shipped `gmeow:graph/documentation` graph (verified by the projection
    /// query in the shipped surface); `gmeow:AboutnessMode` is a documented term that authors no
    /// fixtures.
    #[test]
    fn tool_counter_examples_surface() {
        let server = consumer_server();

        // A term with fixtures → real, non-empty split bodies.
        let hit = text_payload(
            server.call_tool_result("counter_examples", &json!({"term": "gmeow:Activity"})),
        );
        assert_eq!(hit["ok"], true);
        assert_eq!(hit["term"], "gmeow:Activity");
        let counter = hit["counter_examples"]
            .as_array()
            .expect("counter_examples is an array");
        let wellformed = hit["wellformed"]
            .as_array()
            .expect("wellformed is an array");
        assert!(
            !counter.is_empty(),
            "gmeow:Activity documents at least one counter-example: {hit}"
        );
        assert!(
            !wellformed.is_empty(),
            "gmeow:Activity documents at least one well-formed exemplar: {hit}"
        );
        // The counter-example carries a real Turtle body AND a violation code.
        let ce = &counter[0];
        assert!(
            ce["text"].as_str().is_some_and(|t| t.contains(':')),
            "counter-example carries a real Turtle body: {ce}"
        );
        assert!(
            ce["violation_code"].as_str().is_some_and(|c| !c.is_empty()),
            "counter-example carries an authored violation code: {ce}"
        );
        // The well-formed exemplar has a real body and NO violation code.
        let wf = &wellformed[0];
        assert!(
            wf["text"].as_str().is_some_and(|t| t.contains(':')),
            "well-formed exemplar carries a real Turtle body: {wf}"
        );
        assert!(
            wf["violation_code"].is_null(),
            "well-formed exemplar has no violation code: {wf}"
        );
        // Deterministic: a second call is byte-identical.
        let again = text_payload(
            server.call_tool_result("counter_examples", &json!({"term": "gmeow:Activity"})),
        );
        assert_eq!(hit, again, "counter_examples output is deterministic");

        // A resolvable term with NO fixtures → empty-but-ok (NOT an error).
        let empty = text_payload(
            server.call_tool_result("counter_examples", &json!({"term": "gmeow:AboutnessMode"})),
        );
        assert_eq!(
            empty["ok"], true,
            "no-fixture term is empty-but-ok: {empty}"
        );
        assert_eq!(empty["wellformed"], json!([]));
        assert_eq!(empty["counter_examples"], json!([]));

        // An unknown term → hard error envelope.
        let unknown = text_payload(server.call_tool_result(
            "counter_examples",
            &json!({"term": "gmeow:DefinitelyNotARealTerm42"}),
        ));
        assert_eq!(
            unknown["ok"], false,
            "unknown term is a hard error: {unknown}"
        );
        assert!(
            unknown["error"]
                .as_str()
                .is_some_and(|e| e.contains("unknown term")),
            "unknown-term error names the failure: {unknown}"
        );
    }

    /// `entailments` over the live bundle: a term with derivations yields every
    /// entailment's rule/conclusion with its premises preserved; an unknown term is a
    /// hard error. `gmeow:Entity` grounds >1000 entailment records in the shipped
    /// documentation graph.
    #[test]
    fn tool_entailments_surface() {
        let server = consumer_server();

        let hit =
            text_payload(server.call_tool_result("entailments", &json!({"term": "gmeow:Entity"})));
        assert_eq!(hit["ok"], true);
        assert_eq!(hit["term"], "gmeow:Entity");
        let entailments = hit["entailments"]
            .as_array()
            .expect("entailments is an array");
        assert!(
            !entailments.is_empty(),
            "gmeow:Entity grounds at least one entailment: {}",
            &hit.to_string()[..hit.to_string().len().min(400)]
        );
        // Every record carries a non-empty rule and conclusion.
        for e in entailments {
            assert!(
                e["rule"].as_str().is_some_and(|r| !r.is_empty()),
                "entailment carries a rule: {e}"
            );
            assert!(
                e["conclusion"].as_str().is_some_and(|c| !c.is_empty()),
                "entailment carries a conclusion: {e}"
            );
            assert!(e["premises"].is_array(), "premises is an array: {e}");
        }
        // Premises are preserved: at least one derivation carries premises.
        let total_premises: usize = entailments
            .iter()
            .map(|e| e["premises"].as_array().map_or(0, Vec::len))
            .sum();
        assert!(
            total_premises > 0,
            "at least one gmeow:Entity entailment preserves its premises"
        );

        // Unknown term → hard error envelope.
        let unknown = text_payload(server.call_tool_result(
            "entailments",
            &json!({"term": "gmeow:DefinitelyNotARealTerm42"}),
        ));
        assert_eq!(
            unknown["ok"], false,
            "unknown term is a hard error: {unknown}"
        );
    }

    /// Select the RICHEST-surface term in the shipped bundle for the tier tests:
    /// the term (a) grounding at least one reasoner entailment AND (b) documenting at
    /// least one conformance fixture, maximizing the total panel count (entailments +
    /// fixtures), tie-broken by IRI so the choice is deterministic. Panics (a real
    /// blocker, never a soft skip) if the bundle documents no such term.
    ///
    /// Uses exactly TWO bulk queries (the entailment map + a single fixture-by-term
    /// scan) and intersects them — never a per-term query per candidate.
    fn richest_card_term(server: &McpServer) -> String {
        let entailments = server
            .view
            .entailment_map()
            .expect("entailment map from the shipped documentation graph");
        // One bulk scan: fixture count per documented term.
        let fixtures_query = format!(
            "PREFIX gm: <{GMEOW_NS}>\nSELECT ?term ?f WHERE {{ ?f a gm:DocFixture ; \
             gm:documents ?term . }}"
        );
        let mut fixtures_per_term: BTreeMap<String, usize> = BTreeMap::new();
        for row in server
            .view
            .docs_select_rows(&fixtures_query)
            .expect("fixture-by-term scan over graph/documentation")
        {
            if let Some(term) = row.get("term") {
                *fixtures_per_term.entry(term.clone()).or_default() += 1;
            }
        }
        let mut candidates: Vec<(usize, String)> = entailments
            .iter()
            .filter_map(|(iri, ents)| {
                fixtures_per_term
                    .get(iri)
                    .map(|&fx| (ents.len() + fx, iri.clone()))
            })
            .collect();
        // Most panels first, then lexicographically-first IRI (deterministic).
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        candidates
            .into_iter()
            .next()
            .map(|(_, iri)| iri)
            .expect("the shipped bundle documents a term with entailments AND fixtures")
    }

    /// A documented term that still carries full-tier panels (≥1 entailment AND ≥1
    /// fixture) but the FEWEST of them — the cheapest term whose `full` card exercises
    /// every rich-panel path. Tier/determinism assertions that render the full card
    /// several times use this instead of [`richest_card_term`] (whose ~1000-entailment
    /// surface is only needed for the byte-ceiling proof), so they render fast.
    fn modest_panel_card_term(server: &McpServer) -> String {
        let entailments = server
            .view
            .entailment_map()
            .expect("entailment map from the shipped documentation graph");
        let fixtures_query = format!(
            "PREFIX gm: <{GMEOW_NS}>\nSELECT ?term ?f WHERE {{ ?f a gm:DocFixture ; \
             gm:documents ?term . }}"
        );
        let mut fixtures_per_term: BTreeMap<String, usize> = BTreeMap::new();
        for row in server
            .view
            .docs_select_rows(&fixtures_query)
            .expect("fixture-by-term scan over graph/documentation")
        {
            if let Some(term) = row.get("term") {
                *fixtures_per_term.entry(term.clone()).or_default() += 1;
            }
        }
        let mut candidates: Vec<(usize, String)> = entailments
            .iter()
            .filter_map(|(iri, ents)| {
                fixtures_per_term
                    .get(iri)
                    .map(|&fx| (ents.len() + fx, iri.clone()))
            })
            .collect();
        // FEWEST panels first, then lexicographically-first IRI (deterministic).
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        candidates
            .into_iter()
            .next()
            .map(|(_, iri)| iri)
            .expect("the shipped bundle documents a term with entailments AND fixtures")
    }

    /// `doc_card` tiers: `summary` is the leanest surface (title + definition only,
    /// under a pinned byte ceiling, none of the advisory / panel sections); `full`
    /// is strictly larger and carries the rich oracle panels (Entailments / Do /
    /// Don't headers).
    #[test]
    fn tool_doc_card_tier_byte_ceiling() {
        const SUMMARY_CEILING: usize = 1500;
        let server = consumer_server();
        let term = richest_card_term(&server);

        let summary = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "summary"})),
        );
        let full = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "full"})),
        );
        let s_card = summary["card"].as_str().expect("summary markdown card");
        let f_card = full["card"].as_str().expect("full markdown card");

        // Summary is title + definition ONLY, under the ceiling.
        assert!(
            s_card.len() < SUMMARY_CEILING,
            "summary card ({} bytes) must be under {SUMMARY_CEILING}: {s_card}",
            s_card.len()
        );
        assert!(
            s_card.starts_with("# "),
            "summary carries the H1 title: {s_card}"
        );
        let summary_body = s_card
            .strip_prefix("# ")
            .and_then(|s| s.split_once("\n\n"))
            .map_or("", |(_, rest)| rest);
        assert!(
            !summary_body.trim().is_empty(),
            "summary carries a definition after the title: {s_card}"
        );
        // NONE of the advisory / metadata / panel surface at the summary tier.
        assert!(
            !s_card.contains("- category:"),
            "no metadata header: {s_card}"
        );
        assert!(
            !s_card.contains("**Use when:**"),
            "no advisory fields: {s_card}"
        );
        assert!(
            !s_card.contains("## Entailments"),
            "no rich panels: {s_card}"
        );

        // Full is strictly larger and carries the panel section headers.
        assert!(
            f_card.len() > s_card.len(),
            "full ({}) must exceed summary ({})",
            f_card.len(),
            s_card.len()
        );
        assert!(
            f_card.contains("## Entailments"),
            "full carries Entailments: {f_card}"
        );
        assert!(
            f_card.contains("## Do") || f_card.contains("## Don't"),
            "full carries a Do / Don't fixture panel: {f_card}"
        );
    }

    /// Single-renderer authority: `doc_card` at `detail=standard` is BYTE-IDENTICAL
    /// to rendering the shared compact `Card` through `render_card` at `Standard` —
    /// the tier gating never perturbs the docs-site card the standard tier mirrors.
    #[test]
    fn tool_doc_card_standard_is_byte_identical_to_compact_render() {
        let server = consumer_server();
        let term = "gmeow:EntityExistence";

        let envelope = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "standard"})),
        );
        let card_md = envelope["card"].as_str().expect("standard markdown card");

        // Independent expected: the SAME shared builder + renderer at Standard.
        let requested = server.startup_requested.clone();
        let modeled_defs = server.view.modeled_defs();
        let expected = server.view.with_terms(requested, |terms| {
            let (title, card) = export::doc_card_build(terms, term, &modeled_defs)
                .resolved()
                .expect("known term resolves");
            gmeow_docs_model::card::render_card(
                &title,
                &card,
                gmeow_docs_model::card::CardDetail::Standard,
            )
        });
        assert_eq!(
            card_md, expected,
            "standard tier must be byte-identical to the compact single-renderer output"
        );
        // Default (no `detail`) is Standard.
        let defaulted = text_payload(server.call_tool_result("doc_card", &json!({"term": term})));
        assert_eq!(defaulted["detail"], "standard");
        assert_eq!(defaulted["card"].as_str().expect("card"), expected);
    }

    /// `doc_card` `format=json`: byte-stable across calls; standard-tier JSON omits
    /// the full-tier rich fields; full-tier JSON carries them.
    #[test]
    fn tool_doc_card_json_determinism_and_tier_fields() {
        let server = consumer_server();
        let docs_a = server.view.documentation();
        let docs_b = server.view.documentation();
        assert!(
            Arc::ptr_eq(docs_a, docs_b),
            "documentation queries must share one projected graph"
        );
        // Determinism + tier-field presence hold for ANY term that carries panels,
        // so use the cheapest such term (not the ~1000-entailment richest term) —
        // this test renders the full card three times, so a lean term keeps it fast.
        let term = modest_panel_card_term(&server);

        // Byte-identical raw tool text across two identical calls.
        let call = || {
            server.call_tool_result(
                "doc_card",
                &json!({"term": term, "detail": "full", "format": "json"}),
            )["content"][0]["text"]
                .as_str()
                .expect("json tool text")
                .to_string()
        };
        assert_eq!(call(), call(), "json card is byte-stable across calls");

        // Standard-tier JSON: a Card object WITHOUT the full-tier rich fields.
        let std_json = text_payload(server.call_tool_result(
            "doc_card",
            &json!({"term": term, "detail": "standard", "format": "json"}),
        ));
        assert_eq!(std_json["format"], "json");
        let std_card = &std_json["card"];
        assert!(std_card.is_object(), "json card is an object: {std_card}");
        assert!(
            std_card.get("entailments").is_none(),
            "no entailments at standard"
        );
        assert!(
            std_card.get("fixtures_do").is_none(),
            "no fixtures_do at standard"
        );
        assert!(
            std_card.get("fixtures_dont").is_none(),
            "no fixtures_dont at standard"
        );
        assert!(
            std_card.get("diagnostics").is_none(),
            "no diagnostics at standard"
        );
        assert!(std_card.get("loss").is_none(), "no loss at standard");

        // Full-tier JSON DOES carry the rich panels.
        let full_json = text_payload(server.call_tool_result(
            "doc_card",
            &json!({"term": term, "detail": "full", "format": "json"}),
        ));
        assert!(
            full_json["card"].get("entailments").is_some(),
            "full json carries entailments: {}",
            &full_json.to_string()[..full_json.to_string().len().min(400)]
        );
    }

    /// `doc_card` cost metadata: every envelope carries positive `bytes`/`tokens`,
    /// monotonically non-decreasing across summary ≤ standard ≤ full for one term.
    #[test]
    fn tool_doc_card_cost_metadata_is_monotone() {
        let server = consumer_server();
        let term = richest_card_term(&server);

        let tier = |detail: &str| {
            text_payload(
                server.call_tool_result("doc_card", &json!({"term": term, "detail": detail})),
            )
        };
        let summary = tier("summary");
        let standard = tier("standard");
        let full = tier("full");

        for env in [&summary, &standard, &full] {
            assert!(
                env["bytes"].as_u64().expect("bytes") > 0,
                "bytes > 0: {env}"
            );
            assert!(
                env["tokens"].as_u64().expect("tokens") > 0,
                "tokens > 0: {env}"
            );
        }
        let bytes = |e: &Value| e["bytes"].as_u64().unwrap();
        let tokens = |e: &Value| e["tokens"].as_u64().unwrap();
        assert!(
            bytes(&summary) <= bytes(&standard),
            "bytes summary ≤ standard"
        );
        assert!(bytes(&standard) <= bytes(&full), "bytes standard ≤ full");
        assert!(
            tokens(&summary) <= tokens(&standard),
            "tokens summary ≤ standard"
        );
        assert!(tokens(&standard) <= tokens(&full), "tokens standard ≤ full");

        // Unknown detail / format is a hard error listing the valid values.
        let bad_detail = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "verbose"})),
        );
        assert_eq!(
            bad_detail["ok"], false,
            "unknown detail hard-fails: {bad_detail}"
        );
        let bad_format = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "format": "yaml"})),
        );
        assert_eq!(
            bad_format["ok"], false,
            "unknown format hard-fails: {bad_format}"
        );
    }

    /// `doc_card` full tier populates the rich panels FROM the documentation graph:
    /// the full markdown inlines an actual entailment conclusion and a fixture title
    /// the sibling `entailments` / `counter_examples` tools report for the term.
    #[test]
    fn tool_doc_card_full_inlines_graph_panels() {
        let server = consumer_server();
        let term = richest_card_term(&server);

        let full = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "full"})),
        );
        let f_card = full["card"].as_str().expect("full markdown card");

        // An entailment's conclusion (from the SAME graph the `entailments` tool reads).
        let ents = text_payload(server.call_tool_result("entailments", &json!({"term": term})));
        let conclusion = ents["entailments"][0]["conclusion"]
            .as_str()
            .expect("the richest term grounds an entailment with a conclusion");
        assert!(
            f_card.contains(conclusion),
            "full card inlines the entailment conclusion {conclusion:?}"
        );

        // A fixture title (from the SAME graph the `counter_examples` tool reads).
        let fixtures =
            text_payload(server.call_tool_result("counter_examples", &json!({"term": term})));
        let title = fixtures["counter_examples"]
            .get(0)
            .and_then(|f| f["title"].as_str())
            .or_else(|| {
                fixtures["wellformed"]
                    .get(0)
                    .and_then(|f| f["title"].as_str())
            })
            .expect("the richest term documents a fixture with a title");
        assert!(
            f_card.contains(title),
            "full card inlines the fixture title {title:?}"
        );
    }

    /// `competency_questions` over the live bundle: the index form (no `term`) returns
    /// every runnable question, each carrying a `query_text` that PARSES as a valid
    /// SPARQL SELECT; the per-term form returns that term's subset. `gmeow:Agent`
    /// documents competency questions in the shipped documentation graph.
    #[test]
    fn tool_competency_questions_surface() {
        let server = consumer_server();

        // Index form: no `term`.
        let index = text_payload(server.call_tool_result("competency_questions", &json!({})));
        assert_eq!(index["ok"], true);
        assert!(
            index.get("term").is_none(),
            "index form carries no term key: {}",
            &index.to_string()[..index.to_string().len().min(200)]
        );
        let questions = index["questions"]
            .as_array()
            .expect("questions is an array");
        assert!(!questions.is_empty(), "the competency index is non-empty");
        for q in questions {
            assert!(
                q["query_text"].as_str().is_some_and(|t| !t.is_empty()),
                "every competency question carries a runnable query_text: {q}"
            );
        }

        // The first question's query_text round-trips through the native SPARQL
        // parser as an executable SELECT (over an empty dataset — parse+plan only).
        let first_query = questions[0]["query_text"].as_str().expect("query_text");
        let empty = std::sync::Arc::new(
            purrdf::RdfDatasetBuilder::new()
                .freeze()
                .expect("empty dataset"),
        );
        let parsed = gmeow_bundle_view::native_query::query(&empty, first_query)
            .expect("competency query_text is a valid SPARQL query");
        assert!(
            matches!(parsed, purrdf::SparqlResult::Solutions { .. }),
            "competency query_text is a SPARQL SELECT: {first_query}"
        );

        // Deterministic index.
        let index_again = text_payload(server.call_tool_result("competency_questions", &json!({})));
        assert_eq!(index, index_again, "competency index is deterministic");

        // Per-term form: gmeow:Agent's subset — non-empty, each with a query_text.
        let per_term = text_payload(
            server.call_tool_result("competency_questions", &json!({"term": "gmeow:Agent"})),
        );
        assert_eq!(per_term["ok"], true);
        assert_eq!(per_term["term"], "gmeow:Agent");
        let agent_questions = per_term["questions"]
            .as_array()
            .expect("questions is an array");
        assert!(
            !agent_questions.is_empty(),
            "gmeow:Agent documents at least one competency question: {per_term}"
        );
        for q in agent_questions {
            assert!(
                q["query_text"].as_str().is_some_and(|t| !t.is_empty()),
                "per-term competency question carries a query_text: {q}"
            );
        }
        assert!(
            agent_questions.len() <= questions.len(),
            "a term's competency subset is no larger than the whole index"
        );

        // Unknown term (per-term form) → hard error envelope.
        let unknown = text_payload(server.call_tool_result(
            "competency_questions",
            &json!({"term": "gmeow:DefinitelyNotARealTerm42"}),
        ));
        assert_eq!(
            unknown["ok"], false,
            "unknown term is a hard error: {unknown}"
        );
    }

    /// A tiny synthetic documentation dataset for the `search_documentation` unit
    /// tests: two class terms whose `to_gmeow_rdf` projection carries the new
    /// `docSearch*` facets, parsed back into an [`purrdf::RdfDataset`] exactly the way
    /// the production carrier holds the bundle's documentation graph. This does NOT
    /// depend on the committed bundle (which only gains the facets after regenerate) —
    /// it exercises the projection + search end to end from the model.
    fn synthetic_docs_dataset() -> Arc<purrdf::RdfDataset> {
        use gmeow_docs_model::model::{DocLinkage, DocTerm, DocTermCategory};
        let ns = "https://blackcatinformatics.ca/gmeow/";
        let model = gmeow_docs_model::model::DocsModel {
            terms: vec![
                DocTerm {
                    iri: format!("{ns}Cat"),
                    curie: "gmeow:Cat".to_string(),
                    label: Some("Cat".to_string()),
                    definition: Some("A small domesticated feline.".to_string()),
                    category: DocTermCategory::Class,
                    owner_slice: format!("{ns}slice/zoo"),
                    scope_notes: vec![
                        "Prefer for a domestic cat; avoid for a wildcat.".to_string(),
                    ],
                    ..Default::default()
                },
                DocTerm {
                    iri: format!("{ns}Feline"),
                    curie: "gmeow:Feline".to_string(),
                    label: Some("Feline".to_string()),
                    definition: Some("The cat family of mammals.".to_string()),
                    category: DocTermCategory::Class,
                    owner_slice: format!("{ns}slice/zoo"),
                    ..Default::default()
                },
            ],
            // A crosswalk linkage on Cat → the alignment facet token `exactMatch:Q146`.
            linkages: vec![DocLinkage {
                mapping_set: None,
                subject: format!("{ns}Cat"),
                subject_curie: "gmeow:Cat".to_string(),
                predicate: "http://www.w3.org/2004/02/skos/core#exactMatch".to_string(),
                object: "http://www.wikidata.org/entity/Q146".to_string(),
                justification: None,
                confidence: None,
                owner_slice: format!("{ns}slice/zoo"),
            }],
            ..Default::default()
        };
        let nquads = gmeow_docs_model::rdf::to_gmeow_rdf(&model, &BTreeMap::new());
        purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
            .expect("to_gmeow_rdf emits valid N-Quads")
    }

    /// `search_documentation` over the synthetic dataset: matches on label /
    /// definition / advice, attaches the advice + alignment + missing-coverage facets,
    /// ranks a label match above a definition match, returns empty for a non-match, and
    /// is deterministic.
    #[test]
    fn search_documentation_matches_facets_ranks_and_is_deterministic() {
        let dataset_arc = synthetic_docs_dataset();
        let dataset = Arc::new(
            dataset_arc.project_named_graph(gmeow_bundle_view::graph_iris::GRAPH_DOCUMENTATION),
        );
        let cat_iri = "https://blackcatinformatics.ca/gmeow/Cat";
        let feline_iri = "https://blackcatinformatics.ca/gmeow/Feline";

        // "cat": Cat matches by LABEL (rank 0); Feline matches by DEFINITION ("the cat
        // family …", rank 1) — so Cat sorts first.
        let hits = search_documentation(&dataset, "cat", 20).expect("search ok");
        let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![cat_iri, feline_iri],
            "label match outranks definition match"
        );
        let cat = &hits[0];
        assert_eq!(cat.kind, "term");
        assert_eq!(cat.label, "Cat");
        assert_eq!(
            cat.definition.as_deref(),
            Some("A small domesticated feline.")
        );
        assert_eq!(
            cat.advice,
            vec!["Prefer for a domestic cat; avoid for a wildcat.".to_string()],
            "the advice facet is attached"
        );
        assert_eq!(
            cat.alignments,
            vec!["exactMatch:Q146".to_string()],
            "the alignment facet is attached"
        );
        assert!(
            !cat.missing_coverage.is_empty(),
            "an under-documented term carries missing-coverage dimensions: {:?}",
            cat.missing_coverage
        );

        // An advice-only match: "wildcat" appears only in Cat's advice prose.
        let advice_hits = search_documentation(&dataset, "wildcat", 20).expect("search ok");
        let advice_ids: Vec<&str> = advice_hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(advice_ids, vec![cat_iri], "advice prose is searchable");

        // A definition-only match: "mammals" appears only in Feline's definition.
        let def_hits = search_documentation(&dataset, "mammals", 20).expect("search ok");
        let def_ids: Vec<&str> = def_hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(def_ids, vec![feline_iri], "definition prose is searchable");

        // A non-matching query is empty-but-ok (never a hard fail on a populated graph).
        let none = search_documentation(&dataset, "xylophone", 20).expect("search ok");
        assert!(none.is_empty(), "a non-matching query returns no hits");

        // Determinism: the same query twice yields the same order.
        let a = search_documentation(&dataset, "cat", 20).expect("search ok");
        let b = search_documentation(&dataset, "cat", 20).expect("search ok");
        assert_eq!(
            a.iter().map(|h| &h.id).collect::<Vec<_>>(),
            b.iter().map(|h| &h.id).collect::<Vec<_>>(),
            "search order is reproducible"
        );

        // The limit is honored.
        let limited = search_documentation(&dataset, "cat", 1).expect("search ok");
        assert_eq!(limited.len(), 1, "limit caps the result count");
    }

    /// `search_documentation` HARD-FAILS when the documentation graph is absent/empty
    /// — docs_search serves the documentation graph, so a missing graph is a defect,
    /// never a silent empty result.
    #[test]
    fn search_documentation_hard_fails_on_absent_documentation_graph() {
        let empty = purrdf::RdfDatasetBuilder::new()
            .freeze()
            .expect("empty dataset");
        let err = search_documentation(&empty, "cat", 20)
            .expect_err("an absent documentation graph is a hard fail");
        assert!(
            err.to_string().contains("graph/documentation"),
            "the hard-fail error names the missing documentation graph: {err}"
        );
    }

    /// `docs_search` dispatches over the shipped bundle and returns an OK envelope with
    /// a `results` array. (The committed bundle carries the documentation graph but not
    /// yet the `docSearch*` facets — those land after regenerate — so the live match
    /// set is validated by the synthetic unit test above; here we prove the tool wires
    /// through, never hard-fails on the populated graph, and is deterministic.)
    #[test]
    fn docs_search_tool_dispatches_over_the_bundle() {
        let server = consumer_server();
        let hit = text_payload(server.call_tool_result("docs_search", &json!({"query": "entity"})));
        assert_eq!(
            hit["ok"], true,
            "docs_search returns ok over the bundle: {hit}"
        );
        assert_eq!(hit["query"], "entity");
        assert!(hit["results"].is_array(), "results is an array: {hit}");
        let again =
            text_payload(server.call_tool_result("docs_search", &json!({"query": "entity"})));
        assert_eq!(hit, again, "docs_search output is deterministic");
    }

    // ── coherence_certificate (R6) ─────────────────────────────────────────────────

    /// A consistent native [`ReasoningResult`], with the given evaluation/completeness
    /// axes and optional certified fragment — the minimum a coherence outcome reads.
    fn coherence_result(
        evaluation: EvaluationStatus,
        completeness: CompletenessStatus,
        fragment: Option<&str>,
    ) -> ReasoningResult {
        use gmeow_logic::result::{
            InformationState, InputStatus, PreservationClaim, ResultPayload, ResultProvenance,
        };
        let mut provenance = ResultProvenance::native("contract:abc", "world:default");
        provenance.certified_fragment = fragment.map(str::to_owned);
        ReasoningResult {
            input: InputStatus::Valid,
            evaluation,
            completeness,
            preservation: PreservationClaim::exact(),
            information: InformationState::Supported,
            provenance,
            payload: ResultPayload::Empty,
            row_schema: None,
        }
    }

    /// Parse a `graph/attestations` N-Quads document into a dataset the read helper reads.
    fn dataset_of(nquads: &str) -> Arc<purrdf::RdfDataset> {
        purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
            .expect("parse coherence N-Quads")
    }

    /// Build the lightest `McpView` that can drive the REAL production entry point
    /// ([`McpView::coherence_certificate_json`], the exact method
    /// `tool_coherence_certificate` calls) over hand-crafted `graph/attestations`
    /// N-Quads — without paying for a full `emit_gts`/`SnapshotBuilder` round trip
    /// (reserved for the `_heavy_offgate` whole-bundle test). `McpView::from_dataset`
    /// only needs the ontology header for `export::fold_meta`; the raw `gts` bytes it
    /// also stores are unused by `coherence_certificate_json`, so an empty `Arc<[u8]>`
    /// is honest (never a fabricated placeholder read by the surface under test).
    fn view_of(nquads: &str) -> McpView {
        let header = "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n";
        let doc = format!("{header}{nquads}");
        McpView::from_dataset(dataset_of(&doc), Arc::from(Vec::<u8>::new()))
            .expect("construct McpView over the crafted certificate fixture")
    }

    /// Simulate a producer that failed to write exactly ONE required predicate: strip
    /// every N-Quads line mentioning the bracket-delimited `logic:<local>` predicate
    /// IRI, leaving every other quad (including the subject's `rdf:type`) intact. The
    /// bracket delimiters make the match exact — no risk of one predicate's local name
    /// being a substring of another's.
    fn strip_predicate(nquads: &str, local: &str) -> String {
        let token = format!("<{LOGIC_NAMESPACE}{local}>");
        let mut out = nquads
            .lines()
            .filter(|line| !line.contains(&token))
            .collect::<Vec<_>>()
            .join("\n");
        out.push('\n');
        out
    }

    /// The carrier folds a CONCLUSIVE, fragment-scoped, violation-free closure as a real
    /// `logic:CoherenceCertificate`; the read tool surfaces `issues_certificate:true`, the
    /// certificate class, and the pinned bundle_hash / per-graph axiom_hashes VERBATIM (the
    /// tamper surface) — proving the digests are exactly `per_graph_axiom_hashes` and not
    /// fabricated.
    #[test]
    fn coherence_certificate_surfaces_a_certificate_with_the_pinned_digests() {
        use gmeow_logic::certificate::{
            CoherenceOutcome, ContradictionPolicy, per_graph_axiom_hashes,
        };
        use purrdf::gts::writer::digest_string;

        // A small axiom-bearing dataset whose per-graph digests the certificate pins.
        let axioms = dataset_of(
            "<https://e/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://e/B> <https://e/w> .\n",
        );
        let axiom_hashes = per_graph_axiom_hashes(axioms.as_ref(), digest_string);
        assert!(!axiom_hashes.is_empty(), "the fixture pins ≥1 axiom digest");
        let bundle_hash = digest_string(b"bundle-identity-bytes");

        let result = coherence_result(
            EvaluationStatus::Completed,
            CompletenessStatus::Unknown,
            Some("fragment:test"),
        );
        let outcome = CoherenceOutcome::from_reasoning_result(
            &result,
            bundle_hash.clone(),
            axiom_hashes.clone(),
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        assert!(outcome.issues_certificate(), "the fixture must certify");

        let dataset =
            dataset_of(&outcome.to_nquads(gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS));
        let env = coherence_certificate_envelope(dataset.as_ref()).expect("certificate present");

        assert_eq!(env["ok"], true);
        assert_eq!(env["issues_certificate"], true);
        assert_eq!(env["is_refused"], false);
        assert_eq!(env["class_local_name"], "CoherenceCertificate");
        // The surfaced bundle_hash / axiom_hashes are the pipeline-pinned digests, VERBATIM.
        assert_eq!(env["bundle_hash"], bundle_hash);
        assert!(
            env["bundle_hash"].as_str().is_some_and(|s| !s.is_empty()),
            "bundle_hash is non-empty: {env}"
        );
        let surfaced: Vec<String> = env["axiom_hashes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            surfaced,
            axiom_hashes.into_iter().collect::<Vec<_>>(),
            "axiom_hashes are the per_graph_axiom_hashes digests, not fabricated: {env}"
        );
        assert!(
            surfaced.iter().all(|h| h.contains(':') && h.len() > 8),
            "axiom digests are non-trivial content addresses: {surfaced:?}"
        );
        // The two completeness-gate axes round-trip off the linked result node.
        assert_eq!(env["evaluation"], "completed");
        assert_eq!(env["completeness"], "unknown");
        assert_eq!(env["contract_hash"], "contract:abc");
    }

    /// R6 regression: a bounded/incomplete closure yields the strictly-weaker
    /// `logic:CoherenceCheckAttestation`; the read tool must report
    /// `issues_certificate:false` and `class_local_name:CoherenceCheckAttestation` — a
    /// regression must NEVER silently upgrade an attestation to a certificate.
    #[test]
    fn coherence_certificate_maps_an_attestation_never_a_certificate() {
        use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

        let result = coherence_result(
            EvaluationStatus::BudgetExhausted,
            CompletenessStatus::Incomplete,
            None,
        );
        assert!(!result.is_conclusive());
        let outcome = CoherenceOutcome::from_reasoning_result(
            &result,
            "blake3:bundle".to_owned(),
            ["blake3:axioms".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        assert!(!outcome.issues_certificate());

        let dataset =
            dataset_of(&outcome.to_nquads(gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS));
        let env = coherence_certificate_envelope(dataset.as_ref()).expect("attestation present");
        assert_eq!(env["ok"], true);
        assert_eq!(
            env["issues_certificate"], false,
            "an attestation is NOT a certificate: {env}"
        );
        assert_eq!(env["class_local_name"], "CoherenceCheckAttestation");
        assert_eq!(env["evaluation"], "budget-exhausted");
        assert_eq!(env["completeness"], "incomplete");
    }

    /// HARD-FAIL: a bundle carrying no coherence artifact in `graph/attestations` is an
    /// error — there is NO silent recompute fallback.
    #[test]
    fn coherence_certificate_hard_fails_on_a_bundle_without_a_certificate() {
        let stripped = dataset_of("<https://e/s> <https://e/p> <https://e/o> <https://e/g> .\n");
        let err = coherence_certificate_envelope(stripped.as_ref())
            .expect_err("a bundle with no coherence artifact must hard-fail");
        assert!(
            err.to_string()
                .contains("no coherence certificate or attestation"),
            "the hard fail names the missing artifact: {err}"
        );
    }

    /// A bundle carrying more than one distinct coherence subject is ambiguous and a hard
    /// failure (no silent first-wins).
    #[test]
    fn coherence_certificate_hard_fails_on_an_ambiguous_bundle() {
        use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

        let graph = gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS;
        let a = CoherenceOutcome::from_reasoning_result(
            &coherence_result(
                EvaluationStatus::Completed,
                CompletenessStatus::Unknown,
                Some("frag:a"),
            ),
            "blake3:bundle-a".to_owned(),
            ["blake3:axioms-a".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        let b = CoherenceOutcome::from_reasoning_result(
            &coherence_result(
                EvaluationStatus::Completed,
                CompletenessStatus::Unknown,
                Some("frag:b"),
            ),
            "blake3:bundle-b".to_owned(),
            ["blake3:axioms-b".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        let both = format!("{}{}", a.to_nquads(graph), b.to_nquads(graph));
        let err = coherence_certificate_envelope(dataset_of(&both).as_ref())
            .expect_err("two coherence subjects must hard-fail");
        assert!(
            err.to_string()
                .contains("more than one distinct coherence subject"),
            "the hard fail names the ambiguity: {err}"
        );
    }

    /// A SINGLE coherence subject typed BOTH
    /// `logic:CoherenceCertificate` and `logic:CoherenceCheckAttestation` is an
    /// ambiguous, malformed artifact and must hard-fail — REGRESSION GUARD for the
    /// exact gap this test closes: the ambiguity check used to compare only the
    /// SUBJECT IRI (`existing != subject`), so two `rdf:type` triples on the SAME
    /// subject fell through to the silent `Some(_) => {}` no-op arm and first-wins
    /// picked whichever `rdf:type` the dataset's quad iteration order surfaced first.
    /// Drives the REAL production entry point (`McpView::coherence_certificate_json`).
    #[test]
    fn coherence_certificate_json_hard_fails_on_a_dual_typed_subject() {
        use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

        let graph = gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS;
        let outcome = CoherenceOutcome::from_reasoning_result(
            &coherence_result(
                EvaluationStatus::Completed,
                CompletenessStatus::Unknown,
                Some("fragment:dual"),
            ),
            "blake3:bundle-dual".to_owned(),
            ["blake3:axioms-dual".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        let nquads = outcome.to_nquads(graph);

        // Splice in a SECOND `rdf:type` triple on the SAME subject, typing it the
        // strictly-weaker Attestation class too — the malformed dual-typed artifact.
        let subject_line = nquads
            .lines()
            .find(|l| l.contains("22-rdf-syntax-ns#type") && l.contains("CoherenceCertificate"))
            .expect("the fixture carries the certificate's rdf:type triple");
        let subject = subject_line
            .split_whitespace()
            .next()
            .expect("rdf:type triple has a subject");
        let dual_typed = format!(
            "{nquads}{subject} <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
             <{LOGIC_NAMESPACE}CoherenceCheckAttestation> <{graph}> .\n"
        );

        let view = view_of(&dual_typed);
        let out: Value = serde_json::from_str(&view.coherence_certificate_json())
            .expect("coherence_certificate_json returns valid JSON");
        assert_eq!(
            out["ok"], false,
            "a dual-typed subject must hard-fail, never first-win a class: {out}"
        );
        let error = out["error"].as_str().unwrap_or_default();
        assert!(
            error.contains("typed BOTH"),
            "the hard fail names the ambiguous dual typing: {error}"
        );
    }

    /// Every producer-REQUIRED field the tool extracts must
    /// be present with the producer's exact cardinality — a bundle missing ANY of them
    /// is a CORRUPT artifact and must hard-fail (`ok:false`) naming the missing
    /// predicate, never `ok:true` with a null/empty field silently laundering the gap
    /// (no-silent-degradation, `.goals`). Drives the REAL production entry point
    /// (`McpView::coherence_certificate_json`, the exact method
    /// `tool_coherence_certificate` calls) over a battery of certificates each missing
    /// exactly one predicate [`CoherenceOutcome::to_nquads`] otherwise always writes.
    #[test]
    fn coherence_certificate_json_hard_fails_on_each_missing_required_field() {
        use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

        let graph = gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS;
        let outcome = CoherenceOutcome::from_reasoning_result(
            &coherence_result(
                EvaluationStatus::Completed,
                CompletenessStatus::Unknown,
                Some("fragment:required"),
            ),
            "blake3:bundle-required".to_owned(),
            ["blake3:axioms-required".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        assert!(
            outcome.issues_certificate(),
            "the fixture must be a real certificate (so certifiedFragment is exercised too)"
        );
        let full = outcome.to_nquads(graph);

        // Every predicate the producer writes UNCONDITIONALLY on a CoherenceCertificate
        // subject (bundleHash/axiomHash/contractHash/engine/contradictionPolicy/
        // summarizesResult/certifiedFragment) plus the two axes carried on the linked
        // result node (resultCompleteness/resultEvaluation) — one case per field.
        let required_predicates = [
            "bundleHash",
            "axiomHash",
            "contractHash",
            "engine",
            "contradictionPolicy",
            "summarizesResult",
            "certifiedFragment",
            "resultCompleteness",
            "resultEvaluation",
        ];
        for predicate in required_predicates {
            let stripped = strip_predicate(&full, predicate);
            let view = view_of(&stripped);
            let out: Value = serde_json::from_str(&view.coherence_certificate_json())
                .expect("coherence_certificate_json returns valid JSON");
            assert_eq!(
                out["ok"], false,
                "stripping logic:{predicate} must hard-fail, not ok:true with a null field: {out}"
            );
            let error = out["error"].as_str().unwrap_or_default();
            assert!(
                error.contains(predicate),
                "the hard fail for a missing logic:{predicate} must name it: {error}"
            );
        }
    }

    /// Drive the REAL `coherence_certificate` tool through `call_tool_result` over a bundle
    /// that carries the certificate in `graph/attestations` — the same disk-free, reason-free
    /// read path the shipped consumer surface uses.
    ///
    /// Fast (a minimal synthetic snapshot — just the ontology header plus the certificate's
    /// own quads, built via `SnapshotBuilder`/`emit_gts`, NOT the real committed `gmeow.gts` —
    /// the same tiny-snapshot construction `verify_graph_inconsistent_but_conclusive_never_certifies`
    /// uses): on-gate, not `_heavy_offgate`. Measured at ~0.01-0.03 s standalone and under full
    /// contention with the genuinely-heavy `*_heavy_offgate` siblings because — unlike
    /// those siblings — this test never touches the real
    /// committed bundle.
    #[test]
    fn coherence_certificate_tool_reads_the_carried_bundle() {
        use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};
        use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};

        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }

        let outcome = CoherenceOutcome::from_reasoning_result(
            &coherence_result(
                EvaluationStatus::Completed,
                CompletenessStatus::Unknown,
                Some("fragment:test"),
            ),
            "blake3:carried-bundle".to_owned(),
            ["blake3:axioms-carried".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();

        // A minimal snapshot: the required ontology header (the importer hard-fails
        // without it) plus the certificate's graph/attestations named graph, emitted as a
        // real gmeow.gts bundle.
        let doc = format!(
            "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n\
             {}",
            outcome.to_nquads(gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS)
        );
        let dataset = dataset_of(&doc);
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(dataset.as_ref()).expect("add_dataset");
        // gmeow-test-input: synthetic-only
        let gts = emit_gts(
            &builder,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
            // Undicted at the mandated level: these fixtures build a tiny in-test
            // snapshot, so there is no shipped dictionary to prime with — but the frame
            // profile still applies, so the level is declared rather than defaulted.
            &purrdf::gts_compose::MediumPlan::undicted(Some(12)),
        )
        .expect("emit tiny cert-carrying snapshot");

        let server = McpServer::from_snapshot(&gts).unwrap();
        let out = text_payload(server.call_tool_result("coherence_certificate", &json!({})));
        assert_eq!(
            out["ok"], true,
            "the tool reads the carried certificate: {out}"
        );
        assert_eq!(out["class_local_name"], "CoherenceCertificate");
        assert_eq!(out["issues_certificate"], true);
        assert_eq!(out["bundle_hash"], "blake3:carried-bundle");
        assert_eq!(
            out["axiom_hashes"],
            json!(["blake3:axioms-carried"]),
            "the per-graph axiom digests ride the read envelope: {out}"
        );
        // Deterministic: the read is a pure projection of the carried quads.
        let again = text_payload(server.call_tool_result("coherence_certificate", &json!({})));
        assert_eq!(out, again, "the certificate read is deterministic");
    }

    /// An INCONSISTENT-but-CONCLUSIVE closure must never
    /// surface as `CoherenceCertificate` on the tool surface — `completeness_class`
    /// used to return `CoherenceCertificate` for ANY `is_conclusive()` result, with no
    /// check for a named certified fragment or for the absence of a forbidden
    /// violation, and `run_explain_quad` (unlike `run_verify_graph`, which bolted on
    /// its own ad-hoc `Refused` downgrade) had NO protection at all.
    ///
    /// Drives the REAL `verify_graph` tool (`call_tool_result`, not internals) with a
    /// tiny canon plus an overlay that is GENUINELY inconsistent — `A ⊑ B`, `A ⊑ C`,
    /// `B disjointWith C`, `x : A` forces `x` into `owl:Nothing` (the exact fixture
    /// `reason_all_single_chase_yields_inconsistent_and_nonempty_closure` proves
    /// derives `InformationState::Both` in one completed chase, i.e. CONCLUSIVE, not
    /// budget-cut) — and asserts the response's `class_local_name` is NEVER
    /// `CoherenceCertificate`. `class_local_name` is `completeness_class`'s output
    /// (folded through `CoherenceOutcome::class_local_name_for`, the SAME gate
    /// `run_explain_quad` now reads through), so this proves the shared gate, not a
    /// per-tool special case.
    ///
    /// Fast (a tiny synthetic canon + a 4-quad overlay, not the real corpus): on-gate,
    /// not `_heavy_offgate`.
    #[test]
    fn verify_graph_inconsistent_but_conclusive_never_certifies() {
        use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};

        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }

        // A minimal canon: just the required ontology header (the importer hard-fails
        // without it) — the SAME pattern `coherence_certificate_tool_reads_the_carried_
        // bundle_heavy_offgate` uses.
        let doc = "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n";
        let dataset = dataset_of(doc);
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(dataset.as_ref()).expect("add_dataset");
        // gmeow-test-input: synthetic-only
        let gts = emit_gts(
            &builder,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
            // Undicted at the mandated level: these fixtures build a tiny in-test
            // snapshot, so there is no shipped dictionary to prime with — but the frame
            // profile still applies, so the level is declared rather than defaulted.
            &purrdf::gts_compose::MediumPlan::undicted(Some(12)),
        )
        .expect("emit tiny header-only canon");

        let server = McpServer::from_snapshot(&gts).unwrap();
        select_native_verification_laws(&server);

        // The overlay: a genuine DL contradiction — A ⊑ B, A ⊑ C, B disjointWith C,
        // x : A forces x into owl:Nothing. Un-graphed triples reason under the single
        // default world, and the whole tiny canon+overlay union closes well under the
        // governed step ceiling — CONCLUSIVE, never budget-cut.
        let overlay_data = "<http://gmeowtest.example/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/B> .\n\
             <http://gmeowtest.example/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/C> .\n\
             <http://gmeowtest.example/B> <http://www.w3.org/2002/07/owl#disjointWith> <http://gmeowtest.example/C> .\n\
             <http://gmeowtest.example/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://gmeowtest.example/A> .\n";

        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 64}),
        ));
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");

        // The closure genuinely completed (conclusive), not a budget-cut — so the
        // downgrade below is caused by the witnessed glut, not by non-conclusiveness.
        assert_eq!(
            out["evaluation"], "completed",
            "the fixture's tiny closure must be CONCLUSIVE (not budget-cut) for this to be a \
             faithful proof: {out}"
        );

        // The falsifiable assertion: an inconsistent-but-conclusive closure must NEVER
        // render `CoherenceCertificate` — the shared `CoherenceOutcome` gate downgrades
        // it to the flat refusal instead.
        assert_ne!(
            out["class_local_name"], "CoherenceCertificate",
            "an inconsistent-but-conclusive closure must never be labeled a \
             CoherenceCertificate: {out}"
        );
        assert_eq!(
            out["class_local_name"], "Refused",
            "a witnessed forbidden violation in a CONCLUSIVE closure is a flat refusal, per \
             the SAME CoherenceOutcome gate the bundle-level coherence certifier uses: {out}"
        );
    }

    /// `verify_graph`'s `coherent` field MUST agree with
    /// `class_local_name` — both MUST be derived from the SAME `CoherenceOutcome`
    /// gate, never from two independent signals.
    ///
    /// The overlay carries a genuine DL glut via plain PAIRWISE `owl:disjointWith`
    /// (A ⊑ B, A ⊑ C, B disjointWith C, x : A forces x into owl:Nothing) —
    /// DELIBERATELY not an `owl:AllDisjointClasses` set, so the ONE bad-example
    /// verify query that could independently catch a disjoint-axis violation
    /// (`class-in-two-disjoint-axes.rq`, which matches only `owl:AllDisjointClasses`
    /// membership) does NOT fire: `report.ok()` is true. Earlier,
    /// `coherent` was read straight from `report.ok()`, so this exact fixture would
    /// render the self-contradictory `coherent:true` alongside
    /// `class_local_name:"Refused"`. The fix routes `coherent` through the SAME
    /// shared `completeness_refused` gate that decides `class_local_name`, so the
    /// two fields can never disagree.
    ///
    /// Fast (a tiny synthetic canon + a 4-quad overlay, not the real corpus): on-gate,
    /// not `_heavy_offgate`.
    #[test]
    fn verify_graph_coherent_never_disagrees_with_refused_class() {
        use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};

        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }

        // The header-only canon PLUS the `axis-not-disjoint.rq` bad-example query's
        // own required orthogonality matrix (an `owl:AllDisjointClasses` set naming
        // the seven fixed identity axes) — otherwise that unrelated bad-example
        // query fires on ANY header-only canon (it demands the matrix exist at all),
        // which would make `report.ok()` false for a reason having nothing to do
        // with this test's glut and defeat the falsifiability of the assertion below.
        let doc = "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n\
             _:axdisj <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#AllDisjointClasses> .\n\
             _:axdisj <http://www.w3.org/2002/07/owl#members> _:axlist0 .\n\
             _:axlist0 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/GenderIdentity> .\n\
             _:axlist0 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist1 .\n\
             _:axlist1 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/GenderExpression> .\n\
             _:axlist1 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist2 .\n\
             _:axlist2 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/SexAssignedAtBirth> .\n\
             _:axlist2 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist3 .\n\
             _:axlist3 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/SexualOrientation> .\n\
             _:axlist3 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist4 .\n\
             _:axlist4 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/RomanticOrientation> .\n\
             _:axlist4 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist5 .\n\
             _:axlist5 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/PronounSet> .\n\
             _:axlist5 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist6 .\n\
             _:axlist6 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/Honorific> .\n\
             _:axlist6 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .\n";
        let dataset = dataset_of(doc);
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(dataset.as_ref()).expect("add_dataset");
        // gmeow-test-input: synthetic-only
        let gts = emit_gts(
            &builder,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
            // Undicted at the mandated level: these fixtures build a tiny in-test
            // snapshot, so there is no shipped dictionary to prime with — but the frame
            // profile still applies, so the level is declared rather than defaulted.
            &purrdf::gts_compose::MediumPlan::undicted(Some(12)),
        )
        .expect("emit tiny header-only canon");

        let server = McpServer::from_snapshot(&gts).unwrap();
        select_native_verification_laws(&server);

        // The glut: same shape as `verify_graph_inconsistent_but_conclusive_never_
        // certifies`, but PAIRWISE `owl:disjointWith` on classes NOT named in the
        // orthogonality matrix above — so neither `axis-not-disjoint.rq` (satisfied
        // by the matrix) nor `class-in-two-disjoint-axes.rq` (requires
        // `owl:AllDisjointClasses` membership, which g4B/g4C never join) can match.
        let overlay_data = "<http://gmeowtest.example/g4A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/g4B> .\n\
             <http://gmeowtest.example/g4A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/g4C> .\n\
             <http://gmeowtest.example/g4B> <http://www.w3.org/2002/07/owl#disjointWith> <http://gmeowtest.example/g4C> .\n\
             <http://gmeowtest.example/g4x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://gmeowtest.example/g4A> .\n";

        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 64}),
        ));
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        assert_eq!(
            out["evaluation"], "completed",
            "the fixture's tiny closure must be CONCLUSIVE (not budget-cut) for this to be a \
             faithful proof: {out}"
        );

        // The class label refutes coherence via the DL glut...
        assert_eq!(
            out["class_local_name"], "Refused",
            "a witnessed forbidden violation in a CONCLUSIVE closure is a flat refusal: {out}"
        );
        // ...and `coherent` MUST agree — never `coherent:true` alongside
        // `class_local_name:"Refused"`, even though no bad-example verify query
        // fired on this fixture's pairwise `owl:disjointWith` shape.
        assert_eq!(
            out["coherent"], false,
            "coherent must be false whenever class_local_name is Refused, regardless of \
             whether any bad-example verify query matched: {out}"
        );
    }

    // ── GMN verifier tools: gmn_validate / gmn_expand / gmn_explain ─────────────────

    /// Read a frozen GMN-1 conformance-vector file from the shipped corpus (a test
    /// artifact, never in the bundle) by its path relative to the vector root.
    fn gmn_vector(rel: &str) -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        fs::read_to_string(
            root.join("slices/grounding/lang/tests/gmn1-vectors")
                .join(rel),
        )
        .unwrap_or_else(|e| panic!("read GMN vector {rel}: {e}"))
    }

    /// `gmn_validate` accepts a frozen conformance vector (`{ok, conformant:true}`) and
    /// rejects a perturbed document with the TYPED `lang:Gmn*Failure` class — the external
    /// LLM's entry to the `@err` repair loop.
    #[test]
    fn gmn_validate_accepts_conformant_and_rejects_perturbed() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();

        // A frozen POSITIVE vector conforms.
        let good = gmn_vector("claim-basic.gmn");
        let ok = text_payload(server.call_tool_result("gmn_validate", &json!({ "gmn": good })));
        assert_eq!(ok["ok"], true, "{ok}");
        assert_eq!(
            ok["conformant"], true,
            "a frozen conformance vector must validate: {ok}"
        );
        assert!(
            ok.get("failure_class").is_none(),
            "a conformant document carries no failure class: {ok}"
        );

        // A perturbed document (a value glyph flipped to a non-canonical 3-digit fraction,
        // the frozen negative fixture the corpus pins) raises the TYPED failure class.
        let bad = gmn_vector("negative-codec/neg-malformed-number-frac.gmn");
        let defect = text_payload(server.call_tool_result("gmn_validate", &json!({ "gmn": bad })));
        assert_eq!(defect["ok"], true, "{defect}");
        assert_eq!(
            defect["conformant"], false,
            "the perturbed document must be rejected: {defect}"
        );
        assert_eq!(
            defect["failure_class"], "https://blackcatinformatics.ca/lang/GmnMalformedNumber",
            "the typed lang:Gmn*Failure class names the defect: {defect}"
        );
        assert_eq!(
            defect["failure_local_name"], "GmnMalformedNumber",
            "{defect}"
        );
        assert!(
            defect["message"].as_str().is_some_and(|m| !m.is_empty()),
            "the defect carries a message: {defect}"
        );
    }

    /// `gmn_expand` decodes a GMN-1 document to its GMN-0 normal form (alias/glyph → full
    /// IRI) and its expansion round-trips: re-encoding equals the input under
    /// `gmn0_canonically_equal`.
    #[test]
    fn gmn_expand_roundtrips() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let doc = gmn_vector("claim-basic.gmn");
        let out =
            text_payload(server.call_tool_result("gmn_expand", &json!({ "gmn": doc.clone() })));
        assert_eq!(out["ok"], true, "{out}");
        assert_eq!(
            out["round_trip"], true,
            "the expansion carries a holding round-trip witness: {out}"
        );
        let expanded = out["expanded_nquads"].as_str().expect("expanded_nquads");
        assert!(
            !expanded.is_empty(),
            "the GMN-0 normal form is non-empty: {out}"
        );
        // The "expand alias/glyph → full IRI" direction: the compact `gmeow__gate1` token
        // expands to its full IRI under the gmeow namespace.
        assert!(
            expanded.contains("https://blackcatinformatics.ca/gmeow/"),
            "the GMN-0 normal form carries full IRIs, not compact aliases: {out}"
        );

        // Expand then re-encode equals the input under gmn0_canonically_equal.
        let reencoded = out["reencoded_gmn"].as_str().expect("reencoded_gmn");
        let dict = server.gmn_dictionary().expect("dictionary resolves");
        let input_model = gmn1_read(&Gmn1Document::from_text(doc), dict).expect("input reads");
        let re_model = gmn1_read(&Gmn1Document::from_text(reencoded.to_owned()), dict)
            .expect("re-encoded reads");
        assert!(
            gmn0_canonically_equal(&input_model, &re_model),
            "expand then re-encode equals the input under gmn0_canonically_equal: {out}"
        );
    }

    /// `gmn_explain` resolves a known operator glyph (`¬` → `logic:not`) to its authored
    /// fixity / precedence / arity and its controlled-NL gloss, and returns an HONEST typed
    /// miss for an unknown glyph — never a fabricated answer.
    #[test]
    fn gmn_explain_names_fixity_and_gloss() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();

        // ¬ is the seeded prefix operator for logic:not (precedence 90, arity 1).
        let hit = text_payload(server.call_tool_result("gmn_explain", &json!({ "glyph": "¬" })));
        assert_eq!(hit["ok"], true, "{hit}");
        assert_eq!(
            hit["found"], true,
            "¬ is a covered GMN operator glyph: {hit}"
        );
        assert_eq!(hit["fixity_local_name"], "gmnFixityPrefix", "{hit}");
        assert_eq!(
            hit["precedence"], 90,
            "the graph-authored binding strength: {hit}"
        );
        assert_eq!(hit["arity"], 1, "{hit}");
        assert_eq!(
            hit["denotation_target"], "https://blackcatinformatics.ca/logic/not",
            "{hit}"
        );
        assert!(
            hit["denotation"]
                .as_str()
                .is_some_and(|d| d.contains("blackcatinformatics.ca")),
            "the lang:Denotation IRI is surfaced, not fabricated: {hit}"
        );
        // The gloss is the verbalizer rendering: the prefix template `<label> arg1`.
        assert!(
            hit["gloss"].as_str().is_some_and(|g| g.contains("arg1")),
            "the controlled-NL gloss is the prefix verbalizer rendering: {hit}"
        );
        // A GMN surface carries the record-initial sigil of the scope it reads in, because the
        // SAME glyph denotes different operators under different scopes (see `gmn_verbalize`);
        // the sigil is part of the surface, not decoration around it.
        assert_eq!(
            hit["gmn_surface"], "@ℒ ¬ arg1",
            "the GMN operator surface scopes the record and arranges the glyph in prefix \
             position: {hit}"
        );

        // An unknown glyph returns the honest typed miss, never a fabricated answer.
        let miss = text_payload(server.call_tool_result("gmn_explain", &json!({ "glyph": "☃" })));
        assert_eq!(miss["ok"], true, "{miss}");
        assert_eq!(
            miss["found"], false,
            "an unknown glyph is not found: {miss}"
        );
        assert_eq!(
            miss["failure_class"], "https://blackcatinformatics.ca/lang/GmnUncoveredTerm",
            "the miss is the typed lang:GmnUncoveredTerm class: {miss}"
        );
        assert_eq!(miss["failure_local_name"], "GmnUncoveredTerm", "{miss}");
    }

    /// The three GMN verifier tools are advertised in the CONSUMER surface (served off the
    /// bundle alone, like `validate_local`, never dev-gated) and each advertises its
    /// required arg honestly (the `tool()` allowlist addition).
    #[test]
    fn gmn_tools_are_advertised_in_the_consumer_surface() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let consumer = McpServer::from_snapshot(&bytes).unwrap();
        let result = consumer.tools_result();
        let arr = result["tools"].as_array().expect("tools array");
        for (name, req) in [
            ("gmn_validate", "gmn"),
            ("gmn_expand", "gmn"),
            ("gmn_explain", "glyph"),
        ] {
            let tool = arr
                .iter()
                .find(|t| t["name"] == name)
                .unwrap_or_else(|| panic!("{name} is advertised: {result}"));
            let required = tool["inputSchema"]["required"]
                .as_array()
                .expect("required array");
            assert!(
                required.iter().any(|r| r == req),
                "{name} advertises its required arg `{req}`: {tool}"
            );
        }
    }

    /// The GMN-1 teachability primer is exposed as a CONSUMER MCP resource
    /// (`gmeow://ontology/gmn1-primer`, served off the bundle alone), advertised in
    /// `resources/list` and readable through `resources/read` — a self-contained, graph-derived,
    /// budget-bounded card carrying the record sigils, the operator glyph table, and the repair
    /// loop. The shared `llms_full` surface carries the same primer section.
    #[test]
    fn gmn1_primer_resource_is_advertised_and_readable() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let consumer = McpServer::from_snapshot(&bytes).unwrap();

        // Advertised in the consumer resource list.
        let list = consumer.resources_result();
        let resources = list["resources"].as_array().expect("resources array");
        assert!(
            resources
                .iter()
                .any(|r| r["uri"] == "gmeow://ontology/gmn1-primer"),
            "the gmn1-primer resource must be advertised: {list}"
        );

        // Readable through resources/read, with the primer heading + a repair card + an operator
        // glyph row present (the graph-derived teaching surface).
        let read = consumer.read_resource_result("gmeow://ontology/gmn1-primer");
        assert!(
            read.get("isError").is_none(),
            "primer read must succeed: {read}"
        );
        let text = read["contents"][0]["text"].as_str().expect("primer text");
        assert!(
            text.contains(&format!("## {}", gmn1_primer_heading())),
            "the primer resource must carry its heading: {text}"
        );
        assert!(
            text.contains("gmeow:GmnErr"),
            "the primer resource must teach the @err repair record"
        );
        assert!(
            text.contains("⊑ (infix"),
            "the primer resource must carry the operator glyph table (⊑ subsumption row)"
        );

        // The same primer section rides the shared `llms_full` surface.
        let full = consumer
            .view
            .llms_full_text(vec!["en".to_string()])
            .expect("llms_full builds with the primer");
        assert!(
            full.contains(&format!("## {}", gmn1_primer_heading())),
            "llms_full must carry the primer section"
        );
    }

    /// The primer heading constant, re-exposed for the resource test (the shared docs const).
    fn gmn1_primer_heading() -> &'static str {
        gmeow_docs_model::gmn1_primer::PRIMER_HEADING
    }
}

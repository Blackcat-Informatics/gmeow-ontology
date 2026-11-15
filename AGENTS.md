<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Agent Guide (AGENTS.md)

The working contract for every agent and contributor in the GMEOW repository. `CLAUDE.md` imports this
file and adds nothing of its own. Order of authority: [CONSTITUTION.md](./CONSTITUTION.md), then § 1 of
this file, then the rest of it. Cite Constitution principles by number (for example "Principle 4") in
commit messages, pull requests and reviews.

---

## 1. Goals and standing rules

### What the project is

* **Greenfield first.** State of the art, the best of all choices. Learn from the limits of earlier
  work; never accept them. No compromises.
* **No backwards compatibility.** No shims, old interfaces or legacy support unless it directly helps
  us. When an element is replaced, the inferior one is removed. Lossy compatibility lives only in
  generated projections, never in the canonical core.
* **Rust first, Python surface.** Core work is Rust. Adding any Python (code, tests, fixtures,
  orchestration) needs explicit authorization; if you think you are about to write Python, you are
  probably doing the wrong thing.
* **Subsume, extend, enhance.** A substandard dependency is subsumed, extended and improved beyond
  the original.
* **RDF 1.2 only.** RDF 1.2 and RDF\* are first class and the only way we work. Never RDF 1.1, quads
  or another shortcut.

### Direction of data

* **From slices.** Slices are the source and the shippable development surface. Push data back into
  slices wherever possible.
* **To `gmeow.gts` and the `gmeow` CLI.** These are the deliverables: the first tools a developer
  reaches for to give a knowledge graph or an AI system more power, depth and reasoning.

### Three maximums

* **Information flow:** carry information from source to output with full fidelity; trim only at an
  exit gate.
* **Ontological use:** link, project and reason over external ontologies wherever possible. More is
  better.
* **Dogfooding:** ground everything we can in GMEOW itself. Links and isomorphisms to other things
  are welcome; GMEOW is our own source of truth.

### When requests conflict

These decide between competing requests. They do not replace the standing rules of this file.

* **Greenfield:** correct the mistakes of the past. Others' mistakes are lessons, not limits.
* **State of the art** over the bones of the past.
* **Optimal:** the Pareto frontier of quality, speed and efficiency.
* **Maximal:** the broadest functionality, after the other three are decided.

### Baseline

* **Review adversarially.** In a gap analysis or review, assume the work is substandard and does not
  meet its specification until the code shows otherwise.
* **No deferrals.** No follow-ups, nothing left for the future inside the unit being landed. Work is
  now. Oversized work is decomposed (§ 9), never deferred.
* **No process references in the repository.** Issue numbers, pull request numbers and development
  history never appear in the ontology, the code or the documentation. A comment that says a line
  exists per some issue, or will be fixed in another, is banned; remove such text where you see it.
  Process belongs on the forge.
* **A bug you see is yours.** Fix it now. Blame and archaeology are a waste of time.
* **`make check` is very expensive and holds a host-global lock.** Make all your changes first, then
  run it once. Never use it, or `make check-sync`, in a change-and-retry loop. This binds sub-agents
  too: say so in their briefs.

### No-optionality

> No-optionality forbids silent capability degradation; it does not forbid explicit feature
> selection.

For a selected operation and profile, every declared input, capability, invariant and output is
mandatory. Explicit profiles, sinks, output formats and DAG branches are allowed when they are first
class, deterministic, cache-keyed and fully validated; once selected, their stages and outputs are
required. A missing cache causes recomputation. A missing dependency, source or implementation is a
hard failure, never permission to use a weaker parser, omit output, keep stale bytes or otherwise
half-implement the operation. `Option<T>` and conditional DAG edges are not violations; opportunistic
fallback to weaker semantics is.

### The deficiency ledger

`.deficiencies` is the log of last resort. Every entry below its marker is unauthorized, is a bug, and
means the issue or pull request that produced it failed: work misrepresented to pass a gate, work
misrepresented by an agent, or an agent found to be defective. It is never an accepted risk, a
descope or a backlog. The normal file holds only its notice and marker. An entry blocks completion,
pull request creation and merge of the work that produced it. Verify the defect against current code
and give it a durable, visible owner before removing the entry; removal does not fix the bug. Never
add an entry to make incomplete work look complete.

---

## 2. Ontology rules

GMEOW is a reasoning-centric, RDF 1.2-native, logic-canonical super-vocabulary that unifies document
metadata, entity descriptions, legal agreements, contacts and person-centric data. OWL 2 DL, SHACL,
gUFO and other external formalisms are typed target views of the canonical kernel, not its owners.

### Critical Ontological Rules

* **One canonical source (Principle 4).** Never hand-edit a generated file. Change the source:
  mappings in [dsl/mappings/](./dsl/mappings/) or a slice's `mappings/`; statements in
  [dsl/statements/](./dsl/statements/); citations in
  [metadata/references.ttl](./metadata/references.ttl), following [docs/CITATIONS.md](./docs/CITATIONS.md).
* **Grounding ownership (Principles 17 and 19).** All grounding to an external formalism is authored
  in exactly one grounding slice: linguistic and serialization surfaces in `slices/grounding/lang`,
  mathematical surfaces in `slices/grounding/math`, upper ontologies, logics, rule and validation
  dialects in `slices/grounding/logic`. The grounding term is the source and the external vocabulary
  the target. A domain slice uses the grounding term and never authors a competing grounding.
  Correspondences ship in the meta-level `graph/correspondence-laws` graph of `gmeow.gts`; SSSOM,
  EDOAL and FnO are generated views. Read [docs/GROUNDING.md](./docs/GROUNDING.md),
  [docs/foundational-bridging.md](./docs/foundational-bridging.md) and the owning slice's design set
  before editing these seams.
* **Typed views.** OWL and RDFS, Datalog, SHACL, Prolog and gUFO are typed views of the `logic:`
  core. BFO, OBO and RO, SUMO, DUL, YAMATO and OpenCyc are commitment-shifting `BridgeView`s. `math:`
  owns Data Cube, STATO and OBCS; `lang:` owns OntoLex, LexInfo, WordNet, NIF and Web Annotation.
  Every live grounding law carries an explicit preservation judgment.
* **RDF 1.2 first (Principles 2 and 3).** Statement-level metadata (provenance, confidence, temporal
  scope) is native RDF 1.2 in the statement DSL. The canonical logical core is `logic:`; OWL 2 DL is
  a generated decidable projection.
* **Co-equal and non-privileged (Principles 9 and 10).** No `primaryName`, no `preferredGender`, no
  single-winner preference. A contested fact is coexisting standpoint-indexed claims. A superseded
  label or deadname is suppressed with `gmeow:displayable false`, never deleted.
* **Annotation completeness (Principles 1, 6, 7).** Every GMEOW term carries `rdfs:label`,
  `skos:definition` and `rdfs:isDefinedBy`. A missing one is a violation in `make validate` and CI.
* **Slices (Principles 15 and 16).** Core slices interlink freely and reason as one union. Extension
  slices depend only on core. Every slice names its consumer in its manifest. Every term is declared
  in exactly one slice.
* **Canonical design sets.** [`slices/grounding/logic/design/`](./slices/grounding/logic/design/),
  [`slices/core/inhabitation/design/`](./slices/core/inhabitation/design/) and
  [`docs/APPLIED_CATEGORY_THEORY/`](./docs/APPLIED_CATEGORY_THEORY/) are canonical. Read the relevant
  ones in full before working in those areas.

### Validation is authored in `logic:`, never on a shape surface (Principle 17)

OWL, SHACL, ShEx and Datalog are generated lossy projections of the `logic:` core. Full doctrine:
[LOGIC-VALIDATION.md](./slices/grounding/logic/design/LOGIC-VALIDATION.md) and
[docs/MIGRATING-SHAPES-TO-LOGIC.md](./docs/MIGRATING-SHAPES-TO-LOGIC.md).

* **Never hand-author a `sh:NodeShape` or `sh:PropertyShape`** in a slice's `shapes.ttl` or in
  `shapes/`. It is a second source of truth that no reasoner backs, sealed off by the
  projection-purity gate. A shape that remains must carry a `logic:formalizes` back-reference. The
  legacy `shapes.ttl` blocks are debt being migrated under equivalence-before-deletion, not a pattern.
* **Never author `owl:` in a `module.ttl`.** That spelling is a generated view, and reintroducing it
  fails the source lint. New terms state their signature with `logic:domain` and `logic:range`.
  Older modules still author two semantic `rdfs:` forms, and neither is a pattern: subsumption
  (`rdfs:subClassOf`, `rdfs:subPropertyOf`) is counted by the ratchet below; the `rdfs:domain` and
  `rdfs:range` signatures are not counted, and are moved to `logic:` when the term is touched.
* **Declarative checks** (cardinality, class, datatype, node kind, value set) are EL-safe `logic:`
  axioms in the slice's `module.ttl`: `logic:domain`, `logic:range`, `logic:allValuesFrom`,
  `logic:someValuesFrom`, `logic:hasValue`, `logic:oneOf`, `logic:disjointWith`,
  `logic:functionalProperty`, and `logic:maxQualifiedCardinality` or `logic:minQualifiedCardinality`
  with `logic:onClass`. The pipeline derives the SHACL and ShEx. Never the unqualified
  `logic:cardinality`, `logic:minCardinality` or `logic:maxCardinality`: they are outside the EL
  fragment and fail `make reason-verify`.
* **A required path** (closed world, at least one value) is `logic:allValuesFrom` plus an explicit
  `logic:ClosureEntry` (`logic:onClass`, `logic:closureKey`, `logic:closureValue
  logic:ClosedWorldClosure`), never a bare existential.
* **Procedural and cross-node checks** (comparisons, guarded existence, uniqueness, forbidden
  patterns) are a `logic:Constraint` whose `logic:integrity` is a real `logic:Formula` tree, with
  `logic:formalizes` and `logic:message`. The pipeline lowers it to a SPARQL constraint. Slice
  sugar exists (`logic:GuardedImplicationConstraint`, `logic:ChoiceGroupConstraint`,
  `logic:ForbiddenPatternConstraint`). Worked examples:
  [slices/core/ai/module.ttl](./slices/core/ai/module.ttl).
* **Laws** are first-order `logic:Formula` trees. A property that truly is not first-order
  axiomatizable is an honest `logic:expressivenessBoundary` record, never a faked formula, and never
  a record for something that could have been authored.
* **The projection-vocabulary ratchet** holds a non-increasing ceiling per slice and guarded
  vocabulary on hand-authored constructs with no `logic:formalizes`: SHACL, gUFO, the BFO, OBO,
  DOLCE and SUMO bridge vocabularies, RDFS subsumption (`rdfs:subClassOf`, `rdfs:subPropertyOf`)
  and the alignment stack. New second-source authoring fails the gate; no tool creates headroom; a
  raise is a maintainer decision made out of band. Moving constructs between slices is declared
  with a dated `gmeow:CeilingRelocation`, which adjusts the base a ceiling is compared with; axis
  floors are not adjusted by a move. See
  [docs/PROJECTION-VOCABULARY-RATCHET.md](./docs/PROJECTION-VOCABULARY-RATCHET.md).
* A check that cannot be an EL-safe axiom or a `logic:Constraint` is flagged residue in the loss
  ledger and a stop-and-ask, never a self-granted exception.

---

## 3. How to work

### Where work happens

* **Never work in the top-level checkout.** It is shared and a daemon resets it to clean `main`;
  uncommitted work there is wiped. All git work happens in a worktree under `.worktrees/<slug>/`.
* **Working files live under the main checkout's `.stage/`** (git-ignored): notes, evidence, logs,
  patches, scratch. A worktree's own `.stage/` goes when the
  worktree is removed, so nothing that must outlive it stays there. Nothing you need goes in
  `/tmp`, in another repository's tree or anywhere else on the host.
* **Do not accumulate worktrees or pull requests.** The only measure of work done is a merge to
  `main`. A contributor or agent session with more than three of its own open lands those, oldest
  first, before starting anything new.

### Deal-breakers

Never: `git checkout --theirs .` or `--ours .`; `git merge -X theirs` or `-X ours`; bypassing a
commit hook; skipping or mocking the component under test; resolving conflicts in bulk to save time;
`git add -f generated/`; rebasing, amending or force-pushing a pushed branch. Never run `gpg` or
`gpgconf` or touch keys: Git's configured signing does the signing, and a step that needs more is
handed to the maintainer.

### Shell calls

A call that raises a permission prompt stops the session until a person looks.

* **One call does one thing.** A few related commands may share a call; a chain of dozens is never
  written. A step that needs a script gets a script file under `.stage/`.
* **A deletion is its own call**, on one exact path inside this repository, after an earlier call
  listed it. No `rm -r`, `find -delete`, `git clean` or forced worktree removal inside a chain, and
  no deletion by glob or variable. Remove worktrees and branches with plain `git worktree remove`
  and `git branch -d`.
* **A refused call is not resent in another form.** Run the plain small version or report it.

### Long commands and evidence

* **Never wait on a watcher.** No `sleep`, `tail -f`, process listing or polling loop to learn that
  something finished. Run the command in the foreground with a timeout that fits it, or run the
  command itself in the background and act on its completion.
* **Record the tool's own exit status.** Send output to a file under `.stage/` followed by the
  status (`cmd > .stage/x.log 2>&1; echo EXIT=$? >> .stage/x.log`). A status taken through a pipe,
  or a line you typed yourself, is not evidence.
* **A cargo command names its tree** with `--manifest-path`, and its log starts with `pwd` and
  `git rev-parse HEAD`. A result from the wrong directory proves nothing.
* **Evidence matches the gate** flag for flag. A hand-written command that differs from the gate
  proves nothing about the gate.
* **One heavy command at a time.** Several builds or test runs at once on one machine starve each
  other. A session that runs agents in parallel serializes their builds and test runs with
  `flock` on one lock file of its own. The gate takes its own host-wide lock; there is no
  override.
* **Times are read from the clock** when you write them, never estimated.
* **No time or effort estimates.** Reason in dependency order and relative risk. The throughput aim
  is an average of six elapsed hours per issue from intake to merge; record what blocked and for
  how long.
* **Measure once.** A measurement taken for one change is the baseline for the next and is retaken
  only when a named merge affects it. Before a long run, say what decision its result changes; if
  none, do not run it.

### Delegating

* The primary session decides, briefs, checks and reports. Sub-agents implement, review and read at
  length. One coding agent per line of work; read-only agents may run beside it.
* **A brief is all the sub-agent knows:** the goal, what is ruled out, the files to read, the exact
  scope, the worktree, the rules that bind it and where its evidence goes.
* **A reviewer gets the change and the requirement, never your conclusion.** Its verdict names the
  exact head and lists what it read and did not read. A verdict without that coverage is not one.
* **A sub-agent's report is a claim.** Read the evidence it names before repeating it or building
  on it. The diff that merges is read by the session that merges it.

### Research

Use the web: upstream documentation, issue trackers, the state of the art. Queries carry the
technical question, never private data. Research is not copying: no copied or closely paraphrased
code, definitions or prose. Citing and referencing other ontologies is part of this project's work
and follows [docs/CITATIONS.md](./docs/CITATIONS.md).

---

## 4. Toolchain

Rust and Cargo throughout; Docker only in explicit maintainer lanes. The Makefile is the task plan:
run `make help` for the current targets and use `make` targets in preference to calling `gmeow-dev`,
`cargo` or helper scripts directly, unless you are building or debugging the target itself.

### The CLI razor — `gmeow` vs `gmeow-dev`

> `gmeow` does not need a repository; `gmeow-dev` does.

* **`gmeow`** ([crates/gmeow-cli](./crates/gmeow-cli)) is the public consumer binary. Every command
  works from the installed binary alone, backed by the embedded `gmeow.gts`: no checkout, Docker,
  generator inputs or repository query trees. Describing a term, transpiling a user's RDF, proving,
  verifying the bundle, and scoring or linting an external slice against the embedded rubric all
  belong here.
* **`gmeow-dev`** ([crates/gmeow-dev-cli](./crates/gmeow-dev-cli)) is repository maintenance and may
  read anything in the tree: regenerating artifacts, repository-wide slice-quality sweeps,
  refreshing vendored snapshots.

A command that needs a repository path the binary does not bundle is `gmeow-dev`, or its data is
bundled first so it can be `gmeow`.

### Commands

| Purpose | Command |
|---|---|
| Task plan | `make help` |
| Bootstrap (producer, `generated/`, consumer CLIs) | `make install` |
| Format; fast hygiene | `make fmt`; `make lint` |
| Materialize, then gate (the normal path) | `make check` |
| Print the gate's wave plan without running | `make check CHECK_ARGS="--explain"` |
| Artifacts only, no gate | `make check-sync SYNC_MODE=update` |
| Read-only drift check | `make check-sync` |
| Turtle, annotations, SHACL | `make validate` |
| Native reasoning and verification, one closure | `make reason-verify` |
| Rust tests; lints; test binaries only | `make rust-test`; `make clippy`; `make rust-build` |
| One crate's tests from the workspace inventory | `make nextest NEXTEST_FILTER='package(<crate>)'` |
| Review intended snapshot changes | `make insta-review` |
| Constitution and crate layering | `make constitution-check`; `make crate-check` |
| Documentation projections | `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs` |
| CLIs, serializations and JSON-LD context into `dist/` | `make build` |
| Portable `gmeow` with receipt | `make cli-build`; `cargo xtask cli install` |
| Release | `make release`; `make release-sign-gts SIGN_KEY=… GTS_OUT=dist/gmeow.gts` |
| Breadth lanes (CI only) | `make heavy` |
| Off-gate heavy Rust suite | `make maint-rust-heavy` |

Targets prefixed `maint-` are maintainer work (network, Docker, heavyweight or report-only) and stay
outside `make check`.

### One producer, one run

The doctrine is [docs/GATE-AND-PIPELINE.md](./docs/GATE-AND-PIPELINE.md). Read it before adding or
moving a gate task, changing a test budget, blessing a ratchet baseline or writing a comment that
says a gate enforces something. A false claim in help text or a comment is itself a defect.

* `make check-sync` is the only target that runs the regeneration pipeline, and `make check` drives
  it in update mode as the first node of the gate. Stale generated tree: run `make check`, once.
  Materializing first and gating second runs the pipeline twice under the same host-global lock;
  `make regen` refuses for that reason.
* `SYNC_OUTPUTS` selects the scope: `generated` (default: the bundle and the `generated/` tree),
  `docs`, or `all` (adds runtime projections such as JSON-LD, YAML-LD and OKF). Narrowing the scope
  never weakens the selected profile's gates. `SYNC_VERBOSE=1` streams stages.
* Update mode writes only changed files and removes stale owned outputs; check mode renders and
  validates without touching files. CI uses check mode, so it fails on drift.
* A clean manifest makes a warm run skip the pipeline. The pipeline uses every CPU unless
  `--jobs N` is given.
* `make check` runs every task in its DAG; there is no selection profile. A task depends on `sync`
  exactly when it reads a generated artifact.
* The pipeline's architecture (the in-memory carrier, the single `gmeow.gts` terminal, the fanout)
  is [docs/PIPELINE_SPINE.md](./docs/PIPELINE_SPINE.md), canonical for anything in `crates/pipeline`
  or under `generated/`. Every generated artifact is a projection of `gmeow.gts`.
* Pipeline and fixture production run only through the authenticated `pipeline` build (`make
  producer-build`, executable and receipt in `dist/bin/`). A test or debug build never stands in for
  it.
* `generated/` is git-ignored. `generated/dist/gmeow.gts` is a local product: after integrating
  `main`, `make check` re-materializes it. A stale bundle is refused, not accepted.
* Documentation projections are never embedded in `gmeow.gts`.

### GTS distribution contract

Every payload-bearing frame of a production GMEOW bundle uses `zstd-rsyncable` at level 12: never
gzip, plain zstd, identity or a size-dependent fallback. The leaf crate `gmeow-gts-profile` is the
only production door to bundle authorship (`emit_gmeow_gts`, `view_to_gmeow_gts`, `GmeowGtsWriter`,
`compact_gmeow_gts`), and `validate_mandated_frames` audits each producer's output. Repository-static
seals keep it the only door. The GTS engines and the format's own CLI live in the separate
`gmeow-gts` repository.

### Reasoning

GMEOW is the single authority on `logic:` semantics: compilation, world scope, grounding and
proof-ledger policy. PurRDF owns the generic mechanisms: RDF, datatype semantics, Datalog and chase,
OWL-Direct decision procedures. One owner per mechanism; an inferior implementation is deleted when
its replacement qualifies. Reasoning is native and in process. There is no second reasoner on the
gate; independent coverage is the frozen `dl_oracle_gold` corpus and the native crosscheck ledger.

---

## 5. Testing

### No test produces the corpus

No test may rebuild, regenerate, materialize or compile the corpus, directly or through library
code, fixtures, hooks, subprocesses, CLIs, Make targets or scripts. No exceptions. A test consumes an
already produced, authenticated corpus whose exact identity is handed to it, and fails closed when
that corpus is absent, stale or wrong. A cache miss never triggers production. A test path that
produces the corpus blocks review, commit and merge.

* `make produce-test-fixtures` is the explicit producer boundary. It writes a selector manifest; the
  runner supplies its digest to each test process; `make nextest` verifies it read-only first.
* A corpus-backed test run without that manifest refuses. That refusal is expected in a checkout
  with no produced fixtures and is not a regression.
* Repository slice specifications are producer work with recorded receipts, not test cases. Tests
  verify the aggregate verdict read-only.
* A test binary may be compiled under another Cargo profile, but it loads the producer-selected
  action; it never derives a key from its own profile. Slice tests do not call the repository
  sweep, its worker or its corpus stores.
* There is no automatic retry: a failed whole-bundle consumer is terminal.

### Tests must earn their keep

A permanent test runs on every change forever.

* It protects a named production behaviour, regression, boundary or invariant, and its doc comment
  says which. If you cannot say it easily, do not add the test.
* Test behaviour, not implementation. No tests for getters, mappers, or what the compiler already
  guarantees; no tests that mirror the code line by line.
* Prefer one property test over many copied cases, and a snapshot over a hand-written wall of
  assertions. Prefer a small in-memory fake to a mock that asserts how it was called.
* Stop when the contract is covered. Removing outdated or overlapping tests is encouraged.
* Generic RDF and SPARQL conformance is PurRDF's. Tests here assert GMEOW product behaviour.

### Running tests

* Run the test, crate or consumer your change touches. A whole-workspace run is for the mandatory
  gate, not for every small change, and not to re-prove failures already known.
* Prefer `make nextest NEXTEST_FILTER=…`: a direct package-scoped Cargo call can start a second
  build lineage.
* **Snapshots** (`insta`) are byte-exact and unredacted. Drift is a hard failure. After an intended
  change run `make insta-review`, read the diff, commit it. Never accept a snapshot unread and never
  leave a `.snap.new` behind.
* **Wall-clock time is not a correctness gate.** `make rust-test`, `make rust-gate` and the CI Rust
  shards fail on correctness, lints and snapshots, never on elapsed time. A test that fails on
  machine load is broken, not flaky. The 60-second slow timeout exists to catch hangs. Do not add fixed concurrency caps; a
  test that starts a full-width pool may reserve `threads-required = "num-cpus"`.
* **Benchmarks, coverage and mutation testing are report-only** and run in the scheduled
  `suite-quality` workflow, off the pull request path. Agents do not run mutation testing.
* An exhaustive whole-repository proof may live on the `maint-heavy` lane when focused tests and
  drift gates cover the same contract. Never move a test there because one sample was slow.

---

## 6. Rust engineering

Optimize for reading, maintenance and compile-time guarantees. Performance work also follows
[docs/RUST-OPTIMIZATION.md](./docs/RUST-OPTIMIZATION.md): measure first, keep output deterministic,
prefer data, dispatch and ownership changes to compiler flags, and keep debug assertions, overflow
checks and the no-debug-symbol policy.

* **Workspace.** Small crates with strict layering; `make crate-check` rejects a cycle. As a design
  rule, a crate that holds domain logic does not depend on one that does I/O or orchestration. New
  dependencies are declared once in the root `Cargo.toml` and inherited; a crate that still pins
  its own version is moved when you touch it. Binaries are thin: they parse arguments and wire
  libraries.
* **Visibility.** Private by default, `pub(crate)` before `pub`. Every `pub` item is a maintenance
  commitment.
* **Reuse before writing.** Prefer what PurRDF and this workspace already provide to a new local
  solution, and never re-implement PurRDF's mechanisms. `make helpers-hygiene` counts duplicated
  helpers and its debt only falls.
* **Files** stay under 5,000 lines; split a larger one when you are working in it.
* **SOLID.** Review for it before planning and in review. Refactor code you are already changing when
  it helps.
* **Types.** Make illegal states unrepresentable. Parse into a domain type at the boundary; do not
  pass a raw `String` or integer that has domain meaning across modules. Encode a lifecycle in
  types, not in booleans. Use dense typed identifiers on hot paths.
* **Signatures.** Take `&str`, `&[T]` and `impl AsRef<Path>`; use `Cow` when a function rarely
  changes its input. Put `#[must_use]` on pure functions, builders and result types. Use
  `let … else` for early returns.
* **Errors.** Library code returns the workspace's typed diagnostics (`gmeow-errors`): a kind for
  each failure mode, bound to a failure class in the ontology where one exists. No
  `Result<_, String>`. Keep the
  source of an error; never swallow it.
* **No panics in production paths.** No `.unwrap()`. An `.expect()` states the invariant that makes
  failure impossible. In tests, `.expect("why the setup is valid")`.
* **Logging.** Diagnostics go through `tracing` with structured fields, as
  `tracing::info!(target: "pipeline_phase", stage, phase, event = "start", "producer phase
  boundary")` does, never interpolated strings, `dbg!` or stray `println!`. The printing that
  belongs is a CLI's report on standard output, which is its product, and a build script's cargo
  directives. Instrument boundaries, not inner loops, and keep secrets and large buffers out of
  spans.
* **Determinism.** Generated bytes and digests do not change unless a digest domain is deliberately
  versioned and the change is named. A nondeterministic artifact is a compiler bug to fix, never an
  output to normalize by hand.

---

## 7. Compilers

The logic lives in the crates under [crates/](./crates/); the Makefile only runs it.

* **Mappings** ([stages/mappings.rs](./crates/pipeline/src/stages/mappings.rs)). Input: Turtle under
  `dsl/mappings/` and each slice's `mappings/`. Output: SSSOM rows, EDOAL cells, the FnO catalog and
  SPARQL projection queries under `generated/`. Artifacts are rendered to staging, cross-layer
  invariants run, and only then are files written; on a failed invariant nothing is written.
  * Authoring units: a native RDF 1.2 alignment cell (a reified `S skos:*Match O {| … |}`) for a
    pure link, and `gmeow:ProjectionMapping` for a directional, possibly lossy projection.
  * `logic:GroundingCorrespondence` marks a grounding on either unit and needs
    `gmeow:justification`, named `logic:sourceEndpoint` and `logic:targetEndpoint`, and explicit
    `logic:morphismClass`, `logic:morphismKind` and `logic:preservationKind`. Incomplete metadata is
    a validation error. Contract:
    [LOGIC-CORRESPONDENCE.md](./slices/grounding/logic/design/LOGIC-CORRESPONDENCE.md).
  * Callable interfaces are `logic:CallableSignature` records in the owning slice's
    `mappings/callable-signatures.ttl`. There is no authored FnO companion.
* **Statements** ([stages/statements.rs](./crates/pipeline/src/stages/statements.rs)). Input: Turtle
  under `dsl/statements/`. Output: `generated/statements/gmeow.rdf12.ttl` (the RDF 1.2 lead) and its
  OWL downcast. The compiler projects to RDF 1.2 natively, normalizes back and requires graph
  isomorphism before writing. Wrong metadata is fixed in the `gmeow:StatementMetadata` cells.
* **Generated artifacts.** A registered generator owns each one, with staging, source-hash banners,
  drift and orphan detection and the internal-tag leak gate. A change to one must be reproducible by
  `make check`.
* **Vocabulary index.** The export stage writes `dist/llms.txt`: a flat index of every class,
  property and individual with its definition. It is not committed; `make check` produces it. Read
  it when you need to look terms up.

---

## 8. Layout

Under `generated/`: a registered generator owns it and you never edit it. Under `dist/`: ephemeral,
never committed. `ontology-docs/` at the root is a generated documentation tree, ignored and rebuilt
on demand. Everything else is authored.

```text
slices/<group>/<name>/   the unit of the ontology. <group> (core, extensions, grounding, profile) is
                         organization only; manifest.ttl is the sole source of identity and tier.
                         Anatomy: manifest.ttl, module.ttl, shapes.ttl, mappings/, queries/,
                         examples/, tests/, i18n/, docs.md
slices/vocabulary.ttl    the slice-manifest authoring vocabulary
ontology/gmeow.ttl       root ontology: the core profile (generated imports)
dsl/mappings/            shared mapping vocabulary and cross-cutting sets
dsl/statements/          statement DSL
shapes/                  the annotation-completeness and DSL shapes the gates run
queries/                 authored SPARQL: competency, verify, qc, codecs, audit
imports/                 vendored externals
metadata/                citations and release records
docs/                    cross-slice doctrine (a slice's guide lives in the slice)
governance/              the constitution's enforcement records
crates/                  the Rust toolchain
tests/                   cross-slice tests (slice tests live in the slice)
generated/  dist/        products, never edited, never committed
```

To add a slice, copy a core slice's anatomy. A slice's English literals carry `@x-gmeow-english`;
its French and Chinese catalogs under `i18n/` must have an entry whose `msgid` equals every English
literal you add or change.

---

## 9. Work sizing: decompose, never defer

A work item is well formed only if one branch can land it as a single squash-merge with the gate
green. An issue no single pull request can close is a planning defect.

* **Decompose before writing code.** Each child is one landable unit. Children name what blocks them
  and what they block. The original becomes a tracking epic (label `epic`) with no code of its own, closed when
  every child has closed. Every requirement maps to exactly one child.
* **Deferral is banned; decomposition is required.** Deferral leaves a requirement of the unit being
  landed unbuilt, stubbed or marked for later. Decomposition splits an oversized unit so that every
  requirement lands in a tracked child, each now, in dependency order.
* **Group related changes.** Changes that touch the same slice or the same mechanism are one body of
  work: one branch, one review, one pull request that closes every issue it covers, one measurement
  at its end. Two pull requests on one slice cost a second merge, review and CI run.

---

## 10. Pull request lifecycle

1. **Integrate `main` by merging it into the branch, never by rebasing.** `git fetch origin main`
   then `git merge origin/main`.
   The branch may hold merge commits; the squash collapses them.
2. **Resolve each conflict by reading both sides** and keeping the most function. Never take one
   side wholesale. In translation catalogs keep every entry from both sides exactly once.
3. **`generated/` is never merged by hand.** It is untracked. A branch that once modified it resolves
   every such conflict in favour of the deletion (`git rm <path>`); the intent lives in the sources.
   Re-materialize with `make check` after integrating.
4. **Read all feedback.** `gh pr view <n> --json reviews` gives review summaries; inline comments,
   where most findings are, come from `gh api repos/<owner>/<repo>/pulls/<n>/comments`.
   Read both automated (CodeRabbit, Gemini) and human reviews.
   An actionable automated finding is binding unless it contradicts the Constitution. A reply to a
   reviewer must be true of the named commit.
5. **Fix in canonical sources only**, then regenerate:

   | Review target | Edit |
   |---|---|
   | SSSOM, EDOAL, FnO, projection queries | the owning slice's `mappings/`, or `dsl/mappings/` |
   | RDF 1.2 or OWL statement artifacts | `dsl/statements/` |
   | Terms, axioms, constraints | `slices/<group>/<name>/module.ttl` |
   | Tests and fixtures | the slice's `tests/`, or `tests/` |

6. **Validate.** `make check` once, after all changes: lint, validate, drift, native reasoning and
   verification, Rust tests. CI builds the branch merged into `main`, so integrate first.
7. **Push.** Stage explicit paths (never `git add -A`), commit through the hooks, plain `git push`.
   No amend, no force.
8. **Land by squash-merge with `ghprsq`**, never by `git merge` into `main` or `gh pr merge`. `main`
   stays linear.
   The description states only what was run and shown, and lists what was not.

No tool or agent credits in source, commits, issues or pull requests.

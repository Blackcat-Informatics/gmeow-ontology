# GMEOW ontology toolchain — canonical task runner.
# Every target shells into the `gmeow-dev` CLI or a focused helper script; no
# logic lives in this file. Run `make help` for the target list.

.DEFAULT_GOAL := help
SHELL := /bin/bash

# Alignment target for `make extract` (license-checked). Override: make extract TARGET=foaf
TARGET ?= foaf

# Override: make commit MESSAGE="feat: add foaf alignment"
MESSAGE ?= "chore: regenerate checked-in artifacts"
GMEOW_DEV ?= uv run --package gmeow-dev gmeow-dev

# Optional cargo-nextest partition for sharded CI runs (e.g., count:1/2)
NEXTEST_PARTITION ?=
NEXTEST_PARTITION_ARG := $(if $(NEXTEST_PARTITION),--partition $(NEXTEST_PARTITION) --no-tests pass,)

# === Dual build: host-tuned LOCAL binaries (opt-in) vs portable wheels (default) ===
# The committed .cargo/config.toml pins Rust to the portable `x86-64-v3` floor, so
# every wheel/CI build ships correctly with no extra flags. `make dev` / `make
# bench` opt INTO host tuning with `target-cpu=native`. Because the RUSTFLAGS env
# var REPLACES (does not merge with) the config's rustflags, these targets must
# re-state the bundled-lld linker flags too. KEEP IN SYNC with the
# `[target.x86_64-unknown-linux-gnu]` rustflags in .cargo/config.toml.
NATIVE_RUSTFLAGS := -Zunstable-options -Clink-self-contained=+linker -Clinker-features=+lld -Ctarget-cpu=native

.PHONY: help install fmt lint validate crosscheck classic-cross-check reason reason-native reason-hermit explain verify verify-docker reasoning-cases statements-docker-check extract \
        mappings wikidata wikidata-live wikidata-coverage wikidata-audit \
        lint-alignment crate-check refresh-target-axioms docs docs-full ontology-docs ontology-docs-full quality \
        normalize build project test test-fast test-docker check check-generated release regenerate commit clean clean-docs pull-images \
        coverage acceptance crossref constitution-check compliance-report compliance-report-full audit evals-score \
        diagnostics-build diagnostics-test diagnostics-py \
        native-py rust-test logic-build logic-test logic-py conformance \
        shacl-build shacl-test shacl-py \
        validate-build validate-test validate-py validate-gts rdf-py clippy slicetest \
        bench dev

help: ## Show this help.
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

install: ## Sync the uv environment (runtime + dev deps) and Git merge drivers.
	uv sync --all-packages
	bash scripts/bootstrap-git-merge-drivers.sh

fmt: ## Auto-format with ruff.
	uv run ruff format .

lint: ## Lint (ruff), type-check (mypy), and full repo-hygiene suite (pre-commit).
	uv run ruff check .
	uv run ruff format --check .
	uv run mypy
	# Symmetry with CI: CI's `lint` job runs the whole pre-commit suite
	# (markdownlint, end-of-file-fixer, codespell, yamllint, shellcheck, …),
	# so the local gate must too — otherwise those lanes only fail in CI.
	uv run pre-commit run --all-files --show-diff-on-failure

validate: diagnostics-py validate-py shacl-py ## Validate syntax, term annotations, and SHACL (Rust-native orchestration).
	$(GMEOW_DEV) validate

validate-gts: diagnostics-py validate-py shacl-py ## Validate the committed GTS bundle directly via the gmeow-gts oxigraph adapter (#644).
	$(GMEOW_DEV) validate --gts generated/dist/gmeow.gts

reason-native: logic-py ## Native Docker-free EL/DL reasoning authority (reason --mode native).
	$(GMEOW_DEV) reason --mode native

reason: reason-native ## OWL consistency reasoning — native, Docker-free authority (alias for reason-native).

# === CLASSIC-CROSS-CHECK LANE — the SOLE Java+Docker surface (#666, Principle 18) ===
# Everything in this block runs the legacy oracles (ELK, HermiT, ROBOT, Jena) and
# the rdflib engine cross-check. It is CROSS-CHECK ONLY: never part of `make check`,
# never in the required CI `quality` gate, never required to use the repo normally.
# The authoritative, Docker-free gate is `make reason` (= reason-native).
classic-cross-check: ## CROSS-CHECK ONLY (Docker/Java oracles) — NOT required for normal repo use (#666).
	$(GMEOW_DEV) reason --mode docker --reasoner ELK --exclude-tautologies structural
	$(GMEOW_DEV) verify --mode docker --reasoner ELK --reasoned-input dist/gmeow-reasoned-elk.ttl
	$(GMEOW_DEV) reason --mode docker --reasoner hermit
	uv run python scripts/reasoning_cases.py
	uv run python scripts/statements_docker_check.py
	uv run python scripts/slme_cross_check.py
	$(GMEOW_DEV) crosscheck-queries
	$(GMEOW_DEV) classic-cross-check
	$(GMEOW_DEV) classic-cross-check-rl
	uv run pytest -n auto --dist loadscope -m "classic_cross_check" -q
	@echo "✓ classic-cross-check (oracle lane) passed — NOT a normal-use requirement"

reason-hermit: ## [lane] Sound + complete consistency check with HermiT (Docker oracle).
	$(GMEOW_DEV) reason --mode docker --reasoner hermit

explain: ## [lane] Explain any unsatisfiable classes (HermiT, Docker oracle).
	$(GMEOW_DEV) explain

verify: logic-py ## Reasoned-graph negative tests (native EL/DL closure, Java/Docker-free — #695).
	$(GMEOW_DEV) verify --mode native

verify-docker: ## [lane] Reasoned-graph negative tests (ELK reason + ROBOT verify over queries/verify/, Docker oracle).
	$(GMEOW_DEV) reason --mode docker --reasoner ELK --exclude-tautologies structural
	$(GMEOW_DEV) verify --mode docker --reasoner ELK --reasoned-input dist/gmeow-reasoned-elk.ttl

reasoning-cases: ## [lane] HermiT/ELK inconsistency and fixture-coherence cases (Docker oracle).
	uv run python scripts/reasoning_cases.py

statements-docker-check: ## [lane] Jena/ROBOT-backed statement artifact and reasoning checks (Docker oracle).
	uv run python scripts/statements_docker_check.py

crosscheck: ## [lane] Prove rdflib (legacy engine) and the native gmeow_rdf engine answer every committed query alike (no Docker).
	$(GMEOW_DEV) crosscheck-queries

extract: ## [maintainer] Import/extract policy for TARGET (native SLME, Java/Docker-free — maintainer-only, NOT normal-use; #695).
	$(GMEOW_DEV) extract --target $(TARGET)


mappings-only: ## Build alignment axioms + VoID linksets (assumes SSSOM files present).
	$(GMEOW_DEV) mappings

mappings: ## Build alignment axioms + VoID linksets from SSSOM; validate QID syntax.
	$(GMEOW_DEV) mappings

lint-alignment: ## Lint SSSOM mappings for inverse / domain-range-mismatched targets (offline).
	$(GMEOW_DEV) lint-alignment

crate-check: ## Verify Rust crate layering: gmeow-rdf kernel purity + acyclic crate DAG (#820 S0).
	$(GMEOW_DEV) crate-check

refresh-target-axioms: ## [maintainer] Re-vendor minimal target-axiom snapshots (pure-Python httpx, Java/Docker-free — maintainer-only, NOT normal-use; #695).
	$(GMEOW_DEV) refresh-target-axioms --target all

wikidata: ## Validate Wikidata QID/PID syntax in the mappings (offline).
	$(GMEOW_DEV) wikidata

wikidata-live: ## Also verify Wikidata ids resolve (network).
	$(GMEOW_DEV) wikidata --existence

wikidata-coverage: ## Report Wikidata mapping coverage by domain (offline).
	$(GMEOW_DEV) wikidata-coverage

wikidata-audit: ## Audit fixtures and modules for Wikidata misuse (offline).
	$(GMEOW_DEV) wikidata --fixtures

coverage: ## Report how much of the vendored entity slice GMEOW covers (hard gate).
	$(GMEOW_DEV) coverage --gaps --min-class 0.92 --min-predicate 0.85

# The aggregate round-trip recall floor (#579). Set just below the current
# measured corpus aggregate (paudley+bii ≈ 64%) with anti-flake margin. The
# per-file gates stay scoreboard/soft; only this pooled aggregate is hard.
ACCEPTANCE_MIN_RECALL ?= 60

acceptance: ## Score the full transpile against external/ snapshots; hard aggregate recall floor (#450/#579).
	$(GMEOW_DEV) acceptance --min-recall $(ACCEPTANCE_MIN_RECALL)

crossref: ## Generate the CrossRef DOI deposit XML.
	$(GMEOW_DEV) crossref

docs: ontology-docs ## Alias for ontology-docs.

ontology-docs: ## Generate the unified ontology-docs site into ontology-docs/.
	$(GMEOW_DEV) docs

docs-full: ontology-docs-full ## Alias for ontology-docs-full.

ontology-docs-full: ## Generate ontology-docs including optional Docker stages into dist/ontology-docs.
	uv run python -c "from gmeow_tools.config import PROJECT_ROOT; from gmeow_tools.ontology_docs import build_ontology_docs; build_ontology_docs(PROJECT_ROOT / 'dist' / 'ontology-docs')"

quality: ## Run OOPS! pitfall scan (network, best-effort).
	$(GMEOW_DEV) quality

normalize: ## Canonicalize the authored ontology sources (rewrites files).
	$(GMEOW_DEV) normalize

check-generated: ## Drift + orphan check for all registered generators.
	$(GMEOW_DEV) check-generated -j $$(nproc 2>/dev/null || echo 4)

constitution-check: ## Every constitutional principle must have live enforcement (#280).
	$(GMEOW_DEV) constitution-check

compliance-report: ## Emit dist/compliance-report.ttl from gates already run by make check/CI (#285).
	$(GMEOW_DEV) compliance-report --from-passing-check

compliance-report-full: ## Run in-process gates, emit dist/compliance-report.ttl (#285).
	$(GMEOW_DEV) compliance-report

audit: ## Claim audit gates over the worked fixture (#55): ungrounded/contradicted/stale.
	$(GMEOW_DEV) audit tests/fixtures/coverage/hallucination-kg.ttl

evals-score: ## Score committed model emissions against the published contract (offline, #298).
	$(GMEOW_DEV) evals score

diagnostics-build: ## Build the gmeow-diagnostics Rust crate (shared Finding/Report core).
	cargo build -p gmeow-diagnostics

diagnostics-test: ## Run the gmeow-diagnostics unit tests.
	cargo nextest run -p gmeow-diagnostics $(NEXTEST_PARTITION_ARG)
	cargo test --doc -p gmeow-diagnostics

logic-build: ## Build the gmeow-logic Rust crate (world-indexed oxigraph store core).
	cargo build -p gmeow-logic

logic-test: ## Run the gmeow-logic unit tests (world-isolation conformance).
	cargo nextest run -p gmeow-logic $(NEXTEST_PARTITION_ARG)
	cargo test --doc -p gmeow-logic

shacl-build: ## Build the gmeow-shacl Rust crate (oxigraph SHACL Core validator).
	cargo build -p gmeow-shacl

shacl-test: ## Run the gmeow-shacl unit + conformance tests.
	cargo nextest run -p gmeow-shacl $(NEXTEST_PARTITION_ARG)
	cargo test --doc -p gmeow-shacl

validate-build: ## Build the gmeow-validate Rust crate (oxigraph validation-path lints).
	cargo build -p gmeow-validate

validate-test: ## Run the gmeow-validate unit + integration tests.
	cargo nextest run -p gmeow-validate $(NEXTEST_PARTITION_ARG)
	cargo test --doc -p gmeow-validate

clippy: ## Run cargo clippy on all Rust targets with warnings as errors.
	cargo clippy --all-targets -- -D warnings

native-py: ## Build and install the single unified gmeow_native Python extension (maturin develop, #630).
	VIRTUAL_ENV="$$(pwd)/.venv" uvx maturin develop --manifest-path crates/native/Cargo.toml

# Legacy per-crate target names kept as aliases of the single `native-py` build
# (the five extensions now fold into one `gmeow_native` cdylib, #630). Docs, CI,
# and memory still reference these names, so `make logic-py` / `make validate-py`
# etc. keep working — they all just build the unified extension.
diagnostics-py logic-py shacl-py validate-py rdf-py: native-py

rust-test: ## Run the Rust workspace tests.
	cargo nextest run $(NEXTEST_PARTITION_ARG)
	cargo test --doc

bench: ## Run criterion benchmarks (release, host-tuned target-cpu=native) — the acceleration-program baseline (#630).
	RUSTFLAGS="$(NATIVE_RUSTFLAGS)" cargo bench -p gmeow-logic -p gmeow-shacl -p gmeow-validate

dev: ## Build + install gmeow_native host-tuned (maturin develop --release, target-cpu=native) for optimized LOCAL runs.
	RUSTFLAGS="$(NATIVE_RUSTFLAGS)" VIRTUAL_ENV="$$(pwd)/.venv" uvx maturin develop --release --manifest-path crates/native/Cargo.toml

slicetest: ## Run the gmeow-slicetest harness in isolation (executes the slice-resident test-DSL specs; #784). Already covered by rust-test / check via the workspace run.
	cargo nextest run -p gmeow-slicetest $(NEXTEST_PARTITION_ARG)
	cargo test --doc -p gmeow-slicetest

conformance: ## Run the native logic conformance harness (#785; oracle ≡ engine, Principle 7 gate). Already covered by rust-test / check via the workspace run.
	cargo nextest run -p gmeow-conformance $(NEXTEST_PARTITION_ARG)

build: ## Build serializations and JSON-LD context into dist/.
	$(GMEOW_DEV) build

project: ## Project GMEOW data to pure schema.org/GeoSPARQL/vCard/FOAF/iCal/OWL-Time profiles (FnO/EDOAL).
	$(GMEOW_DEV) project

test: native-py ## Run the full test suite (incl. heavy ci_only export tests; excludes the classic-cross-check lane).
	uv run pytest -n auto --dist loadscope -m "not classic_cross_check"

test-fast: native-py ## Run the fast test suite (excludes ci_only, docker, and the classic-cross-check lane).
	uv run pytest -n auto --dist loadscope -m "not ci_only and not docker and not classic_cross_check"

test-docker: classic-cross-check ## Compatibility alias for the classic-cross-check (Docker/Java oracle) lane.

test-network: ## Run the network tests (LIVE endpoints) — MANUAL only, never in CI/check.
	GMEOW_RUN_NETWORK=1 uv run pytest -m network

check: logic-py rdf-py ## Fast local gate: core ontology + transforms (native EL/DL reasoning — Java/Docker-free; classic-cross-check oracle lane runs separately).
	$(MAKE) -j$$(nproc 2>/dev/null || echo 4) lint clippy rust-test validate check-generated constitution-check crate-check audit wikidata coverage acceptance reason-native verify mappings-only lint-alignment
	uv run pytest -n auto --dist loadscope -m "not ci_only and not docker and not classic_cross_check"
	$(GMEOW_DEV) compliance-report --from-passing-check
	@echo "✓ all checks passed (Docker-free, Java-free)"

release: ## RDF 1.2 + OWL downcast → native reasoned closure + build + regenerate + CrossRef deposit (Docker-free).
	$(GMEOW_DEV) regenerate
	$(GMEOW_DEV) reason --mode native --merge
	$(GMEOW_DEV) build
	$(MAKE) compliance-report-full
	$(GMEOW_DEV) crossref

regenerate: ## Rebuild all checked-in generated artifacts from canonical sources.
	$(GMEOW_DEV) regenerate -j $$(nproc 2>/dev/null || echo 4)

commit: regenerate ## Regenerate artifacts, stage them, and commit.
	@REGENERATED_PATHS=$$(uv run python -c "from gmeow_tools.load_generators import load_all; load_all(); from gmeow_tools.generator import all_regenerated_paths; print(' '.join(all_regenerated_paths()))"); \
	git add $${REGENERATED_PATHS}; \
	if git diff --cached --quiet; then \
		echo "Nothing to commit."; exit 1; \
	else \
		git commit -m "$(MESSAGE)"; \
	fi
	@git diff --quiet || echo "Warning: unstaged changes remain. Stage them with 'git add' and commit separately if needed."

pull-images: ## [maintainer] Pre-pull the pinned Docker images for the classic-cross-check lane (ROBOT, Jena; #666).
	bash scripts/pull-images.sh

clean: ## Remove ephemeral build artifacts.
	rm -rf dist docs/_generated .stamps
	@echo "✓ cleaned"

clean-docs: ## Remove generated ontology docs (regenerate with make ontology-docs).
	rm -rf dist/ontology-docs ontology-docs
	@echo "✓ cleaned ontology docs"

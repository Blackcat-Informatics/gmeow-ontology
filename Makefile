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

.PHONY: help install fmt lint validate crosscheck reason reason-hermit explain verify reasoning-cases statements-docker-check extract \
        mappings wikidata wikidata-live wikidata-coverage wikidata-audit \
        lint-alignment refresh-target-axioms docs docs-full ontology-docs ontology-docs-full quality \
        normalize build project test test-fast test-docker check check-docker check-generated release regenerate commit clean clean-docs pull-images \
        coverage acceptance crossref constitution-check compliance-report compliance-report-full audit evals-score \
        logic-build logic-test logic-py conformance \
        shacl-build shacl-test shacl-py \
        validate-build validate-test validate-py

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

validate: validate-py shacl-py ## Validate syntax, term annotations, and SHACL (Rust-native orchestration).
	$(GMEOW_DEV) validate

crosscheck: ## Prove rdflib and pyoxigraph answer every committed query alike (no Docker).
	$(GMEOW_DEV) crosscheck-queries

reason: ## Merge, validate OWL 2 DL profile, and check ELK consistency (Docker).
	$(GMEOW_DEV) reason --reasoner ELK --exclude-tautologies structural

reason-hermit: ## Sound + complete consistency check with HermiT (Docker).
	$(GMEOW_DEV) reason --reasoner hermit

explain: ## Explain any unsatisfiable classes (HermiT, Docker).
	$(GMEOW_DEV) explain

verify: reason ## Reasoned-graph negative tests (ROBOT verify over queries/verify/, Docker).
	$(GMEOW_DEV) verify --reasoner ELK --reasoned-input dist/gmeow-reasoned-elk.ttl

reasoning-cases: ## HermiT/ELK inconsistency and fixture-coherence cases (Docker).
	uv run python scripts/reasoning_cases.py

statements-docker-check: ## Jena/ROBOT-backed statement artifact and reasoning checks (Docker).
	uv run python scripts/statements_docker_check.py

extract: ## Report import/extract policy for TARGET (refuses reference-only).
	$(GMEOW_DEV) extract --target $(TARGET)


mappings-only: ## Build alignment axioms + VoID linksets (assumes SSSOM files present).
	$(GMEOW_DEV) mappings

mappings: ## Build alignment axioms + VoID linksets from SSSOM; validate QID syntax.
	$(GMEOW_DEV) mappings

lint-alignment: ## Lint SSSOM mappings for inverse / domain-range-mismatched targets (offline).
	$(GMEOW_DEV) lint-alignment

refresh-target-axioms: ## Re-vendor minimal target-axiom snapshots (IMPORT_OK targets only).
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

ontology-docs: ## Generate the unified ontology-docs site into dist/ontology-docs.
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

logic-build: ## Build the gmeow-logic Rust crate (world-indexed oxigraph store core).
	cargo build -p gmeow-logic

logic-test: ## Run the gmeow-logic unit tests (world-isolation conformance).
	cargo test -p gmeow-logic

logic-py: ## Build and install the gmeow_logic Python extension (maturin develop).
	uvx maturin develop --manifest-path crates/logic/Cargo.toml

shacl-build: ## Build the gmeow-shacl Rust crate (oxigraph SHACL Core validator).
	cargo build -p gmeow-shacl

shacl-test: ## Run the gmeow-shacl unit + conformance tests.
	cargo test -p gmeow-shacl

shacl-py: ## Build and install the gmeow_shacl Python extension (maturin develop).
	uvx maturin develop --manifest-path crates/shacl/Cargo.toml

validate-build: ## Build the gmeow-validate Rust crate (oxigraph validation-path lints).
	cargo build -p gmeow-validate

validate-test: ## Run the gmeow-validate unit + integration tests.
	cargo test -p gmeow-validate

validate-py: ## Build and install the gmeow_validate Python extension (maturin develop).
	uvx maturin develop --manifest-path crates/validate/Cargo.toml

conformance: ## Run the logic: conformance suite (oracle ≡ engine, Principle 7 gate).
	$(GMEOW_DEV) conformance

build: ## Build serializations and JSON-LD context into dist/.
	$(GMEOW_DEV) build

project: ## Project GMEOW data to pure schema.org/GeoSPARQL/vCard/FOAF/iCal/OWL-Time profiles (FnO/EDOAL).
	$(GMEOW_DEV) project

test: ## Run the full test suite (incl. heavy ci_only export tests).
	uv run pytest -n auto --dist loadscope

test-fast: ## Run the fast test suite (excludes ci_only, docker, and CI-only pyoxigraph).
	uv run pytest -n auto --dist loadscope -m "not ci_only and not docker and not pyoxigraph_ci"

test-docker: check-docker ## Compatibility alias for the Docker-backed Make lanes.

test-network: ## Run the network tests (LIVE endpoints) — MANUAL only, never in CI/check.
	GMEOW_RUN_NETWORK=1 uv run pytest -m network

check: ## Fast local gate: core ontology + transforms (ELK only; HermiT runs in its own CI job).
	$(MAKE) -j$$(nproc 2>/dev/null || echo 4) lint validate crosscheck check-generated constitution-check audit wikidata coverage acceptance reason verify mappings-only lint-alignment
	$(MAKE) test-fast
	$(GMEOW_DEV) compliance-report --from-passing-check
	@echo "✓ all checks passed"

check-docker: ## Optional local Docker gate: HermiT, reasoning cases, and Jena statements.
	$(MAKE) reason
	$(GMEOW_DEV) verify --reasoner ELK --reasoned-input dist/gmeow-reasoned-elk.ttl
	$(MAKE) reason-hermit
	$(MAKE) reasoning-cases
	$(MAKE) statements-docker-check
	@echo "✓ all Docker checks passed"

release: ## RDF 1.2 + OWL downcast → reasoned closure (HermiT) + build + regenerate + CrossRef deposit.
	$(GMEOW_DEV) regenerate
	$(GMEOW_DEV) reason --reasoner hermit --full
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

pull-images: ## Pre-pull the pinned Docker images (ROBOT, Jena).
	bash scripts/pull-images.sh

clean: ## Remove ephemeral build artifacts.
	rm -rf dist docs/_generated .stamps
	@echo "✓ cleaned"

clean-docs: ## Remove generated ontology docs (regenerate with make ontology-docs).
	rm -rf dist/ontology-docs ontology-docs
	@echo "✓ cleaned ontology docs"

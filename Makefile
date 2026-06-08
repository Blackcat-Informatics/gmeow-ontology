# GMEOW ontology toolchain — canonical task runner.
# Every target shells into the `gmeow` CLI (src/gmeow_tools) or the dev tools;
# no logic lives in this file. Run `make help` for the target list.

.DEFAULT_GOAL := help
SHELL := /bin/bash

# Alignment target for `make extract` (license-checked). Override: make extract TARGET=foaf
TARGET ?= foaf

# Checked-in generated artifacts (refreshed by make regenerate).
REGENERATED_PATHS := mappings/ projections/ queries/projections/ statements/ metadata/ apache/gmeow.conf llms.txt

# Override: make commit MESSAGE="feat: add foaf alignment"
MESSAGE ?= "chore: regenerate checked-in artifacts"

.PHONY: help install fmt lint validate reason reason-hermit explain verify extract compile-mappings \
        compile-check compile-statements statements-check compile-statements-pyoxigraph statements-check-pyoxigraph \
        mappings wikidata wikidata-live wikidata-coverage wikidata-audit lint-alignment refresh-target-axioms \
        metadata apache docs docs-full rdf12 rdf12-pyoxigraph quality normalize build export project test check \
        release regenerate commit clean pull-images

help: ## Show this help.
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

install: ## Sync the uv environment (runtime + dev deps).
	uv sync

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

validate: ## Validate syntax, term annotations, and SHACL (pure Python).
	uv run gmeow validate

reason: ## Merge, validate OWL 2 DL profile, and check ELK consistency (Docker).
	uv run gmeow reason --reasoner ELK

reason-hermit: ## Sound + complete consistency check with HermiT (Docker).
	uv run gmeow reason --reasoner hermit

explain: ## Explain any unsatisfiable classes (HermiT, Docker).
	uv run gmeow explain

verify: ## Reasoned-graph negative tests (ROBOT verify over queries/verify/, Docker).
	uv run gmeow verify --reasoner ELK

extract: ## Report import/extract policy for TARGET (refuses reference-only).
	uv run gmeow extract --target $(TARGET)

compile-mappings: ## Compile the mapping DSL → SSSOM + EDOAL + FnO + SPARQL (in-place).
	uv run gmeow compile-mappings

compile-check: ## Fail if the committed projection artifacts are stale vs the DSL.
	uv run gmeow compile-mappings --check

compile-statements: ## Compile statement-dsl/ → RDF 1.2 lead artifact + OWL downcast (Jena).
	uv run gmeow compile-statements

statements-check: ## Fail if the committed statement artifacts are stale vs statement-dsl/ (Jena).
	uv run gmeow compile-statements --check

compile-statements-pyoxigraph: ## Compile statement-dsl/ → RDF 1.2 + OWL downcast (pyoxigraph cross-check).
	uv run gmeow compile-statements-pyoxigraph

statements-check-pyoxigraph: ## Fail if pyoxigraph cross-check artifacts diverge from committed.
	uv run gmeow compile-statements-pyoxigraph --check

mappings-only: ## Build alignment axioms + VoID linksets (assumes SSSOM files present).
	uv run gmeow mappings

mappings: compile-mappings ## Build alignment axioms + VoID linksets from SSSOM; validate QID syntax.
	uv run gmeow mappings

lint-alignment: ## Lint SSSOM mappings for inverse / domain-range-mismatched targets (offline).
	uv run gmeow lint-alignment

refresh-target-axioms: ## Re-vendor minimal target-axiom snapshots (IMPORT_OK targets only).
	uv run gmeow refresh-target-axioms --target all

wikidata: ## Validate Wikidata QID/PID syntax in the mappings (offline).
	uv run gmeow wikidata

wikidata-live: ## Also verify Wikidata ids resolve (network).
	uv run gmeow wikidata --existence

wikidata-coverage: ## Report Wikidata mapping coverage by domain (offline).
	uv run gmeow wikidata-coverage

wikidata-audit: ## Audit fixtures and modules for Wikidata misuse (offline).
	uv run gmeow wikidata --fixtures

metadata: ## Generate VoID + DCAT dataset descriptions.
	uv run gmeow metadata

coverage: ## Report how much of the vendored entity slice GMEOW covers.
	uv run gmeow coverage --gaps

apache: ## Render the Apache content-negotiation include.
	uv run gmeow apache

crossref: ## Generate the CrossRef DOI deposit XML.
	uv run gmeow crossref

docs: ## Generate pyLODE HTML documentation.
	uv run gmeow docs

docs-full: ## Generate pyLODE + WIDOCO documentation (Docker).
	uv run gmeow docs --widoco

rdf12: ## Emit the RDF 1.2 / RDF* lead artifact + OWL downcast (alias of compile-statements; Jena).
	uv run gmeow rdf12

rdf12-pyoxigraph: ## Emit the RDF 1.2 / RDF* lead artifact + OWL downcast (pyoxigraph alias).
	uv run gmeow rdf12-pyoxigraph

quality: ## Run OOPS! pitfall scan (network, best-effort).
	uv run gmeow quality

normalize: ## Canonicalize the authored ontology sources (rewrites files).
	uv run gmeow normalize

build: ## Build all serializations + JSON-LD context + apache.conf into dist/.
	uv run gmeow build

export: ## Generate flattened exports (CSV/CSVW, Markdown, JSONL, llms.txt) into dist/ and copy llms.txt to root.
	uv run gmeow export
	cp dist/llms.txt llms.txt

project: compile-mappings ## Project GMEOW data to pure schema.org/GeoSPARQL/vCard/FOAF/iCal/OWL-Time profiles (FnO/EDOAL).
	uv run gmeow project

test: ## Run the test suite.
	uv run pytest -n auto

check: ## Full local quality gate (parallelized where safe).
	$(MAKE) -j$$(nproc 2>/dev/null || echo 4) lint validate statements-check statements-check-pyoxigraph compile-check wikidata coverage reason reason-hermit verify mappings-only lint-alignment
	$(MAKE) test
	@echo "✓ all checks passed"

release: ## RDF 1.2 + OWL downcast → reasoned closure (HermiT) + build + metadata + CrossRef deposit.
	uv run gmeow compile-statements
	uv run gmeow reason --reasoner hermit --full
	uv run gmeow build
	uv run gmeow metadata
	uv run gmeow crossref

regenerate: ## Rebuild all checked-in generated artifacts from canonical sources.
	$(MAKE) compile-mappings
	$(MAKE) compile-statements
	$(MAKE) metadata
	$(MAKE) apache
	$(MAKE) export

commit: regenerate ## Regenerate artifacts, stage them, and commit.
	git add $(REGENERATED_PATHS)
	@if git diff --cached --quiet; then \
		echo "Nothing to commit."; exit 1; \
	else \
		git commit -m "$(MESSAGE)"; \
	fi
	@git diff --quiet || echo "Warning: unstaged changes remain. Stage them with 'git add' and commit separately if needed."

pull-images: ## Pre-pull the pinned Docker images (ROBOT, WIDOCO, Jena).
	bash scripts/pull-images.sh

clean: ## Remove generated artifacts.
	rm -rf dist docs/_generated
	@echo "✓ cleaned"

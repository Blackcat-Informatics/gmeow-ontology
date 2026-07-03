# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
#
# Standalone classic-cross-check validator-zoo lane (NOT wired into the repo's
# make check / maint-* / CI). Cross-checks GMEOW's native reasoning against the
# classical ELK/HermiT/ROBOT/Jena/owlrl oracles. See README.md.
#
# It runs via the repository's own uv environment (`uv run --project ../..`) — a
# forward dependency on the built repo, exactly like the sibling
# openehr-bloodpressure lane reading ../../docs/.../fixtures. Nothing in the
# mainline references this lane.

# The relocated `oracles` package + `rdf12` module live at this directory root.
export PYTHONPATH := $(CURDIR)

UV := uv run --project ../..
GMEOW_DEV := cargo run -q --manifest-path ../../Cargo.toml -p gmeow-dev-cli --

.PHONY: help validate test reasoning-cases statements-check slme oracle-gold \
        classic classic-rl reason-docker verify-docker crosscheck-queries clean

help: ## Show the lane targets.
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
	  awk 'BEGIN{FS=":.*?## "}{printf "  %-18s %s\n", $$1, $$2}'

validate: reason-docker verify-docker reasoning-cases statements-check slme \
          crosscheck-queries classic classic-rl test ## Full non-required Docker/Java oracle lane. Prints agreement or a named divergence.
	@echo "classic cross-check oracle lane passed"

test: ## Docker-free suite unit tests (the Rust comparator / canonicalization / SARIF layers).
	$(UV) pytest tests -q -m "not classic_cross_check"

reason-docker: ## Reason the bundle under the ELK + HermiT oracles (Docker).
	$(GMEOW_DEV) reason --mode docker --reasoner ELK --exclude-tautologies structural
	$(GMEOW_DEV) reason --mode docker --reasoner hermit

verify-docker: ## Verify the ELK-reasoned graph (Docker).
	$(GMEOW_DEV) verify --mode docker --reasoner ELK --reasoned-input ../../dist/gmeow-reasoned-elk.ttl

reasoning-cases: ## HermiT/ROBOT reasoning fixture consistency cases.
	$(UV) python scripts/reasoning_cases.py

statements-check: ## Jena/ROBOT RDF-1.2 statement artifact oracle checks.
	$(UV) python scripts/statements_docker_check.py

slme: ## SLME module-extraction cross-check.
	$(UV) python scripts/slme_cross_check.py

oracle-gold: ## (Re)freeze the curated-DL oracle gold via HermiT/ELK (Docker).
	$(UV) python scripts/gen_dl_oracle_gold.py

crosscheck-queries: ## rdflib↔gmeow_rdf query-answer agreement (mainline gmeow-dev; the rdflib oracle).
	$(GMEOW_DEV) crosscheck-queries

classic: ## Enforced native↔ELK/HermiT divergence-ledger cross-check (Principle 18).
	$(UV) python scripts/run_classic_cross_check.py

classic-rl: ## Enforced native-RL ≡ owlrl-RL agreement axis.
	$(UV) python scripts/run_rl_agreement.py

clean: ## Remove per-run scratch.
	-rm -f result.txt

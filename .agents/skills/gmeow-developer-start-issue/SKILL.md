---
name: gmeow-developer-start-issue
description: >-
  Guides the agent in properly framing and beginning work on a GitHub issue in the GMEOW repository.
  Use when a new issue has been assigned, picked up, or needs analysis before implementation.
---

# GMEOW Developer — Starting a GitHub Issue

This skill governs how an agent transitions from "issue assigned" to "implementation ready" in the GMEOW ontology repository. It is a behavioural contract: follow it in order, do not skip steps, and cite the [CONSTITUTION.md](../../../CONSTITUTION.md) principles that govern every decision.

> **Meta-rule:** If any step below conflicts with a Constitution principle, the principle wins. Either revise the step or amend the Constitution in the same pull request — never silently override.

---

## 1. Before You Start — Read the Constitution

Every issue is a design decision. Before touching code, mappings, or ontology modules:

1. Open [`CONSTITUTION.md`](../../../CONSTITUTION.md) and read the twelve principles.
2. Identify which principles are most relevant to the issue at hand.
3. Cite the principle(s) by number (e.g. "Principle 4", "Principles 5 & 9") in your branch plan, commit messages, and eventual PR description.
4. If the issue appears to conflict with a principle, flag it **immediately** in the issue thread or PR description. A design that conflicts with a principle must either:
   - be revised to comply, **or**
   - ship together with an explicit amendment to `CONSTITUTION.md`.

> **Never silently override a principle.** Principle numbers are stable identifiers across history.

---

## 2. Plan Mode & Issue Transparency

For any non-trivial issue — new features, architectural changes, multi-file refactors, or ontology additions — enter plan mode before writing implementation code.

1. **ALL COMMENTS MUST BE READ** You must read all the github issue comments using the gh cli and JSON format.
2. **Write the plan** to the active plan file (the agent's designated plan path).
3. **Get approval** via `ExitPlanMode` before proceeding.
4. **Post the approved plan** as a comment on the GitHub issue.

> **Why:** The approved plan is the design contract. Posting it to the issue creates a transparent, time-stamped record that reviewers and future maintainers can reference. It prevents drift between "what was agreed" and "what was implemented."

When posting, include:

- A brief summary of the approach
- The key Constitution principle(s) that govern the design
- Any research findings (existing ontologies checked, partial matches found, etc.)
- A link to or verbatim paste of the plan content

If the plan is amended during implementation, post a follow-up comment explaining the delta and why it was necessary.

---

## 3. Worktree Discipline

GMEOW uses git worktrees for isolation. The repository root contains a `.worktrees/` directory (already gitignored). Follow this lifecycle exactly.

### 3.1 Create the worktree

```bash
# From the repository root (main checkout)
git fetch origin
issue_slug="issue-${ISSUE_NUMBER}-$(echo ${ISSUE_TITLE} | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/-*$//')"
git worktree add ".worktrees/${issue_slug}" -b "${issue_slug}"
cd ".worktrees/${issue_slug}"
```

- Branch name should be descriptive and include the issue number.
- Worktree directory lives under `.worktrees/` in the repo root.

### 3.2 Work all the way to the PR

Finish the entire plan, perform any checking required, then commit with a comprehensive commit message and PR.  Do not stop until you have PR'd the branch.

### 3.3 Keep the worktree alive until merge

The worktree and branch **must survive** until the associated pull request is merged into `main`. Do not delete them after the first commit, after opening the PR, or after initial review. They are the isolated workspace for the entire issue lifecycle.

### 3.4 After merge — clean up

Once the PR is merged and `main` is green:

```bash
# From the repository root (main checkout)
git worktree remove ".worktrees/${issue_slug}"
git branch -D "${issue_slug}"
git pull origin main
```

- Remove the worktree first, then delete the branch.
- Pull an updated `main` so the next issue starts from the latest canonical source.

---

## 4. Ontological Orientation — Projections, Logic, and Linkage

GMEOW is reasoning-centric, OWL 2 DL, and upper-ontology-grounded. Every issue must be analysed through these three lenses before implementation begins.

### 4.1 Projections (the boundary layer)

- If the issue touches cross-vocabulary alignment, the canonical source is `mapping-dsl/` — never hand-edit `mappings/*.sssom.tsv`, `projections/*.edoal.ttl`, or `queries/projections/*.rq`.
- A projection is a **deliberately lossy, directional downcast**. It lives at the boundary, not in the core.
- Before adding a new projection, ask: does this exist already in SSSOM, EDOAL, FnO, or SPARQL form? If not, plan the mapping-dsl source that generates it (Principle 4).

### 4.2 Logic (the reasoned core)

- Every new class or property must be grounded under a gUFO category and stay inside OWL 2 DL.
- Every definition must carry `rdfs:label` and `skos:definition`.
- Before implementation, predict whether the native reasoner (`make reason`) will stay green. If you are unsure, run a quick reason check on a spike branch first.
- If the issue involves statement-level metadata (provenance, confidence, temporal scope), the canonical source is `statement-dsl/` in RDF 1.2 / RDF* shape — the OWL axiom-annotation form is a generated downcast (Principles 2–3).

### 4.3 Linkage (maximal bridging by reference)

- Principle 5: mint exactly one canonical term per concept and align it to every relevant surface vocabulary by reference — never rewrite anyone else's data.
- Before minting a new term, search existing mappings and external vocabularies. The goal is maximal superset, maximal bridging.
- Link by IRI freely; never copy axioms from a reference-only or copyleft source.

---

## 5. Research First, Mint Second

**Researching existing ontologies, vocabularies, or standards that could augment or align with current work is ALWAYS a good idea.**

Before proposing a new GMEOW term or mapping, investigate:

- **Surface vocabularies:** schema.org, FOAF, vCard, GeoSPARQL, GEDCOM, DOAP, PROV-O, ORG, SKOS, DCTerms
- **Foundational / upper ontologies:** gUFO, BFO 2020, DOLCE
- **Domain ontologies:** FIBO (finance), OWL-Time (temporal), QUDT / OM (measurement), Lexvo (language)
- **Hubs & knowledge bases:** Wikidata (QIDs for entities, PIDs for properties)

Document your findings in the issue or PR — even if the conclusion is "nothing suitable exists; minting new." This audit trail is valuable for reviewers and future maintainers.

> **Tip:** If you find a partial match, model the GMEOW term correctly and bridge to the weaker form by reference (Principle 1: *SOTA by being SOTA* — model what should have been written, never inherit the weakness).

---

## 6. Git Workflow & Rebase Hygiene

### 6.1 Stay current with `main`

If `main` has moved ahead of your feature branch, **rebase carefully** before committing new work or before pushing:

```bash
git fetch origin
git rebase origin/main
```

- Prefer `git rebase main` over merge commits. GMEOW values linear, reviewable history.
- Resolve conflicts in the worktree, run the full gate (`make check`), and only then continue the rebase.

### 6.2 Force-push only when safe

After a rebase, force-push is acceptable **only on a personal feature branch** that no one else is working on:

```bash
git push --force-with-lease origin "${issue_slug}"
```

### 6.3 Conventional Commits

Use [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` — new functionality or ontology terms
- `fix:` — bug fixes
- `docs:` — documentation-only changes
- `refactor:` — internal restructuring without behaviour change
- `test:` — test additions or updates
- `chore:` — maintenance work

**Cite Constitution principle numbers in commit messages where relevant.** Example:

```text
feat: add Location reference-frame Profile and coarsenTo projection

Principle 11 (frame-relativity) + Principle 4 (canonical source):
- Canonical term authored in ontology/modules/location.ttl
- Projection authored in mapping-dsl/projections/geosparql.ttl
- Coarsening function added to projections/transforms.fno.ttl
```

---

## 7. Issue-to-PR Checklist

Before declaring an issue "ready to implement" or opening a pull request, verify:

- [ ] Constitution principle(s) identified and cited in issue/PR description
- [ ] Worktree created under `.worktrees/` on a descriptive branch
- [ ] Research conducted: existing ontologies/vocabularies checked before minting
- [ ] Projections planned in `mapping-dsl/` (not hand-edited in `mappings/` or `projections/`)
- [ ] Logic checked: OWL 2 DL, gUFO grounding, labels + definitions present
- [ ] Statement metadata planned in `statement-dsl/` if applicable (not hand-edited in `statements/`)
- [ ] Generated artifacts re-materialized by `make check` (its single producer does this) if any canonical source was changed
- [ ] Branch rebased onto latest `main` with no merge commits
- [ ] `make check` passes (full gate: lint, validate, compile-check, reason, verify, tests)
- [ ] Commit messages follow Conventional Commits and cite principles where relevant

---

## See Also

- [`gmeow-developer-workflows`](../../gmeow-developer-workflows/SKILL.md) — running `make` targets, formatting, tests
- [`gmeow-ontology-authoring`](../../gmeow-ontology-authoring/SKILL.md) — editing ontology modules, mappings, statements
- [`CONSTITUTION.md`](../../../CONSTITUTION.md) — the twelve normative principles
- [`CONTRIBUTING.md`](../../../CONTRIBUTING.md) — human-facing contribution guide

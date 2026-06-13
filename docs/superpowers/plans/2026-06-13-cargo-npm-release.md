# Cargo + npm GTS Release Setup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the Rust crate and TypeScript package to `gmeow-gts`, then add tag-triggered GitHub Actions workflows that publish to crates.io and npm.

**Architecture:** Follow the existing monorepo release pattern already used by `release-go-gts.yaml`: prefix-tags (`crates/gts/v*`, `ts/gts/v*`) trigger dedicated workflows with the minimum registry permissions. Cargo uses `cargo publish`; npm uses `npm publish` after a clean build.

**Tech Stack:** GitHub Actions, cargo, npm, crates.io, npm registry

---

## Task 1: Rename Rust crate `gts` → `gmeow-gts`

**Files:**

- Modify: `crates/gts/Cargo.toml:5` (rename + add explicit `[[bin]]` section)
- Modify: `Cargo.lock` (regenerate)
- Modify: `crates/gts/src/bin/gts.rs` (update `use gts::` → `use gmeow_gts::`, then `cargo fmt`)
- Modify: `crates/gts/tests/cli.rs` (update `use gts::` / `gts::` → `use gmeow_gts::`)
- Modify: `crates/gts/tests/conformance.rs` (update `use gts::` → `use gmeow_gts::`)

- [ ] **Step 1: Update crate name and declare binary explicitly**

  ```toml
  [package]
  name = "gmeow-gts"

  [[bin]]
  name = "gts"
  path = "src/bin/gts.rs"
  ```

- [ ] **Step 2: Regenerate lockfile**

  Run: `cargo update`
  Expected: `Cargo.lock` shows `name = "gmeow-gts"`

- [ ] **Step 3: Verify build, format, and tests pass**

  Run: `cargo test` then `cargo fmt --check`
  Expected: all tests pass and format gate is clean

- [ ] **Step 4: Commit"

  ```bash
  git add crates/gts/Cargo.toml crates/gts/src/bin/gts.rs crates/gts/tests/cli.rs crates/gts/tests/conformance.rs Cargo.lock
  git commit -m "feat(crates/gts): rename crate to gmeow-gts for registry release"
  ```

---

## Task 2: Add Cargo publish workflow

**Files:**

- Create: `.github/workflows/release-cargo-gts.yaml`

- [ ] **Step 1: Create workflow file**

  ```yaml
  name: Release Cargo gmeow-gts

  on:
    push:
      tags:
        - 'crates/gts/v*'

  permissions:
    contents: read

  jobs:
    publish:
      runs-on: ubuntu-latest
      steps:
        - name: Checkout
          uses: actions/checkout@df4cb1c069e1874edd31b4311f1884172cec0e10 # v6.0.3
          with:
            persist-credentials: false

        - name: Install Rust (stable)
          uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8 # stable

        - name: Verify package builds and tests
          run: cargo test -p gmeow-gts

        - name: Publish to crates.io
          run: cargo publish -p gmeow-gts --token ${{ secrets.CARGO_REGISTRY_TOKEN }}
  ```

- [ ] **Step 2: Commit**

  ```bash
  git add .github/workflows/release-cargo-gts.yaml
  git commit -m "ci: add crates.io publish workflow for gmeow-gts"
  ```

---

## Task 3: Rename npm package `@blackcatinformatics/gts` → `@blackcatinformatics/gmeow-gts`

**Files:**

- Modify: `ts/gts/package.json:2`
- Modify: `ts/gts/package-lock.json` (regenerate)

- [ ] **Step 1: Update package name**

  ```json
  {
    "name": "@blackcatinformatics/gmeow-gts",
    ...
  }
  ```

- [ ] **Step 2: Regenerate package-lock.json**

  Run: `cd ts/gts && rm -rf node_modules package-lock.json && npm install`
  Expected: `package-lock.json` reflects `@blackcatinformatics/gmeow-gts`

- [ ] **Step 3: Verify build and tests**

  Run: `cd ts/gts && npm test`
  Expected: build + tests pass

- [ ] **Step 4: Commit**

  ```bash
  git add ts/gts/package.json ts/gts/package-lock.json
  git commit -m "feat(ts/gts): rename npm package to @blackcatinformatics/gmeow-gts"
  ```

---

## Task 4: Add npm publish workflow

**Files:**

- Create: `.github/workflows/release-npm-gts.yaml`

- [ ] **Step 1: Create workflow file**

  ```yaml
  name: Release npm gmeow-gts

  on:
    push:
      tags:
        - 'ts/gts/v*'

  permissions:
    contents: read

  jobs:
    publish:
      runs-on: ubuntu-latest
      steps:
        - name: Checkout
          uses: actions/checkout@df4cb1c069e1874edd31b4311f1884172cec0e10 # v6.0.3
          with:
            persist-credentials: false

        - name: Set up Node.js
          uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4
          with:
            node-version-file: ts/gts/package.json
            registry-url: https://registry.npmjs.org

        - name: Install dependencies
          working-directory: ts/gts
          run: npm ci

        - name: Run tests
          working-directory: ts/gts
          run: npm test

        - name: Publish to npm
          working-directory: ts/gts
          run: npm publish --access public
          env:
            NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
  ```

- [ ] **Step 2: Commit**

  ```bash
  git add .github/workflows/release-npm-gts.yaml
  git commit -m "ci: add npm publish workflow for @blackcatinformatics/gmeow-gts"
  ```

---

## Task 5: Local cargo publish dry-run

**Files:** none

- [ ] **Step 1: Run dry-run**

  Run: `cargo publish -p gmeow-gts --dry-run --allow-dirty`
  Expected: passes with no errors

- [ ] **Step 2: If dry-run fails, fix and commit**

---

## Task 6: Local npm pack verification

**Files:** none

- [ ] **Step 1: Pack the package**

  Run: `cd ts/gts && npm pack`
  Expected: produces `blackcatinformatics-gmeow-gts-0.1.0.tgz`

- [ ] **Step 2: Inspect tarball contents**

  Run: `tar -tzf blackcatinformatics-gmeow-gts-0.1.0.tgz`
  Expected: contains `package/dist/index.js`, `package/dist/index.d.ts`, `package/dist/bin/gts.js`

- [ ] **Step 3: Clean up tarball**

  Run: `rm blackcatinformatics-gmeow-gts-0.1.0.tgz`

---

## Task 7: Final branch status and handoff

- [ ] **Step 1: Show summary diff**

  Run: `git log --oneline main..HEAD && git diff --stat main`

- [ ] **Step 2: Report ready for tag/push**

  Tell the user:

  - The branch is `release/cargo-npm-publishing` in `.worktrees/release/cargo-npm-publishing`
  - To release, push tags `crates/gts/v0.1.0` and `ts/gts/v0.1.0` after ensuring `secrets.CARGO_REGISTRY_TOKEN` and `secrets.NPM_TOKEN` are configured.

---

## Self-Review

**1. Spec coverage:** The request was to create a release branch and get cargo + npm release working. Every requirement maps to a task above: branch isolation (already done via worktree), crate rename, package rename, cargo workflow, npm workflow, local verification.

**2. Placeholder scan:** No TBD/TODO/fill-in-details found. All workflow steps, commands, and file paths are concrete.

**3. Type consistency:** Crate name `gmeow-gts` is used consistently in `Cargo.toml`, `cargo publish`, and the workflow. NPM package name `@blackcatinformatics/gmeow-gts` is used consistently in `package.json`, `package-lock.json`, and the workflow.

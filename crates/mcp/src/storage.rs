// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The storage seam — the one place `gmeow-mcp` touches persistence, and the reason
//! the engine is browser-runnable.
//!
//! Every write the MCP surface performs lands in exactly three places: the
//! grounded-memory claim package, the append-only conjecture library, and the
//! append-only candidate library. Every configuration read is an environment
//! variable. Before this module those were `std::fs` and `std::env` calls scattered
//! through `lib.rs`, which pinned the whole engine to a host with a filesystem — a
//! hard blocker for the wasm32 target the crate's whole boundary discipline exists to
//! reach.
//!
//! # The shape of the seam
//!
//! Three traits, one bundle:
//!
//! * [`ClaimStore`] — the grounded-memory triad (`store_claim` / `recall` /
//!   `revise_belief`) plus the tool-call trajectory and its audit segments.
//! * [`SegmentLibrary`] — an append-only GTS segment collection with an exclusive
//!   lock and an all-or-nothing replace. Both the conjecture library and the candidate
//!   library ARE one of these; the lock + whole-file replace is what makes a
//!   read → decide → append sequence atomic.
//! * [`Storage`] — the backend itself: environment lookup, the wall/logical clock, and
//!   the three stores. [`storage`] returns the process's backend.
//!
//! # The two backends, and why neither is a stub
//!
//! * **Native** ([`FsStorage`], `cfg(not(target_arch = "wasm32"))`) — real files at
//!   real paths, with the SAME `flock` sidecar, `tempfile` write-then-rename, and
//!   `GMEOW_*_PATH` / `HOME` / `USERPROFILE` resolution the engine has always used.
//!   The claim store delegates verbatim to `purrdf`'s `agent_memory::Memory`, so the
//!   on-disk `memory.gts` and every byte of native behaviour is unchanged.
//! * **Browser** ([`InMemoryStorage`]) — a REAL working store held in process memory.
//!   It is not a refusal and it is not a `Result::Err` factory: claims are stored,
//!   recalled with the same relevance ordering, revised, and superseded; tool calls
//!   are recorded; audit segment bytes are appended; libraries lock, read back, and
//!   replace. A browser session that stores a claim can recall it, and one that
//!   submits a candidate can list it.
//!
//! [`InMemoryStorage`] is compiled on EVERY target, not just wasm32. That is
//! deliberate: a backend nobody can test is a backend nobody can trust, and
//! `wasm32-unknown-unknown` has no test harness here. The native test suite exercises
//! the browser backend directly.
//!
//! # The clock
//!
//! `wasm32-unknown-unknown` has no ambient wall clock (`SystemTime::now` is not
//! implemented there), so the backend owns the timestamp: [`Storage::now_rfc3339`].
//! The native backend has none of its own — `purrdf`'s memory package stamps real UTC
//! internally, exactly as before. The browser backend stamps an explicitly LOGICAL
//! clock: a monotone counter rendered as an `xsd:dateTime` anchored at the Unix epoch
//! (`1970-01-01T00:00:00Z`, `…:01Z`, …). That is honest by construction — a reader
//! cannot mistake it for a wall time — and it preserves the only property the memory
//! package actually depends on, which is that later records stamp later instants.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use purrdf::gts::examples::agent_memory::{
    Claim, RecallOptions, RevisionOptions, StoreOptions, ToolCallOptions, ToolCallRecord,
};

use gmeow_errors::Result;

use crate::error::Mcp;

/// Raise a storage-layer defect as the crate's typed `mcp` diagnostic.
fn err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(Mcp {
        message: message.into(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// The traits
// ─────────────────────────────────────────────────────────────────────────────

/// The grounded-memory claim package: the append-only store behind `store_claim`,
/// `recall`, `revise_belief`, and the tool-call trajectory the audit segments key on.
///
/// The option/record types are `purrdf`'s, not re-declared here: the native backend
/// IS `purrdf`'s memory package, and a parallel set of structs would be a second
/// vocabulary for the same claims.
pub trait ClaimStore: Send + Sync {
    /// Append one claim, returning the stored record (its minted id and stamp).
    ///
    /// # Errors
    ///
    /// An empty claim, an out-of-range confidence, or a backend write failure.
    fn store_claim(&self, text: &str, options: StoreOptions<'_>) -> Result<Claim>;

    /// Append a value-wise suppression for `claim_id`, optionally naming the successor
    /// that supersedes it.
    ///
    /// # Errors
    ///
    /// A backend write failure.
    fn revise_claim(&self, claim_id: &str, options: RevisionOptions<'_>) -> Result<()>;

    /// Record one executed tool call on the trajectory, returning the stored record.
    ///
    /// # Errors
    ///
    /// A backend write failure.
    fn record_tool_call(&self, tool: &str, options: ToolCallOptions<'_>) -> Result<ToolCallRecord>;

    /// The claims matching `options.query`, best match first.
    ///
    /// # Errors
    ///
    /// A backend read failure.
    fn recall(&self, options: RecallOptions<'_>) -> Result<Vec<Claim>>;

    /// Every claim in storage order, suppressed ones included and flagged.
    ///
    /// # Errors
    ///
    /// A backend read failure.
    fn claims(&self) -> Result<Vec<Claim>>;

    /// Every recorded tool call in storage order.
    ///
    /// # Errors
    ///
    /// A backend read failure.
    fn tool_calls(&self) -> Result<Vec<ToolCallRecord>>;

    /// Append one already-serialized trajectory-audit GTS segment to the SAME package
    /// the claims live in, so a cold audit of the package verifies the executed turn.
    ///
    /// # Errors
    ///
    /// A backend write failure.
    fn append_audit_segment(&self, segment: &[u8]) -> Result<()>;
}

/// An exclusive hold on a [`SegmentLibrary`], released when the value is dropped.
///
/// A marker trait with no methods: the whole contract IS the lifetime. The native
/// implementation holds an `flock`ed sidecar file handle; the browser implementation
/// holds a mutex guard. Callers that must read-then-decide-then-append run the entire
/// sequence while one of these is alive, which is what closes the lost-update window.
pub trait LibraryLock {}

/// An append-only GTS segment collection with an exclusive lock and an
/// all-or-nothing replace — the shape both the conjecture library and the candidate
/// library have.
///
/// There is deliberately no `append` primitive. A commit assembles the WHOLE new
/// contents (current bytes + every new segment, in order) in memory and replaces the
/// library in one step, so a commit that writes more than one segment can never land
/// some of them and lose the rest.
pub trait SegmentLibrary: Send + Sync {
    /// The library's current bytes. A library that has never been written is EMPTY,
    /// not an error — a first-ever read of an untouched library is normal.
    ///
    /// # Errors
    ///
    /// A backend read failure other than "not yet written".
    fn read_bytes(&self) -> Result<Vec<u8>>;

    /// Replace the library's entire contents with `bytes`, all-or-nothing: either the
    /// whole new content lands, or the prior content is left completely untouched.
    ///
    /// # Errors
    ///
    /// A backend write failure.
    fn replace_bytes(&self, bytes: &[u8]) -> Result<()>;

    /// Take the library's exclusive lock, blocking until it is available. The lock is
    /// released when the returned value drops.
    ///
    /// # Errors
    ///
    /// A backend failure acquiring the lock.
    fn lock(&self) -> Result<Box<dyn LibraryLock + '_>>;
}

/// The process's persistence + configuration backend.
///
/// One trait rather than three free functions because the three stores and the
/// environment are ONE choice: a host either has a filesystem and an environment or it
/// does not, and mixing a real claim store with a synthetic environment would be a
/// backend nobody declared.
pub trait Storage: Send + Sync {
    /// The value of environment variable `key`, or `None` when it is unset or empty.
    ///
    /// An empty value reads as unset on purpose: every caller here treats an empty
    /// `GMEOW_*_PATH` as "not configured", and folding that into one place stops the
    /// three path resolvers from each re-deciding it.
    fn env_var(&self, key: &str) -> Option<String>;

    /// The instant to stamp on a record the ENGINE mints (the trajectory-audit
    /// segments). See the module docs for the browser backend's logical clock.
    fn now_rfc3339(&self) -> String;

    /// The grounded-memory claim package.
    ///
    /// # Errors
    ///
    /// A backend that cannot resolve or open its package (natively: neither `HOME` nor
    /// `USERPROFILE` set with `GMEOW_MEMORY_PATH` empty).
    fn claim_store(&self) -> Result<Arc<dyn ClaimStore>>;

    /// The append-only conjecture library.
    ///
    /// # Errors
    ///
    /// As [`claim_store`](Self::claim_store), for `GMEOW_CONJECTURE_PATH`.
    fn conjecture_library(&self) -> Result<Arc<dyn SegmentLibrary>>;

    /// The append-only candidate library.
    ///
    /// # Errors
    ///
    /// As [`claim_store`](Self::claim_store), for `GMEOW_CANDIDATE_PATH`.
    fn candidate_library(&self) -> Result<Arc<dyn SegmentLibrary>>;
}

/// The process's storage backend, selected by target at compile time: real files and
/// a real environment on a host that has them, an in-process store in the browser.
///
/// A `cfg` rather than a runtime switch because the choice is a property of the
/// TARGET, not of a call: a wasm image has no filesystem to fall back to and a native
/// host has no reason to pretend it lacks one.
#[must_use]
pub fn storage() -> &'static dyn Storage {
    #[cfg(not(target_arch = "wasm32"))]
    {
        static NATIVE: FsStorage = FsStorage;
        &NATIVE
    }
    #[cfg(target_arch = "wasm32")]
    {
        browser_storage()
    }
}

/// The browser backend's single process-wide instance.
///
/// One instance, so a claim stored by one tool call is recalled by the next — a fresh
/// store per call would be a store that forgets, which is not a store.
#[must_use]
pub fn browser_storage() -> &'static InMemoryStorage {
    static BROWSER: OnceLock<InMemoryStorage> = OnceLock::new();
    BROWSER.get_or_init(InMemoryStorage::new)
}

// ─────────────────────────────────────────────────────────────────────────────
// The native, filesystem-backed backend
// ─────────────────────────────────────────────────────────────────────────────

/// The native backend: real environment variables and real files, byte-for-byte the
/// behaviour the engine had before the seam existed.
#[cfg(not(target_arch = "wasm32"))]
pub struct FsStorage;

#[cfg(not(target_arch = "wasm32"))]
pub use native::{fs_claim_store, fs_segment_library};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::fs;
    use std::io::{self, Write as _};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use gmeow_errors::Result;
    use purrdf::gts::examples::agent_memory::{
        Claim, Memory, RecallOptions, RevisionOptions, StoreOptions, ToolCallOptions,
        ToolCallRecord,
    };

    use super::{ClaimStore, FsStorage, LibraryLock, SegmentLibrary, Storage, err};

    /// A native claim package at an EXPLICIT path, bypassing the environment resolution.
    ///
    /// The seam's normal entry is [`Storage::claim_store`], which reads
    /// `GMEOW_MEMORY_PATH`; this is for a caller that already knows the package it means
    /// (a launcher given an explicit `--memory` path, and the crate's own tests, which
    /// must address a temp package without mutating the process environment).
    ///
    /// # Errors
    ///
    /// If the package's parent directory cannot be created.
    pub fn fs_claim_store(path: impl Into<PathBuf>) -> Result<Arc<dyn ClaimStore>> {
        let path = path.into();
        ensure_parent(&path)?;
        Ok(Arc::new(FsClaimStore {
            memory: Memory::new(path.clone()),
            path,
        }))
    }

    /// A native append-only library at an EXPLICIT path. The [`fs_claim_store`] rationale,
    /// for the conjecture / candidate libraries.
    #[must_use]
    pub fn fs_segment_library(path: impl Into<PathBuf>) -> Arc<dyn SegmentLibrary> {
        Arc::new(FsSegmentLibrary { path: path.into() })
    }

    /// Expand a leading `~` / `~/` in a configured path against the home directory.
    /// A path with no `~`, or a host with no home, is returned unchanged.
    fn expand_home(raw: &str) -> PathBuf {
        if raw == "~" {
            return home_dir().map_or_else(|| PathBuf::from(raw), PathBuf::from);
        }
        if let Some(rest) = raw.strip_prefix("~/")
            && let Some(home) = home_dir()
        {
            return Path::new(&home).join(rest);
        }
        PathBuf::from(raw)
    }

    /// The user's home directory: `HOME`, else `USERPROFILE` (Windows).
    fn home_dir() -> Option<String> {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()
    }

    /// Resolve one of the three store paths: the override variable when it carries a
    /// non-empty value (home-expanded), else `~/.gmeow/<default_file>`. A host with no
    /// home and no override is a HARD FAIL naming both, never a silent fallback to a
    /// relative path in whatever directory the process happens to sit in.
    fn resolve_path(var: &str, default_file: &str) -> Result<PathBuf> {
        if let Some(raw) = FsStorage.env_var(var) {
            return Ok(expand_home(&raw));
        }
        let home = home_dir().ok_or_else(|| {
            err(format!(
                "neither HOME nor USERPROFILE is set and {var} is empty"
            ))
        })?;
        Ok(Path::new(&home).join(".gmeow").join(default_file))
    }

    /// Create the parent directory of `path`, so a first-ever write to a configured
    /// path under a not-yet-existing directory succeeds instead of failing on ENOENT.
    fn ensure_parent(path: &Path) -> Result<()> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    impl Storage for FsStorage {
        fn env_var(&self, key: &str) -> Option<String> {
            std::env::var(key)
                .ok()
                .filter(|value| !value.trim().is_empty())
        }

        fn now_rfc3339(&self) -> String {
            // The native claim package stamps its own real UTC instant on every record
            // it mints; the ENGINE-minted audit segment reuses the stamp the package
            // just produced rather than taking a second, slightly-later reading of the
            // clock (two stamps for one turn would be a false ordering). Nothing on the
            // native path calls this, and returning the package's own fallback keeps the
            // contract total.
            "1970-01-01T00:00:00Z".to_owned()
        }

        fn claim_store(&self) -> Result<Arc<dyn ClaimStore>> {
            let path = resolve_path("GMEOW_MEMORY_PATH", "memory.gts")?;
            ensure_parent(&path)?;
            Ok(Arc::new(FsClaimStore {
                memory: Memory::new(path.clone()),
                path,
            }))
        }

        fn conjecture_library(&self) -> Result<Arc<dyn SegmentLibrary>> {
            let path = resolve_path("GMEOW_CONJECTURE_PATH", "conjectures.gts")?;
            Ok(Arc::new(FsSegmentLibrary { path }))
        }

        fn candidate_library(&self) -> Result<Arc<dyn SegmentLibrary>> {
            let path = resolve_path("GMEOW_CANDIDATE_PATH", "candidates.gts")?;
            Ok(Arc::new(FsSegmentLibrary { path }))
        }
    }

    /// The native claim store: `purrdf`'s append-only memory package at `path`.
    ///
    /// Every method delegates, so the on-disk `memory.gts` and the claim algebra
    /// (content-addressed ids, RDF 1.2 reified annotations, the recall ranking) are the
    /// upstream implementation, not a copy of it.
    pub(super) struct FsClaimStore {
        pub(super) memory: Memory,
        /// The package path, retained so the audit segment appends to the SAME file the
        /// claims and tool calls live in.
        pub(super) path: PathBuf,
    }

    impl ClaimStore for FsClaimStore {
        fn store_claim(&self, text: &str, options: StoreOptions<'_>) -> Result<Claim> {
            Ok(self.memory.store(text, options)?)
        }

        fn revise_claim(&self, claim_id: &str, options: RevisionOptions<'_>) -> Result<()> {
            Ok(self.memory.revise(claim_id, options)?)
        }

        fn record_tool_call(
            &self,
            tool: &str,
            options: ToolCallOptions<'_>,
        ) -> Result<ToolCallRecord> {
            Ok(self.memory.record_tool_call(tool, options)?)
        }

        fn recall(&self, options: RecallOptions<'_>) -> Result<Vec<Claim>> {
            Ok(self.memory.recall(options)?)
        }

        fn claims(&self) -> Result<Vec<Claim>> {
            Ok(self.memory.claims()?)
        }

        fn tool_calls(&self) -> Result<Vec<ToolCallRecord>> {
            Ok(self.memory.tool_calls()?)
        }

        fn append_audit_segment(&self, segment: &[u8]) -> Result<()> {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?;
            file.write_all(segment)?;
            Ok(())
        }
    }

    /// The native append-only library: a GTS file plus a sidecar `.lock`.
    pub(super) struct FsSegmentLibrary {
        pub(super) path: PathBuf,
    }

    /// The sidecar advisory-lock path: the library path with a literal `.lock` suffix.
    ///
    /// The lock file's own bytes are never read; it exists solely as a stable
    /// `flock`/`LockFileEx` target that survives the library file being replaced out
    /// from under it by [`FsSegmentLibrary::replace_bytes`]'s atomic rename (an `flock`
    /// on the DATA file itself would be silently dropped by a rename-replace, since the
    /// lock is bound to the inode, not the path).
    fn lock_path(library_path: &Path) -> PathBuf {
        let mut os = library_path.as_os_str().to_owned();
        os.push(".lock");
        PathBuf::from(os)
    }

    /// The native lock: an `flock`ed file handle, unlocked on drop.
    struct FsLibraryLock {
        file: fs::File,
    }

    impl LibraryLock for FsLibraryLock {}

    impl Drop for FsLibraryLock {
        fn drop(&mut self) {
            // Releasing is best-effort: closing the handle drops the lock regardless,
            // and a failure here has no recovery the caller could perform.
            let _ = self.file.unlock();
        }
    }

    impl SegmentLibrary for FsSegmentLibrary {
        fn read_bytes(&self) -> Result<Vec<u8>> {
            match fs::read(&self.path) {
                Ok(bytes) => Ok(bytes),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
                Err(e) => Err(e.into()),
            }
        }

        fn replace_bytes(&self, bytes: &[u8]) -> Result<()> {
            // Same-directory temp file + fsync + atomic rename: a rename either lands
            // the WHOLE new file or leaves the PRIOR file completely untouched, so a
            // failure partway through can never leave a torn library.
            let dir = self
                .path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map_or_else(|| PathBuf::from("."), PathBuf::from);
            let mut tmp = tempfile::Builder::new()
                .prefix(".gmeow-library-")
                .suffix(".tmp")
                .tempfile_in(&dir)?;
            tmp.write_all(bytes)?;
            tmp.as_file().sync_all()?;
            tmp.persist(&self.path)
                .map_err(|e| err(format!("commit library {}: {e}", self.path.display())))?;
            Ok(())
        }

        fn lock(&self) -> Result<Box<dyn LibraryLock + '_>> {
            ensure_parent(&self.path)?;
            let file = fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(lock_path(&self.path))?;
            // Blocking exclusive lock (`flock(LOCK_EX)` / `LockFileEx` exclusive) — a
            // concurrent holder blocks here rather than racing past a TOCTOU window.
            file.lock()?;
            Ok(Box::new(FsLibraryLock { file }))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The in-process backend (the browser's real store)
// ─────────────────────────────────────────────────────────────────────────────

/// The browser backend: a REAL store held in process memory.
///
/// Compiled on every target so the native suite can prove it works; selected by
/// [`storage`] only on `wasm32`.
pub struct InMemoryStorage {
    /// The configuration environment. A browser host has no process environment, so it
    /// is one the host populates through [`InMemoryStorage::set_env`] (a launcher
    /// setting `GMEOW_LANG` from the page's locale, say). Unset means unset — the same
    /// answer a native host gives for a variable nobody exported.
    env: Mutex<BTreeMap<String, String>>,
    claims: Arc<InMemoryClaimStore>,
    conjectures: Arc<InMemorySegmentLibrary>,
    candidates: Arc<InMemorySegmentLibrary>,
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryStorage {
    /// An empty backend: no configuration, no claims, no library contents.
    #[must_use]
    pub fn new() -> Self {
        Self {
            env: Mutex::new(BTreeMap::new()),
            claims: Arc::new(InMemoryClaimStore::default()),
            conjectures: Arc::new(InMemorySegmentLibrary::default()),
            candidates: Arc::new(InMemorySegmentLibrary::default()),
        }
    }

    /// Set one configuration value — the browser host's equivalent of exporting an
    /// environment variable before launching the server.
    ///
    /// # Panics
    ///
    /// If the configuration lock was poisoned by a panic in another thread.
    pub fn set_env(&self, key: &str, value: &str) {
        self.env
            .lock()
            .expect("in-memory storage environment lock")
            .insert(key.to_owned(), value.to_owned());
    }
}

impl Storage for InMemoryStorage {
    fn env_var(&self, key: &str) -> Option<String> {
        self.env
            .lock()
            .expect("in-memory storage environment lock")
            .get(key)
            .filter(|value| !value.trim().is_empty())
            .cloned()
    }

    fn now_rfc3339(&self) -> String {
        self.claims.tick()
    }

    fn claim_store(&self) -> Result<Arc<dyn ClaimStore>> {
        Ok(Arc::clone(&self.claims) as Arc<dyn ClaimStore>)
    }

    fn conjecture_library(&self) -> Result<Arc<dyn SegmentLibrary>> {
        Ok(Arc::clone(&self.conjectures) as Arc<dyn SegmentLibrary>)
    }

    fn candidate_library(&self) -> Result<Arc<dyn SegmentLibrary>> {
        Ok(Arc::clone(&self.candidates) as Arc<dyn SegmentLibrary>)
    }
}

/// One claim revision: the suppression, its reason, and the successor that supersedes
/// the retired claim. Retained (rather than collapsed into a boolean) because the
/// revision record IS part of the grounded memory — the native package writes it as
/// annotations on the suppression, and a browser session that revises a belief must be
/// able to say why.
#[derive(Clone)]
struct Revision {
    claim_id: String,
    reason: Option<String>,
    superseded_by: Option<String>,
}

/// The browser claim store's whole state.
#[derive(Default)]
struct ClaimState {
    /// Claims in storage (append) order — the order `recall` scores over.
    claims: Vec<Claim>,
    /// Claim ids retired by a revision.
    suppressed: BTreeSet<String>,
    /// Every revision in application order.
    revisions: Vec<Revision>,
    /// Recorded tool calls in storage order.
    calls: Vec<ToolCallRecord>,
    /// The concatenated trajectory-audit GTS segment bytes, kept verbatim so the
    /// browser's trajectory is a real auditable byte stream rather than a discard.
    audit: Vec<u8>,
    /// The monotone logical clock / id counter. See the module docs.
    seq: u64,
}

/// The browser's grounded-memory claim package.
///
/// # Why this is an implementation and not a copy
///
/// The native store IS `purrdf`'s memory package, whose claim algebra is expressed
/// over GTS segment bytes: it exists to make an append-only FILE auditable. In process
/// memory there is no file to make auditable, so re-encoding claims to GTS just to
/// decode them again on the next `recall` would be ceremony, not fidelity. What must
/// agree between the two backends is the OBSERVABLE contract — storage order, the
/// suppression rule, the confidence filter, and the relevance ranking — and that
/// contract is asserted against both backends by the crate's tests rather than assumed.
#[derive(Default)]
pub struct InMemoryClaimStore {
    state: Mutex<ClaimState>,
}

impl InMemoryClaimStore {
    /// Advance the logical clock and render the new instant.
    fn tick(&self) -> String {
        let mut state = self.state.lock().expect("in-memory claim store lock");
        state.seq += 1;
        logical_instant(state.seq)
    }

    /// Every revision recorded so far, in application order — the browser twin of the
    /// suppression annotations the native package writes into `memory.gts`.
    ///
    /// # Panics
    ///
    /// If the store lock was poisoned by a panic in another thread.
    #[must_use]
    pub fn revisions(&self) -> Vec<(String, Option<String>, Option<String>)> {
        self.state
            .lock()
            .expect("in-memory claim store lock")
            .revisions
            .iter()
            .map(|r| (r.claim_id.clone(), r.reason.clone(), r.superseded_by.clone()))
            .collect()
    }

    /// The accumulated trajectory-audit segment bytes.
    ///
    /// # Panics
    ///
    /// If the store lock was poisoned by a panic in another thread.
    #[must_use]
    pub fn audit_bytes(&self) -> Vec<u8> {
        self.state
            .lock()
            .expect("in-memory claim store lock")
            .audit
            .clone()
    }
}

/// The logical instant for sequence number `seq`: `seq` seconds after the Unix epoch,
/// rendered as an `xsd:dateTime`. Anchoring at the epoch is what makes the stamp
/// self-identifying as logical rather than a plausible-looking fake wall time.
fn logical_instant(seq: u64) -> String {
    let days = seq / 86_400;
    let rest = seq % 86_400;
    let (h, m, s) = (rest / 3600, (rest % 3600) / 60, rest % 60);
    // Day 0 is 1970-01-01; the counter is a session-scoped monotone sequence, so the
    // date only advances after 86_400 records and never leaves 1970 in practice.
    let day = days + 1;
    format!("1970-01-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// A deterministic opaque id for an in-memory record: the store kind, the sequence
/// number, and the record's identifying parts folded with SHA-256.
///
/// Content-addressed like the native package's ids (which fold blake3 over the file
/// length and the same parts) and, like them, opaque to every caller — the sequence
/// number is in the fold so two identical claims stored twice are still distinct
/// records.
fn record_id(kind: &str, seq: u64, parts: &[&str]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update(seq.to_le_bytes());
    for part in parts {
        hasher.update([0u8]);
        hasher.update(part.as_bytes());
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    format!("urn:gmeow:mcp:{kind}:sha256:{hex}")
}

impl ClaimStore for InMemoryClaimStore {
    fn store_claim(&self, text: &str, options: StoreOptions<'_>) -> Result<Claim> {
        // The SAME two input rules the native package enforces, refused here rather
        // than stored and discovered later.
        if text.trim().is_empty() {
            return Err(err("store_claim: the claim text is empty"));
        }
        if let Some(confidence) = options.confidence
            && (!confidence.is_finite() || !(0.0..=1.0).contains(&confidence))
        {
            return Err(err(format!(
                "store_claim: confidence {confidence} is outside the inclusive range 0.0..=1.0"
            )));
        }

        let mut state = self.state.lock().expect("in-memory claim store lock");
        state.seq += 1;
        let seq = state.seq;
        let created = logical_instant(seq);
        let confidence_text = options.confidence.map(|value| value.to_string());
        let id = record_id(
            "claim",
            seq,
            &[
                text,
                created.as_str(),
                options.source.unwrap_or(""),
                confidence_text.as_deref().unwrap_or(""),
                options.according_to.unwrap_or(""),
            ],
        );
        let claim = Claim {
            id,
            text: text.to_owned(),
            confidence: options.confidence,
            according_to: options.according_to.map(str::to_owned),
            source: options.source.map(str::to_owned),
            created: Some(created),
            suppressed: false,
        };
        state.claims.push(claim.clone());
        Ok(claim)
    }

    fn revise_claim(&self, claim_id: &str, options: RevisionOptions<'_>) -> Result<()> {
        let mut state = self.state.lock().expect("in-memory claim store lock");
        state.seq += 1;
        state.suppressed.insert(claim_id.to_owned());
        state.revisions.push(Revision {
            claim_id: claim_id.to_owned(),
            reason: options.reason.map(str::to_owned),
            superseded_by: options.superseded_by.map(str::to_owned),
        });
        Ok(())
    }

    fn record_tool_call(&self, tool: &str, options: ToolCallOptions<'_>) -> Result<ToolCallRecord> {
        let mut state = self.state.lock().expect("in-memory claim store lock");
        state.seq += 1;
        let seq = state.seq;
        let created = logical_instant(seq);
        let id = record_id(
            "call",
            seq,
            &[
                tool,
                created.as_str(),
                options.arguments.unwrap_or(""),
                options.result.unwrap_or(""),
                options.invocation.unwrap_or(""),
            ],
        );
        let record = ToolCallRecord {
            id,
            tool: tool.to_owned(),
            arguments: options.arguments.map(str::to_owned),
            result: options.result.map(str::to_owned),
            invocation: options.invocation.map(str::to_owned),
            created: Some(created),
            generated: options.generated.iter().map(|g| (*g).to_owned()).collect(),
        };
        state.calls.push(record.clone());
        Ok(record)
    }

    fn recall(&self, options: RecallOptions<'_>) -> Result<Vec<Claim>> {
        // The ranking is the native package's, term for term: filter by suppression and
        // by the confidence floor; with no query terms return storage order REVERSED
        // (most recent first); otherwise score by token overlap and order by
        // (score desc, storage index desc), dropping every zero-overlap claim.
        let mut claims: Vec<Claim> = self
            .claims()?
            .into_iter()
            .filter(|claim| options.include_suppressed || !claim.suppressed)
            .filter(|claim| match options.min_confidence {
                None => true,
                Some(min) => claim.confidence.is_some_and(|got| got >= min),
            })
            .collect();

        let tokens: HashSet<String> = options
            .query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        if tokens.is_empty() {
            claims.reverse();
        } else {
            let mut scored: Vec<(usize, usize, Claim)> = claims
                .into_iter()
                .enumerate()
                .map(|(index, claim)| {
                    let claim_tokens: HashSet<String> = claim
                        .text
                        .to_lowercase()
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect();
                    let score = tokens.intersection(&claim_tokens).count();
                    (score, index, claim)
                })
                .filter(|(score, _, _)| *score > 0)
                .collect();
            scored.sort_by_key(|(score, index, _)| {
                (std::cmp::Reverse(*score), std::cmp::Reverse(*index))
            });
            claims = scored.into_iter().map(|(_, _, claim)| claim).collect();
        }
        claims.truncate(options.limit);
        Ok(claims)
    }

    fn claims(&self) -> Result<Vec<Claim>> {
        let state = self.state.lock().expect("in-memory claim store lock");
        Ok(state
            .claims
            .iter()
            .map(|claim| {
                let mut claim = claim.clone();
                claim.suppressed = state.suppressed.contains(&claim.id);
                claim
            })
            .collect())
    }

    fn tool_calls(&self) -> Result<Vec<ToolCallRecord>> {
        Ok(self
            .state
            .lock()
            .expect("in-memory claim store lock")
            .calls
            .clone())
    }

    fn append_audit_segment(&self, segment: &[u8]) -> Result<()> {
        self.state
            .lock()
            .expect("in-memory claim store lock")
            .audit
            .extend_from_slice(segment);
        Ok(())
    }
}

/// The browser's append-only segment library: the same bytes the native backend would
/// hold in a file, held in process memory instead.
#[derive(Default)]
pub struct InMemorySegmentLibrary {
    /// The library's bytes. Empty means "never written" — the same thing an absent
    /// file means natively.
    bytes: Mutex<Vec<u8>>,
    /// The exclusive lock, held for the whole read → decide → replace sequence. A
    /// SEPARATE mutex from `bytes` so a caller holding the library lock can still read
    /// and replace through it; sharing one mutex would deadlock the very sequence the
    /// lock exists to protect.
    gate: Mutex<()>,
}

/// The browser lock: a mutex guard, released on drop.
struct InMemoryLibraryLock<'a> {
    _guard: std::sync::MutexGuard<'a, ()>,
}

impl LibraryLock for InMemoryLibraryLock<'_> {}

impl SegmentLibrary for InMemorySegmentLibrary {
    fn read_bytes(&self) -> Result<Vec<u8>> {
        Ok(self
            .bytes
            .lock()
            .expect("in-memory segment library lock")
            .clone())
    }

    fn replace_bytes(&self, bytes: &[u8]) -> Result<()> {
        // Wholesale replacement under one lock IS the all-or-nothing guarantee here:
        // there is no partial write for a reader to observe.
        *self.bytes.lock().expect("in-memory segment library lock") = bytes.to_vec();
        Ok(())
    }

    fn lock(&self) -> Result<Box<dyn LibraryLock + '_>> {
        let guard = self
            .gate
            .lock()
            .map_err(|_| err("in-memory segment library lock was poisoned by a panic"))?;
        Ok(Box::new(InMemoryLibraryLock { _guard: guard }))
    }
}

// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One dependency-leaf authority for deterministic action keys and bounded immutable
//! receipt/blob storage.
//!
//! Callers own product semantics and codecs. This crate owns the generic mechanics:
//! canonical contexts, SHA-256 action identities, bounded reads, self-digested
//! receipts, immutable content-addressed blobs, atomic publication, process-wide
//! build election, reader-safe reachability GC, and explicit store quotas.

pub mod executable;
pub mod selection;

mod context_json;
mod receipt_json;

pub use receipt_json::EmptyPayload;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};

use purrdf_lex::json::{
    self,
    record::{FromJson, ToJson},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Schema of the common action receipt envelope.
pub const RECEIPT_SCHEMA_VERSION: u32 = 1;

/// Physical receipt/blob store layout. Product codecs and domain schemas belong in
/// [`ActionContext`]; every caller opens this same version so independent DAG domains
/// share one quota and one reachability collector.
pub const STORE_FORMAT_VERSION: u32 = 1;

/// A compact error kept independent of every caller's diagnostic substrate.
#[derive(Debug)]
pub struct ActionCacheError(ActionCacheFailure);

#[derive(Debug)]
enum ActionCacheFailure {
    Message(String),
    Io(io::Error),
}

impl ActionCacheError {
    #[must_use]
    pub fn message(message: impl Into<String>) -> Self {
        Self(ActionCacheFailure::Message(message.into()))
    }
}

impl std::fmt::Display for ActionCacheError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            ActionCacheFailure::Message(message) => formatter.write_str(message),
            ActionCacheFailure::Io(error) => write!(formatter, "action cache I/O: {error}"),
        }
    }
}

impl std::error::Error for ActionCacheError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            ActionCacheFailure::Message(_) => None,
            ActionCacheFailure::Io(error) => Some(error),
        }
    }
}

impl From<io::Error> for ActionCacheError {
    fn from(error: io::Error) -> Self {
        Self(ActionCacheFailure::Io(error))
    }
}

impl From<serde_json::Error> for ActionCacheError {
    fn from(error: serde_json::Error) -> Self {
        Self::message(format!("action cache JSON: {error}"))
    }
}

/// Exact producer/toolchain unit used by an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducerIdentity {
    pub digest: String,
    pub toolchain: Option<String>,
    pub target: Option<String>,
    pub profile: Option<String>,
    pub features: Vec<String>,
}

impl ProducerIdentity {
    #[must_use]
    pub fn new(digest: impl Into<String>) -> Self {
        Self {
            digest: digest.into(),
            toolchain: None,
            target: None,
            profile: None,
            features: Vec::new(),
        }
    }

    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.features.sort();
        self.features.dedup();
        self
    }
}

/// Filesystem kind authenticated by a raw action input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileKind {
    File,
    Symlink,
    Aggregate,
}

/// One typed input to an action key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ActionInput {
    Upstream {
        producer: String,
        entity: Option<String>,
        /// Authenticated upstream receipt when the caller has one. A live DAG
        /// product may be identified directly by `product_digest`; inventing a
        /// receipt digest in that case would make the action context lie.
        receipt_digest: Option<String>,
        product_digest: String,
    },
    Raw {
        logical_path: String,
        file_kind: FileKind,
        executable: bool,
        digest: String,
    },
}

/// Complete canonical identity of one executable action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionContext {
    pub schema_version: u32,
    pub domain: String,
    pub action: String,
    pub implementation: ProducerIdentity,
    pub codec: String,
    pub inputs: Vec<ActionInput>,
    pub dimensions: BTreeMap<String, String>,
}

impl ActionContext {
    #[must_use]
    pub fn new(
        domain: impl Into<String>,
        action: impl Into<String>,
        implementation: ProducerIdentity,
        codec: impl Into<String>,
        mut inputs: Vec<ActionInput>,
    ) -> Self {
        inputs.sort();
        inputs.dedup();
        Self {
            schema_version: RECEIPT_SCHEMA_VERSION,
            domain: domain.into(),
            action: action.into(),
            implementation: implementation.normalized(),
            codec: codec.into(),
            inputs,
            dimensions: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_dimension(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.dimensions.insert(name.into(), value.into());
        self
    }

    #[must_use]
    pub fn key(&self) -> ActionKey {
        use purrdf_lex::json::{self, record::ToJson as _};

        let value = self.to_json();
        let mut resident = purrdf_lex::allocation::Resident;
        let mut memory = purrdf_lex::allocation::Memory::new(&mut resident);
        let encoded = json::write_with_memory(&value, json::Format::COMPACT, &mut memory)
            .expect("closed native ActionContext JSON");
        ActionKey(content_digest(&[
            b"gmeow:action-key:v1",
            encoded.as_bytes(),
        ]))
    }
}

/// SHA-256 identity of an [`ActionContext`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionKey(String);

impl ActionKey {
    pub fn from_hex(value: impl Into<String>) -> Result<Self, ActionCacheError> {
        let value = value.into();
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ActionCacheError::message(format!(
                "action key is not a lowercase/uppercase 64-digit hex digest: {value:?}"
            )));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ActionKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One immutable content-addressed blob.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRef {
    pub digest: String,
    pub bytes: u64,
}

/// Common receipt plus a caller-owned semantic payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionReceipt<P> {
    pub schema_version: u32,
    pub action_key: ActionKey,
    pub context: ActionContext,
    pub product_digest: String,
    pub product_blob: BlobRef,
    pub payload: P,
}

impl<P: ToJson> ActionReceipt<P> {
    #[must_use]
    pub fn digest(&self) -> String {
        let mut value = self.to_json();
        json::record::sorted_last_wins(&mut value);
        let bytes = json::record::to_vec(&value);
        content_digest(&[b"gmeow:action-receipt:v1", &bytes])
    }
}

#[derive(Debug, Clone)]
struct ReceiptEnvelope<P> {
    receipt_digest: String,
    receipt: ActionReceipt<P>,
}

/// One fully verified action entry.
#[derive(Debug)]
pub struct VerifiedEntry<P> {
    pub receipt: ActionReceipt<P>,
    pub bytes: Vec<u8>,
}

/// Explicit storage limits; eviction is a miss policy, never a correctness switch.
#[derive(Debug, Clone, Copy)]
pub struct StoreLimits {
    pub max_entry_bytes: u64,
    pub max_receipt_bytes: u64,
    pub max_entries: usize,
    pub max_total_bytes: u64,
}

/// Shared storage budget for the worktree-local action namespace.
pub const DEFAULT_MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// A single complete action may use the store's byte budget. A separate bound
/// based on a historical product size must not reject a valid growing compiler
/// publication that fits the selected store. Producers and read-only consumers
/// use the same admission contract.
pub const DEFAULT_MAX_ENTRY_BYTES: u64 = DEFAULT_MAX_TOTAL_BYTES;

/// Shared receipt bound for the one action-store namespace. Pipeline receipts retain
/// the exact path and digest of every declared raw input; the conformance producer's
/// broad corpus census can exceed 4 MiB without carrying product bytes. Sixteen MiB
/// keeps that complete action identity bounded independently of product bytes.
pub const DEFAULT_MAX_RECEIPT_BYTES: u64 = 16 * 1024 * 1024;

impl Default for StoreLimits {
    fn default() -> Self {
        Self {
            max_entry_bytes: DEFAULT_MAX_ENTRY_BYTES,
            max_receipt_bytes: DEFAULT_MAX_RECEIPT_BYTES,
            // The producer DAG carries one independently reusable receipt per slice
            // specification in addition to pipeline stages. Retain several source
            // generations under the unchanged byte ceiling so a one-slice edit does
            // not evict the rest of the corpus-wide warm frontier.
            max_entries: 4_096,
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
        }
    }
}

/// One reader-safe immutable action store.
pub struct ActionStore {
    dir: PathBuf,
    limits: StoreLimits,
    inert: bool,
    read_only: bool,
    _lease: Option<File>,
}

struct LockGuard(File);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

impl ActionStore {
    /// Conventional shared authority under a repository root.
    #[must_use]
    pub fn default_root(repo_root: &Path) -> PathBuf {
        repo_root.join(".cache").join("gmeow-sync").join("actions")
    }

    pub fn open(
        root: impl Into<PathBuf>,
        format_version: u32,
        limits: StoreLimits,
    ) -> Result<Self, ActionCacheError> {
        if format_version == 0
            || limits.max_entry_bytes == 0
            || limits.max_receipt_bytes == 0
            || limits.max_entries == 0
            || limits.max_total_bytes == 0
            || limits.max_entry_bytes > limits.max_total_bytes
        {
            return Err(ActionCacheError::message(
                "action-store format and every quota must be positive, with entry bytes no larger than total bytes",
            ));
        }
        let root = root.into();
        fs::create_dir_all(&root)?;
        ensure_real_directory(&root, "action-store root")?;
        let root_guard = open_file_lock(&root.join(".action-cache.lock"))?;
        root_guard.lock()?;
        validate_store_root(&root)?;
        let sentinel = root.join(".gmeow-action-cache-v1");
        publish_identical(
            &sentinel,
            b"gmeow-action-cache:v1\n",
            limits.max_receipt_bytes,
        )?;
        let dir = root.join(format!("v{format_version}"));
        fs::create_dir_all(&dir)?;
        ensure_real_directory(&dir, "action-store version root")?;
        for lane in ["blobs", "receipts", "locks", "elections"] {
            let lane = dir.join(lane);
            fs::create_dir_all(&lane)?;
            ensure_real_directory(&lane, "action-store lane")?;
        }
        let lease = open_file_lock(&dir.join(".lease.lock"))?;
        validate_version_root(&dir)?;
        prune_obsolete_version_roots(&root, &dir)?;
        lease.lock_shared()?;
        root_guard.unlock()?;
        let store = Self {
            dir,
            limits,
            inert: false,
            read_only: false,
            _lease: Some(lease),
        };
        // An outer CI cache restore can materialize more entries than the current
        // limits without executing a publication. Enforce the bound on admission so
        // an all-hit process cannot leave an oversized store indefinitely.
        if store.quota_exceeded()? {
            store.prune(None)?;
        }
        Ok(store)
    }

    /// Open an already-admitted action store without creating, locking, pruning, or
    /// publishing any filesystem entry.
    ///
    /// This is the consumer authority used after the producer DAG has completed. A
    /// missing root, sentinel, version directory, or lane is an error rather than an
    /// initialization request. The producer/consumer DAG excludes concurrent mutation,
    /// so immutable receipt and blob reads need no writer-election files.
    pub fn open_existing_read_only(
        root: impl Into<PathBuf>,
        format_version: u32,
        limits: StoreLimits,
    ) -> Result<Self, ActionCacheError> {
        Self::open_existing(root, format_version, limits, true)
    }

    /// Join an already initialized producer store without repeating root admission,
    /// obsolete-version pruning, or quota census while sibling workers publish.
    ///
    /// The coordinating producer must call [`Self::open`] first. This child-worker
    /// path validates every owned directory and sentinel, then takes the version lease;
    /// it may publish through the normal store/action locks but cannot initialize a
    /// missing store. Avoiding the admission census matters because atomic publishers
    /// legitimately create and rename temporary lane entries while a cold DAG fans out.
    pub fn open_existing_writable(
        root: impl Into<PathBuf>,
        format_version: u32,
        limits: StoreLimits,
    ) -> Result<Self, ActionCacheError> {
        Self::open_existing(root, format_version, limits, false)
    }

    fn open_existing(
        root: impl Into<PathBuf>,
        format_version: u32,
        limits: StoreLimits,
        read_only: bool,
    ) -> Result<Self, ActionCacheError> {
        if format_version == 0
            || limits.max_entry_bytes == 0
            || limits.max_receipt_bytes == 0
            || limits.max_entries == 0
            || limits.max_total_bytes == 0
            || limits.max_entry_bytes > limits.max_total_bytes
        {
            return Err(ActionCacheError::message(
                "action-store format and every quota must be positive, with entry bytes no larger than total bytes",
            ));
        }
        let root = root.into();
        ensure_real_directory(&root, "action-store root")?;
        validate_store_root(&root)?;
        let sentinel = read_bounded(
            &root.join(".gmeow-action-cache-v1"),
            limits.max_receipt_bytes,
            "action-store sentinel",
        )?;
        if sentinel != b"gmeow-action-cache:v1\n" {
            return Err(ActionCacheError::message(
                "action-store sentinel identity mismatch",
            ));
        }
        let dir = root.join(format!("v{format_version}"));
        ensure_real_directory(&dir, "action-store version root")?;
        for lane in ["blobs", "receipts", "locks", "elections"] {
            ensure_real_directory(&dir.join(lane), "action-store lane")?;
        }
        validate_version_root(&dir)?;
        let lease = if read_only {
            None
        } else {
            let lease = open_file_lock(&dir.join(".lease.lock"))?;
            lease.lock_shared()?;
            Some(lease)
        };
        Ok(Self {
            dir,
            limits,
            inert: false,
            read_only,
            _lease: lease,
        })
    }

    #[must_use]
    pub fn inert() -> Self {
        Self {
            dir: PathBuf::new(),
            limits: StoreLimits {
                max_entry_bytes: 0,
                max_receipt_bytes: 0,
                max_entries: 0,
                max_total_bytes: 0,
            },
            inert: true,
            read_only: false,
            _lease: None,
        }
    }

    #[must_use]
    pub fn with_limits(mut self, limits: StoreLimits) -> Self {
        self.limits = limits;
        self
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.dir
    }

    #[must_use]
    pub fn receipt_path(&self, key: &ActionKey) -> PathBuf {
        self.dir.join("receipts").join(format!("{key}.json"))
    }

    #[must_use]
    pub fn blob_path(&self, digest: &str) -> PathBuf {
        self.dir.join("blobs").join(digest)
    }

    pub fn get<P>(
        &self,
        context: &ActionContext,
    ) -> Result<Option<VerifiedEntry<P>>, ActionCacheError>
    where
        P: FromJson + ToJson,
    {
        if self.inert {
            return Ok(None);
        }
        let key = context.key();
        if self.read_only {
            return self.get_unlocked(context, &key);
        }
        let _store = self.lock_store(false)?;
        let _action = self.lock_action(&key, false)?;
        self.get_unlocked(context, &key)
    }

    fn get_unlocked<P>(
        &self,
        context: &ActionContext,
        key: &ActionKey,
    ) -> Result<Option<VerifiedEntry<P>>, ActionCacheError>
    where
        P: FromJson + ToJson,
    {
        let path = self.receipt_path(key);
        if !path_entry_exists(&path)? {
            return Ok(None);
        }
        let receipt = self.read_receipt::<P>(&path)?;
        self.validate_identity(context, &receipt)?;
        let bytes = self.read_blob(&receipt.product_blob)?;
        Ok(Some(VerifiedEntry { receipt, bytes }))
    }

    pub fn inspect<P>(
        &self,
        context: &ActionContext,
    ) -> Result<Option<ActionReceipt<P>>, ActionCacheError>
    where
        P: FromJson + ToJson,
    {
        if self.inert {
            return Ok(None);
        }
        let key = context.key();
        if self.read_only {
            return self.inspect_unlocked(context, &key);
        }
        let _store = self.lock_store(false)?;
        let _action = self.lock_action(&key, false)?;
        self.inspect_unlocked(context, &key)
    }

    fn inspect_unlocked<P>(
        &self,
        context: &ActionContext,
        key: &ActionKey,
    ) -> Result<Option<ActionReceipt<P>>, ActionCacheError>
    where
        P: FromJson + ToJson,
    {
        let path = self.receipt_path(key);
        if !path_entry_exists(&path)? {
            return Ok(None);
        }
        let receipt = self.read_receipt::<P>(&path)?;
        self.validate_identity(context, &receipt)?;
        self.verify_blob(&receipt.product_blob)?;
        Ok(Some(receipt))
    }

    pub fn publish<P>(
        &self,
        context: &ActionContext,
        product_digest: impl Into<String>,
        payload: P,
        bytes: &[u8],
    ) -> Result<ActionReceipt<P>, ActionCacheError>
    where
        P: Clone + FromJson + PartialEq + ToJson,
    {
        self.require_writable()?;
        let byte_count = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if byte_count > self.limits.max_entry_bytes || byte_count > self.limits.max_total_bytes {
            return Err(ActionCacheError::message(format!(
                "action product is {byte_count} bytes, above entry/store bounds {}/{}",
                self.limits.max_entry_bytes, self.limits.max_total_bytes
            )));
        }
        let blob = BlobRef {
            digest: bytes_digest(bytes),
            bytes: byte_count,
        };
        let lease = self.lock_store(false)?;
        let (receipt, inserted) =
            self.publish_prepared(context, product_digest.into(), payload, blob, |path| {
                write_content_addressed(path, bytes, self.limits.max_entry_bytes)
            })?;
        drop(lease);
        if inserted {
            self.prune(Some(&receipt.action_key))?;
        }
        Ok(receipt)
    }

    /// Encode a bounded product directly into staging and use its byte identity
    /// to construct the caller's receipt payload before immutable publication.
    /// An encoding or payload error leaves no new receipt or published blob.
    /// Keep both callbacks independent of this store while its lease is held.
    ///
    /// # Errors
    /// Propagates encoding, quota, I/O, receipt construction and identity failures.
    pub fn publish_stream<P, E>(
        &self,
        context: &ActionContext,
        product_digest: impl Into<String>,
        encode: impl FnOnce(&mut dyn Write) -> Result<(), E>,
        payload: impl FnOnce(&BlobRef) -> Result<P, E>,
    ) -> Result<ActionReceipt<P>, E>
    where
        P: Clone + FromJson + PartialEq + ToJson,
        E: From<ActionCacheError>,
    {
        self.require_writable()?;
        // Pruning scans the blob lane under the exclusive store lock. Keep the
        // temporary in this lease through publication or error-path cleanup.
        let lease = self.lock_store(false)?;
        let mut temporary = temporary_blob(&self.dir.join("blobs"))?;
        let mut writer = BufWriter::new(BlobWriter {
            file: temporary.as_file_mut(),
            remaining: self.limits.max_entry_bytes.min(self.limits.max_total_bytes),
            bytes: 0,
            digest: Sha256::new(),
            failure: None,
        });
        let encoded = encode(&mut writer).and_then(|()| {
            writer.flush().map_err(ActionCacheError::from)?;
            Ok(())
        });
        let BlobWriter {
            bytes,
            digest,
            failure,
            ..
        } = writer.into_parts().0;
        // Prefer the original write failure even if the encoder wrapped or ignored it.
        if let Some(error) = failure {
            return Err(ActionCacheError::from(error).into());
        }
        encoded?;
        let blob = BlobRef {
            bytes,
            digest: format!("{:x}", digest.finalize()),
        };
        let payload = payload(&blob)?;
        let (receipt, inserted) =
            self.publish_prepared(context, product_digest.into(), payload, blob, |path| {
                publish_staged_blob(path, temporary, bytes)
            })?;
        drop(lease);
        if inserted {
            self.prune(Some(&receipt.action_key))?;
        }
        Ok(receipt)
    }

    fn require_writable(&self) -> Result<(), ActionCacheError> {
        if self.inert || self.read_only {
            return Err(ActionCacheError::message(
                "cannot publish through an inert or read-only action cache",
            ));
        }
        Ok(())
    }

    // The caller holds the shared store lease until the staged payload is gone.
    fn publish_prepared<P>(
        &self,
        context: &ActionContext,
        product_digest: String,
        payload: P,
        blob: BlobRef,
        publish_blob: impl FnOnce(&Path) -> Result<(), ActionCacheError>,
    ) -> Result<(ActionReceipt<P>, bool), ActionCacheError>
    where
        P: Clone + FromJson + PartialEq + ToJson,
    {
        let byte_count = blob.bytes;
        let receipt = ActionReceipt {
            schema_version: RECEIPT_SCHEMA_VERSION,
            action_key: context.key(),
            context: context.clone(),
            product_digest,
            product_blob: blob.clone(),
            payload,
        };
        let envelope = ReceiptEnvelope {
            receipt_digest: receipt.digest(),
            receipt: receipt.clone(),
        };
        let receipt_bytes = receipt_json::encode_envelope(&envelope)?;
        let receipt_byte_count = u64::try_from(receipt_bytes.len()).unwrap_or(u64::MAX);
        if receipt_byte_count > self.limits.max_receipt_bytes {
            return Err(ActionCacheError::message(format!(
                "action receipt for {} is {receipt_byte_count} bytes, above its {}-byte structural bound",
                receipt.action_key, self.limits.max_receipt_bytes
            )));
        }
        if receipt_byte_count.saturating_add(byte_count) > self.limits.max_total_bytes {
            return Err(ActionCacheError::message(
                "action receipt plus product exceeds the total store bound",
            ));
        }
        let _action = self.lock_action(&receipt.action_key, true)?;
        let path = self.receipt_path(&receipt.action_key);
        if path_entry_exists(&path)? {
            let existing = self.read_receipt::<P>(&path)?;
            if existing != receipt {
                return Err(ActionCacheError::message(format!(
                    "same action key produced divergent receipts: existing={} candidate={}",
                    existing.digest(),
                    receipt.digest()
                )));
            }
            self.verify_blob(&blob)?;
            return Ok((existing, false));
        }
        publish_blob(&self.blob_path(&blob.digest))?;
        write_atomic(&path, &receipt_bytes)?;
        Ok((receipt, true))
    }

    /// Probe, elect one process on a miss, re-probe, then build while holding the
    /// per-action election lock. The producer decides how and when it publishes.
    pub fn coordinate<T, E, Probe, Build>(
        &self,
        key: &ActionKey,
        probe: Probe,
        build: Build,
    ) -> Result<Coordinated<T>, E>
    where
        E: From<ActionCacheError>,
        Probe: Fn() -> Result<Option<T>, E>,
        Build: FnOnce() -> Result<T, E>,
    {
        if self.read_only {
            return Err(E::from(ActionCacheError::message(
                "read-only action cache cannot coordinate or execute callbacks",
            )));
        }
        if let Some(value) = probe()? {
            return Ok(Coordinated {
                value,
                built: false,
            });
        }
        if self.inert {
            return Ok(Coordinated {
                value: build()?,
                built: true,
            });
        }
        // A stable stripe bounds lock-file growth while preserving the only
        // correctness requirement: two builders for the same action key always
        // contend on the same kernel lock. Unrelated colliding actions merely
        // serialize their miss elections.
        let stripe = key.as_str().get(..2).unwrap_or("00");
        let election = open_file_lock(
            &self
                .dir
                .join("elections")
                .join(format!("action-{stripe}.lock")),
        )
        .map_err(E::from)?;
        election
            .lock()
            .map_err(ActionCacheError::from)
            .map_err(E::from)?;
        let _election = LockGuard(election);
        if let Some(value) = probe()? {
            return Ok(Coordinated {
                value,
                built: false,
            });
        }
        Ok(Coordinated {
            value: build()?,
            built: true,
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        fs::read_dir(self.dir.join("receipts"))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| receipt_key(&entry.path()).is_some())
                    .count()
            })
            .unwrap_or(0)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn validate_identity<P>(
        &self,
        context: &ActionContext,
        receipt: &ActionReceipt<P>,
    ) -> Result<(), ActionCacheError> {
        let expected = context.key();
        if receipt.action_key != expected || receipt.context != *context {
            return Err(ActionCacheError::message(format!(
                "action receipt identity mismatch: expected {expected}, got {}",
                receipt.action_key
            )));
        }
        Ok(())
    }

    fn read_receipt<P>(&self, path: &Path) -> Result<ActionReceipt<P>, ActionCacheError>
    where
        P: FromJson + ToJson,
    {
        let bytes = read_bounded(path, self.limits.max_receipt_bytes, "action receipt")?;
        let envelope: ReceiptEnvelope<P> =
            json::record::from_slice(&bytes).map_err(receipt_json::decode_error)?;
        if envelope.receipt.schema_version != RECEIPT_SCHEMA_VERSION {
            return Err(ActionCacheError::message(format!(
                "action receipt schema {} != {RECEIPT_SCHEMA_VERSION}",
                envelope.receipt.schema_version
            )));
        }
        let actual = envelope.receipt.digest();
        if actual != envelope.receipt_digest {
            return Err(ActionCacheError::message(format!(
                "action receipt digest mismatch: expected {}, actual {actual}",
                envelope.receipt_digest
            )));
        }
        let path_key = receipt_key(path).ok_or_else(|| {
            ActionCacheError::message(format!(
                "action receipt path is not a digest-keyed JSON root: {}",
                path.display()
            ))
        })?;
        if envelope.receipt.action_key != path_key
            || envelope.receipt.context.key() != envelope.receipt.action_key
        {
            return Err(ActionCacheError::message(format!(
                "action receipt key/context does not match root {}",
                path.display()
            )));
        }
        if !valid_digest(&envelope.receipt.product_blob.digest) {
            return Err(ActionCacheError::message(
                "action receipt carries a malformed product blob digest",
            ));
        }
        if envelope.receipt.product_blob.bytes > self.limits.max_entry_bytes
            || envelope.receipt.product_blob.bytes > self.limits.max_total_bytes
        {
            return Err(ActionCacheError::message(
                "action receipt declares product bytes above the store bounds",
            ));
        }
        Ok(envelope.receipt)
    }

    fn read_blob(&self, blob: &BlobRef) -> Result<Vec<u8>, ActionCacheError> {
        if blob.bytes > self.limits.max_entry_bytes || blob.bytes > self.limits.max_total_bytes {
            return Err(ActionCacheError::message(
                "action blob declares bytes above its bound",
            ));
        }
        let bytes = read_bounded(&self.blob_path(&blob.digest), blob.bytes, "action blob")?;
        let actual = BlobRef {
            digest: bytes_digest(&bytes),
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        };
        if actual != *blob {
            return Err(ActionCacheError::message(format!(
                "action blob mismatch: expected {blob:?}, actual {actual:?}"
            )));
        }
        Ok(bytes)
    }

    fn verify_blob(&self, blob: &BlobRef) -> Result<(), ActionCacheError> {
        if blob.bytes > self.limits.max_entry_bytes || blob.bytes > self.limits.max_total_bytes {
            return Err(ActionCacheError::message(
                "action blob declares bytes above its bound",
            ));
        }
        let (digest, bytes) =
            digest_bounded(&self.blob_path(&blob.digest), blob.bytes, "action blob")?;
        if digest != blob.digest || bytes != blob.bytes {
            return Err(ActionCacheError::message(format!(
                "action blob mismatch: expected {}:{}, actual {digest}:{bytes}",
                blob.digest, blob.bytes
            )));
        }
        Ok(())
    }

    fn lock_store(&self, exclusive: bool) -> Result<LockGuard, ActionCacheError> {
        lock_path(&self.dir.join("locks/store.lock"), exclusive)
    }

    fn lock_action(&self, key: &ActionKey, exclusive: bool) -> Result<LockGuard, ActionCacheError> {
        let stripe = key.as_str().get(..2).unwrap_or("00");
        lock_path(
            &self.dir.join("locks").join(format!("action-{stripe}.lock")),
            exclusive,
        )
    }

    fn quota_exceeded(&self) -> Result<bool, ActionCacheError> {
        // Publishers rename and remove temporary paths under a shared lock.
        // Enumeration and metadata reads require one stable inventory, so a
        // publisher cannot remove an entry between those two operations.
        let _store = self.lock_store(true)?;
        let mut entries = 0_usize;
        let mut bytes = 0_u64;
        for (lane, count_receipts) in [("receipts", true), ("blobs", false)] {
            for entry in fs::read_dir(self.dir.join(lane))? {
                let entry = entry?;
                let file_type = entry.file_type()?;
                if !file_type.is_file() || file_type.is_symlink() {
                    return Err(ActionCacheError::message(format!(
                        "action-store {lane} entry is not a regular file: {}",
                        entry.path().display()
                    )));
                }
                bytes = bytes.saturating_add(entry.metadata()?.len());
                if count_receipts && receipt_key(&entry.path()).is_some() {
                    entries = entries.saturating_add(1);
                }
            }
        }
        Ok(entries > self.limits.max_entries || bytes > self.limits.max_total_bytes)
    }

    fn prune(&self, protected: Option<&ActionKey>) -> Result<(), ActionCacheError> {
        let _store = self.lock_store(true)?;
        let receipts_dir = self.dir.join("receipts");
        let mut roots = Vec::new();
        for entry in fs::read_dir(&receipts_dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if !file_type.is_file() || file_type.is_symlink() {
                return Err(ActionCacheError::message(format!(
                    "action receipt root is not a regular file: {}",
                    path.display()
                )));
            }
            if receipt_key(&path).is_none() {
                fs::remove_file(path)?;
                continue;
            }
            let modified = entry
                .metadata()?
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let bytes = read_bounded(&path, self.limits.max_receipt_bytes, "action receipt")?;
            let receipt = self.read_receipt::<json::Value>(&path)?;
            roots.push((
                path,
                modified,
                u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                receipt,
            ));
        }
        roots.sort_by(|left, right| (&left.1, &left.0).cmp(&(&right.1, &right.0)));
        let mut referenced = BTreeMap::<String, (usize, u64)>::new();
        for (_, _, _, receipt) in &roots {
            let reference = referenced
                .entry(receipt.product_blob.digest.clone())
                .or_insert((0, receipt.product_blob.bytes));
            if reference.1 != receipt.product_blob.bytes {
                return Err(ActionCacheError::message(format!(
                    "action receipts disagree on blob {} byte length",
                    receipt.product_blob.digest
                )));
            }
            reference.0 += 1;
        }
        let mut blob_sizes = BTreeMap::new();
        for (digest, (_, declared_bytes)) in &referenced {
            let path = self.blob_path(digest);
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(ActionCacheError::message(format!(
                    "action blob root is not a regular file: {}",
                    path.display()
                )));
            }
            let actual_bytes = metadata.len();
            if actual_bytes != *declared_bytes {
                return Err(ActionCacheError::message(format!(
                    "action blob {digest} length mismatch: declared={declared_bytes} actual={actual_bytes}"
                )));
            }
            blob_sizes.insert(digest.clone(), actual_bytes);
        }
        let mut bytes = roots
            .iter()
            .fold(0_u64, |sum, (_, _, receipt_bytes, _)| {
                sum.saturating_add(*receipt_bytes)
            })
            .saturating_add(blob_sizes.values().copied().sum::<u64>());
        let mut retained = roots.len();
        for (path, _, receipt_bytes, receipt) in &roots {
            if retained <= self.limits.max_entries && bytes <= self.limits.max_total_bytes {
                break;
            }
            if protected.is_some_and(|key| key == &receipt.action_key) {
                continue;
            }
            let action_file = open_file_lock(&self.dir.join("locks").join(format!(
                "action-{}.lock",
                receipt.action_key.as_str().get(..2).unwrap_or("00")
            )))?;
            match action_file.try_lock() {
                Ok(()) => {
                    fs::remove_file(path)?;
                    retained = retained.saturating_sub(1);
                    bytes = bytes.saturating_sub(*receipt_bytes);
                    if let Some((count, _)) = referenced.get_mut(&receipt.product_blob.digest) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            bytes = bytes.saturating_sub(
                                blob_sizes
                                    .get(&receipt.product_blob.digest)
                                    .copied()
                                    .unwrap_or(0),
                            );
                        }
                    }
                    action_file.unlock()?;
                }
                Err(TryLockError::WouldBlock) => continue,
                Err(TryLockError::Error(error)) => return Err(error.into()),
            }
        }
        if retained > self.limits.max_entries || bytes > self.limits.max_total_bytes {
            return Err(ActionCacheError::message(format!(
                "action-store quota could not be satisfied: entries={retained}/{} bytes={bytes}/{}",
                self.limits.max_entries, self.limits.max_total_bytes
            )));
        }
        let mut live = BTreeSet::new();
        for entry in fs::read_dir(&receipts_dir)? {
            let entry = entry?;
            if receipt_key(&entry.path()).is_none() {
                continue;
            }
            let bytes = read_bounded(
                &entry.path(),
                self.limits.max_receipt_bytes,
                "action receipt",
            )?;
            let envelope: ReceiptEnvelope<json::Value> =
                json::record::from_slice(&bytes).map_err(receipt_json::decode_error)?;
            live.insert(envelope.receipt.product_blob.digest);
        }
        for entry in fs::read_dir(self.dir.join("blobs"))? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if !file_type.is_file() || file_type.is_symlink() {
                return Err(ActionCacheError::message(format!(
                    "action blob root is not a regular file: {}",
                    path.display()
                )));
            }
            if !live.contains(entry.file_name().to_string_lossy().as_ref()) {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
}

/// Result of an elected action.
pub struct Coordinated<T> {
    pub value: T,
    pub built: bool,
}

/// SHA-256 over ordered domain-separated byte fields.
#[must_use]
pub fn content_digest(fields: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update(field);
        hasher.update([0x1f]);
    }
    format!("{:x}", hasher.finalize())
}

/// SHA-256 of one blob.
#[must_use]
pub fn bytes_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn receipt_key(path: &Path) -> Option<ActionKey> {
    if path.extension()?.to_str()? != "json" {
        return None;
    }
    let key = path.file_stem()?.to_str()?;
    (key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| ActionKey(key.to_owned()))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn path_entry_exists(path: &Path) -> Result<bool, ActionCacheError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn ensure_real_directory(path: &Path, lane: &str) -> Result<(), ActionCacheError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(ActionCacheError::message(format!(
            "{lane} is not a real directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_store_root(root: &Path) -> Result<(), ActionCacheError> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let known_file = matches!(
            name.as_ref(),
            ".action-cache.lock" | ".gmeow-action-cache-v1"
        );
        let known_version = name.strip_prefix('v').is_some_and(|version| {
            !version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit())
        });
        if (known_file && file_type.is_file() && !file_type.is_symlink())
            || (known_version && file_type.is_dir() && !file_type.is_symlink())
        {
            continue;
        }
        return Err(ActionCacheError::message(format!(
            "action-store root contains an unrelated or unsafe entry: {}",
            entry.path().display()
        )));
    }
    Ok(())
}

fn validate_version_root(root: &Path) -> Result<(), ActionCacheError> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let known_lane = matches!(name.as_ref(), "blobs" | "receipts" | "locks" | "elections");
        let known_lease = name == ".lease.lock";
        if (known_lane && file_type.is_dir() && !file_type.is_symlink())
            || (known_lease && file_type.is_file() && !file_type.is_symlink())
        {
            continue;
        }
        return Err(ActionCacheError::message(format!(
            "action-store version root contains an unrelated or unsafe entry: {}",
            entry.path().display()
        )));
    }
    Ok(())
}

/// Remove obsolete on-disk format roots only after proving their complete tree is
/// store-owned and taking their lease exclusively. An older process keeps its shared
/// lease for its whole `ActionStore` lifetime, so an active reader/writer is retained;
/// sequential format upgrades cannot accumulate unbounded dead namespaces.
fn prune_obsolete_version_roots(root: &Path, current: &Path) -> Result<(), ActionCacheError> {
    let mut obsolete = fs::read_dir(root)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            entry.path() != current
                && name.strip_prefix('v').is_some_and(|version| {
                    !version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit())
                })
        })
        .collect::<Vec<_>>();
    obsolete.sort_by_key(std::fs::DirEntry::file_name);
    let mut removed_any = false;
    for entry in obsolete {
        let path = entry.path();
        ensure_real_directory(&path, "obsolete action-store version root")?;
        let lease = open_file_lock(&path.join(".lease.lock"))?;
        match lease.try_lock() {
            Ok(()) => {
                validate_version_root(&path)?;
                validate_owned_version_tree(&path)?;
                fs::remove_dir_all(&path)?;
                removed_any = true;
            }
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
    }
    if removed_any {
        File::open(root)?.sync_all()?;
    }
    Ok(())
}

fn validate_owned_version_tree(root: &Path) -> Result<(), ActionCacheError> {
    for lane in ["blobs", "receipts", "locks", "elections"] {
        let path = root.join(lane);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                    return Err(ActionCacheError::message(format!(
                        "obsolete action-store lane is not a real directory: {}",
                        path.display()
                    )));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if !file_type.is_file() || file_type.is_symlink() {
                return Err(ActionCacheError::message(format!(
                    "obsolete action-store tree contains an unsafe entry: {}",
                    entry.path().display()
                )));
            }
        }
    }
    Ok(())
}

fn open_file_lock(path: &Path) -> Result<File, ActionCacheError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() || metadata.file_type().is_symlink() => {
            return Err(ActionCacheError::message(format!(
                "action lock path is not a regular file: {}",
                path.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(ActionCacheError::message(format!(
            "opened action lock path is not a regular file: {}",
            path.display()
        )));
    }
    Ok(file)
}

fn lock_path(path: &Path, exclusive: bool) -> Result<LockGuard, ActionCacheError> {
    let file = open_file_lock(path)?;
    if exclusive {
        file.lock()?;
    } else {
        file.lock_shared()?;
    }
    Ok(LockGuard(file))
}

fn read_bounded(path: &Path, max_bytes: u64, lane: &str) -> Result<Vec<u8>, ActionCacheError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > max_bytes
    {
        return Err(ActionCacheError::message(format!(
            "{lane} {} is not a regular file within {max_bytes} bytes",
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    File::open(path)?
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(ActionCacheError::message(format!(
            "{lane} {} grew beyond {max_bytes} bytes while read",
            path.display()
        )));
    }
    Ok(bytes)
}

fn digest_bounded(
    path: &Path,
    max_bytes: u64,
    lane: &str,
) -> Result<(String, u64), ActionCacheError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > max_bytes
    {
        return Err(ActionCacheError::message(format!(
            "{lane} {} is not a regular file within {max_bytes} bytes",
            path.display()
        )));
    }
    let mut reader = File::open(path)?.take(max_bytes.saturating_add(1));
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        if total > max_bytes {
            return Err(ActionCacheError::message(format!(
                "{lane} {} grew beyond {max_bytes} bytes while read",
                path.display()
            )));
        }
        hasher.update(&buffer[..read]);
    }
    Ok((format!("{:x}", hasher.finalize()), total))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ActionCacheError> {
    if path_entry_exists(path)? {
        return Err(ActionCacheError::message(format!(
            "refusing to replace an existing action receipt path: {}",
            path.display()
        )));
    }
    let parent = path
        .parent()
        .ok_or_else(|| ActionCacheError::message("action cache path has no parent"))?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".action-receipt-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| error.error)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn write_content_addressed(
    path: &Path,
    bytes: &[u8],
    max_bytes: u64,
) -> Result<(), ActionCacheError> {
    if path_entry_exists(path)? {
        let existing = read_bounded(path, max_bytes, "content-addressed blob")?;
        if existing == bytes {
            return sync_blob_directory(path);
        }
        return Err(ActionCacheError::message(format!(
            "content-addressed collision: expected {}, found {}",
            bytes_digest(bytes),
            bytes_digest(&existing)
        )));
    }
    let parent = path
        .parent()
        .ok_or_else(|| ActionCacheError::message("action blob path has no parent"))?;
    let mut temporary = temporary_blob(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(path) {
        Ok(_) => {
            File::open(parent)?.sync_all()?;
            Ok(())
        }
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = read_bounded(path, max_bytes, "concurrent content-addressed blob")?;
            if existing == bytes {
                sync_blob_directory(path)
            } else {
                Err(ActionCacheError::message(
                    "concurrent content-addressed collision",
                ))
            }
        }
        Err(error) => Err(error.error.into()),
    }
}

struct BlobWriter<'a> {
    file: &'a mut File,
    remaining: u64,
    bytes: u64,
    digest: Sha256,
    failure: Option<io::Error>,
}

impl BlobWriter<'_> {
    fn remember_error(&mut self, error: io::Error) -> io::Error {
        let original = self.failure.get_or_insert(error);
        io::Error::new(original.kind(), original.to_string())
    }
}

impl Write for BlobWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if let Some(error) = &self.failure {
            return Err(io::Error::new(error.kind(), error.to_string()));
        }
        let count = u64::try_from(buffer.len()).unwrap_or(u64::MAX);
        if count > self.remaining {
            return Err(self.remember_error(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "streamed action product exceeds its entry or store byte bound",
            )));
        }
        self.file
            .write_all(buffer)
            .map_err(|error| self.remember_error(error))?;
        self.remaining -= count;
        self.bytes += count;
        self.digest.update(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(error) = &self.failure {
            return Err(io::Error::new(error.kind(), error.to_string()));
        }
        self.file
            .flush()
            .map_err(|error| self.remember_error(error))
    }
}

fn temporary_blob(directory: &Path) -> Result<tempfile::NamedTempFile, ActionCacheError> {
    Ok(tempfile::Builder::new()
        .prefix(".action-blob-")
        .suffix(".tmp")
        .tempfile_in(directory)?)
}

fn publish_staged_blob(
    path: &Path,
    temporary: tempfile::NamedTempFile,
    bytes: u64,
) -> Result<(), ActionCacheError> {
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(path) {
        Ok(_) => {}
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_staged_blob(path, error.file, bytes)?;
        }
        Err(error) => return Err(error.error.into()),
    }
    sync_blob_directory(path)
}

fn verify_staged_blob(
    path: &Path,
    mut staged: tempfile::NamedTempFile,
    bytes: u64,
) -> Result<(), ActionCacheError> {
    let metadata = fs::symlink_metadata(path)?;
    let collision =
        || ActionCacheError::message(format!("content-addressed collision at {}", path.display()));
    if !metadata.is_file() || metadata.len() != bytes {
        return Err(collision());
    }
    let staged = staged.as_file_mut();
    staged.rewind()?;
    let mut existing = File::open(path)?;
    let mut left = [0_u8; 8192];
    let mut right = [0_u8; 8192];
    let mut remaining = bytes;
    while remaining != 0 {
        let chunk = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(left.len());
        existing.read_exact(&mut left[..chunk])?;
        staged.read_exact(&mut right[..chunk])?;
        if left[..chunk] != right[..chunk] {
            return Err(collision());
        }
        remaining -= u64::try_from(chunk).unwrap_or(u64::MAX);
    }
    if existing.read(&mut left[..1])? != 0 || staged.read(&mut right[..1])? != 0 {
        return Err(collision());
    }
    Ok(())
}

// A concurrent writer may see an installed blob before its owner syncs the
// directory. Every first receipt for a blob needs its own durability barrier.
fn sync_blob_directory(path: &Path) -> Result<(), ActionCacheError> {
    let directory = path
        .parent()
        .ok_or_else(|| ActionCacheError::message("action blob path has no parent"))?;
    File::open(directory)?.sync_all()?;
    Ok(())
}

fn publish_identical(path: &Path, bytes: &[u8], max_bytes: u64) -> Result<(), ActionCacheError> {
    if path_entry_exists(path)? {
        let existing = read_bounded(path, max_bytes, "action-store sentinel")?;
        if existing == bytes {
            return Ok(());
        }
        return Err(ActionCacheError::message(
            "action-store sentinel identity mismatch",
        ));
    }
    write_content_addressed(path, bytes, max_bytes)
}

impl<P> VerifiedEntry<P> {
    #[must_use]
    pub fn payload(self) -> P {
        self.receipt.payload
    }
}

#[path = "lib.tests.rs"]
#[cfg(test)]
mod tests;

// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One dependency-leaf authority for deterministic action keys and bounded immutable
//! receipt/blob storage.
//!
//! Callers own product semantics and codecs. This crate owns the generic mechanics:
//! canonical contexts, SHA-256 action identities, bounded reads, self-digested
//! receipts, immutable content-addressed blobs, atomic publication, process-wide
//! build election, reader-safe reachability GC, and explicit store quotas.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
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
pub struct ActionCacheError(String);

impl ActionCacheError {
    #[must_use]
    pub fn message(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl std::fmt::Display for ActionCacheError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ActionCacheError {}

impl From<std::io::Error> for ActionCacheError {
    fn from(error: std::io::Error) -> Self {
        Self(format!("action cache I/O: {error}"))
    }
}

impl From<serde_json::Error> for ActionCacheError {
    fn from(error: serde_json::Error) -> Self {
        Self(format!("action cache JSON: {error}"))
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
        let bytes = serde_json::to_vec(self).expect("closed ActionContext JSON");
        ActionKey(content_digest(&[b"gmeow:action-key:v1", &bytes]))
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionReceipt<P> {
    pub schema_version: u32,
    pub action_key: ActionKey,
    pub context: ActionContext,
    pub product_digest: String,
    pub product_blob: BlobRef,
    pub payload: P,
}

impl<P: Serialize> ActionReceipt<P> {
    #[must_use]
    pub fn digest(&self) -> String {
        let value = canonical_json_value(
            serde_json::to_value(self).expect("closed action receipt JSON value"),
        );
        let bytes = serde_json::to_vec(&value).expect("closed canonical action receipt JSON");
        content_digest(&[b"gmeow:action-receipt:v1", &bytes])
    }
}

fn canonical_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(canonical_json_value).collect())
        }
        serde_json::Value::Object(values) => {
            let mut entries = values.into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            serde_json::Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonical_json_value(value)))
                    .collect(),
            )
        }
        scalar => scalar,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Default for StoreLimits {
    fn default() -> Self {
        Self {
            max_entry_bytes: 256 * 1024 * 1024,
            max_receipt_bytes: 4 * 1024 * 1024,
            // The producer DAG carries one independently reusable receipt per slice
            // specification in addition to pipeline stages. Retain several source
            // generations under the unchanged byte ceiling so a one-slice edit does
            // not evict the rest of the corpus-wide warm frontier.
            max_entries: 4_096,
            max_total_bytes: 8 * 1024 * 1024 * 1024,
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
        P: DeserializeOwned + Serialize,
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
        P: DeserializeOwned + Serialize,
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
        P: DeserializeOwned + Serialize,
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
        P: DeserializeOwned + Serialize,
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
        P: Clone + DeserializeOwned + PartialEq + Serialize,
    {
        if self.inert || self.read_only {
            return Err(ActionCacheError::message(
                "cannot publish through an inert or read-only action cache",
            ));
        }
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
        let receipt = ActionReceipt {
            schema_version: RECEIPT_SCHEMA_VERSION,
            action_key: context.key(),
            context: context.clone(),
            product_digest: product_digest.into(),
            product_blob: blob.clone(),
            payload,
        };
        let envelope = ReceiptEnvelope {
            receipt_digest: receipt.digest(),
            receipt: receipt.clone(),
        };
        let receipt_bytes = serde_json::to_vec_pretty(&envelope)?;
        let receipt_byte_count = u64::try_from(receipt_bytes.len()).unwrap_or(u64::MAX);
        if receipt_byte_count > self.limits.max_receipt_bytes {
            return Err(ActionCacheError::message(
                "action receipt exceeds its structural bound",
            ));
        }
        if receipt_byte_count.saturating_add(byte_count) > self.limits.max_total_bytes {
            return Err(ActionCacheError::message(
                "action receipt plus product exceeds the total store bound",
            ));
        }
        {
            let _store = self.lock_store(false)?;
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
                return Ok(existing);
            }
            write_content_addressed(
                &self.blob_path(&blob.digest),
                bytes,
                self.limits.max_entry_bytes,
            )?;
            write_atomic(&path, &receipt_bytes)?;
        }
        self.prune(Some(&receipt.action_key))?;
        Ok(receipt)
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
        P: DeserializeOwned + Serialize,
    {
        let bytes = read_bounded(path, self.limits.max_receipt_bytes, "action receipt")?;
        let envelope: ReceiptEnvelope<P> = serde_json::from_slice(&bytes)?;
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
        let bytes = read_bounded(
            &self.blob_path(&blob.digest),
            self.limits.max_entry_bytes,
            "action blob",
        )?;
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
        let (digest, bytes) = digest_bounded(
            &self.blob_path(&blob.digest),
            self.limits.max_entry_bytes,
            "action blob",
        )?;
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
        let _store = self.lock_store(false)?;
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
            let receipt = self.read_receipt::<serde_json::Value>(&path)?;
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
            let envelope: ReceiptEnvelope<serde_json::Value> = serde_json::from_slice(&bytes)?;
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
    let mut buffer = [0_u8; 1024 * 1024];
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
            return Ok(());
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
    let mut temporary = tempfile::Builder::new()
        .prefix(".action-blob-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
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
                Ok(())
            } else {
                Err(ActionCacheError::message(
                    "concurrent content-addressed collision",
                ))
            }
        }
        Err(error) => Err(error.error.into()),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct StructuredPayload {
        schema_version: u32,
        artifact: String,
        input_digest: String,
    }

    fn context(action: &str, inputs: Vec<ActionInput>) -> ActionContext {
        ActionContext::new(
            "test",
            action,
            ProducerIdentity::new("producer"),
            "json-v1",
            inputs,
        )
    }

    fn store(root: &Path) -> ActionStore {
        ActionStore::open(root, 1, StoreLimits::default()).unwrap()
    }

    #[test]
    fn action_key_sorts_inputs_and_binds_dimensions() {
        let one = ActionInput::Raw {
            logical_path: "b".into(),
            file_kind: FileKind::File,
            executable: false,
            digest: "2".into(),
        };
        let two = ActionInput::Raw {
            logical_path: "a".into(),
            file_kind: FileKind::File,
            executable: false,
            digest: "1".into(),
        };
        assert_eq!(
            context("x", vec![one.clone(), two.clone()]).key(),
            context("x", vec![two, one]).key()
        );
        assert_ne!(
            context("x", vec![]).with_dimension("language", "en").key(),
            context("x", vec![]).with_dimension("language", "fr").key()
        );
    }

    #[test]
    fn cold_publish_and_warm_read_share_one_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let cache = store(temp.path());
        let context = context("round-trip", vec![]);
        let receipt = cache
            .publish(&context, "semantic", 7_u32, b"payload")
            .unwrap();
        let hit = cache.get::<u32>(&context).unwrap().unwrap();
        assert_eq!(hit.receipt, receipt);
        assert_eq!(hit.bytes, b"payload");
    }

    #[test]
    fn existing_open_modes_separate_read_only_consumers_from_writable_workers() {
        let absent_parent = tempfile::tempdir().unwrap();
        let absent = absent_parent.path().join("absent");
        assert!(ActionStore::open_existing_read_only(&absent, 1, StoreLimits::default()).is_err());
        assert!(
            !absent.exists(),
            "a read-only miss must not initialize the cache root"
        );

        let temp = tempfile::tempdir().unwrap();
        let producer = store(temp.path()); // gmeow-test-input: synthetic-only
        let present = context("present", vec![]);
        producer
            .publish(&present, "semantic", 7_u32, b"payload")
            .unwrap();
        drop(producer);

        let consumer =
            ActionStore::open_existing_read_only(temp.path(), 1, StoreLimits::default()).unwrap();
        let hit = consumer.get::<u32>(&present).unwrap().unwrap();
        assert_eq!(hit.bytes, b"payload");
        assert!(
            consumer
                .publish(&present, "semantic", 7_u32, b"payload")
                .is_err(),
            "a read-only handle must reject publication even when bytes agree"
        );

        let missing = context("read-only-miss", vec![]);
        let stripe = missing.key().as_str()[..2].to_string();
        let lock_path = consumer
            .root()
            .join("locks")
            .join(format!("action-{stripe}.lock"));
        let existed_before = lock_path.exists();
        assert!(consumer.get::<u32>(&missing).unwrap().is_none());
        assert_eq!(
            lock_path.exists(),
            existed_before,
            "a read-only miss must not create an action lock"
        );
        let error = match consumer.coordinate::<_, ActionCacheError, _, _>(
            &missing.key(),
            || Ok(None),
            || Ok(9_u32),
        ) {
            Err(error) => error,
            Ok(_) => panic!("a read-only miss must never execute its build callback"),
        };
        assert!(error.to_string().contains("read-only action cache"));

        drop(consumer);
        let worker =
            ActionStore::open_existing_writable(temp.path(), 1, StoreLimits::default()).unwrap();
        worker
            .publish(&missing, "worker-semantic", 9_u32, b"worker-payload")
            .unwrap();
        assert_eq!(
            worker.get::<u32>(&missing).unwrap().unwrap().bytes,
            b"worker-payload"
        );
    }

    #[test]
    fn structured_payload_receipt_is_canonical_across_generic_gc_reads() {
        let temp = tempfile::tempdir().unwrap();
        let cache = store(temp.path());
        let context = context("structured", vec![]);
        let payload = StructuredPayload {
            schema_version: 1,
            artifact: "site".to_string(),
            input_digest: "input".to_string(),
        };
        let receipt = cache
            .publish(&context, "semantic", payload.clone(), b"payload")
            .unwrap();
        let hit = cache
            .get::<StructuredPayload>(&context)
            .unwrap()
            .expect("structured receipt remains valid after publish-time GC");
        assert_eq!(hit.receipt, receipt);
        assert_eq!(hit.receipt.payload, payload);
    }

    #[test]
    fn same_key_divergence_and_tampered_blob_hard_fail() {
        let temp = tempfile::tempdir().unwrap();
        let cache = store(temp.path());
        let context = context("integrity", vec![]);
        let receipt = cache.publish(&context, "one", 1_u32, b"one").unwrap();
        assert!(cache.publish(&context, "two", 2_u32, b"two").is_err());
        fs::write(cache.blob_path(&receipt.product_blob.digest), b"tampered").unwrap();
        assert!(cache.get::<u32>(&context).is_err());
    }

    #[test]
    fn malformed_receipt_missing_blob_and_wrong_root_hard_fail() {
        let temp = tempfile::tempdir().unwrap();
        let cache = store(temp.path());
        let original = context("original", vec![]);
        let receipt = cache
            .publish(&original, "semantic", 1_u32, b"payload")
            .unwrap();

        fs::remove_file(cache.blob_path(&receipt.product_blob.digest)).unwrap();
        assert!(cache.get::<u32>(&original).is_err());

        let truncated_root = tempfile::tempdir().unwrap();
        let truncated = store(truncated_root.path());
        let truncated_context = context("truncated", vec![]);
        truncated
            .publish(&truncated_context, "semantic", 1_u32, b"payload")
            .unwrap();
        fs::write(truncated.receipt_path(&truncated_context.key()), b"{").unwrap();
        assert!(truncated.get::<u32>(&truncated_context).is_err());

        let wrong_root = tempfile::tempdir().unwrap();
        let wrong = store(wrong_root.path());
        let source = context("source", vec![]);
        wrong
            .publish(&source, "semantic", 1_u32, b"payload")
            .unwrap();
        let target = context("target", vec![]);
        fs::rename(
            wrong.receipt_path(&source.key()),
            wrong.receipt_path(&target.key()),
        )
        .unwrap();
        assert!(wrong.get::<u32>(&target).is_err());
    }

    #[test]
    fn election_rechecks_before_building() {
        let temp = tempfile::tempdir().unwrap();
        let cache = store(temp.path());
        let context = context("election", vec![]);
        let key = context.key();
        let result = cache
            .coordinate(
                &key,
                || {
                    cache
                        .get::<u32>(&context)
                        .map(|entry| entry.map(|entry| entry.payload()))
                },
                || {
                    cache.publish(&context, "value", 9_u32, b"nine")?;
                    Ok(9)
                },
            )
            .unwrap();
        assert!(result.built);
        let warm = cache
            .coordinate(
                &key,
                || {
                    cache
                        .get::<u32>(&context)
                        .map(|entry| entry.map(|entry| entry.payload()))
                },
                || panic!("warm coordination must not rebuild"),
            )
            .unwrap();
        assert!(!warm.built);
        assert_eq!(warm.value, 9);
        let elections = fs::read_dir(cache.root().join("elections"))
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        assert_eq!(elections.len(), 1);
        assert!(
            elections[0]
                .file_name()
                .to_string_lossy()
                .starts_with("action-")
        );
    }

    #[test]
    fn inert_coordination_builds_without_touching_the_filesystem() {
        let cache = ActionStore::inert();
        let key = context("inert", vec![]).key();
        let result = cache
            .coordinate::<_, ActionCacheError, _, _>(&key, || Ok(None), || Ok(11_u32))
            .unwrap();
        assert!(result.built);
        assert_eq!(result.value, 11);
    }

    #[test]
    fn unrelated_store_root_is_refused_without_deleting_anything() {
        let temp = tempfile::tempdir().unwrap();
        let unrelated = temp.path().join("owned-by-someone-else");
        fs::write(&unrelated, b"preserve me").unwrap();
        let error = ActionStore::open(temp.path(), STORE_FORMAT_VERSION, StoreLimits::default())
            .err()
            .expect("a broad unrelated root must not become a cache authority");
        assert!(error.to_string().contains("unrelated or unsafe entry"));
        assert_eq!(fs::read(unrelated).unwrap(), b"preserve me");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_blob_hard_fails_even_when_target_bytes_match() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let cache = store(temp.path());
        let context = context("symlink", vec![]);
        let receipt = cache
            .publish(&context, "semantic", 7_u32, b"payload")
            .unwrap();
        let external = temp.path().join("matching-external-bytes");
        fs::write(&external, b"payload").unwrap();
        let blob = cache.blob_path(&receipt.product_blob.digest);
        fs::remove_file(&blob).unwrap();
        symlink(&external, &blob).unwrap();
        assert!(cache.get::<u32>(&context).is_err());
    }

    #[test]
    fn quota_gc_waits_for_live_reader_and_collects_only_unreachable_blobs() {
        use std::sync::mpsc;
        use std::time::Duration;

        let temp = tempfile::tempdir().unwrap();
        let limits = StoreLimits {
            max_entry_bytes: 1024,
            max_receipt_bytes: 4096,
            max_entries: 1,
            max_total_bytes: 1024,
        };
        let cache = ActionStore::open(temp.path(), STORE_FORMAT_VERSION, limits).unwrap();
        let first = context("first", vec![]);
        let second = context("second", vec![]);
        let first_receipt = cache.publish(&first, "first", 1_u32, b"first").unwrap();

        // Hold the same shared store lock a reader owns from receipt read through
        // blob verification. Publication can finish its immutable write, but quota GC
        // cannot delete any root/blob until this reader releases the lock.
        let reader = cache.lock_store(false).unwrap();
        let root = temp.path().to_path_buf();
        let (sent, received) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let cache = ActionStore::open(root, STORE_FORMAT_VERSION, limits).unwrap();
            cache.publish(&second, "second", 2_u32, b"second").unwrap();
            sent.send(()).unwrap();
        });
        assert!(
            received.recv_timeout(Duration::from_millis(50)).is_err(),
            "GC must wait while a live reader holds the shared store lease"
        );
        assert!(cache.receipt_path(&first_receipt.action_key).is_file());
        drop(reader);
        received.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.join().unwrap();

        assert!(cache.get::<u32>(&first).unwrap().is_none());
        assert!(
            cache
                .get::<u32>(&context("second", vec![]))
                .unwrap()
                .is_some()
        );
        assert_eq!(
            fs::read_dir(cache.root().join("blobs"))
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_file())
                .count(),
            1,
        );
    }

    #[test]
    fn total_quota_counts_receipts_as_well_as_unique_blobs() {
        let temp = tempfile::tempdir().unwrap();
        let first = context("first", vec![]);
        let initial = store(temp.path());
        let first_receipt = initial.publish(&first, "value", 1_u32, b"aaaa").unwrap();
        let first_total = fs::metadata(initial.receipt_path(&first_receipt.action_key))
            .unwrap()
            .len()
            + fs::metadata(initial.blob_path(&first_receipt.product_blob.digest))
                .unwrap()
                .len();
        drop(initial);

        let limits = StoreLimits {
            max_entry_bytes: 4,
            max_receipt_bytes: 4096,
            max_entries: 10,
            max_total_bytes: first_total * 2 - 1,
        };
        let bounded = ActionStore::open(temp.path(), STORE_FORMAT_VERSION, limits).unwrap();
        let second = context("other", vec![]);
        bounded.publish(&second, "value", 1_u32, b"bbbb").unwrap();
        assert!(bounded.get::<u32>(&first).unwrap().is_none());
        assert!(bounded.get::<u32>(&second).unwrap().is_some());
    }

    #[test]
    fn opening_an_outer_restored_store_enforces_the_current_entry_quota() {
        let temp = tempfile::tempdir().unwrap();
        {
            let cache = store(temp.path());
            cache
                .publish(&context("first", vec![]), "first", 1_u32, b"first")
                .unwrap();
            cache
                .publish(&context("second", vec![]), "second", 2_u32, b"second")
                .unwrap();
            assert_eq!(cache.len(), 2);
        }
        let bounded = ActionStore::open(
            temp.path(),
            STORE_FORMAT_VERSION,
            StoreLimits {
                max_entries: 1,
                ..StoreLimits::default()
            },
        )
        .unwrap();
        assert_eq!(bounded.len(), 1);
    }

    #[test]
    fn obsolete_format_roots_are_collected_only_after_live_leases_end() {
        let temp = tempfile::tempdir().unwrap();
        let first = ActionStore::open(temp.path(), 1, StoreLimits::default()).unwrap();
        first
            .publish(&context("old", vec![]), "old", 1_u32, b"old")
            .unwrap();

        let second = ActionStore::open(temp.path(), 2, StoreLimits::default()).unwrap();
        assert!(temp.path().join("v1").is_dir());
        assert!(temp.path().join("v2").is_dir());
        drop(first);

        let second_reader = ActionStore::open(temp.path(), 2, StoreLimits::default()).unwrap();
        assert!(!temp.path().join("v1").exists());
        assert!(temp.path().join("v2").is_dir());
        drop(second_reader);
        drop(second);
    }

    #[test]
    fn zero_or_inverted_store_limits_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        assert!(
            ActionStore::open(
                temp.path(),
                STORE_FORMAT_VERSION,
                StoreLimits {
                    max_entry_bytes: 2,
                    max_receipt_bytes: 1,
                    max_entries: 1,
                    max_total_bytes: 1,
                },
            )
            .is_err()
        );
    }
}

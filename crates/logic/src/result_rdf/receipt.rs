// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Immutable binary receipt ownership and explicit content-address resolution.

use crate::execution_cbor::MAX_RECEIPT_BYTES;
use crate::result::NativeExecutionEvidence;

use super::{native, result_err};

/// The complete native receipt's dedicated GTS representation.
pub const NATIVE_RECEIPT_REPRESENTATION: &str = "native-execution-receipt";
/// Native execution v3 is a binary CBOR payload.
pub const NATIVE_RECEIPT_MEDIA_TYPE: &str = "application/cbor";

/// The canonical native GTS address of one complete execution receipt.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NativeReceiptAddress(String);

impl NativeReceiptAddress {
    /// Admit the complete canonical `blake3:` spelling.
    ///
    /// # Errors
    /// Rejects another algorithm, length, case or nonhexadecimal digit.
    pub fn parse(value: &str) -> gmeow_errors::Result<Self> {
        let Some(digest) = value.strip_prefix("blake3:") else {
            return Err(result_err(
                "native receipt address requires blake3:<64 lowercase hex>".into(),
            ));
        };
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err(result_err(
                "native receipt address requires blake3:<64 lowercase hex>".into(),
            ));
        }
        Ok(Self(value.to_owned()))
    }

    /// Borrow the canonical interoperable GTS address.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The native GTS identity of already owned bytes; semantic admission is separate.
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(purrdf::gts::wire::digest_str(bytes))
    }
}

impl TryFrom<String> for NativeReceiptAddress {
    type Error = gmeow_errors::Diag;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<NativeReceiptAddress> for String {
    fn from(value: NativeReceiptAddress) -> Self {
        value.0
    }
}

impl std::borrow::Borrow<str> for NativeReceiptAddress {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

enum ReceiptBytes<'a> {
    Owned(Vec<u8>),
    Borrowed(&'a [u8]),
}

/// A publication's one owned buffer, or an explicit borrow of that same buffer.
/// Cloning a publication cannot accidentally duplicate its payload: this type
/// deliberately implements no `Clone`.
#[must_use]
pub struct NativeReceipt<'a> {
    address: NativeReceiptAddress,
    bytes: ReceiptBytes<'a>,
}

impl NativeReceipt<'static> {
    pub(super) fn publish(execution: &NativeExecutionEvidence) -> gmeow_errors::Result<Self> {
        let started = crate::timing::Instant::now();
        tracing::info!(target: "pipeline_phase", stage = "stage-reason", phase = "encode-native-reasoning-receipt", event = "start", "reason phase boundary");
        let bytes = native::encode(execution)?;
        tracing::info!(target: "pipeline_phase", stage = "stage-reason", phase = "encode-native-reasoning-receipt", event = "end", elapsed_ms = started.elapsed().as_millis(), encoded_bytes = bytes.len(), "reason phase boundary");
        let address = NativeReceiptAddress::of_bytes(&bytes);
        // This same-run equality covers every original source/proof field before
        // any RDF summary normalization. Canonical replay is validation, separate
        // from the one publication-buffer encoding above.
        let authenticated = NativeReceiptRef::admit(&address, &bytes)?;
        authenticated.verify_execution(execution)?;
        Ok(Self {
            address,
            bytes: ReceiptBytes::Owned(bytes),
        })
    }
}

impl<'a> NativeReceipt<'a> {
    pub(super) fn borrowed(value: NativeReceiptRef<'a>) -> Self {
        Self {
            address: value.address.clone(),
            bytes: ReceiptBytes::Borrowed(value.bytes),
        }
    }

    /// Borrow the exact immutable payload and its typed address.
    #[must_use]
    pub fn as_ref(&self) -> NativeReceiptRef<'_> {
        let bytes = match &self.bytes {
            ReceiptBytes::Owned(bytes) => bytes.as_slice(),
            ReceiptBytes::Borrowed(bytes) => bytes,
        };
        NativeReceiptRef {
            address: &self.address,
            bytes,
        }
    }

    /// Move the initial publication buffer into its declared artifact owner.
    ///
    /// # Errors
    /// A borrowed publication cannot transfer its owner's buffer.
    pub fn into_owned_bytes(self) -> gmeow_errors::Result<Vec<u8>> {
        match self.bytes {
            ReceiptBytes::Owned(bytes) => Ok(bytes),
            ReceiptBytes::Borrowed(_) => Err(result_err(
                "native receipt buffer transfer requires its original publication owner".into(),
            )),
        }
    }

    pub(super) fn into_emission_bytes(self) -> Vec<u8> {
        match self.bytes {
            ReceiptBytes::Owned(bytes) => bytes,
            // The final native BlobRow/writer owns its payload. This is the
            // explicit terminal copy of a borrowed internal artifact.
            ReceiptBytes::Borrowed(bytes) => bytes.to_vec(),
        }
    }
}

/// A receipt view whose lifetime follows the actual immutable payload owner.
#[derive(Clone, Copy)]
pub struct NativeReceiptRef<'a> {
    address: &'a NativeReceiptAddress,
    bytes: &'a [u8],
}

impl<'a> NativeReceiptRef<'a> {
    /// Check the complete decoded record against its original execution. Owned
    /// publication and public borrowed-byte publication share this same boundary.
    pub(super) fn verify_execution(
        self,
        execution: &NativeExecutionEvidence,
    ) -> gmeow_errors::Result<()> {
        let decoded = native::decode(self.bytes())?;
        if decoded.as_ref() != execution {
            return Err(result_err(
                "binary native receipt readback differs from complete original execution evidence"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Authenticate a retained artifact against its declared typed address.
    /// Native schema/semantic admission remains the result reader's responsibility.
    ///
    /// # Errors
    /// Rejects over-budget or differently addressed bytes.
    pub fn admit(address: &'a NativeReceiptAddress, bytes: &'a [u8]) -> gmeow_errors::Result<Self> {
        if bytes.len() > MAX_RECEIPT_BYTES {
            return Err(result_err(format!(
                "native receipt {} has {} bytes, exceeding the compact limit {MAX_RECEIPT_BYTES}",
                address.as_str(),
                bytes.len(),
            )));
        }
        if purrdf::gts::wire::digest_str(bytes) != address.as_str() {
            return Err(result_err(format!(
                "native receipt bytes do not match {}",
                address.as_str()
            )));
        }
        Ok(Self { address, bytes })
    }

    /// The authenticated GTS content address.
    #[must_use]
    pub fn address(self) -> &'a NativeReceiptAddress {
        self.address
    }

    /// The complete compact native CBOR bytes, never hexadecimal text.
    #[must_use]
    pub fn bytes(self) -> &'a [u8] {
        self.bytes
    }
}

/// Every reader explicitly names the authority that supplies required bytes.
pub trait NativeReceiptResolver {
    /// Resolve exactly this address; missing bytes are an actionable failure.
    fn resolve(&self, address: &NativeReceiptAddress) -> gmeow_errors::Result<&[u8]>;

    /// Admit the required isolated closure against the same execution's catalog.
    /// Receipt-only authorities fail closed when a selected result requires it.
    fn admit_closure(
        &self,
        address: &NativeReceiptAddress,
        _worlds: &[crate::physical::LogicalGraph],
    ) -> gmeow_errors::Result<()> {
        Err(result_err(format!(
            "required RDF closure {} is absent",
            address.as_str()
        )))
    }
}

impl NativeReceiptResolver for NativeReceiptRef<'_> {
    fn resolve(&self, address: &NativeReceiptAddress) -> gmeow_errors::Result<&[u8]> {
        if self.address != address {
            return Err(result_err(format!(
                "native receipt {} is not supplied",
                address.as_str()
            )));
        }
        Ok(self.bytes)
    }
}

/// An explicit authority for operations that executed no native closure.
pub struct NoNativeReceipts;

impl NativeReceiptResolver for NoNativeReceipts {
    fn resolve(&self, address: &NativeReceiptAddress) -> gmeow_errors::Result<&[u8]> {
        Err(result_err(format!(
            "required native receipt {} is absent",
            address.as_str()
        )))
    }
}

impl NativeReceiptResolver for Option<NativeReceiptRef<'_>> {
    fn resolve(&self, address: &NativeReceiptAddress) -> gmeow_errors::Result<&[u8]> {
        match self {
            Some(receipt) => receipt.resolve(address),
            None => NoNativeReceipts.resolve(address),
        }
    }
}

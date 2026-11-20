// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Immutable binary receipt ownership and explicit content-address resolution.

use crate::execution_cbor::MAX_RECEIPT_BYTES;
use crate::result::ReasoningResult;

use super::{native, result_err};

/// The complete native receipt's dedicated GTS representation.
pub const NATIVE_RECEIPT_REPRESENTATION: &str = "native-execution-receipt";
/// The strict native receipt v1 is a binary CBOR payload.
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
    closure: Option<NativeReceiptAddress>,
}

impl NativeReceipt<'static> {
    pub(super) fn from_admitted_payload(
        address: NativeReceiptAddress,
        bytes: Vec<u8>,
        closure: Option<NativeReceiptAddress>,
    ) -> Self {
        Self {
            address,
            bytes: ReceiptBytes::Owned(bytes),
            closure,
        }
    }

    pub(super) fn publish(
        result: &ReasoningResult,
        closure: Option<&NativeReceiptAddress>,
    ) -> gmeow_errors::Result<Self> {
        let record = native::ReceiptRecord::original(result, closure)?;
        let started = crate::timing::Instant::now();
        tracing::info!(target: "pipeline_phase", stage = "stage-reason", phase = "encode-native-reasoning-receipt", event = "start", "reason phase boundary");
        let bytes = native::encode(&record)?;
        tracing::info!(target: "pipeline_phase", stage = "stage-reason", phase = "encode-native-reasoning-receipt", event = "end", elapsed_ms = started.elapsed().as_millis(), encoded_bytes = bytes.len(), "reason phase boundary");
        let address = NativeReceiptAddress::of_bytes(&bytes);
        // This same-run equality covers every original source/proof field before
        // any RDF summary normalization. Canonical replay is validation, separate
        // from the one publication-buffer encoding above.
        let authenticated = NativeReceiptRef::admit(&address, &bytes)?;
        authenticated.admit_result(result)?;
        Ok(Self {
            address,
            bytes: ReceiptBytes::Owned(bytes),
            closure: record.closure,
        })
    }
}

impl<'a> NativeReceipt<'a> {
    pub(super) fn borrowed(
        value: NativeReceiptRef<'a>,
        closure: Option<NativeReceiptAddress>,
    ) -> Self {
        Self {
            address: value.address.clone(),
            bytes: ReceiptBytes::Borrowed(value.bytes),
            closure,
        }
    }

    pub(super) fn closure(&self) -> Option<&NativeReceiptAddress> {
        self.closure.as_ref()
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
    /// Admit the complete native record and constructor assessment against this
    /// result. Shared deserialized values gain retained-receipt reuse authority,
    /// never original-closure publication authority. Returns the immutable closure
    /// attachment selected by the exact supplied receipt.
    ///
    /// # Errors
    /// Refuses malformed records, different execution or assessment fields, and
    /// replacement of an already admitted receipt or closure selection.
    pub fn admit_result(
        self,
        result: &ReasoningResult,
    ) -> gmeow_errors::Result<Option<NativeReceiptAddress>> {
        if let Some(closure) = result.retained_receipt_selection(self.address())? {
            return Ok(closure.clone());
        }
        let decoded = native::decode(self.bytes())?;
        result.admit_native_receipt(
            decoded.assessment,
            &decoded.execution,
            self.address(),
            decoded.closure.as_ref(),
        )?;
        Ok(decoded.closure)
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

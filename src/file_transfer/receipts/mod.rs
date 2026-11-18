//! Persistent records of transfer effects and recovery evidence.
mod adapter;
mod frames;
mod recovery;
pub(super) mod wire;

pub(super) mod store;
pub use recovery::{
    ReceiptDetail, ReceiptEntryDetail, ReceiptEntryStatus, ReceiptEvidence, ReceiptEvidenceRole,
    ReceiptHealth, ReceiptHistory, ReceiptHistoryError, ReceiptObservation, ReceiptSummary,
    ReceiptTarget, ReceiptTargetChoice, VerifiedMoveEvidence,
};

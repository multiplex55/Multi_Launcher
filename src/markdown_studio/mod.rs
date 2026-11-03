//! Source-oriented Markdown document state for the Markdown Studio.

mod document;
mod persistence;

pub use document::{
    MAX_MARKDOWN_UNDO_BYTES, MAX_MARKDOWN_UNDO_ENTRIES, MarkdownDocument, MarkdownDocumentError,
    MarkdownSaveOutcome, MarkdownTransitionDecision, MarkdownTransitionOutcome,
    PendingMarkdownTransition,
};
pub use persistence::{
    MAX_MARKDOWN_DOCUMENT_BYTES, MarkdownDiskConflictKind, MarkdownFile, MarkdownPersistenceError,
};

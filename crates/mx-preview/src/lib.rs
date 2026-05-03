//! Midnight X content preview. Defines the `Previewer` trait — the seam the
//! future `mxview` binary and rich previewers (markdown, image, hex) will
//! implement. Phase 2 ships only `PlainTextPreviewer`.

#![forbid(unsafe_code)]

pub mod plain_text;

pub use plain_text::{PlainTextPreviewer, Preview, Previewer};

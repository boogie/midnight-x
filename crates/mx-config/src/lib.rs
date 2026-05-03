//! Midnight X config: parses TOML into types defined in [`mx-core`] and reports
//! warnings for partially-bad input without ever panicking.
//!
//! [`mx-core`]: ../mx_core/index.html

#![forbid(unsafe_code)]

//! Shared support for the end-to-end test binaries.
//!
//! Submodules here are compiled into each test binary that declares
//! `mod support;`. They are test-only code; nothing is shipped.

pub mod mock_ha;

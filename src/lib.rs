//! Home Assistant kiosk dashboard.
//!
//! This crate is built wave by wave. The sandboxed extension host lives in
//! [`extensions`] and is not part of the v1 release gate.

#![deny(unsafe_code)]

pub mod extensions;

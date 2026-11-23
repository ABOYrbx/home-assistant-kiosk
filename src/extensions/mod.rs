//! Wave 6 extension shop support.
//!
//! Only the UI-facing shop view model lives here today. The signed catalog,
//! package store, and Rhai host are separate Wave 6 tasks.

pub mod shop;

pub use shop::{
    ConfigField, ExtensionPermissions, ExtensionStatus, InstalledExtension, ShopAction, ShopEntry,
    ShopError, ShopPending, ShopRequest, ShopView,
};

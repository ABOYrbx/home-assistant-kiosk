// UI bridge: AppState and shop view models -> Slint properties/models.

use crate::AppState;
use crate::ShopEntryView;
use crate::extensions::shop::{ExtensionStatus, ShopEntry, ShopView};
use slint::{ModelRc, SharedString, VecModel};
use std::path::Path;
use std::rc::Rc;

pub struct UiBridge;

impl UiBridge {
    pub fn apply(_state: &AppState) {
        // Data-widget bindings arrive with their own wave tasks.
    }
}

pub fn shop_entry_view(entry: &ShopEntry, view: &ShopView) -> ShopEntryView {
    let secret_path = if !entry.config_schema_available
        || entry
            .config_schema
            .iter()
            .any(|field| field.kind == "secret")
    {
        entry
            .secret_file(Path::new(&view.secret_dir))
            .display()
            .to_string()
    } else {
        String::new()
    };
    ShopEntryView {
        id: entry.id.clone().into(),
        name: entry.name.clone().into(),
        version: entry.version.clone().into(),
        author: entry.author.clone().into(),
        license: entry.license.clone().into(),
        description: entry.description.clone().into(),
        status: entry.status().label().into(),
        permissions: entry.permissions_summary().into(),
        config_keys: entry.config_keys_summary().into(),
        config_path: view.config_path.clone().into(),
        config_table: entry.settings_table().into(),
        secret_path: secret_path.into(),
        error: entry.error.clone().unwrap_or_default().into(),
        installed: entry.is_installed(),
        enabled: entry.enabled,
        update_available: entry.status() == ExtensionStatus::UpdateAvailable,
        available: entry.available,
    }
}

pub fn shop_installed(view: &ShopView) -> ModelRc<ShopEntryView> {
    let entries: Vec<ShopEntryView> = view
        .installed_entries()
        .into_iter()
        .map(|entry| shop_entry_view(entry, view))
        .collect();
    Rc::new(VecModel::from(entries)).into()
}

pub fn shop_available(view: &ShopView) -> ModelRc<ShopEntryView> {
    let entries: Vec<ShopEntryView> = view
        .available_entries()
        .into_iter()
        .map(|entry| shop_entry_view(entry, view))
        .collect();
    Rc::new(VecModel::from(entries)).into()
}

pub fn extension_errors(view: &ShopView) -> ModelRc<SharedString> {
    let errors: Vec<SharedString> = view
        .extension_errors()
        .into_iter()
        .map(SharedString::from)
        .collect();
    Rc::new(VecModel::from(errors)).into()
}

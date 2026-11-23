// UI bridge: view models -> Slint properties/models. Runs on the UI thread only.

use crate::sources::tiles::TileBoard;
use crate::{AppState, MainWindow, TileItem};

pub struct UiBridge;

impl UiBridge {
    pub fn apply(_state: &AppState) {
        // Non-tile widgets are bound by their own pages; the shell stays thin.
    }

    pub fn apply_tiles(window: &MainWindow, tiles: &TileBoard) {
        let columns = tiles.columns() as i32;
        let items: Vec<TileItem> = tiles
            .tiles()
            .iter()
            .enumerate()
            .map(|(index, tile)| TileItem {
                entity_id: tile.entity_id.as_str().into(),
                label: tile.label.as_str().into(),
                icon: tile.icon.as_str().into(),
                state: tile.state.as_str().into(),
                detail: tile.detail.as_str().into(),
                read_only: tile.read_only,
                pending: tile.pending,
                error: tile.error.as_deref().unwrap_or("").into(),
                row: (index as i32) / columns.max(1),
                col: (index as i32) % columns.max(1),
            })
            .collect();
        window.set_tiles(slint::ModelRc::new(slint::VecModel::from(items)));
        window.set_tile_columns(columns);
    }
}

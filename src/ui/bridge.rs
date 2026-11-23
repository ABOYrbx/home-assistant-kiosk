// UI bridge: AppState -> Slint properties/models. Wave 2 shell keeps this thin;
// widget-specific bindings arrive in later waves.

use crate::AppState;

pub struct UiBridge;

impl UiBridge {
    pub fn apply(_state: &AppState) {
        // Shell has no data widgets yet; later waves bind state here.
    }
}

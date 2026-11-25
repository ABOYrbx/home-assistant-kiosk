use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;

slint::slint! {
    #[edition = "1.19"]
    import { HaTile } from "../ui/widgets/ha_tile.slint";

    export component TileHarness inherits Window {
        width: 320px;
        height: 240px;

        in-out property <int> taps: 0;
        in-out property <int> pending-taps: 0;
        in-out property <int> read-only-taps: 0;
        in-out property <string> tile-error: "";

        VerticalLayout {
            tile := HaTile {
                label: "Test tile";
                state: "off";
                error: root.tile-error;
                tapped => { root.taps += 1; }
            }

            pending-tile := HaTile {
                label: "Pending tile";
                state: "on";
                pending: true;
                tapped => { root.pending-taps += 1; }
            }

            read-only-tile := HaTile {
                label: "Read-only tile";
                state: "42";
                read-only: true;
                tapped => { root.read-only-taps += 1; }
            }
        }
    }
}

fn find(app: &TileHarness, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("element {id} not found"))
}

/// The error line is conditional, so its presence proves the tile renders a
/// failed service call instead of silently showing the old state.
fn shows_error(app: &TileHarness) -> bool {
    ElementHandle::find_by_element_id(app, "HaTile::tile-error")
        .next()
        .is_some()
}

#[test]
fn a_tap_reports_once_and_ignores_read_only_and_pending_tiles() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = TileHarness::new().unwrap();
        app.window().set_size(slint::PhysicalSize::new(320, 240));

        assert!(
            !shows_error(&app),
            "a successful tile must not show an error"
        );
        app.set_tile_error("Service light.toggle not found".into());
        assert!(
            shows_error(&app),
            "a failed service call must be visible on its tile"
        );
        app.set_tile_error(String::new().into());
        assert!(!shows_error(&app), "a retry must clear the old error");

        let tile = find(&app, "TileHarness::tile");
        tile.single_click(PointerEventButton::Left).await;
        assert_eq!(app.get_taps(), 1, "a tap must reach the tile callback");

        let pending = find(&app, "TileHarness::pending-tile");
        pending.single_click(PointerEventButton::Left).await;
        assert_eq!(
            app.get_pending_taps(),
            0,
            "a pending tile must ignore repeated taps"
        );

        let read_only = find(&app, "TileHarness::read-only-tile");
        read_only.single_click(PointerEventButton::Left).await;
        assert_eq!(
            app.get_read_only_taps(),
            0,
            "a read-only tile must not report a tap action"
        );

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}

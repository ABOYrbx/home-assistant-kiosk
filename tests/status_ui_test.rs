use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, LogicalPosition, PhysicalSize};
use std::rc::Rc;

fn matching_text(app: &ha_kiosk::MainWindow, text: &str) -> Vec<ElementHandle> {
    ElementHandle::find_by_accessible_label(app, text).collect()
}

async fn catalog_offline_diagnostic_recovers() {
    let app = ha_kiosk::MainWindow::new().unwrap();
    app.window().set_size(PhysicalSize::new(800, 480));
    app.set_current_page(3);

    app.set_shop_catalog_online(false);
    app.set_shop_catalog_error("network unreachable".into());

    assert!(
        !matching_text(&app, "Catalog offline").is_empty(),
        "Status must show that the catalog is offline"
    );
    assert!(
        !matching_text(&app, "network unreachable").is_empty(),
        "Status must show the catalog failure reason"
    );

    app.set_shop_catalog_online(true);
    app.set_shop_catalog_error("".into());

    assert!(
        matching_text(&app, "Catalog offline").is_empty(),
        "a successful refresh must clear the offline warning"
    );
    assert!(
        matching_text(&app, "network unreachable").is_empty(),
        "a successful refresh must clear the stale failure reason"
    );
}

fn element(app: &ha_kiosk::MainWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("element {id} not found"))
}

fn assert_within(element: &ElementHandle, label: &str, width: f32, height: f32) {
    let position = element.absolute_position();
    let size = element.size();
    assert!(
        position.x >= -0.5 && position.y >= -0.5,
        "{label} starts outside the window at ({:.1}, {:.1})",
        position.x,
        position.y
    );
    assert!(
        position.x + size.width <= width + 0.5 && position.y + size.height <= height + 0.5,
        "{label} overflows {width}x{height}: ({:.1}, {:.1}) + {:.1}x{:.1}",
        position.x,
        position.y,
        size.width,
        size.height
    );
}

async fn long_error_list_scrolls_without_changing_pages() {
    let app = ha_kiosk::MainWindow::new().unwrap();
    app.window().set_size(PhysicalSize::new(800, 480));
    app.set_extensions_enabled(true);
    app.set_current_page(3);
    app.set_shop_catalog_online(false);
    app.set_shop_catalog_error("network unreachable".into());

    let last_error = "Extension 12: a long failure reason that wraps across several lines and must remain reachable without covering the page navigation".to_string();
    let mut errors: Vec<slint::SharedString> = (1..=12)
        .map(|index| {
            slint::SharedString::from(format!(
                "Extension {index}: a long failure reason that wraps across several lines and must remain reachable without covering the page navigation"
            ))
        })
        .collect();
    errors.push(last_error.clone().into());
    app.set_extension_errors(Rc::new(slint::VecModel::from(errors)).into());

    let list = element(&app, "StatusPage::status-list");
    assert_within(&list, "Status error list", 800.0, 480.0);
    for tab in [
        "tab-overview",
        "tab-media",
        "tab-agenda",
        "tab-status",
        "tab-shop",
    ] {
        assert_within(
            &element(&app, &format!("MainWindow::{tab}")),
            tab,
            800.0,
            480.0,
        );
    }

    let last_row = matching_text(&app, &last_error)
        .into_iter()
        .next()
        .expect("the final error is rendered");
    let before = last_row.absolute_position().y;
    let position = list.absolute_position();
    let size = list.size();
    let center = LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    );
    list.drag(
        LogicalPosition::new(center.x, center.y - 180.0),
        PointerEventButton::Left,
    )
    .await;

    let after = matching_text(&app, &last_error)
        .into_iter()
        .next()
        .expect("the final error remains rendered")
        .absolute_position()
        .y;
    assert!(
        after < before,
        "the error list did not scroll: {before} -> {after}"
    );
    assert_eq!(app.get_current_page(), 3, "scrolling must not change pages");

    for (width, height) in [(1024, 600), (1280, 800), (2048, 1200)] {
        app.window().set_size(PhysicalSize::new(width, height));
        assert_within(
            &element(&app, "StatusPage::status-list"),
            "Status error list",
            width as f32,
            height as f32,
        );
        assert_within(
            &element(&app, "MainWindow::tab-status"),
            "Status tab",
            width as f32,
            height as f32,
        );
    }
}

#[test]
fn status_page_diagnostics_remain_usable_offline() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        catalog_offline_diagnostic_recovers().await;
        long_error_list_scrolls_without_changing_pages().await;
        slint::quit_event_loop().unwrap();
    })
    .unwrap();

    slint::run_event_loop().unwrap();
}

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

    let first_error = "Extension 1: a long failure reason that wraps across several lines and must remain reachable without covering the page navigation";
    let last_error = "Final extension: unique final row".to_string();
    let mut errors: Vec<slint::SharedString> = (1..=12)
        .map(|index| {
            slint::SharedString::from(format!(
                "Extension {index}: a long failure reason that wraps across several lines and must remain reachable without covering the page navigation"
            ))
        })
        .collect();
    errors.push(last_error.clone().into());
    app.set_extension_errors(Rc::new(slint::VecModel::from(errors)).into());

    for text in [
        "Catalog offline",
        "network unreachable",
        "Extension errors",
        first_error,
    ] {
        assert!(
            !matching_text(&app, text).is_empty(),
            "Status must render {text:?} alongside the other diagnostics"
        );
    }

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

    assert!(
        matching_text(&app, &last_error).is_empty(),
        "the final row should start outside the visible viewport"
    );
    let position = list.absolute_position();
    let size = list.size();
    let viewport_top = position.y;
    let viewport_bottom = position.y + size.height;
    let center = LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    );

    for _ in 0..12 {
        if let Some(row) = matching_text(&app, &last_error).into_iter().last() {
            let row_position = row.absolute_position();
            let row_size = row.size();
            if row_position.y >= viewport_top - 0.5
                && row_position.y + row_size.height <= viewport_bottom + 0.5
            {
                break;
            }
        }
        list.drag(
            LogicalPosition::new(center.x, center.y - 140.0),
            PointerEventButton::Left,
        )
        .await;
    }

    let final_row = matching_text(&app, &last_error)
        .into_iter()
        .last()
        .expect("the actual final row becomes rendered and reachable");
    let final_position = final_row.absolute_position();
    let final_size = final_row.size();
    assert!(
        final_position.y >= viewport_top - 0.5
            && final_position.y + final_size.height <= viewport_bottom + 0.5,
        "the actual final row must be reachable: y={} height={} viewport={}..{}",
        final_position.y,
        final_size.height,
        viewport_top,
        viewport_bottom
    );
    assert_eq!(app.get_current_page(), 3, "scrolling must not change pages");

    for (width, height) in [(1024, 600), (1280, 800), (1440, 900), (2048, 1200)] {
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

async fn unbroken_diagnostic_tokens_wrap() {
    let app = ha_kiosk::MainWindow::new().unwrap();
    app.window().set_size(PhysicalSize::new(800, 480));
    app.set_current_page(3);
    app.set_shop_catalog_online(false);

    let catalog_token = "C".repeat(600);
    let extension_token = "E".repeat(600);
    app.set_shop_catalog_error(catalog_token.clone().into());
    app.set_extension_errors(
        Rc::new(slint::VecModel::from(vec![slint::SharedString::from(
            extension_token.clone(),
        )]))
        .into(),
    );

    for (label, token) in [
        ("catalog reason", catalog_token.as_str()),
        ("extension error", extension_token.as_str()),
    ] {
        let row = matching_text(&app, token)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("{label} is rendered"));
        let position = row.absolute_position();
        let size = row.size();
        assert!(
            size.height > 28.0,
            "{label} must wrap an unbroken token across multiple lines: height={}",
            size.height
        );
        assert!(
            position.x >= -0.5 && position.x + size.width <= 800.5,
            "{label} must stay within the viewport: x={} width={}",
            position.x,
            size.width
        );
    }
}

#[test]
fn status_page_diagnostics_remain_usable_offline() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        catalog_offline_diagnostic_recovers().await;
        long_error_list_scrolls_without_changing_pages().await;
        unbroken_diagnostic_tokens_wrap().await;
        slint::quit_event_loop().unwrap();
    })
    .unwrap();

    slint::run_event_loop().unwrap();
}

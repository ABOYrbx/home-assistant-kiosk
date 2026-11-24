use i_slint_backend_testing::ElementHandle;
use slint::LogicalSize;
use slint::language::AccessibleRole;
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, LogicalPosition, PhysicalSize};

const TOUCH_TARGET: f32 = 44.0;

fn find(app: &MainWindow, id: &str) -> ElementHandle {
    let suffix = format!("::{id}");
    app.root_element()
        .query_descendants()
        .match_predicate(move |element| {
            element
                .id()
                .is_some_and(|qualified| qualified.ends_with(suffix.as_str()))
        })
        .find_first()
        .unwrap_or_else(|| panic!("element id {id:?} not found"))
}

fn assert_touch_target(element: &ElementHandle, name: &str) {
    let size = element.size();
    assert!(
        size.width + 0.01 >= TOUCH_TARGET && size.height + 0.01 >= TOUCH_TARGET,
        "{name} is {:.1}x{:.1}, below the 44x44 logical-pixel minimum",
        size.width,
        size.height
    );
}

fn assert_within(element: &ElementHandle, name: &str, width: f32, height: f32) {
    let position = element.absolute_position();
    let size = element.size();
    assert!(
        position.x >= -0.5 && position.y >= -0.5,
        "{name} starts outside the window at ({:.1}, {:.1})",
        position.x,
        position.y
    );
    assert!(
        position.x + size.width <= width + 0.5 && position.y + size.height <= height + 0.5,
        "{name} ends outside {width}x{height}: ({:.1}, {:.1}) + {:.1}x{:.1}",
        position.x,
        position.y,
        size.width,
        size.height
    );
}

async fn tap(app: &MainWindow, id: &str) {
    find(app, id).single_click(PointerEventButton::Left).await;
}

async fn drag_by(app: &MainWindow, id: &str, dx: f32, dy: f32) {
    let element = find(app, id);
    let position = element.absolute_position();
    let size = element.size();
    let center = LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    );
    element
        .drag(
            LogicalPosition::new(center.x + dx, center.y + dy),
            PointerEventButton::Left,
        )
        .await;
}

fn resize(app: &MainWindow, width: u32, height: u32) {
    let scale = app.window().scale_factor();
    app.window().set_size(PhysicalSize::new(
        (width as f32 * scale) as u32,
        (height as f32 * scale) as u32,
    ));
}

/// Minimum interactive hit area from `DESIGN.md` and `PLAN.md`, in logical
/// pixels.
const MIN_TOUCH_TARGET: f32 = 44.0;

/// Roles that accept input. `Text`, `Image`, and the landmark roles are
/// intentionally excluded: they carry information rather than hit areas.
fn is_interactive_role(role: AccessibleRole) -> bool {
    matches!(
        role,
        AccessibleRole::Button
            | AccessibleRole::Checkbox
            | AccessibleRole::Combobox
            | AccessibleRole::Slider
            | AccessibleRole::Spinbox
            | AccessibleRole::Tab
            | AccessibleRole::Switch
            | AccessibleRole::TextInput
            | AccessibleRole::RadioButton
            | AccessibleRole::ListItem
    )
}

fn meets_minimum_touch_target(size: LogicalSize) -> bool {
    size.width >= MIN_TOUCH_TARGET && size.height >= MIN_TOUCH_TARGET
}

#[test]
fn too_small_targets_fail_the_minimum_check() {
    assert!(meets_minimum_touch_target(LogicalSize::new(44.0, 44.0)));
    assert!(meets_minimum_touch_target(LogicalSize::new(120.0, 48.0)));
    assert!(!meets_minimum_touch_target(LogicalSize::new(43.0, 44.0)));
    assert!(!meets_minimum_touch_target(LogicalSize::new(44.0, 43.0)));
    assert!(!meets_minimum_touch_target(LogicalSize::new(20.0, 20.0)));
}

#[test]
fn shell_navigation_gestures_targets_and_layouts() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = MainWindow::new().unwrap();
        resize(&app, 800, 480);

        // Every interactive element must expose at least a 44 x 44 hit area.
        // Walk the shell by accessible role so controls added later are audited
        // too, and fail if the four tab buttons are missing.
        let root = ElementHandle::find_by_element_id(&app, "MainWindow::app-root")
            .next()
            .expect("the shell root must be addressable for the touch-target audit");

        let mut interactive = 0;
        root.visit_descendants(|element| {
            if let Some(role) = element.accessible_role()
                && is_interactive_role(role)
            {
                interactive += 1;
                let size = element.size();
                assert!(
                    meets_minimum_touch_target(size),
                    "{} ({role:?}) is {} x {}, below the {} x {} minimum",
                    element.id().unwrap_or_default(),
                    size.width,
                    size.height,
                    MIN_TOUCH_TARGET,
                    MIN_TOUCH_TARGET,
                );
            }
            std::ops::ControlFlow::<(), ()>::Continue(())
        });
        assert!(
            interactive >= 4,
            "expected at least the four tab buttons in the audit, found {interactive}"
        );

        // Tap on a tab changes the page.
        let tab = ElementHandle::find_by_element_id(&app, "MainWindow::tab-media")
            .next()
            .unwrap();
        tab.single_click(PointerEventButton::Left).await;
        assert_eq!(app.get_current_page(), 1);

        // A gesture that starts on a slider changes the value, not the page.
        let slider = find(&app, "media-slider");
        assert_touch_target(&slider, "Media slider");
        assert_within(&slider, "Media slider", 800.0, 480.0);
        let before = app.get_media_slider_value();
        drag_by(&app, "media-slider", 120.0, 0.0).await;
        assert_eq!(app.get_current_page(), 1);
        assert_ne!(app.get_media_slider_value(), before);

        // A vertical swipe in the agenda Flickable scrolls the list. Neither
        // vertical nor horizontal gestures that start on the list page-flip.
        tap(&app, "tab-agenda").await;
        let list = find(&app, "agenda-list");
        assert_within(&list, "Agenda list", 800.0, 480.0);
        assert_eq!(app.get_agenda_scroll_y(), 0.0);
        drag_by(&app, "agenda-list", 0.0, -140.0).await;
        assert_eq!(app.get_current_page(), 2);
        assert!(
            app.get_agenda_scroll_y() < 0.0,
            "agenda list did not scroll: {:.1}",
            app.get_agenda_scroll_y()
        );
        drag_by(&app, "agenda-list", -220.0, 0.0).await;
        assert_eq!(app.get_current_page(), 2);

        // Both kiosk breakpoints keep controls and touch targets in bounds.
        for (width, height) in [(800u32, 480u32), (1024u32, 600u32)] {
            resize(&app, width, height);

            let host = find(&app, "page-host");
            assert_within(&host, "Page host", width as f32, height as f32);
            assert!(
                (host.size().width - width as f32).abs() < 1.0,
                "page host width {:.1} does not fill {width}",
                host.size().width
            );

            for tab in ["tab-overview", "tab-media", "tab-agenda", "tab-status"] {
                let element = find(&app, tab);
                assert_touch_target(&element, tab);
                assert_within(&element, tab, width as f32, height as f32);
            }

            assert_within(
                &find(&app, "top-bar-title"),
                "Top bar title",
                width as f32,
                height as f32,
            );

            tap(&app, "tab-overview").await;
            let tile = find(&app, "overview-tile");
            assert_touch_target(&tile, "Overview tile");
            assert_within(&tile, "Overview tile", width as f32, height as f32);

            tap(&app, "tab-media").await;
            let slider = find(&app, "media-slider");
            assert_touch_target(&slider, "Media slider");
            assert_within(&slider, "Media slider", width as f32, height as f32);

            tap(&app, "tab-agenda").await;
            assert_within(
                &find(&app, "agenda-list"),
                "Agenda list",
                width as f32,
                height as f32,
            );

            tap(&app, "tab-status").await;
            assert_within(
                &find(&app, "status-list"),
                "Status list",
                width as f32,
                height as f32,
            );
        }

        slint::quit_event_loop().unwrap();
    })
    .unwrap();

    slint::run_event_loop().unwrap();
}

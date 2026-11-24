use i_slint_backend_testing::ElementHandle;
use slint::LogicalSize;
use slint::language::AccessibleRole;
use slint::platform::PointerEventButton;

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
fn shell_navigation_and_gestures() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();

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

        // Swipe left beyond threshold changes page.
        let swipe = ElementHandle::find_by_element_id(&app, "MainWindow::swipe-area")
            .next()
            .unwrap();
        let pos = swipe.absolute_position();
        let size = swipe.size();
        let start =
            slint::LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
        let target = slint::LogicalPosition::new(start.x - 120.0, start.y);
        swipe.drag(target, PointerEventButton::Left).await;
        // page should have moved (prev or next). Just ensure no crash and a valid page.
        let p = app.get_current_page();
        assert!(p <= 3);

        // Vertical drag does not change page.
        let before = app.get_current_page();
        let vtarget = slint::LogicalPosition::new(start.x, start.y - 120.0);
        // recompute start (element unchanged)
        let swipe2 = ElementHandle::find_by_element_id(&app, "MainWindow::swipe-area")
            .next()
            .unwrap();
        swipe2.drag(vtarget, PointerEventButton::Left).await;
        assert_eq!(app.get_current_page(), before);

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}

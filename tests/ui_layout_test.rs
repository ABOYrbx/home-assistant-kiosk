use i_slint_backend_testing::ElementHandle;
use slint::{ComponentHandle, PhysicalSize};

fn set_window_size(app: &ha_kiosk::MainWindow, width: u32, height: u32) {
    app.window().set_size(PhysicalSize::new(width, height));
    slint::platform::update_timers_and_animations();
}

fn element(app: &ha_kiosk::MainWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("missing element {id}"))
}

fn has_element(app: &ha_kiosk::MainWindow, id: &str) -> bool {
    ElementHandle::find_by_element_id(app, id).next().is_some()
}

fn assert_inside(element: &ElementHandle, max_x: f32, label: &str) {
    let position = element.absolute_position();
    let size = element.size();
    assert!(
        position.x >= -0.5,
        "{label} starts left of the page: x={}",
        position.x
    );
    assert!(
        position.x + size.width <= max_x + 0.5,
        "{label} overflows the page: right={} page={max_x}",
        position.x + size.width
    );
}

#[test]
fn overview_uses_compact_two_and_three_column_layouts() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();
        app.set_current_page(0);

        // 800 x 480: compact layout with one contained vertical scroll region.
        set_window_size(&app, 800, 480);
        assert_eq!(app.get_overview_layout_mode(), 0);
        let page = element(&app, "MainWindow::overview-page");
        assert!(page.size().width <= 800.0);
        let topbar = element(&app, "OverviewPage::overview-topbar");
        assert_inside(&topbar, 800.0, "compact top bar");
        assert!(
            topbar.size().height <= 56.5,
            "the compact top bar keeps its fixed height"
        );
        assert!(has_element(&app, "OverviewPage::compact-layout"));
        assert!(!has_element(&app, "OverviewPage::two-column-layout"));
        assert!(!has_element(&app, "OverviewPage::three-column-layout"));
        let scroll = element(&app, "OverviewPage::compact-scroll");
        assert_inside(&scroll, 800.0, "compact scroll region");
        assert!(
            scroll.size().height < page.size().height,
            "the compact scroll region must be contained below the fixed top bar and hero"
        );

        // Breakpoints are inclusive at 1024 and 1280.
        set_window_size(&app, 1023, 600);
        assert_eq!(app.get_overview_layout_mode(), 0);
        set_window_size(&app, 1024, 600);
        assert_eq!(app.get_overview_layout_mode(), 1);
        set_window_size(&app, 1279, 800);
        assert_eq!(app.get_overview_layout_mode(), 1);
        set_window_size(&app, 1280, 800);
        assert_eq!(app.get_overview_layout_mode(), 2);

        // 1024 x 600: two columns, hero in the leading column.
        set_window_size(&app, 1024, 600);
        assert_eq!(app.get_overview_layout_mode(), 1);
        assert!(!has_element(&app, "OverviewPage::compact-layout"));
        assert!(has_element(&app, "OverviewPage::two-column-layout"));
        assert!(!has_element(&app, "OverviewPage::three-column-layout"));
        let hero = element(&app, "OverviewPage::two-column-hero");
        let secondary = element(&app, "OverviewPage::two-column-secondary");
        assert!(
            hero.absolute_position().x < secondary.absolute_position().x,
            "hero must be in the leading column"
        );
        assert_inside(&hero, 1024.0, "two-column hero");
        assert_inside(&secondary, 1024.0, "two-column secondary column");

        // 1280 x 800: three columns.
        set_window_size(&app, 1280, 800);
        assert_eq!(app.get_overview_layout_mode(), 2);
        assert!(!has_element(&app, "OverviewPage::compact-layout"));
        assert!(!has_element(&app, "OverviewPage::two-column-layout"));
        assert!(has_element(&app, "OverviewPage::three-column-layout"));
        let scroll = element(&app, "OverviewPage::three-column-scroll");
        assert_inside(&scroll, 1280.0, "three-column scroll region");
        let content = element(&app, "OverviewPage::three-column-content");
        assert!(
            content.size().height <= scroll.size().height + 0.5,
            "three-column content must not force a vertical scrollbar at 1280x800: content={} scroll={}",
            content.size().height,
            scroll.size().height
        );
        let hero = element(&app, "OverviewPage::three-column-hero");
        let secondary = element(&app, "OverviewPage::three-column-secondary");
        let tertiary = element(&app, "OverviewPage::three-column-tertiary");
        assert!(
            hero.absolute_position().x < secondary.absolute_position().x
                && secondary.absolute_position().x < tertiary.absolute_position().x,
            "three-column layout must place columns left to right"
        );
        assert_inside(&hero, 1280.0, "three-column hero");
        assert_inside(&secondary, 1280.0, "three-column secondary column");
        assert_inside(&tertiary, 1280.0, "three-column tertiary column");

        // 2048 x 1200: same structure, capped content width.
        set_window_size(&app, 2048, 1200);
        assert_eq!(app.get_overview_layout_mode(), 2);
        let hero = element(&app, "OverviewPage::three-column-hero");
        let tertiary = element(&app, "OverviewPage::three-column-tertiary");
        assert!(
            hero.absolute_position().x >= 304.0,
            "content must be capped at 1440px and centered: hero x={}",
            hero.absolute_position().x
        );
        assert!(
            tertiary.absolute_position().x + tertiary.size().width <= 1744.5,
            "content must be capped at 1440px and centered"
        );

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}

use ha_kiosk::{KioskModel, MainWindow};
use i_slint_backend_testing::ElementHandle;
use slint::ComponentHandle;
use slint::platform::PointerEventButton;
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn overview_top_bar_and_hero_controls() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = MainWindow::new().unwrap();
        let model = app.global::<KioskModel>();
        model.set_hero_configured(true);
        model.set_hero_available(true);
        model.set_hero_loading(false);
        model.set_hero_name("Example lamp".into());
        model.set_hero_on(true);
        model.set_hero_brightness(40);
        model.set_hero_color_temp(3200);
        model.set_hero_brightness_supported(true);
        model.set_hero_color_temp_supported(true);
        model.set_hero_brightness_text("40%".into());
        model.set_hero_color_temp_text("3200 K".into());
        model.set_hero_state_text("On".into());

        for id in [
            "OverviewPage::top-bar",
            "OverviewPage::hero",
            "TopBar::brand",
            "TopBar::connection-label",
            "TopBar::clock-text",
            "TopBar::date-text",
            "HeroLight::hero-toggle",
            "HeroLight::hero-brightness",
            "HeroLight::hero-color-temp",
        ] {
            assert_eq!(
                ElementHandle::find_by_element_id(&app, id).count(),
                1,
                "element id {id}"
            );
        }

        let toggles = Rc::new(Cell::new(0));
        let toggles_handler = toggles.clone();
        model.on_hero_toggled(move || toggles_handler.set(toggles_handler.get() + 1));
        let toggle = ElementHandle::find_by_element_id(&app, "HeroLight::hero-toggle")
            .next()
            .expect("hero toggle exists");
        toggle.single_click(PointerEventButton::Left).await;
        assert_eq!(toggles.get(), 1);

        let brightness = Rc::new(Cell::new(-1));
        let brightness_handler = brightness.clone();
        model.on_hero_brightness_changed(move |value| brightness_handler.set(value));
        let slider = ElementHandle::find_by_element_id(&app, "HeroLight::hero-brightness")
            .next()
            .expect("brightness slider exists");
        slider.single_click(PointerEventButton::Left).await;
        let value = brightness.get();
        assert!(
            (45..=55).contains(&value),
            "brightness callback value: {value}"
        );
        assert_eq!(model.get_hero_brightness(), value);

        let color_temp = Rc::new(Cell::new(-1));
        let color_temp_handler = color_temp.clone();
        model.on_hero_color_temp_changed(move |value| color_temp_handler.set(value));
        let slider = ElementHandle::find_by_element_id(&app, "HeroLight::hero-color-temp")
            .next()
            .expect("colour temperature slider exists");
        slider.single_click(PointerEventButton::Left).await;
        let value = color_temp.get();
        assert!(
            (4500..=4700).contains(&value),
            "colour temperature callback value: {value}"
        );
        assert_eq!(model.get_hero_color_temp(), value);

        slint::quit_event_loop().unwrap();
    })
    .unwrap();

    slint::run_event_loop().unwrap();
}

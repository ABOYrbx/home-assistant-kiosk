use std::collections::HashMap;
use std::fs;
use std::path::Path;

fn theme_source() -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/theme.slint")).unwrap()
}

fn palette(name: &str) -> HashMap<String, (u8, u8, u8)> {
    let source = theme_source();
    let header = format!("export global {name} {{");
    let start = source
        .find(&header)
        .unwrap_or_else(|| panic!("{name} block"));
    let end = source[start..].find("\n}").expect("palette end") + start;
    let mut colors = HashMap::new();
    for line in source[start..end].lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("out property <color> ") else {
            continue;
        };
        let Some((key, value)) = rest.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_end_matches(';');
        let Some(hex) = value.strip_prefix('#') else {
            continue;
        };
        if hex.len() != 6 {
            continue;
        }
        let channel = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).unwrap();
        colors.insert(key.trim().to_string(), (channel(0), channel(2), channel(4)));
    }
    assert!(!colors.is_empty(), "{name} has no colours");
    colors
}

fn luminance(color: (u8, u8, u8)) -> f64 {
    let channel = |value: u8| {
        let value = value as f64 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(color.0) + 0.7152 * channel(color.1) + 0.0722 * channel(color.2)
}

fn contrast(foreground: (u8, u8, u8), background: (u8, u8, u8)) -> f64 {
    let a = luminance(foreground);
    let b = luminance(background);
    let (high, low) = if a > b { (a, b) } else { (b, a) };
    (high + 0.05) / (low + 0.05)
}

fn assert_contrast(
    colors: &HashMap<String, (u8, u8, u8)>,
    palette_name: &str,
    foreground: &str,
    background: &str,
    minimum: f64,
) {
    let fg = *colors
        .get(foreground)
        .unwrap_or_else(|| panic!("{palette_name} missing {foreground}"));
    let bg = *colors
        .get(background)
        .unwrap_or_else(|| panic!("{palette_name} missing {background}"));
    let ratio = contrast(fg, bg);
    assert!(
        ratio >= minimum,
        "{palette_name}: {foreground} on {background} is {ratio:.2}:1, needs {minimum:.1}:1"
    );
}

#[test]
fn text_and_focus_contrast_passes_in_both_palettes() {
    for palette_name in ["ThemeDark", "ThemeLight"] {
        let colors = palette(palette_name);
        for foreground in ["fg", "fg-2", "muted"] {
            for background in ["bg", "surface", "surface-warm"] {
                assert_contrast(&colors, palette_name, foreground, background, 4.5);
            }
        }
        assert_contrast(&colors, palette_name, "accent-on", "accent", 4.5);
        for semantic in ["success", "warn", "danger"] {
            assert_contrast(&colors, palette_name, semantic, "bg", 4.5);
            assert_contrast(&colors, palette_name, semantic, "surface", 4.5);
        }
        assert_contrast(&colors, palette_name, "focus-ring", "bg", 3.0);
    }
}

#[test]
fn theme_has_dark_first_palette_and_spacing_type_radius_tokens() {
    let source = theme_source();
    assert!(source.contains("export global ThemeDark"));
    assert!(source.contains("export global ThemeLight"));
    assert!(source.contains("in-out property <bool> dark-mode: true;"));

    for token in [
        "space-1",
        "space-12",
        "text-xs",
        "text-4xl",
        "radius-sm",
        "radius-pill",
        "touch-target",
        "swipe-threshold",
        "focus-ring-width",
    ] {
        assert!(
            source.contains(&format!("property <length> {token}")),
            "missing token {token}"
        );
    }
}

#[test]
fn weather_icons_are_bundled_and_local() {
    let icons = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/icons");
    let expected = [
        "weather-clear-day.svg",
        "weather-clear-night.svg",
        "weather-cloudy.svg",
        "weather-partly-cloudy-day.svg",
        "weather-partly-cloudy-night.svg",
        "weather-rainy.svg",
        "weather-snowy.svg",
        "weather-fog.svg",
        "weather-thunderstorm.svg",
        "weather-windy.svg",
        "weather-unknown.svg",
    ];
    for name in expected {
        let path = icons.join(name);
        let contents = fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing {path:?}"));
        assert!(contents.contains("<svg"), "{name} is not an SVG");
        assert!(
            !contents.contains("href=\"http"),
            "{name} must not reference a remote asset"
        );
    }

    let component = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/components/weather_icon.slint"),
    )
    .unwrap();
    for name in expected {
        assert!(
            component.contains(name),
            "weather_icon.slint does not map {name}"
        );
    }
}

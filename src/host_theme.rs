//! Resolves the herdr host theme's accent color so the picker blends in.
//!
//! herdr (as of 0.7.1) exposes no theme API or env var to plugins, so this
//! mirrors the host's own resolution: read `[theme]` from the herdr
//! config.toml, honor a `[theme.custom] accent` override, else look the
//! built-in palette's accent up by name (values lifted from herdr's
//! `Palette::from_name` table).

use std::path::Path;

use ratatui::style::Color;

/// The host accent, best effort. `plugin_config_dir` is
/// `$HERDR_PLUGIN_CONFIG_DIR` (= `<config_dir>/plugins/config/<id>`), from
/// which the host `config.toml` is three levels up.
pub fn host_accent(plugin_config_dir: &Path) -> Option<Color> {
    let host_config = plugin_config_dir
        .parent()?
        .parent()?
        .parent()?
        .join("config.toml");
    let text = std::fs::read_to_string(host_config).ok()?;
    accent_from_config(&text)
}

fn accent_from_config(config_toml: &str) -> Option<Color> {
    let doc: toml::Value = config_toml.parse().ok()?;
    let theme = doc.get("theme");

    // A custom accent override wins, exactly like the host.
    if let Some(custom) = theme
        .and_then(|t| t.get("custom"))
        .and_then(|c| c.get("accent"))
        .and_then(|v| v.as_str())
    {
        if let Some(color) = crate::ui::parse_color(custom) {
            return Some(color);
        }
    }

    // auto_switch picks dark_name/light_name from the host appearance,
    // which a plugin cannot observe; prefer `name`, else assume dark.
    let name = theme
        .and_then(|t| t.get("name"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            theme
                .and_then(|t| t.get("dark_name"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("catppuccin"); // the host default
    builtin_accent(name)
}

/// Accents of herdr's built-in palettes (src/app/state.rs), keyed with the
/// same name normalization as the host's `Palette::from_name`.
fn builtin_accent(name: &str) -> Option<Color> {
    let rgb = |r, g, b| Some(Color::Rgb(r, g, b));
    match name.to_lowercase().replace([' ', '_'], "-").as_str() {
        "catppuccin" | "catppuccin-mocha" => rgb(137, 180, 250),
        "catppuccin-latte" | "latte" | "light" => rgb(30, 102, 245),
        "terminal" => Some(Color::Blue),
        "tokyo-night" | "tokyonight" => rgb(122, 162, 247),
        "tokyo-night-day" | "tokyo-day" | "tokyonight-day" => rgb(46, 125, 233),
        "dracula" => rgb(189, 147, 249),
        "nord" => rgb(136, 192, 208),
        "gruvbox" | "gruvbox-dark" => rgb(215, 153, 33),
        "gruvbox-light" => rgb(7, 102, 120),
        "one-dark" | "onedark" => rgb(97, 175, 239),
        "one-light" | "onelight" => rgb(64, 120, 242),
        "solarized" | "solarized-dark" | "solarized-light" => rgb(38, 139, 210),
        "kanagawa" => rgb(126, 156, 216),
        "kanagawa-lotus" | "lotus" => rgb(77, 105, 155),
        "rose-pine" | "rosepine" => rgb(196, 167, 231),
        "rose-pine-dawn" | "rosepine-dawn" | "dawn" => rgb(144, 122, 169),
        "vesper" => rgb(255, 199, 153),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_builtin_theme_accents() {
        assert_eq!(
            accent_from_config("[theme]\nname = \"dracula\"\n"),
            Some(Color::Rgb(189, 147, 249))
        );
        assert_eq!(
            accent_from_config("[theme]\nname = \"Tokyo Night\"\n"),
            Some(Color::Rgb(122, 162, 247)),
            "same name normalization as the host"
        );
    }

    #[test]
    fn custom_accent_override_wins() {
        let config = "[theme]\nname = \"dracula\"\n\n[theme.custom]\naccent = \"#ff79c6\"\n";
        assert_eq!(
            accent_from_config(config),
            Some(Color::Rgb(0xff, 0x79, 0xc6))
        );
    }

    #[test]
    fn defaults_to_catppuccin_and_falls_back_to_dark_name() {
        assert_eq!(
            accent_from_config("# no theme section\n"),
            Some(Color::Rgb(137, 180, 250)),
            "the host defaults to catppuccin"
        );
        let auto = "[theme]\nauto_switch = true\ndark_name = \"nord\"\n";
        assert_eq!(accent_from_config(auto), Some(Color::Rgb(136, 192, 208)));
    }

    #[test]
    fn unknown_theme_or_broken_config_yields_none() {
        assert_eq!(accent_from_config("[theme]\nname = \"my-theme\"\n"), None);
        assert_eq!(accent_from_config("not [ toml"), None);
    }

    #[test]
    fn host_accent_walks_up_from_the_plugin_config_dir() {
        let root = tempfile::tempdir().unwrap();
        let plugin_dir = root.path().join("plugins/config/some.plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            root.path().join("config.toml"),
            "[theme]\nname = \"dracula\"\n",
        )
        .unwrap();

        assert_eq!(host_accent(&plugin_dir), Some(Color::Rgb(189, 147, 249)));
        assert_eq!(
            host_accent(&root.path().join("plugins/config/missing")),
            Some(Color::Rgb(189, 147, 249)),
            "only the ancestor path matters"
        );
    }
}

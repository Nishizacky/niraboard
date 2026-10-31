use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use ratatui::style::Color;
use serde::Deserialize;

use crate::app::{Card, LayoutMode};

const DEFAULT_MAIN_COLOR: Color = Color::Cyan;
const DEFAULT_BAR_COLOR: Color = Color::Green;
const DEFAULT_STEP: u8 = 5;
const MIN_STEP: u8 = 1;
const MAX_STEP: u8 = 100;

/// Colours used by the dashboard.
///
/// `main` is the accent colour used for the header, focus highlights, buttons,
/// and key hints. `bar` is the normal fill colour of gauges; warning and
/// critical states keep their yellow/red colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colors {
    pub main: Color,
    pub bar: Color,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            main: DEFAULT_MAIN_COLOR,
            bar: DEFAULT_BAR_COLOR,
        }
    }
}

/// Percentage changed by one brightness or volume adjustment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Steps {
    pub brightness: u8,
    pub volume: u8,
}

impl Default for Steps {
    fn default() -> Self {
        Self {
            brightness: DEFAULT_STEP,
            volume: DEFAULT_STEP,
        }
    }
}

impl Steps {
    pub fn brightness_delta(self) -> i8 {
        i8::try_from(self.brightness).unwrap_or(DEFAULT_STEP as i8)
    }

    pub fn volume_delta(self) -> i8 {
        i8::try_from(self.volume).unwrap_or(DEFAULT_STEP as i8)
    }
}

/// User configuration loaded from `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub colors: Colors,
    pub layout: LayoutMode,
    pub steps: Steps,
    /// Cards in display order. Cards that do not fit on screen are not drawn.
    pub cards: Vec<Card>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            colors: Colors::default(),
            layout: LayoutMode::default(),
            steps: Steps::default(),
            cards: Card::ALL.to_vec(),
        }
    }
}

impl Config {
    /// Load the configuration from disk. A missing file means defaults.
    ///
    /// The path is `$NIRABOARD_CONFIG` when set, otherwise
    /// `$XDG_CONFIG_HOME/niraboard/config.toml`, otherwise
    /// `$HOME/.config/niraboard/config.toml`.
    pub fn load() -> Result<Self> {
        let Some(path) = path() else {
            return Ok(Self::default());
        };

        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => {
                return Err(error).with_context(|| format!("failed to read {}", path.display()));
            }
        };

        Self::parse(&text).with_context(|| format!("invalid config {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let raw: RawConfig = toml::from_str(text)?;

        let mut config = Self::default();

        if let Some(value) = raw.colors.main.as_deref() {
            config.colors.main = parse_color(value, "colors.main")?;
        }
        if let Some(value) = raw.colors.bar.as_deref() {
            config.colors.bar = parse_color(value, "colors.bar")?;
        }

        if let Some(value) = raw.layout.as_deref() {
            config.layout = LayoutMode::from_name(value)
                .with_context(|| format!("invalid layout `{value}`"))?;
        }

        if let Some(value) = raw.steps.brightness {
            config.steps.brightness = parse_step(value, "steps.brightness")?;
        }
        if let Some(value) = raw.steps.volume {
            config.steps.volume = parse_step(value, "steps.volume")?;
        }

        if let Some(cards) = raw.cards {
            config.cards = parse_cards(&cards)?;
        }

        Ok(config)
    }

    /// Render this configuration as TOML.
    ///
    /// Used by `--print-default-config`; every value is taken from the live
    /// defaults, so the output cannot drift from what a missing file produces.
    pub fn to_toml(&self) -> String {
        let cards = self
            .cards
            .iter()
            .map(|card| format!("    \"{}\",\n", card.name()))
            .collect::<String>();

        format!(
            "layout = \"{}\"\n\
             \n\
             cards = [\n\
             {cards}\
             ]\n\
             \n\
             [colors]\n\
             main = \"{}\"\n\
             bar = \"{}\"\n\
             \n\
             [steps]\n\
             brightness = {}\n\
             volume = {}\n",
            self.layout.name(),
            self.colors.main,
            self.colors.bar,
            self.steps.brightness,
            self.steps.volume,
        )
    }
}

/// Location of the configuration file, if a home/config directory is known.
pub fn path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("NIRABOARD_CONFIG").filter(|path| !path.is_empty()) {
        return Some(PathBuf::from(path));
    }

    let base = env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|home| !home.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })?;

    Some(base.join("niraboard").join("config.toml"))
}

fn parse_color(value: &str, key: &str) -> Result<Color> {
    Color::from_str(value).with_context(|| {
        format!("invalid colour `{value}` for {key}; use a name, 0-255 index, or #rrggbb")
    })
}

fn parse_step(value: u8, key: &str) -> Result<u8> {
    if !(MIN_STEP..=MAX_STEP).contains(&value) {
        bail!("{key} must be between {MIN_STEP} and {MAX_STEP}");
    }

    Ok(value)
}

fn parse_cards(names: &[String]) -> Result<Vec<Card>> {
    let mut cards = Vec::with_capacity(names.len());

    for (index, name) in names.iter().enumerate() {
        let card = Card::from_name(name)
            .with_context(|| format!("invalid card `{name}` at cards[{index}]"))?;

        if cards.contains(&card) {
            bail!("duplicate card `{}` at cards[{index}]", card.name());
        }

        cards.push(card);
    }

    Ok(cards)
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    colors: RawColors,
    layout: Option<String>,
    #[serde(default)]
    steps: RawSteps,
    cards: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawColors {
    main: Option<String>,
    bar: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSteps {
    brightness: Option<u8>,
    volume: Option<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_renders_as_parseable_toml() {
        let text = Config::default().to_toml();

        assert_eq!(
            Config::parse(&text).expect("rendered config"),
            Config::default()
        );
    }

    #[test]
    fn missing_file_uses_defaults() {
        let config = Config::load_from(Path::new("/nonexistent/niraboard/config.toml"))
            .expect("missing config");

        assert_eq!(config, Config::default());
    }

    #[test]
    fn parses_colors_layout_and_steps() {
        let config = Config::parse(
            r##"
                layout = "two-columns"

                [colors]
                main = "#ff8800"
                bar = "light-blue"

                [steps]
                brightness = 10
                volume = 2
            "##,
        )
        .expect("valid config");

        assert_eq!(config.colors.main, Color::Rgb(0xff, 0x88, 0x00));
        assert_eq!(config.colors.bar, Color::LightBlue);
        assert_eq!(config.layout, LayoutMode::TwoColumns);
        assert_eq!(config.steps.brightness, 10);
        assert_eq!(config.steps.volume, 2);
    }

    #[test]
    fn parses_card_order() {
        let config = Config::parse(r#"cards = ["audio", "cpu", "battery"]"#).expect("valid cards");

        assert_eq!(config.cards, vec![Card::Audio, Card::Cpu, Card::Battery]);
    }

    #[test]
    fn defaults_to_every_card_in_dashboard_order() {
        let config = Config::parse("").expect("empty config");

        assert_eq!(config.cards, Card::ALL.to_vec());
    }

    #[test]
    fn rejects_unknown_card() {
        let error = Config::parse(r#"cards = ["cpu", "gpu"]"#).expect_err("unknown card");

        assert!(format!("{error:#}").contains("invalid card `gpu`"));
    }

    #[test]
    fn rejects_duplicate_card() {
        let error = Config::parse(r#"cards = ["cpu", "cpu"]"#).expect_err("duplicate card");

        assert!(format!("{error:#}").contains("duplicate card `cpu`"));
    }

    #[test]
    fn empty_config_uses_defaults() {
        assert_eq!(Config::parse("").expect("empty config"), Config::default());
    }

    #[test]
    fn rejects_invalid_color() {
        let error = Config::parse(
            r#"
                [colors]
                main = "not-a-colour"
            "#,
        )
        .expect_err("invalid colour");

        assert!(format!("{error:#}").contains("colors.main"));
    }

    #[test]
    fn rejects_invalid_layout() {
        let error = Config::parse(r#"layout = "diagonal""#).expect_err("invalid layout");
        assert!(format!("{error:#}").contains("invalid layout"));
    }

    #[test]
    fn rejects_zero_step() {
        let error = Config::parse("[steps]\nvolume = 0").expect_err("zero step");
        assert!(format!("{error:#}").contains("steps.volume"));
    }

    #[test]
    fn rejects_unknown_keys() {
        let error = Config::parse("[colors]\naccent = \"red\"").expect_err("unknown key");
        assert!(format!("{error:#}").contains("unknown field"));
    }
}

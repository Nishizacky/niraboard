use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::app::{Card, LayoutMode};

const DEFAULT_MAIN_COLOR: Color = Color::Cyan;
const DEFAULT_BAR_COLOR: Color = Color::Green;
const DEFAULT_TEXT_COLOR: Color = Color::Gray;
const DEFAULT_MUTED_COLOR: Color = Color::DarkGray;
const DEFAULT_OK_COLOR: Color = Color::Green;
const DEFAULT_WARN_COLOR: Color = Color::Yellow;
const DEFAULT_BAD_COLOR: Color = Color::Red;
const DEFAULT_STEP: u8 = 5;
const MIN_STEP: u8 = 1;
const MAX_STEP: u8 = 100;

/// Colours used by the dashboard.
///
/// `main` is the accent colour used for the header, focus highlights, buttons,
/// and key hints, while `bar` is the normal fill colour of gauges. `text` is
/// used for regular values and `muted` for labels and secondary hints. `ok`,
/// `warn`, and `bad` colour the state indicators (connected, warning, and
/// critical respectively).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colors {
    pub main: Color,
    pub bar: Color,
    pub text: Color,
    pub muted: Color,
    pub ok: Color,
    pub warn: Color,
    pub bad: Color,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            main: DEFAULT_MAIN_COLOR,
            bar: DEFAULT_BAR_COLOR,
            text: DEFAULT_TEXT_COLOR,
            muted: DEFAULT_MUTED_COLOR,
            ok: DEFAULT_OK_COLOR,
            warn: DEFAULT_WARN_COLOR,
            bad: DEFAULT_BAD_COLOR,
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
    /// `override_path` comes from `--config` and takes precedence. Otherwise the
    /// path is `$NIRABOARD_CONFIG` when set, then
    /// `$XDG_CONFIG_HOME/niraboard/config.toml`, then
    /// `$HOME/.config/niraboard/config.toml`.
    pub fn load(override_path: Option<&Path>) -> Result<Self> {
        let Some(path) = override_path.map(Path::to_path_buf).or_else(path) else {
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
        if let Some(value) = raw.colors.text.as_deref() {
            config.colors.text = parse_color(value, "colors.text")?;
        }
        if let Some(value) = raw.colors.muted.as_deref() {
            config.colors.muted = parse_color(value, "colors.muted")?;
        }
        if let Some(value) = raw.colors.ok.as_deref() {
            config.colors.ok = parse_color(value, "colors.ok")?;
        }
        if let Some(value) = raw.colors.warn.as_deref() {
            config.colors.warn = parse_color(value, "colors.warn")?;
        }
        if let Some(value) = raw.colors.bad.as_deref() {
            config.colors.bad = parse_color(value, "colors.bad")?;
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
    /// Used by `--print-default-config`. The document is serialized from the
    /// same structure that [`Config::parse`] deserializes, so the output cannot
    /// drift from what a missing file produces.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(&RawConfig::from(self))
            .context("failed to render configuration as TOML")
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

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    layout: Option<String>,
    cards: Option<Vec<String>>,
    #[serde(default)]
    colors: RawColors,
    #[serde(default)]
    steps: RawSteps,
}

impl From<&Config> for RawConfig {
    fn from(config: &Config) -> Self {
        Self {
            layout: Some(config.layout.name().to_owned()),
            cards: Some(
                config
                    .cards
                    .iter()
                    .map(|card| card.name().to_owned())
                    .collect(),
            ),
            colors: RawColors {
                main: Some(config.colors.main.to_string()),
                bar: Some(config.colors.bar.to_string()),
                text: Some(config.colors.text.to_string()),
                muted: Some(config.colors.muted.to_string()),
                ok: Some(config.colors.ok.to_string()),
                warn: Some(config.colors.warn.to_string()),
                bad: Some(config.colors.bad.to_string()),
            },
            steps: RawSteps {
                brightness: Some(config.steps.brightness),
                volume: Some(config.steps.volume),
            },
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawColors {
    main: Option<String>,
    bar: Option<String>,
    text: Option<String>,
    muted: Option<String>,
    ok: Option<String>,
    warn: Option<String>,
    bad: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
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
        let text = Config::default().to_toml().expect("rendered config");

        assert_eq!(
            Config::parse(&text).expect("rendered config"),
            Config::default()
        );
    }

    #[test]
    fn custom_config_round_trips_through_toml() {
        let mut config = Config::default();
        config.colors.main = Color::Rgb(0xff, 0x88, 0x00);
        config.colors.bar = Color::Indexed(42);
        config.colors.text = Color::LightBlue;
        config.colors.muted = Color::DarkGray;
        config.colors.ok = Color::LightGreen;
        config.colors.warn = Color::LightYellow;
        config.colors.bad = Color::LightRed;
        config.layout = LayoutMode::ThreeColumns;
        config.steps.brightness = 10;
        config.steps.volume = 1;
        config.cards = vec![Card::Audio, Card::Cpu, Card::Network];

        let text = config.to_toml().expect("rendered config");

        assert_eq!(Config::parse(&text).expect("rendered config"), config);
    }

    #[test]
    fn example_config_matches_defaults() {
        let config =
            Config::parse(include_str!("../examples/config.toml")).expect("example config");

        assert_eq!(config, Config::default());
    }

    #[test]
    fn missing_file_uses_defaults() {
        let config = Config::load_from(Path::new("/nonexistent/niraboard/config.toml"))
            .expect("missing config");

        assert_eq!(config, Config::default());
    }

    #[test]
    fn loads_config_from_explicit_path() {
        let path =
            std::env::temp_dir().join(format!("niraboard-config-{}.toml", std::process::id()));
        fs::write(&path, "layout = \"3-columns\"\n[steps]\nvolume = 2\n").expect("write config");

        let config = Config::load(Some(&path)).expect("loaded config");
        let _ = fs::remove_file(&path);

        assert_eq!(config.layout, LayoutMode::ThreeColumns);
        assert_eq!(config.steps.volume, 2);
    }

    #[test]
    fn parses_colors_layout_and_steps() {
        let config = Config::parse(
            r##"
                layout = "two-columns"

                [colors]
                main = "#ff8800"
                bar = "light-blue"
                text = "white"
                muted = "dark-gray"
                ok = "light-green"
                warn = "light-yellow"
                bad = "light-red"

                [steps]
                brightness = 10
                volume = 2
            "##,
        )
        .expect("valid config");

        assert_eq!(config.colors.main, Color::Rgb(0xff, 0x88, 0x00));
        assert_eq!(config.colors.bar, Color::LightBlue);
        assert_eq!(config.colors.text, Color::White);
        assert_eq!(config.colors.muted, Color::DarkGray);
        assert_eq!(config.colors.ok, Color::LightGreen);
        assert_eq!(config.colors.warn, Color::LightYellow);
        assert_eq!(config.colors.bad, Color::LightRed);
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

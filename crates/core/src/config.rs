use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

impl Edge {
    pub fn is_vertical(self) -> bool {
        matches!(self, Edge::Left | Edge::Right)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backdrop {
    Acrylic,
    Mica,
    Solid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarConfig {
    pub edge: Edge,
    pub align: Align,
    pub thickness: u32,
    pub gap: u32,
    pub corner_radius: u32,
    pub backdrop: Backdrop,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            edge: Edge::Top,
            align: Align::Center,
            thickness: 40,
            gap: 8,
            corner_radius: 12,
            backdrop: Backdrop::Acrylic,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub foreground: [u8; 4],
    pub pill: [u8; 4],
    pub font: String,
    pub font_size: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            foreground: [255, 255, 255, 255],
            pill: [255, 255, 255, 26],
            font: "Segoe UI Variable Text".to_string(),
            font_size: 14.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub bar: BarConfig,
    pub theme: Theme,
    pub modules: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            bar: BarConfig::default(),
            theme: Theme::default(),
            modules: vec!["battery".to_string(), "clock".to_string()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    Syntax(String),
    UnsupportedVersion(u32),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Syntax(msg) => write!(f, "config syntax error: {msg}"),
            ConfigError::UnsupportedVersion(v) => write!(f, "unsupported config version: {v}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn parse(text: &str) -> Result<Config, ConfigError> {
        if text.trim().is_empty() {
            return Ok(Config::default());
        }
        let mut cfg: Config =
            toml::from_str(text).map_err(|e| ConfigError::Syntax(e.to_string()))?;
        if cfg.version > CONFIG_VERSION {
            return Err(ConfigError::UnsupportedVersion(cfg.version));
        }
        cfg.version = CONFIG_VERSION;
        Ok(cfg)
    }

    pub fn clamped(mut self) -> Config {
        self.bar.thickness = self.bar.thickness.clamp(24, 120);
        self.bar.gap = self.bar.gap.min(64);
        self.bar.corner_radius = self.bar.corner_radius.min(40);
        if !self.theme.font_size.is_finite() {
            self.theme.font_size = Theme::default().font_size;
        }
        self.theme.font_size = self.theme.font_size.clamp(8.0, 32.0);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_config() {
        let text = r#"
version = 1
modules = ["clock", "battery"]

[bar]
edge = "bottom"
align = "start"
thickness = 48
gap = 10
corner_radius = 8
backdrop = "mica"

[theme]
foreground = [200, 210, 220, 255]
pill = [10, 20, 30, 40]
font = "Consolas"
font_size = 16.0
"#;
        let cfg = Config::parse(text).expect("valid config");
        assert_eq!(cfg.version, 1);
        assert_eq!(cfg.modules, vec!["clock".to_string(), "battery".to_string()]);
        assert_eq!(cfg.bar.edge, Edge::Bottom);
        assert!(!cfg.bar.edge.is_vertical());
        assert!(Edge::Left.is_vertical());
        assert!(Edge::Right.is_vertical());
        assert_eq!(cfg.bar.align, Align::Start);
        assert_eq!(cfg.bar.thickness, 48);
        assert_eq!(cfg.bar.gap, 10);
        assert_eq!(cfg.bar.corner_radius, 8);
        assert_eq!(cfg.bar.backdrop, Backdrop::Mica);
        assert_eq!(cfg.theme.foreground, [200, 210, 220, 255]);
        assert_eq!(cfg.theme.pill, [10, 20, 30, 40]);
        assert_eq!(cfg.theme.font, "Consolas");
        assert_eq!(cfg.theme.font_size, 16.0);

        let serialized = toml::to_string(&cfg).expect("serializes");
        let reparsed = Config::parse(&serialized).expect("reparses");
        assert_eq!(cfg, reparsed);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let cfg = Config::parse("version = 1").expect("valid");
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn empty_text_is_default_error_free() {
        let cfg = Config::parse("").expect("valid");
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn preserves_user_customization_on_partial_or_older_version() {
        let cfg = Config::parse("version = 0\n[bar]\nedge = \"right\"\nthickness = 48")
            .expect("migrated");
        assert_eq!(cfg.version, 1);
        assert_eq!(cfg.bar.edge, Edge::Right);
        assert_eq!(cfg.bar.thickness, 48);
        assert_eq!(cfg.bar.align, Config::default().bar.align);
        assert_eq!(cfg.bar.gap, Config::default().bar.gap);
        assert_eq!(cfg.bar.corner_radius, Config::default().bar.corner_radius);
        assert_eq!(cfg.bar.backdrop, Config::default().bar.backdrop);
        assert_eq!(cfg.theme, Config::default().theme);
        assert_eq!(cfg.modules, Config::default().modules);
    }

    #[test]
    fn broken_toml_is_syntax_error() {
        let err = Config::parse("version = [not valid").unwrap_err();
        assert!(matches!(err, ConfigError::Syntax(_)));
    }

    #[test]
    fn future_version_rejected() {
        let err = Config::parse("version = 99").unwrap_err();
        assert_eq!(err, ConfigError::UnsupportedVersion(99));
    }

    #[test]
    fn clamped_limits_thickness() {
        let mut low = Config::default();
        low.bar.thickness = 0;
        low.bar.gap = 100;
        low.bar.corner_radius = 100;
        low.theme.font_size = 2.0;
        let low_clamped = low.clamped();
        assert_eq!(low_clamped.bar.thickness, 24);
        assert_eq!(low_clamped.bar.gap, 64);
        assert_eq!(low_clamped.bar.corner_radius, 40);
        assert_eq!(low_clamped.theme.font_size, 8.0);

        let mut high = Config::default();
        high.bar.thickness = 5000;
        high.theme.font_size = 100.0;
        let high_clamped = high.clamped();
        assert_eq!(high_clamped.bar.thickness, 120);
        assert_eq!(high_clamped.theme.font_size, 32.0);

        let mut nan_cfg = Config::default();
        nan_cfg.theme.font_size = f32::NAN;
        let nan_clamped = nan_cfg.clamped();
        assert_eq!(nan_clamped.theme.font_size, 14.0);
    }
}

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetSettings {
    pub show_icon: bool,
    pub show_label: bool,
    pub params: BTreeMap<String, String>,
}

impl Default for WidgetSettings {
    fn default() -> Self {
        Self {
            show_icon: true,
            show_label: true,
            params: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidgetCatalogEntry {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
}

pub const KNOWN_WIDGETS: &[WidgetCatalogEntry] = &[
    WidgetCatalogEntry {
        id: "battery",
        title: "Pil ve Güç",
        description: "Sistem pil yüzdesi ve şarj durumu",
    },
    WidgetCatalogEntry {
        id: "clock",
        title: "Saat",
        description: "Yerel sistem saati (24s / 12s)",
    },
];

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
    pub module_config: BTreeMap<String, WidgetSettings>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            bar: BarConfig::default(),
            theme: Theme::default(),
            modules: vec!["battery".to_string(), "clock".to_string()],
            module_config: BTreeMap::new(),
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

    pub fn widget_settings(&self, id: &str) -> WidgetSettings {
        self.module_config.get(id).cloned().unwrap_or_default()
    }

    pub fn widget_settings_mut(&mut self, id: &str) -> &mut WidgetSettings {
        self.module_config.entry(id.to_string()).or_default()
    }

    pub fn is_module_enabled(&self, id: &str) -> bool {
        self.modules.iter().any(|m| m == id)
    }

    pub fn set_module_enabled(&mut self, id: &str, enabled: bool) {
        if enabled {
            if !self.is_module_enabled(id) {
                self.modules.push(id.to_string());
            }
        } else {
            self.modules.retain(|m| m != id);
        }
    }

    pub fn move_module(&mut self, id: &str, delta: isize) -> bool {
        let Some(idx) = self.modules.iter().position(|m| m == id) else {
            return false;
        };
        let target = idx as isize + delta;
        if target < 0 || target >= self.modules.len() as isize {
            return false;
        }
        let target = target as usize;
        if target == idx {
            return false;
        }
        self.modules.swap(idx, target);
        true
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

    #[test]
    fn module_config_slots_roundtrip_and_preserve_on_migration() {
        let raw = r#"
version = 0
modules = ["clock", "battery"]

[bar]
edge = "right"

[module_config.clock]
show_icon = false
show_label = true
[module_config.clock.params]
format = "12h"

[module_config.battery]
show_icon = false
[module_config.battery.params]
show_bolt = "false"
"#;
        let mut cfg = Config::parse(raw).expect("parses v0 with module_config");
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert_eq!(cfg.bar.edge, Edge::Right);

        let clock = cfg.widget_settings("clock");
        assert!(!clock.show_icon);
        assert!(clock.show_label);
        assert_eq!(clock.params.get("format").map(String::as_str), Some("12h"));

        let battery = cfg.widget_settings("battery");
        assert!(!battery.show_icon);
        assert!(battery.show_label);
        assert_eq!(
            battery.params.get("show_bolt").map(String::as_str),
            Some("false")
        );

        assert!(cfg.is_module_enabled("clock"));
        assert!(cfg.move_module("battery", -1));
        assert_eq!(cfg.modules, vec!["battery".to_string(), "clock".to_string()]);
        assert!(!cfg.move_module("battery", -1));

        cfg.set_module_enabled("battery", false);
        assert!(!cfg.is_module_enabled("battery"));
        assert_eq!(cfg.modules, vec!["clock".to_string()]);

        cfg.set_module_enabled("battery", true);
        cfg.set_module_enabled("battery", true);
        assert_eq!(cfg.modules, vec!["clock".to_string(), "battery".to_string()]);

        let serialized = toml::to_string(&cfg).expect("serializes");
        let reparsed = Config::parse(&serialized).expect("reparses");
        assert_eq!(cfg, reparsed);
        assert!(KNOWN_WIDGETS.iter().any(|w| w.id == "clock"));
        assert!(KNOWN_WIDGETS.iter().any(|w| w.id == "battery"));
    }
}

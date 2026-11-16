use std::fs;
use std::path::PathBuf;

use core::{Align, Backdrop, Config, Edge, WidgetCatalogEntry, KNOWN_WIDGETS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    Widgets,
    About,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    SelectTab(SettingsTab),
    SetEdge(Edge),
    SetAlign(Align),
    SetBackdrop(Backdrop),
    AdjustThickness(i32),
    SetThickness(u32),
    SetThicknessRatio(f32),
    AdjustGap(i32),
    AdjustCornerRadius(i32),
    AdjustFontSize(f32),
    ApplyThemePreset(usize),
    SelectWidget(String),
    ToggleWidget(String),
    MoveWidget { id: String, delta: isize },
    ToggleWidgetIcon(String),
    ToggleWidgetLabel(String),
    SetClockFormat(String),
    ToggleBatteryBolt,
    CheckUpdates,
    Revert,
    Save,
    WindowMinimize,
    WindowMaximize,
    WindowClose,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionEffect {
    None,
    PreviewChanged,
    RevertRequested,
    SaveRequested,
    LaunchUpdater,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePreset {
    pub name: &'static str,
    pub foreground: [u8; 4],
    pub pill: [u8; 4],
}

pub const THEME_PRESETS: &[ThemePreset] = &[
    ThemePreset {
        name: "Buz Beyazı",
        foreground: [255, 255, 255, 255],
        pill: [255, 255, 255, 28],
    },
    ThemePreset {
        name: "Gece Mavisi",
        foreground: [224, 242, 254, 255],
        pill: [56, 189, 248, 42],
    },
    ThemePreset {
        name: "Zümrüt",
        foreground: [220, 252, 231, 255],
        pill: [52, 211, 153, 40],
    },
    ThemePreset {
        name: "Kehribar",
        foreground: [254, 243, 199, 255],
        pill: [251, 191, 36, 42],
    },
];

pub fn default_config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Taskbar").join("config.toml")
}

pub fn load_config_from_disk(path: &std::path::Path) -> Config {
    let Ok(text) = fs::read_to_string(path) else {
        return Config::default();
    };
    Config::parse(&text)
        .map(Config::clamped)
        .unwrap_or_else(|_| Config::default())
}

pub struct SettingsAppState {
    pub tab: SettingsTab,
    pub saved_config: Config,
    pub working_config: Config,
    pub selected_widget: String,
    pub status_text: String,
    pub live_connected: bool,
    pub config_path: PathBuf,
    pub window_maximized: bool,
}

impl SettingsAppState {
    pub fn new(initial: Config, config_path: PathBuf, live_connected: bool) -> Self {
        let clamped = initial.clamped();
        let selected_widget = clamped
            .modules
            .iter()
            .find(|id| KNOWN_WIDGETS.iter().any(|w| w.id == id.as_str()))
            .cloned()
            .unwrap_or_else(|| "battery".to_string());
        Self {
            tab: SettingsTab::Appearance,
            saved_config: clamped.clone(),
            working_config: clamped,
            selected_widget,
            status_text: String::new(),
            live_connected,
            config_path,
            window_maximized: false,
        }
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.working_config != self.saved_config
    }

    pub fn ordered_widgets(&self) -> Vec<&'static WidgetCatalogEntry> {
        let mut out = Vec::with_capacity(KNOWN_WIDGETS.len());
        for active_id in &self.working_config.modules {
            if let Some(entry) = KNOWN_WIDGETS.iter().find(|w| w.id == active_id.as_str()) {
                if !out.iter().any(|e: &&WidgetCatalogEntry| e.id == entry.id) {
                    out.push(entry);
                }
            }
        }
        for entry in KNOWN_WIDGETS {
            if !out.iter().any(|e| e.id == entry.id) {
                out.push(entry);
            }
        }
        out
    }

    pub fn apply_action(&mut self, action: UiAction) -> ActionEffect {
        match action {
            UiAction::SelectTab(tab) => {
                self.tab = tab;
                ActionEffect::None
            }
            UiAction::SetEdge(edge) => {
                self.working_config.bar.edge = edge;
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::SetAlign(align) => {
                self.working_config.bar.align = align;
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::SetBackdrop(backdrop) => {
                self.working_config.bar.backdrop = backdrop;
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::AdjustThickness(delta) => {
                let next = (self.working_config.bar.thickness as i32 + delta).max(0) as u32;
                self.working_config.bar.thickness = next;
                self.working_config = self.working_config.clone().clamped();
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::SetThickness(thickness) => {
                self.working_config.bar.thickness = thickness.clamp(24, 120);
                self.working_config = self.working_config.clone().clamped();
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::SetThicknessRatio(ratio) => {
                let r = ratio.clamp(0.0, 1.0);
                let thickness = (24.0 + r * (120.0 - 24.0)).round() as u32;
                self.working_config.bar.thickness = thickness;
                self.working_config = self.working_config.clone().clamped();
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::AdjustGap(delta) => {
                let next = (self.working_config.bar.gap as i32 + delta).max(0) as u32;
                self.working_config.bar.gap = next;
                self.working_config = self.working_config.clone().clamped();
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::AdjustCornerRadius(delta) => {
                let next = (self.working_config.bar.corner_radius as i32 + delta).max(0) as u32;
                self.working_config.bar.corner_radius = next;
                self.working_config = self.working_config.clone().clamped();
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::AdjustFontSize(delta) => {
                self.working_config.theme.font_size += delta;
                self.working_config = self.working_config.clone().clamped();
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::ApplyThemePreset(idx) => {
                if let Some(preset) = THEME_PRESETS.get(idx) {
                    self.working_config.theme.foreground = preset.foreground;
                    self.working_config.theme.pill = preset.pill;
                    self.status_text.clear();
                    ActionEffect::PreviewChanged
                } else {
                    ActionEffect::None
                }
            }
            UiAction::SelectWidget(id) => {
                self.selected_widget = id;
                ActionEffect::None
            }
            UiAction::ToggleWidget(id) => {
                let enabled = self.working_config.is_module_enabled(&id);
                self.working_config.set_module_enabled(&id, !enabled);
                self.selected_widget = id;
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::MoveWidget { id, delta } => {
                if self.working_config.move_module(&id, delta) {
                    self.selected_widget = id;
                    self.status_text.clear();
                    ActionEffect::PreviewChanged
                } else {
                    ActionEffect::None
                }
            }
            UiAction::ToggleWidgetIcon(id) => {
                let slot = self.working_config.widget_settings_mut(&id);
                slot.show_icon = !slot.show_icon;
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::ToggleWidgetLabel(id) => {
                let slot = self.working_config.widget_settings_mut(&id);
                slot.show_label = !slot.show_label;
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::SetClockFormat(fmt) => {
                self.working_config
                    .widget_settings_mut("clock")
                    .params
                    .insert("format".to_string(), fmt);
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::ToggleBatteryBolt => {
                let slot = self.working_config.widget_settings_mut("battery");
                let currently_on =
                    slot.params.get("show_bolt").map(|s| s.as_str()) != Some("false");
                slot.params.insert(
                    "show_bolt".to_string(),
                    if currently_on { "false" } else { "true" }.to_string(),
                );
                self.status_text.clear();
                ActionEffect::PreviewChanged
            }
            UiAction::CheckUpdates => ActionEffect::LaunchUpdater,
            UiAction::Revert => {
                self.revert_changes();
                ActionEffect::RevertRequested
            }
            UiAction::Save => ActionEffect::SaveRequested,
            UiAction::WindowMinimize | UiAction::WindowMaximize | UiAction::WindowClose => {
                ActionEffect::None
            }
        }
    }

    pub fn save_to_disk(&mut self) -> Result<(), String> {
        let clamped = self.working_config.clone().clamped();
        if let Some(parent) = self.config_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("dizin oluşturulamadı: {e}"))?;
            }
        }
        let text =
            toml::to_string_pretty(&clamped).map_err(|e| format!("TOML serileştirme: {e}"))?;
        fs::write(&self.config_path, text).map_err(|e| format!("dosya yazılamadı: {e}"))?;
        self.working_config = clamped.clone();
        self.saved_config = clamped;
        self.status_text = "Ayarlar kaydedildi ✓".to_string();
        Ok(())
    }

    pub fn revert_changes(&mut self) {
        self.working_config = self.saved_config.clone();
        self.status_text = "Değişiklikler geri alındı".to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_cfg_path(name: &str) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "taskbar_settings_state_{}_{}_{}",
            std::process::id(),
            name,
            id
        ));
        let _ = fs::remove_dir_all(&dir);
        dir.join("Taskbar").join("config.toml")
    }

    #[test]
    fn appearance_actions_mutate_working_config_and_emit_preview() {
        let path = temp_cfg_path("appearance");
        let mut state = SettingsAppState::new(Config::default(), path, true);
        assert!(!state.has_unsaved_changes());

        assert_eq!(
            state.apply_action(UiAction::SetEdge(Edge::Right)),
            ActionEffect::PreviewChanged
        );
        assert_eq!(state.working_config.bar.edge, Edge::Right);
        assert!(state.has_unsaved_changes());

        assert_eq!(
            state.apply_action(UiAction::SetAlign(Align::End)),
            ActionEffect::PreviewChanged
        );
        assert_eq!(state.working_config.bar.align, Align::End);

        assert_eq!(
            state.apply_action(UiAction::SetBackdrop(Backdrop::Mica)),
            ActionEffect::PreviewChanged
        );
        assert_eq!(state.working_config.bar.backdrop, Backdrop::Mica);

        assert_eq!(
            state.apply_action(UiAction::AdjustThickness(200)),
            ActionEffect::PreviewChanged
        );
        assert_eq!(state.working_config.bar.thickness, 120);

        assert_eq!(
            state.apply_action(UiAction::SetThicknessRatio(0.0)),
            ActionEffect::PreviewChanged
        );
        assert_eq!(state.working_config.bar.thickness, 24);

        assert_eq!(
            state.apply_action(UiAction::AdjustGap(4)),
            ActionEffect::PreviewChanged
        );
        assert_eq!(state.working_config.bar.gap, 12);

        assert_eq!(
            state.apply_action(UiAction::AdjustCornerRadius(-4)),
            ActionEffect::PreviewChanged
        );
        assert_eq!(state.working_config.bar.corner_radius, 8);

        assert_eq!(
            state.apply_action(UiAction::AdjustFontSize(2.0)),
            ActionEffect::PreviewChanged
        );
        assert_eq!(state.working_config.theme.font_size, 16.0);

        assert_eq!(
            state.apply_action(UiAction::ApplyThemePreset(1)),
            ActionEffect::PreviewChanged
        );
        assert_eq!(
            state.working_config.theme.foreground,
            THEME_PRESETS[1].foreground
        );
        assert_eq!(state.working_config.theme.pill, THEME_PRESETS[1].pill);
    }

    #[test]
    fn widget_toggle_reorder_and_slot_settings_update_working_config() {
        let path = temp_cfg_path("widgets");
        let mut state = SettingsAppState::new(Config::default(), path, false);
        assert_eq!(
            state
                .ordered_widgets()
                .iter()
                .map(|w| w.id)
                .collect::<Vec<_>>(),
            vec!["battery", "clock"]
        );

        assert_eq!(
            state.apply_action(UiAction::MoveWidget {
                id: "clock".to_string(),
                delta: -1
            }),
            ActionEffect::PreviewChanged
        );
        assert_eq!(
            state.working_config.modules,
            vec!["clock".to_string(), "battery".to_string()]
        );
        assert_eq!(
            state
                .ordered_widgets()
                .iter()
                .map(|w| w.id)
                .collect::<Vec<_>>(),
            vec!["clock", "battery"]
        );

        assert_eq!(
            state.apply_action(UiAction::ToggleWidget("battery".to_string())),
            ActionEffect::PreviewChanged
        );
        assert!(!state.working_config.is_module_enabled("battery"));

        assert_eq!(
            state.apply_action(UiAction::SelectWidget("clock".to_string())),
            ActionEffect::None
        );
        assert_eq!(
            state.apply_action(UiAction::SetClockFormat("12h".to_string())),
            ActionEffect::PreviewChanged
        );
        assert_eq!(
            state
                .working_config
                .widget_settings("clock")
                .params
                .get("format")
                .map(String::as_str),
            Some("12h")
        );

        assert_eq!(
            state.apply_action(UiAction::ToggleWidgetIcon("battery".to_string())),
            ActionEffect::PreviewChanged
        );
        assert!(!state.working_config.widget_settings("battery").show_icon);

        assert_eq!(
            state.apply_action(UiAction::ToggleWidgetLabel("clock".to_string())),
            ActionEffect::PreviewChanged
        );
        assert!(!state.working_config.widget_settings("clock").show_label);

        assert_eq!(
            state.apply_action(UiAction::ToggleBatteryBolt),
            ActionEffect::PreviewChanged
        );
        assert_eq!(
            state
                .working_config
                .widget_settings("battery")
                .params
                .get("show_bolt")
                .map(String::as_str),
            Some("false")
        );
    }

    #[test]
    fn save_persists_to_disk_and_revert_restores_saved_config() {
        let path = temp_cfg_path("save_revert");
        let mut state = SettingsAppState::new(Config::default(), path.clone(), true);

        state.apply_action(UiAction::SetEdge(Edge::Left));
        assert_eq!(state.apply_action(UiAction::Revert), ActionEffect::RevertRequested);
        assert_eq!(state.working_config.bar.edge, Edge::Top);
        assert!(!state.has_unsaved_changes());

        state.apply_action(UiAction::SetEdge(Edge::Right));
        state.apply_action(UiAction::SetClockFormat("12h".to_string()));
        assert_eq!(state.apply_action(UiAction::Save), ActionEffect::SaveRequested);
        state.save_to_disk().expect("save_to_disk succeeds");
        assert!(!state.has_unsaved_changes());
        assert!(path.exists());

        let reloaded = load_config_from_disk(&path);
        assert_eq!(reloaded.bar.edge, Edge::Right);
        assert_eq!(
            reloaded
                .widget_settings("clock")
                .params
                .get("format")
                .map(String::as_str),
            Some("12h")
        );

        let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn set_thickness_clamps_between_24_and_120() {
        let path = temp_cfg_path("thickness_clamp");
        let mut state = SettingsAppState::new(Config::default(), path, true);

        assert_eq!(
            state.apply_action(UiAction::SetThickness(10)),
            ActionEffect::PreviewChanged
        );
        assert_eq!(state.working_config.bar.thickness, 24);

        assert_eq!(
            state.apply_action(UiAction::SetThickness(200)),
            ActionEffect::PreviewChanged
        );
        assert_eq!(state.working_config.bar.thickness, 120);

        assert_eq!(
            state.apply_action(UiAction::SetThickness(56)),
            ActionEffect::PreviewChanged
        );
        assert_eq!(state.working_config.bar.thickness, 56);
    }

    #[test]
    fn window_control_actions_do_not_dirty_config() {
        let path = temp_cfg_path("window_controls");
        let mut state = SettingsAppState::new(Config::default(), path, true);
        assert!(!state.window_maximized);

        for action in [
            UiAction::WindowMinimize,
            UiAction::WindowMaximize,
            UiAction::WindowClose,
        ] {
            assert_eq!(state.apply_action(action), ActionEffect::None);
            assert!(!state.has_unsaved_changes());
        }
    }
}

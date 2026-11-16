use std::collections::HashMap;

pub const MIN_SPLASH_SECS: f32 = 0.85;
pub const FADE_DURATION_SECS: f32 = 0.26;
pub const TAB_ROW_STEP: f32 = 46.0;
pub const TAB_BASE_Y: f32 = 82.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppPhase {
    Splash,
    Main,
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn exp_smooth(current: f32, target: f32, speed: f32, dt: f32, eps: f32) -> f32 {
    if (target - current).abs() <= eps {
        return target;
    }
    let alpha = 1.0 - (-speed * dt.max(0.0)).exp();
    let next = current + (target - current) * alpha;
    if (target - next).abs() <= eps {
        target
    } else {
        next
    }
}

#[derive(Debug, Clone)]
pub struct UiAnimState {
    phase: AppPhase,
    splash_elapsed: f32,
    main_fade_elapsed: f32,
    tab_fade_elapsed: f32,
    active_tab_y: f32,
    thickness_visual: f32,
    hovered_id: Option<String>,
    pressed_id: Option<String>,
    hover_amounts: HashMap<String, f32>,
    press_amounts: HashMap<String, f32>,
}

impl UiAnimState {
    pub fn new(initial_tab_idx: usize, initial_thickness: u32) -> Self {
        Self {
            phase: AppPhase::Splash,
            splash_elapsed: 0.0,
            main_fade_elapsed: 0.0,
            tab_fade_elapsed: FADE_DURATION_SECS,
            active_tab_y: TAB_BASE_Y + initial_tab_idx as f32 * TAB_ROW_STEP,
            thickness_visual: initial_thickness.clamp(24, 120) as f32,
            hovered_id: None,
            pressed_id: None,
            hover_amounts: HashMap::new(),
            press_amounts: HashMap::new(),
        }
    }

    pub fn settled_main(tab_idx: usize, thickness: u32) -> Self {
        let mut s = Self::new(tab_idx, thickness);
        s.phase = AppPhase::Main;
        s.splash_elapsed = MIN_SPLASH_SECS;
        s.main_fade_elapsed = FADE_DURATION_SECS;
        s.tab_fade_elapsed = FADE_DURATION_SECS;
        s
    }

    pub fn phase(&self) -> AppPhase {
        self.phase
    }

    pub fn splash_progress(&self) -> f32 {
        ease_in_out_cubic((self.splash_elapsed / MIN_SPLASH_SECS).clamp(0.0, 1.0))
    }

    pub fn main_alpha(&self) -> f32 {
        match self.phase {
            AppPhase::Splash => 0.0,
            AppPhase::Main => ease_out_cubic((self.main_fade_elapsed / FADE_DURATION_SECS).clamp(0.0, 1.0)),
        }
    }

    pub fn tab_alpha(&self) -> f32 {
        ease_out_cubic((self.tab_fade_elapsed / FADE_DURATION_SECS).clamp(0.0, 1.0))
    }

    pub fn active_tab_y(&self) -> f32 {
        self.active_tab_y
    }

    pub fn thickness_visual(&self) -> f32 {
        self.thickness_visual
    }

    pub fn hover_amount(&self, id: &str) -> f32 {
        self.hover_amounts.get(id).copied().unwrap_or(0.0)
    }

    pub fn press_amount(&self, id: &str) -> f32 {
        self.press_amounts.get(id).copied().unwrap_or(0.0)
    }

    pub fn set_hovered(&mut self, id: Option<String>) {
        if let Some(ref key) = id {
            self.hover_amounts.entry(key.clone()).or_insert(0.0);
        }
        self.hovered_id = id;
    }

    pub fn set_pressed(&mut self, id: Option<String>) {
        if let Some(ref key) = id {
            self.press_amounts.entry(key.clone()).or_insert(0.0);
        }
        self.pressed_id = id;
    }

    pub fn on_tab_changed(&mut self, _tab_idx: usize) {
        self.tab_fade_elapsed = 0.0;
        self.hovered_id = None;
        self.pressed_id = None;
        self.hover_amounts.clear();
        self.press_amounts.clear();
    }

    pub fn is_animating(&self, target_tab_idx: usize, target_thickness: u32) -> bool {
        if self.phase == AppPhase::Splash
            || self.main_fade_elapsed < FADE_DURATION_SECS
            || self.tab_fade_elapsed < FADE_DURATION_SECS
        {
            return true;
        }
        let target_y = TAB_BASE_Y + target_tab_idx as f32 * TAB_ROW_STEP;
        if (self.active_tab_y - target_y).abs() > 0.05 {
            return true;
        }
        let target_t = target_thickness.clamp(24, 120) as f32;
        if (self.thickness_visual - target_t).abs() > 0.05 {
            return true;
        }
        for (id, &val) in &self.hover_amounts {
            let target = if self.hovered_id.as_deref() == Some(id.as_str()) {
                1.0
            } else {
                0.0
            };
            if (val - target).abs() > 0.005 {
                return true;
            }
        }
        for (id, &val) in &self.press_amounts {
            let target = if self.pressed_id.as_deref() == Some(id.as_str()) {
                1.0
            } else {
                0.0
            };
            if (val - target).abs() > 0.005 {
                return true;
            }
        }
        false
    }

    pub fn tick(&mut self, dt_secs: f32, target_tab_idx: usize, target_thickness: u32) {
        let dt = dt_secs.clamp(0.0, 0.1);

        match self.phase {
            AppPhase::Splash => {
                self.splash_elapsed += dt;
                if self.splash_elapsed >= MIN_SPLASH_SECS {
                    self.splash_elapsed = MIN_SPLASH_SECS;
                    self.phase = AppPhase::Main;
                    self.main_fade_elapsed = 0.0;
                }
            }
            AppPhase::Main => {
                if self.main_fade_elapsed < FADE_DURATION_SECS {
                    self.main_fade_elapsed = (self.main_fade_elapsed + dt).min(FADE_DURATION_SECS);
                }
            }
        }

        if self.tab_fade_elapsed < FADE_DURATION_SECS {
            self.tab_fade_elapsed = (self.tab_fade_elapsed + dt).min(FADE_DURATION_SECS);
        }

        let target_y = TAB_BASE_Y + target_tab_idx as f32 * TAB_ROW_STEP;
        self.active_tab_y = exp_smooth(self.active_tab_y, target_y, 18.0, dt, 0.05);

        let target_t = target_thickness.clamp(24, 120) as f32;
        self.thickness_visual = exp_smooth(self.thickness_visual, target_t, 22.0, dt, 0.05);

        for (id, val) in &mut self.hover_amounts {
            let target = if self.hovered_id.as_deref() == Some(id.as_str()) {
                1.0
            } else {
                0.0
            };
            *val = exp_smooth(*val, target, 16.0, dt, 0.005);
        }
        self.hover_amounts
            .retain(|id, &mut v| v > 0.0 || self.hovered_id.as_deref() == Some(id.as_str()));

        for (id, val) in &mut self.press_amounts {
            let target = if self.pressed_id.as_deref() == Some(id.as_str()) {
                1.0
            } else {
                0.0
            };
            *val = exp_smooth(*val, target, 24.0, dt, 0.005);
        }
        self.press_amounts
            .retain(|id, &mut v| v > 0.0 || self.pressed_id.as_deref() == Some(id.as_str()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_completes_after_min_duration_and_fades_into_main() {
        let mut anim = UiAnimState::new(0, 40);
        assert_eq!(anim.phase(), AppPhase::Splash);
        assert_eq!(anim.splash_progress(), 0.0);
        assert_eq!(anim.main_alpha(), 0.0);

        for _ in 0..4 {
            anim.tick(0.1, 0, 40);
        }
        assert_eq!(anim.phase(), AppPhase::Splash);
        assert!(anim.splash_progress() > 0.0 && anim.splash_progress() < 1.0);

        for _ in 0..5 {
            anim.tick(0.1, 0, 40);
        }
        assert_eq!(anim.phase(), AppPhase::Main);
        assert_eq!(anim.splash_progress(), 1.0);

        for _ in 0..4 {
            anim.tick(0.1, 0, 40);
        }
        assert_eq!(anim.main_alpha(), 1.0);
    }

    #[test]
    fn tab_and_hover_interpolate_smoothly() {
        let mut anim = UiAnimState::new(0, 40);
        for _ in 0..10 {
            anim.tick(0.1, 0, 40);
        }
        let start_y = anim.active_tab_y();

        anim.on_tab_changed(1);
        anim.set_hovered(Some("btn".to_string()));
        anim.tick(0.05, 1, 64);

        assert!(anim.hover_amount("btn") > 0.0);
        assert!(anim.active_tab_y() > start_y);
        assert!(anim.thickness_visual() > 40.0);
    }

    #[test]
    fn revert_after_thickness_drag_restores_config_and_visual_slider() {
        use crate::state::{SettingsAppState, UiAction};
        use core::Config;
        use std::path::PathBuf;

        let mut state = SettingsAppState::new(Config::default(), PathBuf::from("dummy"), true);
        let mut anim = UiAnimState::settled_main(0, state.working_config.bar.thickness);
        let orig_thickness = state.working_config.bar.thickness;

        state.apply_action(UiAction::SetThickness(96));
        for _ in 0..30 {
            anim.tick(0.016, 0, state.working_config.bar.thickness);
        }
        assert!((anim.thickness_visual() - 96.0).abs() < 1.0);

        state.apply_action(UiAction::Revert);
        assert_eq!(state.working_config.bar.thickness, orig_thickness);
        for _ in 0..30 {
            anim.tick(0.016, 0, state.working_config.bar.thickness);
        }
        assert!((anim.thickness_visual() - orig_thickness as f32).abs() < 1.0);
    }

    #[test]
    fn is_animating_becomes_false_once_settled() {
        let mut anim = UiAnimState::new(0, 40);
        assert!(anim.is_animating(0, 40));

        for _ in 0..120 {
            anim.tick(0.016, 0, 40);
        }
        assert!(!anim.is_animating(0, 40));

        anim.set_hovered(Some("save".to_string()));
        assert!(anim.is_animating(0, 40));
    }
}

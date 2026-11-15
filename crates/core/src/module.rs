use crate::config::{Config, Edge, WidgetSettings};
use crate::modules::{BatteryModule, ClockModule};
use std::collections::BTreeMap;
use std::rc::Rc;

pub trait Module {
    fn id(&self) -> &str;
    fn text(&self, edge: Edge) -> String;
    fn refresh(&mut self) -> bool;
}

pub trait Clock {
    fn now(&self) -> (u8, u8);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerState {
    pub percent: u8,
    pub on_ac: bool,
}

pub trait PowerSource {
    fn read(&self) -> Option<PowerState>;
}

struct SharedClock(Rc<dyn Clock>);

impl Clock for SharedClock {
    fn now(&self) -> (u8, u8) {
        self.0.now()
    }
}

struct SharedPower(Rc<dyn PowerSource>);

impl PowerSource for SharedPower {
    fn read(&self) -> Option<PowerState> {
        self.0.read()
    }
}

fn build_modules_inner(
    ids: &[String],
    module_config: &BTreeMap<String, WidgetSettings>,
    clock: Box<dyn Clock>,
    power: Box<dyn PowerSource>,
) -> Vec<Box<dyn Module>> {
    let clock: Rc<dyn Clock> = Rc::from(clock);
    let power: Rc<dyn PowerSource> = Rc::from(power);
    let mut out: Vec<Box<dyn Module>> = Vec::new();
    for id in ids {
        let settings = module_config.get(id).cloned().unwrap_or_default();
        match id.as_str() {
            "clock" => out.push(Box::new(ClockModule::with_settings(
                Box::new(SharedClock(Rc::clone(&clock))),
                settings,
            ))),
            "battery" => out.push(Box::new(BatteryModule::with_settings(
                Box::new(SharedPower(Rc::clone(&power))),
                settings,
            ))),
            _ => {}
        }
    }
    out
}

pub fn build_modules(
    ids: &[String],
    clock: Box<dyn Clock>,
    power: Box<dyn PowerSource>,
) -> Vec<Box<dyn Module>> {
    build_modules_inner(ids, &BTreeMap::new(), clock, power)
}

pub fn build_modules_with_config(
    cfg: &Config,
    clock: Box<dyn Clock>,
    power: Box<dyn PowerSource>,
) -> Vec<Box<dyn Module>> {
    build_modules_inner(&cfg.modules, &cfg.module_config, clock, power)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubClock;
    impl Clock for StubClock {
        fn now(&self) -> (u8, u8) {
            (9, 5)
        }
    }

    struct StubPower;
    impl PowerSource for StubPower {
        fn read(&self) -> Option<PowerState> {
            Some(PowerState {
                percent: 82,
                on_ac: true,
            })
        }
    }

    #[test]
    fn unknown_module_ids_skipped() {
        let ids = vec![
            "unknown".to_string(),
            "battery".to_string(),
            "nonexistent".to_string(),
            "clock".to_string(),
        ];
        let mods = build_modules(&ids, Box::new(StubClock), Box::new(StubPower));
        let built_ids: Vec<&str> = mods.iter().map(|m| m.id()).collect();
        assert_eq!(built_ids, vec!["battery", "clock"]);
    }

    #[test]
    fn build_modules_with_config_passes_widget_settings() {
        let mut cfg = crate::config::Config::default();
        cfg.modules = vec!["clock".to_string(), "battery".to_string()];
        cfg.widget_settings_mut("clock")
            .params
            .insert("format".to_string(), "12h".to_string());
        cfg.widget_settings_mut("battery").show_icon = false;

        let mods = build_modules_with_config(&cfg, Box::new(StubClock), Box::new(StubPower));
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].text(Edge::Top), "09:05 AM");
        assert_eq!(mods[1].text(Edge::Top), "82% ⚡");
    }
}

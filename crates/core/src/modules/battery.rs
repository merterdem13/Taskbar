use crate::config::{Edge, WidgetSettings};
use crate::module::{Module, PowerSource, PowerState};

pub struct BatteryModule {
    source: Box<dyn PowerSource>,
    current: Option<PowerState>,
    settings: WidgetSettings,
}

impl BatteryModule {
    pub fn new(source: Box<dyn PowerSource>) -> Self {
        Self::with_settings(source, WidgetSettings::default())
    }

    pub fn with_settings(source: Box<dyn PowerSource>, settings: WidgetSettings) -> Self {
        let current = source.read();
        Self {
            source,
            current,
            settings,
        }
    }
}

impl Module for BatteryModule {
    fn id(&self) -> &str {
        "battery"
    }

    fn text(&self, edge: Edge) -> String {
        let vertical = edge.is_vertical();
        let show_icon = self.settings.show_icon;
        let show_bolt = self
            .settings
            .params
            .get("show_bolt")
            .map(|s| s.as_str())
            != Some("false");

        match self.current {
            Some(PowerState { percent, on_ac }) => {
                let charging = on_ac && show_bolt;
                if vertical {
                    if charging {
                        format!("{percent}%\n⚡")
                    } else {
                        format!("{percent}%")
                    }
                } else {
                    let prefix = if show_icon { "🔋 " } else { "" };
                    let suffix = if charging { " ⚡" } else { "" };
                    format!("{prefix}{percent}%{suffix}")
                }
            }
            None => {
                if vertical || !show_icon {
                    "AC".to_string()
                } else {
                    "🔌 AC".to_string()
                }
            }
        }
    }

    fn refresh(&mut self) -> bool {
        let next = self.source.read();
        if next != self.current {
            self.current = next;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    struct FakePower {
        state: Rc<Cell<Option<PowerState>>>,
    }

    impl PowerSource for FakePower {
        fn read(&self) -> Option<PowerState> {
            self.state.get()
        }
    }

    #[test]
    fn battery_text_horizontal_and_vertical() {
        let state = Rc::new(Cell::new(Some(PowerState {
            percent: 82,
            on_ac: true,
        })));
        let mut m = BatteryModule::new(Box::new(FakePower {
            state: Rc::clone(&state),
        }));
        assert_eq!(m.id(), "battery");
        assert_eq!(m.text(Edge::Top), "🔋 82% ⚡");
        assert_eq!(m.text(Edge::Bottom), "🔋 82% ⚡");
        assert_eq!(m.text(Edge::Left), "82%\n⚡");
        assert_eq!(m.text(Edge::Right), "82%\n⚡");
        assert!(!m.refresh());

        state.set(Some(PowerState {
            percent: 82,
            on_ac: false,
        }));
        assert!(m.refresh());
        assert_eq!(m.text(Edge::Top), "🔋 82%");
        assert_eq!(m.text(Edge::Left), "82%");
        assert!(!m.refresh());
    }

    #[test]
    fn battery_none_shows_ac_only_label() {
        let state = Rc::new(Cell::new(None));
        let m = BatteryModule::new(Box::new(FakePower { state }));
        assert_eq!(m.text(Edge::Top), "🔌 AC");
        assert_eq!(m.text(Edge::Bottom), "🔌 AC");
        assert_eq!(m.text(Edge::Right), "AC");
        assert_eq!(m.text(Edge::Left), "AC");
    }

    #[test]
    fn battery_respects_show_icon_and_show_bolt_settings() {
        let state = Rc::new(Cell::new(Some(PowerState {
            percent: 82,
            on_ac: true,
        })));
        let mut settings = crate::config::WidgetSettings::default();
        settings.show_icon = false;
        let m_no_icon = BatteryModule::with_settings(
            Box::new(FakePower {
                state: Rc::clone(&state),
            }),
            settings.clone(),
        );
        assert_eq!(m_no_icon.text(Edge::Top), "82% ⚡");
        assert_eq!(m_no_icon.text(Edge::Left), "82%\n⚡");

        settings.show_icon = true;
        settings
            .params
            .insert("show_bolt".to_string(), "false".to_string());
        let m_no_bolt = BatteryModule::with_settings(
            Box::new(FakePower {
                state: Rc::clone(&state),
            }),
            settings,
        );
        assert_eq!(m_no_bolt.text(Edge::Top), "🔋 82%");
        assert_eq!(m_no_bolt.text(Edge::Right), "82%");

        let ac_none = Rc::new(Cell::new(None));
        let mut no_icon_none = crate::config::WidgetSettings::default();
        no_icon_none.show_icon = false;
        let m_none =
            BatteryModule::with_settings(Box::new(FakePower { state: ac_none }), no_icon_none);
        assert_eq!(m_none.text(Edge::Top), "AC");
    }
}

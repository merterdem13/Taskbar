use crate::config::{Edge, WidgetSettings};
use crate::module::{Clock, Module};

pub struct ClockModule {
    clock: Box<dyn Clock>,
    current: (u8, u8),
    settings: WidgetSettings,
}

impl ClockModule {
    pub fn new(clock: Box<dyn Clock>) -> Self {
        Self::with_settings(clock, WidgetSettings::default())
    }

    pub fn with_settings(clock: Box<dyn Clock>, settings: WidgetSettings) -> Self {
        let current = clock.now();
        Self {
            clock,
            current,
            settings,
        }
    }
}

impl Module for ClockModule {
    fn id(&self) -> &str {
        "clock"
    }

    fn text(&self, edge: Edge) -> String {
        let use_12h = self
            .settings
            .params
            .get("format")
            .map(|s| s.as_str())
            == Some("12h");
        let (h, m) = self.current;
        let display_h = if use_12h {
            match h % 12 {
                0 => 12,
                v => v,
            }
        } else {
            h
        };

        if edge.is_vertical() {
            format!("{display_h:02}\n{m:02}")
        } else if use_12h && self.settings.show_label {
            let suffix = if h < 12 { "AM" } else { "PM" };
            format!("{display_h:02}:{m:02} {suffix}")
        } else {
            format!("{display_h:02}:{m:02}")
        }
    }

    fn refresh(&mut self) -> bool {
        let next = self.clock.now();
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

    struct FakeClock {
        time: Rc<Cell<(u8, u8)>>,
    }

    impl Clock for FakeClock {
        fn now(&self) -> (u8, u8) {
            self.time.get()
        }
    }

    #[test]
    fn clock_formats_horizontal_and_vertical() {
        let time = Rc::new(Cell::new((9, 5)));
        let m = ClockModule::new(Box::new(FakeClock { time }));
        assert_eq!(m.id(), "clock");
        assert_eq!(m.text(Edge::Top), "09:05");
        assert_eq!(m.text(Edge::Bottom), "09:05");
        assert_eq!(m.text(Edge::Right), "09\n05");
        assert_eq!(m.text(Edge::Left), "09\n05");
    }

    #[test]
    fn clock_refresh_false_when_unchanged() {
        let time = Rc::new(Cell::new((9, 5)));
        let mut m = ClockModule::new(Box::new(FakeClock {
            time: Rc::clone(&time),
        }));
        assert!(!m.refresh());
        time.set((9, 6));
        assert!(m.refresh());
        assert_eq!(m.text(Edge::Top), "09:06");
        assert!(!m.refresh());
    }

    #[test]
    fn clock_respects_12h_format_widget_setting() {
        let mut settings = crate::config::WidgetSettings::default();
        settings
            .params
            .insert("format".to_string(), "12h".to_string());

        let time = Rc::new(Cell::new((0, 5)));
        let mut m = ClockModule::with_settings(
            Box::new(FakeClock {
                time: Rc::clone(&time),
            }),
            settings.clone(),
        );
        assert_eq!(m.text(Edge::Top), "12:05 AM");
        assert_eq!(m.text(Edge::Right), "12\n05");

        time.set((14, 30));
        assert!(m.refresh());
        assert_eq!(m.text(Edge::Top), "02:30 PM");
        assert_eq!(m.text(Edge::Left), "02\n30");

        settings.show_label = false;
        let m_no_label = ClockModule::with_settings(Box::new(FakeClock { time }), settings);
        assert_eq!(m_no_label.text(Edge::Top), "02:30");
    }
}

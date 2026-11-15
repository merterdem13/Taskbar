use crate::config::Edge;
use crate::module::{Module, PowerSource, PowerState};

pub struct BatteryModule {
    source: Box<dyn PowerSource>,
    current: Option<PowerState>,
}

impl BatteryModule {
    pub fn new(source: Box<dyn PowerSource>) -> Self {
        let current = source.read();
        Self { source, current }
    }
}

impl Module for BatteryModule {
    fn id(&self) -> &str {
        "battery"
    }

    fn text(&self, edge: Edge) -> String {
        let vertical = edge.is_vertical();
        match self.current {
            Some(PowerState { percent, on_ac }) => match (vertical, on_ac) {
                (false, true) => format!("🔋 {percent}% ⚡"),
                (false, false) => format!("🔋 {percent}%"),
                (true, true) => format!("{percent}%\n⚡"),
                (true, false) => format!("{percent}%"),
            },
            None => {
                if vertical {
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
}

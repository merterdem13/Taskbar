use crate::config::Edge;
use crate::module::{Clock, Module};

pub struct ClockModule {
    clock: Box<dyn Clock>,
    current: (u8, u8),
}

impl ClockModule {
    pub fn new(clock: Box<dyn Clock>) -> Self {
        let current = clock.now();
        Self { clock, current }
    }
}

impl Module for ClockModule {
    fn id(&self) -> &str {
        "clock"
    }

    fn text(&self, edge: Edge) -> String {
        let (h, m) = self.current;
        if edge.is_vertical() {
            format!("{h:02}\n{m:02}")
        } else {
            format!("{h:02}:{m:02}")
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
}

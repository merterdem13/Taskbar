use crate::layout::{Rect, Size};

pub trait Canvas {
    fn fill_round_rect(&mut self, r: Rect, radius: f32, color: [u8; 4]);
    fn draw_text(&mut self, r: Rect, text: &str, color: [u8; 4], font: &str, size: f32);
    fn measure_text(&mut self, text: &str, font: &str, size: f32) -> Size;
}

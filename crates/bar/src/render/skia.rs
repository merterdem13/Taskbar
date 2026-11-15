use std::collections::HashMap;
use std::mem::size_of;
use std::sync::{Mutex, OnceLock};
use fontdue::{Font, FontSettings, Metrics};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};
use windows::core::Result;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    GetDC, ReleaseDC, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use core::{Canvas, Rect, Size};

#[derive(Clone)]
struct CachedGlyph {
    metrics: Metrics,
    bitmap: Vec<u8>,
}

#[derive(Default)]
struct GlyphCache {
    line_metrics: HashMap<u32, (f32, f32, i32)>,
    glyphs: HashMap<(char, u32), CachedGlyph>,
}

static GLYPH_CACHE: OnceLock<Mutex<GlyphCache>> = OnceLock::new();

const WARMUP_CHARS: &str = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~çğıöşüÇĞİÖŞÜ🔋🔌⚡•–—";

fn load_font_from_paths(paths: &[&str]) -> Option<Font> {
    for path in paths {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(font) = Font::from_bytes(bytes, FontSettings::default()) {
                return Some(font);
            }
        }
    }
    None
}

impl GlyphCache {
    fn ensure_populated(&mut self, px: f32, text: &str) {
        let bits = px.to_bits();
        let needs_line = !self.line_metrics.contains_key(&bits);
        let needs_char = text
            .chars()
            .any(|ch| ch != '\n' && !self.glyphs.contains_key(&(ch, bits)));

        if !needs_line && !needs_char {
            return;
        }

        let primary = load_font_from_paths(&[
            r"C:\Windows\Fonts\segoeuib.ttf",
            r"C:\Windows\Fonts\SegUIVar.ttf",
            r"C:\Windows\Fonts\segoeui.ttf",
        ])
        .expect("Windows Segoe UI font must exist");
        let mut symbol: Option<Font> = None;

        if needs_line {
            let metrics = if let Some(lm) = primary.horizontal_line_metrics(px) {
                let line_h = (lm.ascent - lm.descent + lm.line_gap).ceil().max(1.0) as i32;
                (lm.ascent, lm.descent, line_h)
            } else {
                let ascent = px * 0.8;
                let descent = -px * 0.2;
                let line_h = (px * 1.25).ceil().max(1.0) as i32;
                (ascent, descent, line_h)
            };
            self.line_metrics.insert(bits, metrics);
        }

        for ch in WARMUP_CHARS.chars().chain(text.chars()) {
            if ch == '\n' || self.glyphs.contains_key(&(ch, bits)) {
                continue;
            }
            let (metrics, bitmap) = if primary.lookup_glyph_index(ch) != 0 {
                primary.rasterize(ch, px)
            } else {
                let sym = symbol.get_or_insert_with(|| {
                    load_font_from_paths(&[r"C:\Windows\Fonts\seguisym.ttf"])
                        .expect("Windows Segoe UI Symbol font must exist")
                });
                if sym.lookup_glyph_index(ch) != 0 {
                    sym.rasterize(ch, px)
                } else {
                    primary.rasterize(ch, px)
                }
            };
            self.glyphs.insert(
                (ch, bits),
                CachedGlyph {
                    metrics,
                    bitmap,
                },
            );
        }
    }

    fn line_metrics(&self, px: f32) -> (f32, f32, i32) {
        self.line_metrics
            .get(&px.to_bits())
            .copied()
            .unwrap_or((px * 0.8, -px * 0.2, (px * 1.25).ceil().max(1.0) as i32))
    }

    fn measure_line_width(&self, line: &str, px: f32) -> f32 {
        let bits = px.to_bits();
        line.chars()
            .filter_map(|ch| self.glyphs.get(&(ch, bits)))
            .map(|g| g.metrics.advance_width)
            .sum()
    }
}

fn cache() -> &'static Mutex<GlyphCache> {
    GLYPH_CACHE.get_or_init(|| Mutex::new(GlyphCache::default()))
}

pub struct SkiaCanvas {
    hwnd: HWND,
    pixmap: Pixmap,
    bgra_buf: Vec<u8>,
}

impl SkiaCanvas {
    pub fn new(hwnd: HWND) -> Result<Self> {
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(hwnd, &mut rc);
        }
        let w = (rc.right - rc.left).max(1) as u32;
        let h = (rc.bottom - rc.top).max(1) as u32;
        let pixmap = Pixmap::new(w, h).expect("valid pixmap dimensions");
        let bgra_buf = vec![0u8; (w as usize) * (h as usize) * 4];
        Ok(Self {
            hwnd,
            pixmap,
            bgra_buf,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);
        if self.pixmap.width() != w || self.pixmap.height() != h {
            self.pixmap = Pixmap::new(w, h).expect("valid pixmap dimensions");
            self.bgra_buf.resize((w as usize) * (h as usize) * 4, 0);
        }
    }

    pub fn size(&self) -> Size {
        Size {
            w: self.pixmap.width() as i32,
            h: self.pixmap.height() as i32,
        }
    }

    pub fn begin_draw(&mut self) {
        self.pixmap.fill(Color::TRANSPARENT);
    }

    pub fn end_draw(&mut self) -> Result<()> {
        let w = self.pixmap.width();
        let h = self.pixmap.height();
        let src = self.pixmap.data();
        for (dst_px, src_px) in self
            .bgra_buf
            .chunks_exact_mut(4)
            .zip(src.chunks_exact(4))
        {
            dst_px[0] = src_px[2]; // B
            dst_px[1] = src_px[1]; // G
            dst_px[2] = src_px[0]; // R
            dst_px[3] = src_px[3]; // A (already premultiplied)
        }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        unsafe {
            let hdc = GetDC(Some(self.hwnd));
            if !hdc.is_invalid() {
                let _ = StretchDIBits(
                    hdc,
                    0,
                    0,
                    w as i32,
                    h as i32,
                    0,
                    0,
                    w as i32,
                    h as i32,
                    Some(self.bgra_buf.as_ptr() as *const _),
                    &bmi,
                    DIB_RGB_COLORS,
                    SRCCOPY,
                );
                let _ = ReleaseDC(Some(self.hwnd), hdc);
            }
        }
        Ok(())
    }

    pub fn pixels(&self) -> &[u8] {
        self.pixmap.data()
    }
}

impl Canvas for SkiaCanvas {
    fn fill_round_rect(&mut self, r: Rect, radius: f32, color: [u8; 4]) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let x0 = r.x as f32;
        let y0 = r.y as f32;
        let x1 = (r.x + r.w) as f32;
        let y1 = (r.y + r.h) as f32;
        let rad = radius
            .max(0.0)
            .min(r.w as f32 * 0.5)
            .min(r.h as f32 * 0.5);

        let path = if rad <= 0.0 {
            let Some(rect) = tiny_skia::Rect::from_xywh(x0, y0, r.w as f32, r.h as f32) else {
                return;
            };
            PathBuilder::from_rect(rect)
        } else {
            let c = rad * 0.552_284_8;
            let mut pb = PathBuilder::new();
            pb.move_to(x0 + rad, y0);
            pb.line_to(x1 - rad, y0);
            pb.cubic_to(x1 - rad + c, y0, x1, y0 + rad - c, x1, y0 + rad);
            pb.line_to(x1, y1 - rad);
            pb.cubic_to(x1, y1 - rad + c, x1 - rad + c, y1, x1 - rad, y1);
            pb.line_to(x0 + rad, y1);
            pb.cubic_to(x0 + rad - c, y1, x0, y1 - rad + c, x0, y1 - rad);
            pb.line_to(x0, y0 + rad);
            pb.cubic_to(x0, y0 + rad - c, x0 + rad - c, y0, x0 + rad, y0);
            pb.close();
            let Some(path) = pb.finish() else {
                return;
            };
            path
        };

        let mut paint = Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
        paint.anti_alias = true;
        self.pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    fn draw_text(&mut self, r: Rect, text: &str, color: [u8; 4], _font: &str, size: f32) {
        if r.w <= 0 || r.h <= 0 || text.is_empty() || color[3] == 0 {
            return;
        }
        let px = size.max(1.0);
        let bits = px.to_bits();
        let mut guard = cache().lock().expect("glyph cache lock");
        guard.ensure_populated(px, text);
        let (ascent, descent, line_h) = guard.line_metrics(px);
        let lines: Vec<&str> = text.split('\n').collect();
        let total_h = lines.len() as f32 * line_h as f32;
        let block_top = r.y as f32 + (r.h as f32 - total_h) * 0.5;

        let pix_w = self.pixmap.width() as i32;
        let pix_h = self.pixmap.height() as i32;
        let clip_x0 = r.x.max(0);
        let clip_y0 = r.y.max(0);
        let clip_x1 = (r.x + r.w).min(pix_w);
        let clip_y1 = (r.y + r.h).min(pix_h);
        if clip_x0 >= clip_x1 || clip_y0 >= clip_y1 {
            return;
        }

        let data = self.pixmap.data_mut();
        let stride = pix_w as usize;

        for (line_idx, line) in lines.iter().enumerate() {
            let line_w = guard.measure_line_width(line, px);
            let mut pen_x = r.x as f32 + (r.w as f32 - line_w) * 0.5;
            let line_top = block_top + line_idx as f32 * line_h as f32;
            let baseline_y = line_top + (line_h as f32 + ascent + descent) * 0.5;

            for ch in line.chars() {
                let Some(g) = guard.glyphs.get(&(ch, bits)) else {
                    continue;
                };
                let metrics = &g.metrics;
                let bitmap = &g.bitmap;
                let gx0 = (pen_x + metrics.xmin as f32).round() as i32;
                let gy0 = (baseline_y - metrics.height as f32 - metrics.ymin as f32).round() as i32;
                pen_x += metrics.advance_width;

                for gy in 0..metrics.height {
                    let py = gy0 + gy as i32;
                    if py < clip_y0 || py >= clip_y1 {
                        continue;
                    }
                    let row_off = gy * metrics.width;
                    for gx in 0..metrics.width {
                        let px_x = gx0 + gx as i32;
                        if px_x < clip_x0 || px_x >= clip_x1 {
                            continue;
                        }
                        let cov = bitmap[row_off + gx] as u32;
                        if cov == 0 {
                            continue;
                        }
                        let src_a = (color[3] as u32 * cov + 127) / 255;
                        if src_a == 0 {
                            continue;
                        }
                        let src_r = (color[0] as u32 * src_a + 127) / 255;
                        let src_g = (color[1] as u32 * src_a + 127) / 255;
                        let src_b = (color[2] as u32 * src_a + 127) / 255;
                        let inv_a = 255 - src_a;

                        let idx = (py as usize * stride + px_x as usize) * 4;
                        let dst = &mut data[idx..idx + 4];
                        dst[0] = (src_r + (dst[0] as u32 * inv_a + 127) / 255).min(255) as u8;
                        dst[1] = (src_g + (dst[1] as u32 * inv_a + 127) / 255).min(255) as u8;
                        dst[2] = (src_b + (dst[2] as u32 * inv_a + 127) / 255).min(255) as u8;
                        dst[3] = (src_a + (dst[3] as u32 * inv_a + 127) / 255).min(255) as u8;
                    }
                }
            }
        }
    }

    fn measure_text(&mut self, text: &str, _font: &str, size: f32) -> Size {
        if text.is_empty() {
            return Size { w: 0, h: 0 };
        }
        let px = size.max(1.0);
        let mut guard = cache().lock().expect("glyph cache lock");
        guard.ensure_populated(px, text);
        let (_, _, line_h) = guard.line_metrics(px);
        let mut max_w = 0i32;
        let mut line_count = 0i32;
        for line in text.split('\n') {
            line_count += 1;
            let w = guard.measure_line_width(line, px).ceil() as i32;
            if w > max_w {
                max_w = w;
            }
        }
        Size {
            w: max_w,
            h: line_count * line_h,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::paint_bar;
    use crate::render::tests::{create_test_hwnd, FixedClock, FixedPower};
    use core::{build_modules, Config, Edge};
    use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;

    #[test]
    fn skia_measures_single_and_multiline_text() {
        let hwnd = create_test_hwnd(200, 40);
        let mut canvas = SkiaCanvas::new(hwnd).expect("create SkiaCanvas");

        let single = canvas.measure_text("09:05", "Segoe UI Variable Text", 14.0);
        let multi = canvas.measure_text("09\n05", "Segoe UI Variable Text", 14.0);

        assert!(single.w > 0 && single.h > 0, "single line size was {single:?}");
        assert!(multi.w > 0 && multi.h > 0, "multi line size was {multi:?}");
        assert!(
            multi.h > single.h,
            "expected two-line height ({}) > single-line height ({})",
            multi.h,
            single.h
        );
        assert!(
            multi.w < single.w,
            "expected two-line width ({}) < single-line width ({})",
            multi.w,
            single.w
        );

        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }

    #[test]
    fn skia_draws_round_rect_and_text_with_nonzero_alpha_pixels() {
        let hwnd = create_test_hwnd(200, 40);
        let mut canvas = SkiaCanvas::new(hwnd).expect("create SkiaCanvas");

        canvas.begin_draw();
        assert!(
            canvas.pixels().iter().all(|&b| b == 0),
            "pixmap should be cleared to transparent"
        );

        let pill_rect = Rect {
            x: 4,
            y: 4,
            w: 110,
            h: 28,
        };
        canvas.fill_round_rect(pill_rect, 12.0, [255, 255, 255, 26]);
        let pill_nonzero = canvas
            .pixels()
            .chunks_exact(4)
            .filter(|px| px[3] > 0)
            .count();
        assert!(pill_nonzero > 100, "expected pill pixels with alpha > 0");

        canvas.draw_text(
            pill_rect,
            "🔋 82% ⚡",
            [255, 255, 255, 255],
            "Segoe UI Variable Text",
            14.0,
        );
        let high_alpha = canvas
            .pixels()
            .chunks_exact(4)
            .filter(|px| px[3] > 100)
            .count();
        assert!(
            high_alpha > 20,
            "expected drawn glyph pixels with alpha > 100, got {high_alpha}"
        );

        canvas.end_draw().expect("end_draw succeeds");

        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }

    #[test]
    fn skia_paint_bar_all_four_edges_end_to_end() {
        let hwnd = create_test_hwnd(1920, 40);
        let mut skia = SkiaCanvas::new(hwnd).expect("create SkiaCanvas");

        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            let mut cfg = Config::default();
            cfg.bar.edge = edge;
            let bar_size = if edge.is_vertical() {
                Size { w: 40, h: 1080 }
            } else {
                Size { w: 1920, h: 40 }
            };
            skia.resize(bar_size.w as u32, bar_size.h as u32);
            let modules = build_modules(&cfg.modules, Box::new(FixedClock), Box::new(FixedPower));

            for m in &modules {
                let text = m.text(edge);
                let sz = skia.measure_text(&text, &cfg.theme.font, cfg.theme.font_size);
                assert!(sz.w > 0 && sz.h > 0);
                if edge.is_vertical() {
                    assert!(
                        sz.w <= bar_size.w - 4,
                        "vertical text {:?} width {} exceeds narrow bar cross-axis {}",
                        text,
                        sz.w,
                        bar_size.w - 4
                    );
                } else {
                    assert!(
                        sz.h <= bar_size.h - 8,
                        "horizontal text {:?} height {} exceeds bar cross-axis {}",
                        text,
                        sz.h,
                        bar_size.h - 8
                    );
                }
            }

            skia.begin_draw();
            paint_bar(&mut skia, bar_size, &cfg, &modules);
            skia.end_draw().expect("skia end_draw succeeds");
        }

        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

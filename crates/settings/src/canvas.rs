use std::collections::HashMap;
use std::mem::size_of;
use std::sync::{Mutex, OnceLock};

use fontdue::{Font, FontSettings, Metrics};
use tiny_skia::{Color, FillRule, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};
use windows::core::Result;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMSBT_MAINWINDOW, DWM_SYSTEMBACKDROP_TYPE,
};
use windows::Win32::Graphics::Gdi::{
    GetDC, ReleaseDC, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use core::{Canvas, Rect, Size};

pub trait SettingsPainter: Canvas {
    fn stroke_round_rect(&mut self, _r: Rect, _radius: f32, _width: f32, _color: [u8; 4]) {}
    fn draw_line(
        &mut self,
        _x1: f32,
        _y1: f32,
        _x2: f32,
        _y2: f32,
        _width: f32,
        _color: [u8; 4],
    ) {
    }
}

#[derive(Clone)]
struct CachedGlyph {
    metrics: Metrics,
    bitmap: Vec<u8>,
}

#[derive(Default)]
struct GlyphCache {
    primary_regular: Option<Font>,
    primary_bold: Option<Font>,
    symbol: Option<Font>,
    line_metrics: HashMap<(u32, bool), (f32, f32, i32)>,
    glyphs: HashMap<(char, u32, bool), CachedGlyph>,
}

static GLYPH_CACHE: OnceLock<Mutex<GlyphCache>> = OnceLock::new();

const WARMUP_CHARS: &str = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~çğıöşüÇĞİÖŞÜ●○↑↓▾✓🔋🔌⚡•–—";

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
    fn ensure_populated(&mut self, px: f32, bold: bool, text: &str) {
        let bits = px.to_bits();
        let needs_line = !self.line_metrics.contains_key(&(bits, bold));
        let needs_char = text
            .chars()
            .any(|ch| ch != '\n' && !self.glyphs.contains_key(&(ch, bits, bold)));

        if !needs_line && !needs_char {
            return;
        }

        if self.primary_regular.is_none() {
            self.primary_regular = Some(
                load_font_from_paths(&[
                    r"C:\Windows\Fonts\SegUIVar.ttf",
                    r"C:\Windows\Fonts\segoeui.ttf",
                    r"C:\Windows\Fonts\segoeuib.ttf",
                ])
                .expect("Windows Segoe UI font must exist"),
            );
        }
        if bold && self.primary_bold.is_none() {
            self.primary_bold = Some(
                load_font_from_paths(&[
                    r"C:\Windows\Fonts\segoeuib.ttf",
                    r"C:\Windows\Fonts\SegUIVar.ttf",
                    r"C:\Windows\Fonts\segoeui.ttf",
                ])
                .expect("Windows Segoe UI Bold font must exist"),
            );
        }

        let primary = if bold {
            self.primary_bold.as_ref().expect("bold font cached")
        } else {
            self.primary_regular.as_ref().expect("regular font cached")
        };

        if needs_line {
            let metrics = if let Some(lm) = primary.horizontal_line_metrics(px) {
                let line_h = (lm.ascent - lm.descent + lm.line_gap).ceil().max(1.0) as i32;
                (lm.ascent, lm.descent, line_h)
            } else {
                (px * 0.8, -px * 0.2, (px * 1.25).ceil().max(1.0) as i32)
            };
            self.line_metrics.insert((bits, bold), metrics);
        }

        for ch in WARMUP_CHARS.chars().chain(text.chars()) {
            if ch == '\n' || self.glyphs.contains_key(&(ch, bits, bold)) {
                continue;
            }
            let (metrics, bitmap) = if primary.lookup_glyph_index(ch) != 0 {
                primary.rasterize(ch, px)
            } else {
                if self.symbol.is_none() {
                    self.symbol = Some(
                        load_font_from_paths(&[r"C:\Windows\Fonts\seguisym.ttf"])
                            .expect("Windows Segoe UI Symbol font must exist"),
                    );
                }
                let sym = self.symbol.as_ref().expect("symbol font cached");
                if sym.lookup_glyph_index(ch) != 0 {
                    sym.rasterize(ch, px)
                } else {
                    primary.rasterize(ch, px)
                }
            };
            self.glyphs
                .insert((ch, bits, bold), CachedGlyph { metrics, bitmap });
        }
    }

    fn line_metrics(&self, px: f32, bold: bool) -> (f32, f32, i32) {
        self.line_metrics
            .get(&(px.to_bits(), bold))
            .copied()
            .unwrap_or((px * 0.8, -px * 0.2, (px * 1.25).ceil().max(1.0) as i32))
    }

    fn measure_line_width(&self, line: &str, px: f32, bold: bool) -> f32 {
        let bits = px.to_bits();
        line.chars()
            .filter_map(|ch| self.glyphs.get(&(ch, bits, bold)))
            .map(|g| g.metrics.advance_width)
            .sum()
    }
}

fn cache() -> &'static Mutex<GlyphCache> {
    GLYPH_CACHE.get_or_init(|| Mutex::new(GlyphCache::default()))
}

pub fn apply_settings_backdrop(hwnd: HWND) {
    unsafe {
        let dark: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as _,
            size_of::<i32>() as u32,
        );
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        let backdrop = DWM_SYSTEMBACKDROP_TYPE(DWMSBT_MAINWINDOW.0);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as _,
            size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        );
    }
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
            dst_px[0] = src_px[2];
            dst_px[1] = src_px[1];
            dst_px[2] = src_px[0];
            dst_px[3] = src_px[3];
        }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
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

fn build_round_rect_path(r: Rect, radius: f32) -> Option<Path> {
    if r.w <= 0 || r.h <= 0 {
        return None;
    }
    let x0 = r.x as f32;
    let y0 = r.y as f32;
    let x1 = (r.x + r.w) as f32;
    let y1 = (r.y + r.h) as f32;
    let rad = radius
        .max(0.0)
        .min(r.w as f32 * 0.5)
        .min(r.h as f32 * 0.5);

    if rad <= 0.0 {
        let rect = tiny_skia::Rect::from_xywh(x0, y0, r.w as f32, r.h as f32)?;
        Some(PathBuilder::from_rect(rect))
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
        pb.finish()
    }
}

impl SettingsPainter for SkiaCanvas {
    fn stroke_round_rect(&mut self, r: Rect, radius: f32, width: f32, color: [u8; 4]) {
        if color[3] == 0 || width <= 0.0 {
            return;
        }
        let Some(path) = build_round_rect_path(r, radius) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
        paint.anti_alias = true;
        let stroke = Stroke {
            width,
            ..Default::default()
        };
        self.pixmap
            .stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

impl Canvas for SkiaCanvas {
    fn fill_round_rect(&mut self, r: Rect, radius: f32, color: [u8; 4]) {
        if color[3] == 0 {
            return;
        }
        let Some(path) = build_round_rect_path(r, radius) else {
            return;
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

    fn draw_text(&mut self, r: Rect, text: &str, color: [u8; 4], font: &str, size: f32) {
        if r.w <= 0 || r.h <= 0 || text.is_empty() || color[3] == 0 {
            return;
        }
        let px = size.max(1.0);
        let bold = font.contains("Bold");
        let bits = px.to_bits();
        let mut guard = cache().lock().expect("glyph cache lock");
        guard.ensure_populated(px, bold, text);
        let (ascent, descent, line_h) = guard.line_metrics(px, bold);
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
            let line_w = guard.measure_line_width(line, px, bold);
            let mut pen_x = r.x as f32 + (r.w as f32 - line_w) * 0.5;
            let line_top = block_top + line_idx as f32 * line_h as f32;
            let baseline_y = line_top + (line_h as f32 + ascent + descent) * 0.5;

            for ch in line.chars() {
                let Some(g) = guard.glyphs.get(&(ch, bits, bold)) else {
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

    fn measure_text(&mut self, text: &str, font: &str, size: f32) -> Size {
        if text.is_empty() {
            return Size { w: 0, h: 0 };
        }
        let px = size.max(1.0);
        let bold = font.contains("Bold");
        let mut guard = cache().lock().expect("glyph cache lock");
        guard.ensure_populated(px, bold, text);
        let (_, _, line_h) = guard.line_metrics(px, bold);
        let mut max_w = 0i32;
        let mut line_count = 0i32;
        for line in text.split('\n') {
            line_count += 1;
            let w = guard.measure_line_width(line, px, bold).ceil() as i32;
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
    use crate::state::{SettingsAppState, SettingsTab};
    use crate::ui::render_settings_ui;
    use core::Config;
    use std::path::PathBuf;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, WNDCLASSW, WS_POPUP,
    };

    unsafe extern "system" fn dummy_wndproc(
        hwnd: HWND,
        msg: u32,
        wp: WPARAM,
        lp: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    }

    #[test]
    fn skia_renders_all_settings_tabs_end_to_end() {
        let hwnd = unsafe {
            let hi = GetModuleHandleW(PCWSTR::null()).expect("module handle");
            let class = w!("RustTaskbarSettingsRenderTestWindow");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(dummy_wndproc),
                hInstance: hi.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            CreateWindowExW(
                Default::default(),
                class,
                w!("settings_render_test"),
                WS_POPUP,
                0,
                0,
                760,
                560,
                None,
                None,
                Some(hi.into()),
                None,
            )
            .expect("create test window")
        };

        let mut canvas = SkiaCanvas::new(hwnd).expect("create SkiaCanvas");
        canvas.resize(760, 560);
        let mut state = SettingsAppState::new(
            Config::default(),
            PathBuf::from(r"C:\Temp\config.toml"),
            true,
        );

        for tab in [
            SettingsTab::Appearance,
            SettingsTab::Widgets,
            SettingsTab::About,
        ] {
            state.tab = tab;
            canvas.begin_draw();
            let regions = render_settings_ui(&mut canvas, Size { w: 760, h: 560 }, &state);
            assert!(!regions.is_empty(), "expected hit regions on {tab:?}");
            let nonzero_alpha = canvas
                .pixels()
                .chunks_exact(4)
                .filter(|px| px[3] > 0)
                .count();
            assert!(nonzero_alpha > 1000, "expected drawn UI pixels on {tab:?}");
            canvas.end_draw().expect("end_draw succeeds");
        }

        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }

    #[test]
    fn skia_strokes_round_rect_and_renders_bold_font() {
        let hwnd = unsafe {
            let hi = GetModuleHandleW(PCWSTR::null()).expect("module handle");
            let class = w!("RustTaskbarSettingsStrokeTestWindow");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(dummy_wndproc),
                hInstance: hi.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            CreateWindowExW(
                Default::default(),
                class,
                w!("stroke_test"),
                WS_POPUP,
                0,
                0,
                240,
                120,
                None,
                None,
                Some(hi.into()),
                None,
            )
            .expect("create test window")
        };

        let mut canvas = SkiaCanvas::new(hwnd).expect("create SkiaCanvas");
        canvas.resize(240, 120);
        canvas.begin_draw();
        canvas.stroke_round_rect(
            Rect {
                x: 10,
                y: 10,
                w: 220,
                h: 100,
            },
            10.0,
            1.0,
            [255, 255, 255, 40],
        );
        canvas.draw_text(
            Rect {
                x: 20,
                y: 20,
                w: 200,
                h: 80,
            },
            "taskbar",
            [255, 255, 255, 255],
            "Segoe UI Bold",
            40.0,
        );
        let nonzero_alpha = canvas
            .pixels()
            .chunks_exact(4)
            .filter(|px| px[3] > 0)
            .count();
        assert!(nonzero_alpha > 200);

        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

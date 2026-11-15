#[cfg(feature = "render-d2d")]
pub mod d2d;
#[cfg(feature = "render-d2d")]
pub use d2d::D2dCanvas;

#[cfg(feature = "render-skia")]
pub mod skia;
#[cfg(feature = "render-skia")]
pub use skia::SkiaCanvas;

#[cfg(feature = "render-skia")]
pub type ActiveCanvas = SkiaCanvas;
#[cfg(all(not(feature = "render-skia"), feature = "render-d2d"))]
pub type ActiveCanvas = D2dCanvas;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_NONE,
    DWMSBT_TRANSIENTWINDOW,
};
use windows::Win32::UI::Controls::MARGINS;

use core::{layout_modules, Backdrop, Canvas, Config, Module, Size};

pub fn backdrop_type_value(b: Backdrop) -> i32 {
    match b {
        Backdrop::Acrylic => DWMSBT_TRANSIENTWINDOW.0,
        Backdrop::Mica => DWMSBT_MAINWINDOW.0,
        Backdrop::Solid => DWMSBT_NONE.0,
    }
}

pub fn apply_backdrop(hwnd: HWND, b: Backdrop) {
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

        let backdrop = DWM_SYSTEMBACKDROP_TYPE(backdrop_type_value(b));
        // On Windows 10, DWMWA_SYSTEMBACKDROP_TYPE is unsupported and returns an error;
        // ignore it so the window falls back cleanly to solid/translucent without failing.
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as _,
            size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        );
    }
}

pub fn paint_bar(
    canvas: &mut dyn Canvas,
    bar_size: Size,
    cfg: &Config,
    modules: &[Box<dyn Module>],
) {
    if modules.is_empty() || bar_size.w <= 0 || bar_size.h <= 0 {
        return;
    }

    let edge = cfg.bar.edge;
    let is_vertical = edge.is_vertical();
    let (pad_w, pad_h) = if is_vertical { (8, 10) } else { (16, 8) };

    let mut texts = Vec::with_capacity(modules.len());
    let mut item_sizes = Vec::with_capacity(modules.len());

    for m in modules {
        let text = m.text(edge);
        let measured = canvas.measure_text(&text, &cfg.theme.font, cfg.theme.font_size);
        let item_size = if measured.w > 0 && measured.h > 0 {
            Size {
                w: measured.w + pad_w,
                h: measured.h + pad_h,
            }
        } else {
            Size { w: 0, h: 0 }
        };
        texts.push(text);
        item_sizes.push(item_size);
    }

    let rects = layout_modules(
        edge,
        bar_size,
        &item_sizes,
        cfg.bar.align,
        cfg.bar.gap as i32,
    );

    let radius = cfg.bar.corner_radius as f32;
    for (rect, text) in rects.into_iter().zip(texts.into_iter()) {
        if rect.w > 0 && rect.h > 0 {
            canvas.fill_round_rect(rect, radius, cfg.theme.pill);
            canvas.draw_text(
                rect,
                &text,
                cfg.theme.foreground,
                &cfg.theme.font,
                cfg.theme.font_size,
            );
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use core::{build_modules, Clock, Edge, PowerSource, PowerState, Rect};
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, WNDCLASSW,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    };

    unsafe extern "system" fn dummy_wndproc(
        hwnd: HWND,
        msg: u32,
        wp: windows::Win32::Foundation::WPARAM,
        lp: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    }

    pub(crate) fn create_test_hwnd(width: i32, height: i32) -> HWND {
        unsafe {
            let hi = GetModuleHandleW(PCWSTR::null()).expect("module handle");
            let class = w!("RustTaskbarRenderTestWindow");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(dummy_wndproc),
                hInstance: hi.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!("render_test"),
                WS_POPUP,
                0,
                0,
                width,
                height,
                None,
                None,
                Some(hi.into()),
                None,
            )
            .expect("create test window")
        }
    }

    pub(crate) struct FixedClock;
    impl Clock for FixedClock {
        fn now(&self) -> (u8, u8) {
            (9, 5)
        }
    }

    pub(crate) struct FixedPower;
    impl PowerSource for FixedPower {
        fn read(&self) -> Option<PowerState> {
            Some(PowerState {
                percent: 82,
                on_ac: true,
            })
        }
    }

    #[derive(Default)]
    struct RecordingCanvas {
        measured: Vec<(String, String, f32)>,
        pills: Vec<(Rect, f32, [u8; 4])>,
        texts: Vec<(Rect, String, [u8; 4], String, f32)>,
    }

    impl Canvas for RecordingCanvas {
        fn fill_round_rect(&mut self, r: Rect, radius: f32, color: [u8; 4]) {
            self.pills.push((r, radius, color));
        }

        fn draw_text(&mut self, r: Rect, text: &str, color: [u8; 4], font: &str, size: f32) {
            self.texts
                .push((r, text.to_string(), color, font.to_string(), size));
        }

        fn measure_text(&mut self, text: &str, font: &str, size: f32) -> Size {
            self.measured
                .push((text.to_string(), font.to_string(), size));
            let lines: Vec<&str> = text.split('\n').collect();
            let max_chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1) as i32;
            let line_count = lines.len() as i32;
            Size {
                w: max_chars * 8,
                h: line_count * 16,
            }
        }
    }

    #[test]
    fn backdrop_type_maps_acrylic_mica_solid() {
        assert_eq!(backdrop_type_value(Backdrop::Acrylic), 3);
        assert_eq!(backdrop_type_value(Backdrop::Mica), 2);
        assert_eq!(backdrop_type_value(Backdrop::Solid), 1);

        let hwnd = create_test_hwnd(200, 40);
        apply_backdrop(hwnd, Backdrop::Acrylic);
        apply_backdrop(hwnd, Backdrop::Mica);
        apply_backdrop(hwnd, Backdrop::Solid);
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }

    #[test]
    fn paint_bar_horizontal_and_vertical_edges_without_clipping() {
        for edge in [Edge::Top, Edge::Bottom] {
            let mut cfg = Config::default();
            cfg.bar.edge = edge;
            let modules = build_modules(&cfg.modules, Box::new(FixedClock), Box::new(FixedPower));
            let mut canvas = RecordingCanvas::default();
            let bar_size = Size { w: 1920, h: 40 };

            paint_bar(&mut canvas, bar_size, &cfg, &modules);

            assert_eq!(canvas.pills.len(), 2);
            assert_eq!(canvas.texts.len(), 2);
            assert_eq!(canvas.texts[0].1, "🔋 82% ⚡");
            assert_eq!(canvas.texts[1].1, "09:05");
            assert!(canvas.pills[0].0.x + canvas.pills[0].0.w <= canvas.pills[1].0.x);
            for (r, _, _) in &canvas.pills {
                assert!(r.x >= 0 && r.y >= 0);
                assert!(r.x + r.w <= bar_size.w);
                assert!(r.y + r.h <= bar_size.h);
            }
        }

        for edge in [Edge::Left, Edge::Right] {
            let mut cfg = Config::default();
            cfg.bar.edge = edge;
            let modules = build_modules(&cfg.modules, Box::new(FixedClock), Box::new(FixedPower));
            let mut canvas = RecordingCanvas::default();
            let bar_size = Size { w: 40, h: 1080 };

            paint_bar(&mut canvas, bar_size, &cfg, &modules);

            assert_eq!(canvas.pills.len(), 2);
            assert_eq!(canvas.texts.len(), 2);
            assert_eq!(canvas.texts[0].1, "82%\n⚡");
            assert_eq!(canvas.texts[1].1, "09\n05");
            assert!(canvas.pills[0].0.y + canvas.pills[0].0.h <= canvas.pills[1].0.y);
            for (r, _, _) in &canvas.pills {
                assert!(r.x >= 0 && r.y >= 0);
                assert!(
                    r.w <= bar_size.w - 4,
                    "pill width {} clipped in bar {}",
                    r.w,
                    bar_size.w
                );
                assert!(r.x + r.w <= bar_size.w);
                assert!(r.y + r.h <= bar_size.h);
            }
        }
    }

    #[cfg(feature = "render-d2d")]
    #[test]
    fn d2d_paint_bar_all_four_edges_end_to_end() {
        let hwnd = create_test_hwnd(1920, 40);
        let mut d2d = D2dCanvas::new(hwnd).expect("create D2dCanvas");

        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            let mut cfg = Config::default();
            cfg.bar.edge = edge;
            let bar_size = if edge.is_vertical() {
                Size { w: 40, h: 1080 }
            } else {
                Size { w: 1920, h: 40 }
            };
            d2d.resize(bar_size.w as u32, bar_size.h as u32);
            let modules = build_modules(&cfg.modules, Box::new(FixedClock), Box::new(FixedPower));

            // Verify real DirectWrite measured sizes fit inside bar on every edge
            for m in &modules {
                let text = m.text(edge);
                let sz = d2d.measure_text(&text, &cfg.theme.font, cfg.theme.font_size);
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

            d2d.begin_draw();
            paint_bar(&mut d2d, bar_size, &cfg, &modules);
            d2d.end_draw().expect("d2d end_draw succeeds");
        }

        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

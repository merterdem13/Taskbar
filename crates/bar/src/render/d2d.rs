use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_METRICS,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use core::{Canvas, Rect, Size};

pub struct D2dCanvas {
    _factory: ID2D1Factory,
    dwrite: IDWriteFactory,
    target: ID2D1HwndRenderTarget,
    brush: ID2D1SolidColorBrush,
    cached_format: Option<(String, u32, IDWriteTextFormat)>,
}

fn rgba_to_d2d(c: [u8; 4]) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: c[0] as f32 / 255.0,
        g: c[1] as f32 / 255.0,
        b: c[2] as f32 / 255.0,
        a: c[3] as f32 / 255.0,
    }
}

fn rect_to_d2d(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.x as f32,
        top: r.y as f32,
        right: (r.x + r.w) as f32,
        bottom: (r.y + r.h) as f32,
    }
}

impl D2dCanvas {
    pub fn new(hwnd: HWND) -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            };
            let hprops = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U {
                    width: (rc.right - rc.left).max(1) as u32,
                    height: (rc.bottom - rc.top).max(1) as u32,
                },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            let target = factory.CreateHwndRenderTarget(&props, &hprops)?;
            let white = D2D1_COLOR_F {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            };
            let brush = target.CreateSolidColorBrush(&white, None)?;
            Ok(Self {
                _factory: factory,
                dwrite,
                target,
                brush,
                cached_format: None,
            })
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        unsafe {
            let _ = self.target.Resize(&D2D_SIZE_U {
                width: width.max(1),
                height: height.max(1),
            });
        }
    }

    pub fn size(&self) -> Size {
        unsafe {
            let sz = self.target.GetPixelSize();
            Size {
                w: sz.width as i32,
                h: sz.height as i32,
            }
        }
    }

    pub fn begin_draw(&mut self) {
        unsafe {
            self.target.BeginDraw();
            self.target.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));
        }
    }

    pub fn end_draw(&mut self) -> Result<()> {
        unsafe { self.target.EndDraw(None, None) }
    }

    fn get_or_create_format(&mut self, font: &str, size: f32) -> Result<IDWriteTextFormat> {
        let size = size.max(1.0);
        let bits = size.to_bits();
        if let Some((cached_font, cached_bits, format)) = &self.cached_format {
            if cached_font == font && *cached_bits == bits {
                return Ok(format.clone());
            }
        }
        unsafe {
            let font_w: Vec<u16> = font.encode_utf16().chain(std::iter::once(0)).collect();
            let format = self.dwrite.CreateTextFormat(
                PCWSTR(font_w.as_ptr()),
                None,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("en-us"),
            )?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            self.cached_format = Some((font.to_string(), bits, format.clone()));
            Ok(format)
        }
    }
}

impl Canvas for D2dCanvas {
    fn fill_round_rect(&mut self, r: Rect, radius: f32, color: [u8; 4]) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        unsafe {
            self.brush.SetColor(&rgba_to_d2d(color));
            self.target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect_to_d2d(r),
                    radiusX: radius,
                    radiusY: radius,
                },
                &self.brush,
            );
        }
    }

    fn draw_text(&mut self, r: Rect, text: &str, color: [u8; 4], font: &str, size: f32) {
        if r.w <= 0 || r.h <= 0 || text.is_empty() {
            return;
        }
        let Ok(format) = self.get_or_create_format(font, size) else {
            return;
        };
        let text_w: Vec<u16> = text.encode_utf16().collect();
        let rect_f = rect_to_d2d(r);
        unsafe {
            self.brush.SetColor(&rgba_to_d2d(color));
            self.target.DrawText(
                &text_w,
                &format,
                &rect_f,
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    fn measure_text(&mut self, text: &str, font: &str, size: f32) -> Size {
        if text.is_empty() {
            return Size { w: 0, h: 0 };
        }
        let Ok(format) = self.get_or_create_format(font, size) else {
            return Size { w: 0, h: 0 };
        };
        let text_w: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let Ok(layout) = self
                .dwrite
                .CreateTextLayout(&text_w, &format, 10_000.0, 10_000.0)
            else {
                return Size { w: 0, h: 0 };
            };
            let mut metrics = DWRITE_TEXT_METRICS::default();
            if layout.GetMetrics(&mut metrics).is_err() {
                return Size { w: 0, h: 0 };
            }
            Size {
                w: metrics.widthIncludingTrailingWhitespace.ceil() as i32,
                h: metrics.height.ceil() as i32,
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
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
            let class = w!("RustTaskbarD2dTestWindow");
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
                w!("d2d_test"),
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

    #[test]
    fn d2d_measures_single_and_multiline_text_and_draws() {
        let hwnd = create_test_hwnd(200, 40);
        let mut canvas = D2dCanvas::new(hwnd).expect("create D2dCanvas");

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

        canvas.begin_draw();
        canvas.fill_round_rect(
            Rect {
                x: 4,
                y: 4,
                w: 80,
                h: 28,
            },
            12.0,
            [255, 255, 255, 26],
        );
        canvas.draw_text(
            Rect {
                x: 4,
                y: 4,
                w: 80,
                h: 28,
            },
            "09:05",
            [255, 255, 255, 255],
            "Segoe UI Variable Text",
            14.0,
        );
        canvas.end_draw().expect("end_draw succeeds");

        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

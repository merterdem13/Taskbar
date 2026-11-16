use std::collections::HashMap;
use std::mem::size_of;
use windows::core::{w, Result};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows_numerics::Vector2;
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_BOLD,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_METRICS,
};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWCP_ROUND, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWM_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use core::{Canvas, Rect, Size};

use crate::canvas::SettingsPainter;

pub fn apply_integrated_frame(hwnd: HWND) {
    unsafe {
        let dark: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as _,
            size_of::<i32>() as u32,
        );
        let corner: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as _,
            size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
        let margins = MARGINS {
            cxLeftWidth: 0,
            cxRightWidth: 0,
            cyTopHeight: 1,
            cyBottomHeight: 0,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
    }
}

pub struct D2dCanvas {
    _factory: ID2D1Factory,
    dwrite: IDWriteFactory,
    target: ID2D1HwndRenderTarget,
    brush: ID2D1SolidColorBrush,
    formats: HashMap<(bool, u32), IDWriteTextFormat>,
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
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
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
                formats: HashMap::new(),
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

    pub fn begin_draw(&mut self) {
        unsafe {
            self.target.BeginDraw();
            self.target.Clear(Some(&D2D1_COLOR_F {
                r: 7.0 / 255.0,
                g: 7.0 / 255.0,
                b: 8.0 / 255.0,
                a: 1.0,
            }));
        }
    }

    pub fn end_draw(&mut self) -> Result<()> {
        unsafe { self.target.EndDraw(None, None) }
    }

    fn get_or_create_format(&mut self, font: &str, size: f32) -> Result<IDWriteTextFormat> {
        let px = size.max(1.0);
        let bold = font.contains("Bold");
        let key = (bold, px.to_bits());
        if let Some(fmt) = self.formats.get(&key) {
            return Ok(fmt.clone());
        }
        unsafe {
            let weight = if bold {
                DWRITE_FONT_WEIGHT_BOLD
            } else {
                DWRITE_FONT_WEIGHT_NORMAL
            };
            let format = self.dwrite.CreateTextFormat(
                w!("Segoe UI"),
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                px,
                w!("tr-TR"),
            )?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            self.formats.insert(key, format.clone());
            Ok(format)
        }
    }
}

impl SettingsPainter for D2dCanvas {
    fn stroke_round_rect(&mut self, r: Rect, radius: f32, width: f32, color: [u8; 4]) {
        if r.w <= 0 || r.h <= 0 || color[3] == 0 || width <= 0.0 {
            return;
        }
        let inset = if width <= 1.5 { 0.5 } else { 0.0 };
        let rect = D2D_RECT_F {
            left: r.x as f32 + inset,
            top: r.y as f32 + inset,
            right: (r.x + r.w) as f32 - inset,
            bottom: (r.y + r.h) as f32 - inset,
        };
        unsafe {
            self.brush.SetColor(&rgba_to_d2d(color));
            self.target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect,
                    radiusX: radius,
                    radiusY: radius,
                },
                &self.brush,
                width,
                None,
            );
        }
    }

    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: [u8; 4]) {
        if color[3] == 0 || width <= 0.0 {
            return;
        }
        unsafe {
            self.brush.SetColor(&rgba_to_d2d(color));
            self.target.DrawLine(
                Vector2 { X: x1, Y: y1 },
                Vector2 { X: x2, Y: y2 },
                &self.brush,
                width,
                None,
            );
        }
    }
}

impl Canvas for D2dCanvas {
    fn fill_round_rect(&mut self, r: Rect, radius: f32, color: [u8; 4]) {
        if r.w <= 0 || r.h <= 0 || color[3] == 0 {
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
        if r.w <= 0 || r.h <= 0 || text.is_empty() || color[3] == 0 {
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
mod tests {
    use super::*;
    use crate::anim::UiAnimState;
    use crate::state::{SettingsAppState, SettingsTab};
    use crate::ui::render_settings_ui_animated;
    use core::Config;
    use std::path::PathBuf;
    use windows::core::PCWSTR;
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
    fn d2d_renders_splash_and_all_settings_tabs_cleanly() {
        let hwnd = unsafe {
            let hi = GetModuleHandleW(PCWSTR::null()).expect("module handle");
            let class = w!("RustTaskbarSettingsD2dTestWindow");
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
                w!("d2d_settings_test"),
                WS_POPUP,
                0,
                0,
                780,
                600,
                None,
                None,
                Some(hi.into()),
                None,
            )
            .expect("create test window")
        };

        let mut canvas = D2dCanvas::new(hwnd).expect("create D2dCanvas");
        canvas.resize(780, 600);
        let mut state = SettingsAppState::new(
            Config::default(),
            PathBuf::from(r"C:\Temp\config.toml"),
            true,
        );

        let splash_anim = UiAnimState::new(0, 40);
        canvas.begin_draw();
        let splash_regions =
            render_settings_ui_animated(&mut canvas, Size { w: 780, h: 600 }, &state, &splash_anim);
        assert_eq!(splash_regions.len(), 3);
        assert!(splash_regions.iter().all(|hr| matches!(
            hr.action,
            crate::state::UiAction::WindowMinimize
                | crate::state::UiAction::WindowMaximize
                | crate::state::UiAction::WindowClose
        )));
        canvas.end_draw().expect("splash end_draw succeeds");

        for (idx, tab) in [
            SettingsTab::Appearance,
            SettingsTab::Widgets,
            SettingsTab::About,
        ]
        .into_iter()
        .enumerate()
        {
            state.tab = tab;
            let anim = UiAnimState::settled_main(idx, 40);
            canvas.begin_draw();
            let regions =
                render_settings_ui_animated(&mut canvas, Size { w: 780, h: 600 }, &state, &anim);
            assert!(!regions.is_empty(), "expected regions for {tab:?}");
            canvas.end_draw().expect("tab end_draw succeeds");
        }

        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

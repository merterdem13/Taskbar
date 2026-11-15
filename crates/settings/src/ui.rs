use core::{Align, Backdrop, Canvas, Edge, Rect, Size, KNOWN_WIDGETS};

use crate::state::{SettingsAppState, SettingsTab, UiAction, THEME_PRESETS};

const FONT_UI: &str = "Segoe UI";

const COLOR_BG: [u8; 4] = [16, 18, 26, 232];
const COLOR_PANEL: [u8; 4] = [255, 255, 255, 12];
const COLOR_CARD: [u8; 4] = [255, 255, 255, 18];
const COLOR_CARD_ACTIVE: [u8; 4] = [59, 130, 246, 56];
const COLOR_BTN: [u8; 4] = [255, 255, 255, 22];
const COLOR_BTN_ACTIVE: [u8; 4] = [37, 99, 235, 215];
const COLOR_TEXT: [u8; 4] = [242, 245, 250, 255];
const COLOR_MUTED: [u8; 4] = [170, 182, 198, 255];
const COLOR_ACCENT_GREEN: [u8; 4] = [22, 163, 74, 70];
const COLOR_TEXT_GREEN: [u8; 4] = [134, 239, 172, 255];
const COLOR_ACCENT_AMBER: [u8; 4] = [217, 119, 6, 58];
const COLOR_TEXT_AMBER: [u8; 4] = [253, 224, 71, 255];

#[derive(Debug, Clone, PartialEq)]
pub struct HitRegion {
    pub rect: Rect,
    pub action: UiAction,
}

fn draw_left_text(
    canvas: &mut dyn Canvas,
    x: i32,
    y: i32,
    max_w: i32,
    h: i32,
    text: &str,
    color: [u8; 4],
    size: f32,
) {
    let measured = canvas.measure_text(text, FONT_UI, size);
    let w = measured.w.clamp(1, max_w.max(1));
    canvas.draw_text(Rect { x, y, w, h }, text, color, FONT_UI, size);
}

fn draw_button(
    canvas: &mut dyn Canvas,
    regions: &mut Vec<HitRegion>,
    rect: Rect,
    label: &str,
    active: bool,
    action: UiAction,
) {
    let bg = if active { COLOR_BTN_ACTIVE } else { COLOR_BTN };
    let fg = if active { COLOR_TEXT } else { [222, 228, 238, 255] };
    canvas.fill_round_rect(rect, 8.0, bg);
    canvas.draw_text(rect, label, fg, FONT_UI, 13.0);
    regions.push(HitRegion { rect, action });
}

fn render_segmented_row<T: Copy + PartialEq>(
    canvas: &mut dyn Canvas,
    regions: &mut Vec<HitRegion>,
    x: i32,
    y: i32,
    total_w: i32,
    title: &str,
    current: T,
    options: &[(&str, T, UiAction)],
) {
    draw_left_text(canvas, x, y, 132, 34, title, COLOR_MUTED, 13.0);
    let btn_area_x = x + 136;
    let btn_area_w = (total_w - 136).max(120);
    let gap = 8;
    let count = options.len().max(1) as i32;
    let btn_w = ((btn_area_w - gap * (count - 1)) / count).max(44);
    for (idx, (label, val, action)) in options.iter().enumerate() {
        let bx = btn_area_x + idx as i32 * (btn_w + gap);
        draw_button(
            canvas,
            regions,
            Rect {
                x: bx,
                y,
                w: btn_w,
                h: 34,
            },
            label,
            *val == current,
            action.clone(),
        );
    }
}

fn render_stepper(
    canvas: &mut dyn Canvas,
    regions: &mut Vec<HitRegion>,
    x: i32,
    y: i32,
    w: i32,
    label: &str,
    dec_action: UiAction,
    inc_action: UiAction,
) {
    draw_left_text(canvas, x, y, (w - 80).max(80), 34, label, COLOR_MUTED, 13.0);
    let dec_rect = Rect {
        x: x + w - 72,
        y,
        w: 32,
        h: 32,
    };
    let inc_rect = Rect {
        x: x + w - 34,
        y,
        w: 32,
        h: 32,
    };
    draw_button(canvas, regions, dec_rect, "-", false, dec_action);
    draw_button(canvas, regions, inc_rect, "+", false, inc_action);
}

fn render_appearance_tab(
    canvas: &mut dyn Canvas,
    regions: &mut Vec<HitRegion>,
    ix: i32,
    iy: i32,
    iw: i32,
    state: &SettingsAppState,
) {
    let cfg = &state.working_config;
    render_segmented_row(
        canvas,
        regions,
        ix,
        iy,
        iw,
        "Ekran Kenarı",
        cfg.bar.edge,
        &[
            ("Üst", Edge::Top, UiAction::SetEdge(Edge::Top)),
            ("Alt", Edge::Bottom, UiAction::SetEdge(Edge::Bottom)),
            ("Sol", Edge::Left, UiAction::SetEdge(Edge::Left)),
            ("Sağ", Edge::Right, UiAction::SetEdge(Edge::Right)),
        ],
    );

    render_segmented_row(
        canvas,
        regions,
        ix,
        iy + 48,
        iw,
        "Hizalama",
        cfg.bar.align,
        &[
            ("Başlangıç", Align::Start, UiAction::SetAlign(Align::Start)),
            ("Orta", Align::Center, UiAction::SetAlign(Align::Center)),
            ("Son", Align::End, UiAction::SetAlign(Align::End)),
        ],
    );

    render_segmented_row(
        canvas,
        regions,
        ix,
        iy + 96,
        iw,
        "Arka Plan",
        cfg.bar.backdrop,
        &[
            ("Acrylic", Backdrop::Acrylic, UiAction::SetBackdrop(Backdrop::Acrylic)),
            ("Mica", Backdrop::Mica, UiAction::SetBackdrop(Backdrop::Mica)),
            ("Solid", Backdrop::Solid, UiAction::SetBackdrop(Backdrop::Solid)),
        ],
    );

    // Thickness slider + stepper
    let y_thick = iy + 146;
    let thick_label = format!("Kalınlık: {} px", cfg.bar.thickness);
    draw_left_text(canvas, ix, y_thick, 132, 34, &thick_label, COLOR_MUTED, 13.0);
    let dec_rect = Rect {
        x: ix + 136,
        y: y_thick + 1,
        w: 32,
        h: 32,
    };
    draw_button(
        canvas,
        regions,
        dec_rect,
        "-",
        false,
        UiAction::AdjustThickness(-4),
    );
    let inc_rect = Rect {
        x: ix + iw - 34,
        y: y_thick + 1,
        w: 32,
        h: 32,
    };
    draw_button(
        canvas,
        regions,
        inc_rect,
        "+",
        false,
        UiAction::AdjustThickness(4),
    );

    let bar_x = ix + 176;
    let bar_w = (iw - 218).max(80);
    let bar_y = y_thick + 11;
    let bar_h = 12;
    canvas.fill_round_rect(
        Rect {
            x: bar_x,
            y: bar_y,
            w: bar_w,
            h: bar_h,
        },
        6.0,
        [255, 255, 255, 28],
    );
    let ratio = ((cfg.bar.thickness.saturating_sub(24)) as f32 / 96.0).clamp(0.0, 1.0);
    let fill_w = ((bar_w as f32 * ratio).round() as i32).max(12);
    canvas.fill_round_rect(
        Rect {
            x: bar_x,
            y: bar_y,
            w: fill_w.min(bar_w),
            h: bar_h,
        },
        6.0,
        COLOR_BTN_ACTIVE,
    );
    let segments = 5;
    let seg_w = (bar_w / segments).max(10);
    for i in 0..segments {
        let sx = bar_x + i * seg_w;
        let sw = if i == segments - 1 {
            (bar_x + bar_w - sx).max(1)
        } else {
            seg_w
        };
        regions.push(HitRegion {
            rect: Rect {
                x: sx,
                y: y_thick + 3,
                w: sw,
                h: 28,
            },
            action: UiAction::SetThicknessRatio(i as f32 / (segments - 1) as f32),
        });
    }

    // Gap and Corner radius side-by-side
    let y_pair = iy + 196;
    let half_w = (iw - 16) / 2;
    render_stepper(
        canvas,
        regions,
        ix,
        y_pair,
        half_w,
        &format!("Boşluk: {} px", cfg.bar.gap),
        UiAction::AdjustGap(-2),
        UiAction::AdjustGap(2),
    );
    render_stepper(
        canvas,
        regions,
        ix + half_w + 16,
        y_pair,
        half_w,
        &format!("Köşe: {} px", cfg.bar.corner_radius),
        UiAction::AdjustCornerRadius(-2),
        UiAction::AdjustCornerRadius(2),
    );

    // Font size stepper
    let y_font = iy + 244;
    render_stepper(
        canvas,
        regions,
        ix,
        y_font,
        half_w,
        &format!("Yazı Boyutu: {:.0} pt", cfg.theme.font_size),
        UiAction::AdjustFontSize(-1.0),
        UiAction::AdjustFontSize(1.0),
    );

    // Theme presets
    let y_theme = iy + 296;
    draw_left_text(canvas, ix, y_theme, 200, 24, "Renk Teması", COLOR_MUTED, 13.0);
    let preset_gap = 8;
    let preset_count = THEME_PRESETS.len() as i32;
    let preset_w = ((iw - preset_gap * (preset_count - 1)) / preset_count).max(80);
    for (idx, preset) in THEME_PRESETS.iter().enumerate() {
        let px = ix + idx as i32 * (preset_w + preset_gap);
        let active =
            cfg.theme.foreground == preset.foreground && cfg.theme.pill == preset.pill;
        draw_button(
            canvas,
            regions,
            Rect {
                x: px,
                y: y_theme + 28,
                w: preset_w,
                h: 38,
            },
            preset.name,
            active,
            UiAction::ApplyThemePreset(idx),
        );
    }
}

fn render_widgets_tab(
    canvas: &mut dyn Canvas,
    regions: &mut Vec<HitRegion>,
    ix: i32,
    iy: i32,
    iw: i32,
    state: &SettingsAppState,
) {
    draw_left_text(
        canvas,
        ix,
        iy,
        iw,
        24,
        "Widget Sıralaması ve Görünürlük",
        COLOR_TEXT,
        14.0,
    );

    let ordered = state.ordered_widgets();
    let row_start_y = iy + 32;
    for (idx, entry) in ordered.iter().enumerate() {
        let ry = row_start_y + idx as i32 * 66;
        let is_selected = state.selected_widget == entry.id;
        let is_enabled = state.working_config.is_module_enabled(entry.id);
        let row_rect = Rect {
            x: ix,
            y: ry,
            w: iw,
            h: 58,
        };
        canvas.fill_round_rect(
            row_rect,
            10.0,
            if is_selected {
                COLOR_CARD_ACTIVE
            } else {
                COLOR_CARD
            },
        );

        let title_w = canvas
            .measure_text(entry.title, FONT_UI, 14.0)
            .w
            .clamp(1, (iw - 290).max(60));
        draw_left_text(canvas, ix + 14, ry + 8, title_w, 22, entry.title, COLOR_TEXT, 14.0);
        let badge_rect = Rect {
            x: ix + 14 + title_w + 10,
            y: ry + 9,
            w: 54,
            h: 20,
        };
        canvas.fill_round_rect(
            badge_rect,
            10.0,
            if is_enabled {
                COLOR_ACCENT_GREEN
            } else {
                [255, 255, 255, 20]
            },
        );
        canvas.draw_text(
            badge_rect,
            if is_enabled { "Aktif" } else { "Kapalı" },
            if is_enabled {
                COLOR_TEXT_GREEN
            } else {
                COLOR_MUTED
            },
            FONT_UI,
            11.0,
        );
        draw_left_text(
            canvas,
            ix + 14,
            ry + 31,
            (iw - 240).max(120),
            20,
            entry.description,
            COLOR_MUTED,
            11.5,
        );

        let btn_y = ry + 14;
        let right = ix + iw - 12;
        let select_rect = Rect {
            x: right - 72,
            y: btn_y,
            w: 72,
            h: 30,
        };
        let toggle_rect = Rect {
            x: select_rect.x - 70,
            y: btn_y,
            w: 64,
            h: 30,
        };
        let down_rect = Rect {
            x: toggle_rect.x - 36,
            y: btn_y,
            w: 30,
            h: 30,
        };
        let up_rect = Rect {
            x: down_rect.x - 36,
            y: btn_y,
            w: 30,
            h: 30,
        };

        draw_button(
            canvas,
            regions,
            up_rect,
            "↑",
            false,
            UiAction::MoveWidget {
                id: entry.id.to_string(),
                delta: -1,
            },
        );
        draw_button(
            canvas,
            regions,
            down_rect,
            "↓",
            false,
            UiAction::MoveWidget {
                id: entry.id.to_string(),
                delta: 1,
            },
        );
        draw_button(
            canvas,
            regions,
            toggle_rect,
            if is_enabled { "Açık" } else { "Kapalı" },
            is_enabled,
            UiAction::ToggleWidget(entry.id.to_string()),
        );
        draw_button(
            canvas,
            regions,
            select_rect,
            "Ayarlar ▾",
            is_selected,
            UiAction::SelectWidget(entry.id.to_string()),
        );
    }

    // Selected Widget Configuration Slot Card
    let slot_y = row_start_y + ordered.len() as i32 * 66 + 10;
    let slot_rect = Rect {
        x: ix,
        y: slot_y,
        w: iw,
        h: 174,
    };
    canvas.fill_round_rect(slot_rect, 10.0, COLOR_CARD);

    let selected_id = &state.selected_widget;
    let selected_title = KNOWN_WIDGETS
        .iter()
        .find(|w| w.id == selected_id.as_str())
        .map(|w| w.title)
        .unwrap_or(selected_id.as_str());
    draw_left_text(
        canvas,
        ix + 14,
        slot_y + 12,
        iw - 28,
        24,
        &format!("Widget Ayar Yuvası: {selected_title}"),
        COLOR_TEXT,
        14.0,
    );

    let slot_cfg = state.working_config.widget_settings(selected_id);
    draw_button(
        canvas,
        regions,
        Rect {
            x: ix + 14,
            y: slot_y + 48,
            w: 164,
            h: 34,
        },
        if slot_cfg.show_icon {
            "İkon Göster: Açık"
        } else {
            "İkon Göster: Kapalı"
        },
        slot_cfg.show_icon,
        UiAction::ToggleWidgetIcon(selected_id.clone()),
    );
    draw_button(
        canvas,
        regions,
        Rect {
            x: ix + 188,
            y: slot_y + 48,
            w: 164,
            h: 34,
        },
        if slot_cfg.show_label {
            "Etiket Göster: Açık"
        } else {
            "Etiket Göster: Kapalı"
        },
        slot_cfg.show_label,
        UiAction::ToggleWidgetLabel(selected_id.clone()),
    );

    let param_y = slot_y + 100;
    if selected_id == "clock" {
        let is_12h = slot_cfg.params.get("format").map(|s| s.as_str()) == Some("12h");
        draw_left_text(canvas, ix + 14, param_y, 110, 34, "Saat Formatı:", COLOR_MUTED, 13.0);
        draw_button(
            canvas,
            regions,
            Rect {
                x: ix + 128,
                y: param_y,
                w: 100,
                h: 34,
            },
            "24 Saat",
            !is_12h,
            UiAction::SetClockFormat("24h".to_string()),
        );
        draw_button(
            canvas,
            regions,
            Rect {
                x: ix + 236,
                y: param_y,
                w: 140,
                h: 34,
            },
            "12 Saat (AM/PM)",
            is_12h,
            UiAction::SetClockFormat("12h".to_string()),
        );
    } else if selected_id == "battery" {
        let show_bolt = slot_cfg.params.get("show_bolt").map(|s| s.as_str()) != Some("false");
        draw_button(
            canvas,
            regions,
            Rect {
                x: ix + 14,
                y: param_y,
                w: 220,
                h: 34,
            },
            if show_bolt {
                "Şarj Sembolü (⚡): Açık"
            } else {
                "Şarj Sembolü (⚡): Kapalı"
            },
            show_bolt,
            UiAction::ToggleBatteryBolt,
        );
    } else {
        draw_left_text(
            canvas,
            ix + 14,
            param_y,
            iw - 28,
            28,
            "Bu widget için ek özel parametre yok.",
            COLOR_MUTED,
            12.5,
        );
    }
}

fn render_about_tab(
    canvas: &mut dyn Canvas,
    regions: &mut Vec<HitRegion>,
    ix: i32,
    iy: i32,
    iw: i32,
    state: &SettingsAppState,
) {
    let card = Rect {
        x: ix,
        y: iy,
        w: iw,
        h: 210,
    };
    canvas.fill_round_rect(card, 10.0, COLOR_CARD);
    draw_left_text(
        canvas,
        ix + 16,
        iy + 16,
        iw - 32,
        26,
        "Taskbar Hakkında & Güncellemeler",
        COLOR_TEXT,
        15.0,
    );
    draw_left_text(
        canvas,
        ix + 16,
        iy + 52,
        iw - 32,
        22,
        concat!("Sürüm: v", env!("CARGO_PKG_VERSION")),
        COLOR_MUTED,
        13.0,
    );
    draw_left_text(
        canvas,
        ix + 16,
        iy + 80,
        iw - 32,
        22,
        &format!("Yapılandırma: {}", state.config_path.display()),
        COLOR_MUTED,
        12.5,
    );
    draw_left_text(
        canvas,
        ix + 16,
        iy + 110,
        iw - 32,
        22,
        "Güncellemeler yalnızca ikilileri değiştirir; ayarlarınız korunur.",
        COLOR_MUTED,
        12.5,
    );
    draw_button(
        canvas,
        regions,
        Rect {
            x: ix + 16,
            y: iy + 152,
            w: 230,
            h: 36,
        },
        "Güncellemeleri Şimdi Denetle",
        true,
        UiAction::CheckUpdates,
    );
}

pub fn render_settings_ui(
    canvas: &mut dyn Canvas,
    size: Size,
    state: &SettingsAppState,
) -> Vec<HitRegion> {
    let mut regions = Vec::new();
    let w = size.w.max(480);
    let h = size.h.max(420);

    canvas.fill_round_rect(Rect { x: 0, y: 0, w, h }, 0.0, COLOR_BG);

    // Header
    draw_left_text(canvas, 20, 18, 260, 32, "Taskbar Ayarları", COLOR_TEXT, 18.0);
    let status_pill = Rect {
        x: w - 182,
        y: 20,
        w: 162,
        h: 28,
    };
    canvas.fill_round_rect(
        status_pill,
        14.0,
        if state.live_connected {
            COLOR_ACCENT_GREEN
        } else {
            COLOR_ACCENT_AMBER
        },
    );
    canvas.draw_text(
        status_pill,
        if state.live_connected {
            "● Canlı Önizleme"
        } else {
            "○ Çevrimdışı"
        },
        if state.live_connected {
            COLOR_TEXT_GREEN
        } else {
            COLOR_TEXT_AMBER
        },
        FONT_UI,
        12.0,
    );

    // Sidebar
    let body_h = (h - 140).max(260);
    canvas.fill_round_rect(
        Rect {
            x: 16,
            y: 70,
            w: 172,
            h: body_h,
        },
        12.0,
        COLOR_PANEL,
    );
    let tabs = [
        ("Görünüm & Konum", SettingsTab::Appearance),
        ("Widget'lar", SettingsTab::Widgets),
        ("Güncelleme", SettingsTab::About),
    ];
    for (idx, (label, tab)) in tabs.iter().enumerate() {
        draw_button(
            canvas,
            &mut regions,
            Rect {
                x: 24,
                y: 82 + idx as i32 * 46,
                w: 156,
                h: 38,
            },
            label,
            state.tab == *tab,
            UiAction::SelectTab(*tab),
        );
    }

    // Main content panel
    let cx = 200;
    let cy = 70;
    let cw = (w - 216).max(260);
    canvas.fill_round_rect(
        Rect {
            x: cx,
            y: cy,
            w: cw,
            h: body_h,
        },
        12.0,
        COLOR_PANEL,
    );
    let ix = cx + 16;
    let iy = cy + 16;
    let iw = (cw - 32).max(220);

    match state.tab {
        SettingsTab::Appearance => {
            render_appearance_tab(canvas, &mut regions, ix, iy, iw, state);
        }
        SettingsTab::Widgets => {
            render_widgets_tab(canvas, &mut regions, ix, iy, iw, state);
        }
        SettingsTab::About => {
            render_about_tab(canvas, &mut regions, ix, iy, iw, state);
        }
    }

    // Bottom footer bar
    let fy = h - 58;
    canvas.fill_round_rect(
        Rect {
            x: 16,
            y: fy,
            w: w - 32,
            h: 44,
        },
        10.0,
        COLOR_PANEL,
    );
    let footer_status = if !state.status_text.is_empty() {
        state.status_text.as_str()
    } else if state.has_unsaved_changes() {
        "Kaydedilmemiş değişiklikler önizleniyor"
    } else {
        "Tüm ayarlar güncel"
    };
    draw_left_text(
        canvas,
        30,
        fy + 10,
        (w - 300).max(120),
        24,
        footer_status,
        COLOR_MUTED,
        12.5,
    );
    draw_button(
        canvas,
        &mut regions,
        Rect {
            x: w - 258,
            y: fy + 6,
            w: 128,
            h: 32,
        },
        "İptal / Geri Al",
        false,
        UiAction::Revert,
    );
    draw_button(
        canvas,
        &mut regions,
        Rect {
            x: w - 122,
            y: fy + 6,
            w: 100,
            h: 32,
        },
        "Kaydet",
        true,
        UiAction::Save,
    );

    regions
}

pub fn hit_test(regions: &[HitRegion], x: i32, y: i32) -> Option<UiAction> {
    regions
        .iter()
        .rev()
        .find(|hr| {
            x >= hr.rect.x
                && x < hr.rect.x + hr.rect.w
                && y >= hr.rect.y
                && y < hr.rect.y + hr.rect.h
        })
        .map(|hr| hr.action.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::Config;
    use std::path::PathBuf;

    #[derive(Default)]
    struct RecordingCanvas {
        rects: Vec<(Rect, f32, [u8; 4])>,
        texts: Vec<String>,
    }

    impl Canvas for RecordingCanvas {
        fn fill_round_rect(&mut self, r: Rect, radius: f32, color: [u8; 4]) {
            self.rects.push((r, radius, color));
        }

        fn draw_text(&mut self, _r: Rect, text: &str, _color: [u8; 4], _font: &str, _size: f32) {
            self.texts.push(text.to_string());
        }

        fn measure_text(&mut self, text: &str, _font: &str, _size: f32) -> Size {
            Size {
                w: (text.chars().count() as i32 * 8).max(8),
                h: 16,
            }
        }
    }

    fn center_of(r: Rect) -> (i32, i32) {
        (r.x + r.w / 2, r.y + r.h / 2)
    }

    fn click_action(
        regions: &[HitRegion],
        state: &mut SettingsAppState,
        predicate: impl Fn(&UiAction) -> bool,
    ) {
        let hit = regions
            .iter()
            .find(|hr| predicate(&hr.action))
            .expect("expected HitRegion");
        let (cx, cy) = center_of(hit.rect);
        let action = hit_test(regions, cx, cy).expect("hit_test finds region");
        assert!(predicate(&action));
        state.apply_action(action);
    }

    #[test]
    fn render_settings_ui_produces_hit_regions_for_all_tabs_and_controls() {
        let mut state = SettingsAppState::new(
            Config::default(),
            PathBuf::from(r"C:\Temp\config.toml"),
            true,
        );
        let size = Size { w: 760, h: 560 };

        // 1. Appearance tab
        let mut canvas = RecordingCanvas::default();
        let regions = render_settings_ui(&mut canvas, size, &state);
        assert!(canvas.texts.iter().any(|t| t.contains("Taskbar Ayarları")));
        assert!(canvas.texts.iter().any(|t| t.contains("Canlı Önizleme")));
        for hr in &regions {
            assert!(hr.rect.x >= 0 && hr.rect.y >= 0);
            assert!(hr.rect.x + hr.rect.w <= size.w);
            assert!(hr.rect.y + hr.rect.h <= size.h);
        }

        click_action(&regions, &mut state, |a| *a == UiAction::SetEdge(Edge::Right));
        assert_eq!(state.working_config.bar.edge, Edge::Right);

        click_action(&regions, &mut state, |a| {
            *a == UiAction::SelectTab(SettingsTab::Widgets)
        });
        assert_eq!(state.tab, SettingsTab::Widgets);

        // 2. Widgets tab
        let mut w_canvas = RecordingCanvas::default();
        let w_regions = render_settings_ui(&mut w_canvas, size, &state);
        assert!(w_canvas.texts.iter().any(|t| t.contains("Pil ve Güç")));
        assert!(w_canvas.texts.iter().any(|t| t.contains("Saat")));

        click_action(&w_regions, &mut state, |a| {
            matches!(a, UiAction::MoveWidget { id, delta } if id == "clock" && *delta == -1)
        });
        assert_eq!(
            state.working_config.modules,
            vec!["clock".to_string(), "battery".to_string()]
        );

        click_action(&w_regions, &mut state, |a| {
            matches!(a, UiAction::SelectWidget(id) if id == "clock")
        });
        let w_regions2 = render_settings_ui(&mut RecordingCanvas::default(), size, &state);
        click_action(&w_regions2, &mut state, |a| {
            matches!(a, UiAction::SetClockFormat(fmt) if fmt == "12h")
        });
        assert_eq!(
            state
                .working_config
                .widget_settings("clock")
                .params
                .get("format")
                .map(String::as_str),
            Some("12h")
        );

        click_action(&w_regions2, &mut state, |a| {
            matches!(a, UiAction::ToggleWidget(id) if id == "battery")
        });
        assert!(!state.working_config.is_module_enabled("battery"));

        // 3. About tab
        click_action(&w_regions2, &mut state, |a| {
            *a == UiAction::SelectTab(SettingsTab::About)
        });
        let mut a_canvas = RecordingCanvas::default();
        let a_regions = render_settings_ui(&mut a_canvas, size, &state);
        assert!(a_canvas.texts.iter().any(|t| t.contains("config.toml")));
        assert!(a_regions
            .iter()
            .any(|hr| hr.action == UiAction::CheckUpdates));
        assert!(a_regions.iter().any(|hr| hr.action == UiAction::Save));
        assert!(a_regions.iter().any(|hr| hr.action == UiAction::Revert));
    }
}

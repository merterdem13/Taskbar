use core::{Align, Backdrop, Edge, Rect, Size, KNOWN_WIDGETS};

use crate::anim::{AppPhase, UiAnimState};
use crate::canvas::SettingsPainter;
use crate::state::{SettingsAppState, SettingsTab, UiAction, THEME_PRESETS};

const FONT_UI: &str = "Segoe UI";
const FONT_BOLD: &str = "Segoe UI Bold";

// High-contrast dark palette (#070708 background, #FFFFFF foreground, #10B981 emerald accent)
const COLOR_BG: [u8; 4] = [7, 7, 8, 255];
const COLOR_CARD: [u8; 4] = [16, 18, 22, 255];
const COLOR_BORDER: [u8; 4] = [255, 255, 255, 22];
const COLOR_BORDER_STRONG: [u8; 4] = [255, 255, 255, 38];
const COLOR_CONTROL: [u8; 4] = [23, 26, 33, 255];
const COLOR_CONTROL_HOVER: [u8; 4] = [34, 39, 50, 255];
const COLOR_CONTROL_PRESS: [u8; 4] = [18, 21, 27, 255];
const COLOR_TEXT: [u8; 4] = [255, 255, 255, 255];
const COLOR_MUTED: [u8; 4] = [156, 163, 175, 255];
const COLOR_TEXT_DARK: [u8; 4] = [7, 14, 11, 255];

const COLOR_ACCENT: [u8; 4] = [16, 185, 129, 255];
const COLOR_ACCENT_HOVER: [u8; 4] = [52, 211, 153, 255];
const COLOR_ACCENT_SOFT: [u8; 4] = [16, 185, 129, 38];
const COLOR_ACCENT_BORDER: [u8; 4] = [16, 185, 129, 130];

const COLOR_AMBER_SOFT: [u8; 4] = [245, 158, 11, 36];
const COLOR_AMBER_BORDER: [u8; 4] = [245, 158, 11, 110];
const COLOR_TEXT_AMBER: [u8; 4] = [252, 211, 77, 255];

pub const TITLEBAR_DRAG_HEIGHT: i32 = 64;
const COLOR_CLOSE_HOVER: [u8; 4] = [232, 17, 35, 255];
const COLOR_CLOSE_PRESS: [u8; 4] = [196, 16, 30, 255];

#[derive(Debug, Clone, PartialEq)]
pub struct HitRegion {
    pub id: String,
    pub rect: Rect,
    pub action: UiAction,
    pub is_thickness_slider: bool,
}

fn with_alpha(c: [u8; 4], alpha: f32) -> [u8; 4] {
    [
        c[0],
        c[1],
        c[2],
        ((c[3] as f32 * alpha.clamp(0.0, 1.0)).round() as i32).clamp(0, 255) as u8,
    ]
}

fn lerp_color(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t).round() as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t).round() as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t).round() as u8,
        (a[3] as f32 + (b[3] as f32 - a[3] as f32) * t).round() as u8,
    ]
}

fn draw_card(canvas: &mut dyn SettingsPainter, rect: Rect, radius: f32, alpha: f32) {
    canvas.fill_round_rect(rect, radius, with_alpha(COLOR_CARD, alpha));
    canvas.stroke_round_rect(rect, radius, 1.0, with_alpha(COLOR_BORDER, alpha));
}

fn draw_left_text(
    canvas: &mut dyn SettingsPainter,
    x: i32,
    y: i32,
    max_w: i32,
    h: i32,
    text: &str,
    color: [u8; 4],
    font: &str,
    size: f32,
) {
    let measured = canvas.measure_text(text, font, size);
    let w = measured.w.clamp(1, max_w.max(1));
    canvas.draw_text(Rect { x, y, w, h }, text, color, font, size);
}

#[allow(clippy::too_many_arguments)]
fn draw_button(
    canvas: &mut dyn SettingsPainter,
    regions: &mut Vec<HitRegion>,
    anim: &UiAnimState,
    id: &str,
    rect: Rect,
    label: &str,
    active: bool,
    primary: bool,
    alpha: f32,
    action: UiAction,
) {
    let hover = anim.hover_amount(id);
    let press = anim.press_amount(id);

    let (bg, border, fg) = if primary {
        let base = lerp_color(COLOR_ACCENT, COLOR_ACCENT_HOVER, hover);
        let bg = lerp_color(base, COLOR_ACCENT, press * 0.6);
        (bg, COLOR_ACCENT_HOVER, COLOR_TEXT_DARK)
    } else if active {
        let bg = lerp_color(COLOR_ACCENT_SOFT, [16, 185, 129, 58], hover);
        (bg, COLOR_ACCENT_BORDER, COLOR_TEXT)
    } else {
        let hovered = lerp_color(COLOR_CONTROL, COLOR_CONTROL_HOVER, hover);
        let bg = lerp_color(hovered, COLOR_CONTROL_PRESS, press);
        let border = lerp_color(COLOR_BORDER, COLOR_BORDER_STRONG, hover);
        (bg, border, [229, 231, 235, 255])
    };

    canvas.fill_round_rect(rect, 8.0, with_alpha(bg, alpha));
    canvas.stroke_round_rect(rect, 8.0, 1.0, with_alpha(border, alpha));
    canvas.draw_text(
        rect,
        label,
        with_alpha(fg, alpha),
        if primary { FONT_BOLD } else { FONT_UI },
        12.5,
    );
    regions.push(HitRegion {
        id: id.to_string(),
        rect,
        action,
        is_thickness_slider: false,
    });
}

#[allow(clippy::too_many_arguments)]
fn render_segmented_row<T: Copy + PartialEq>(
    canvas: &mut dyn SettingsPainter,
    regions: &mut Vec<HitRegion>,
    anim: &UiAnimState,
    id_prefix: &str,
    x: i32,
    y: i32,
    total_w: i32,
    title: &str,
    current: T,
    alpha: f32,
    options: &[(&str, T, UiAction)],
) {
    let label_w = if total_w < 380 { 96 } else { 126 };
    draw_left_text(
        canvas,
        x,
        y,
        label_w,
        32,
        title,
        with_alpha(COLOR_MUTED, alpha),
        FONT_UI,
        12.5,
    );
    let btn_area_x = x + label_w + 8;
    let btn_area_w = (total_w - label_w - 8).max(120);
    let gap = 6;
    let count = options.len().max(1) as i32;
    let btn_w = ((btn_area_w - gap * (count - 1)) / count).max(36);
    for (idx, (label, val, action)) in options.iter().enumerate() {
        let bx = btn_area_x + idx as i32 * (btn_w + gap);
        let bw = if idx as i32 == count - 1 {
            (btn_area_x + btn_area_w - bx).max(36)
        } else {
            btn_w
        };
        draw_button(
            canvas,
            regions,
            anim,
            &format!("{id_prefix}_{idx}"),
            Rect {
                x: bx,
                y,
                w: bw,
                h: 32,
            },
            label,
            *val == current,
            false,
            alpha,
            action.clone(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_stepper(
    canvas: &mut dyn SettingsPainter,
    regions: &mut Vec<HitRegion>,
    anim: &UiAnimState,
    id_prefix: &str,
    x: i32,
    y: i32,
    w: i32,
    label: &str,
    alpha: f32,
    dec_action: UiAction,
    inc_action: UiAction,
) {
    draw_left_text(
        canvas,
        x,
        y,
        (w - 72).max(60),
        30,
        label,
        with_alpha(COLOR_MUTED, alpha),
        FONT_UI,
        12.5,
    );
    let dec_rect = Rect {
        x: x + w - 66,
        y,
        w: 30,
        h: 30,
    };
    let inc_rect = Rect {
        x: x + w - 32,
        y,
        w: 30,
        h: 30,
    };
    draw_button(
        canvas,
        regions,
        anim,
        &format!("{id_prefix}_dec"),
        dec_rect,
        "-",
        false,
        false,
        alpha,
        dec_action,
    );
    draw_button(
        canvas,
        regions,
        anim,
        &format!("{id_prefix}_inc"),
        inc_rect,
        "+",
        false,
        false,
        alpha,
        inc_action,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_appearance_tab(
    canvas: &mut dyn SettingsPainter,
    regions: &mut Vec<HitRegion>,
    anim: &UiAnimState,
    ix: i32,
    iy: i32,
    iw: i32,
    ih: i32,
    state: &SettingsAppState,
    alpha: f32,
) {
    let cfg = &state.working_config;
    let compact = ih < 380;
    let row_step = if compact { 36 } else { 40 };

    // Card 1: Konum & Boyut
    let c1_h = if compact { 164 } else { 184 };
    let c1_rect = Rect {
        x: ix,
        y: iy,
        w: iw,
        h: c1_h,
    };
    draw_card(canvas, c1_rect, 10.0, alpha);
    let c1x = ix + 14;
    let c1w = (iw - 28).max(160);
    draw_left_text(
        canvas,
        c1x,
        iy + 8,
        c1w,
        20,
        "Konum ve Boyut",
        with_alpha(COLOR_TEXT, alpha),
        FONT_BOLD,
        12.5,
    );

    render_segmented_row(
        canvas,
        regions,
        anim,
        "edge",
        c1x,
        iy + 32,
        c1w,
        "Ekran Kenarı",
        cfg.bar.edge,
        alpha,
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
        anim,
        "align",
        c1x,
        iy + 32 + row_step,
        c1w,
        "Hizalama",
        cfg.bar.align,
        alpha,
        &[
            ("Başlangıç", Align::Start, UiAction::SetAlign(Align::Start)),
            ("Orta", Align::Center, UiAction::SetAlign(Align::Center)),
            ("Son", Align::End, UiAction::SetAlign(Align::End)),
        ],
    );

    // Thickness continuous draggable slider + steppers
    let y_thick = iy + 32 + row_step * 2;
    let label_w = if c1w < 380 { 96 } else { 126 };
    let thick_label = format!("Kalınlık: {} px", cfg.bar.thickness);
    draw_left_text(
        canvas,
        c1x,
        y_thick,
        label_w,
        30,
        &thick_label,
        with_alpha(COLOR_MUTED, alpha),
        FONT_UI,
        12.5,
    );
    let dec_rect = Rect {
        x: c1x + label_w + 8,
        y: y_thick,
        w: 30,
        h: 30,
    };
    draw_button(
        canvas,
        regions,
        anim,
        "thick_dec",
        dec_rect,
        "-",
        false,
        false,
        alpha,
        UiAction::AdjustThickness(-4),
    );
    let inc_rect = Rect {
        x: c1x + c1w - 30,
        y: y_thick,
        w: 30,
        h: 30,
    };
    draw_button(
        canvas,
        regions,
        anim,
        "thick_inc",
        inc_rect,
        "+",
        false,
        false,
        alpha,
        UiAction::AdjustThickness(4),
    );

    let bar_x = dec_rect.x + dec_rect.w + 10;
    let bar_w = (inc_rect.x - 10 - bar_x).max(60);
    let bar_y = y_thick + 11;
    let bar_h = 8;
    let slider_hover = anim.hover_amount("thick_slider");
    canvas.fill_round_rect(
        Rect {
            x: bar_x,
            y: bar_y,
            w: bar_w,
            h: bar_h,
        },
        4.0,
        with_alpha(COLOR_CONTROL, alpha),
    );
    canvas.stroke_round_rect(
        Rect {
            x: bar_x,
            y: bar_y,
            w: bar_w,
            h: bar_h,
        },
        4.0,
        1.0,
        with_alpha(lerp_color(COLOR_BORDER, COLOR_BORDER_STRONG, slider_hover), alpha),
    );

    let ratio = ((anim.thickness_visual() - 24.0) / 96.0).clamp(0.0, 1.0);
    let fill_w = ((bar_w as f32 * ratio).round() as i32).clamp(8, bar_w);
    canvas.fill_round_rect(
        Rect {
            x: bar_x,
            y: bar_y,
            w: fill_w,
            h: bar_h,
        },
        4.0,
        with_alpha(COLOR_ACCENT, alpha),
    );
    let knob_x = (bar_x + fill_w - 6).clamp(bar_x, bar_x + bar_w - 12);
    canvas.fill_round_rect(
        Rect {
            x: knob_x,
            y: bar_y - 2,
            w: 12,
            h: 12,
        },
        6.0,
        with_alpha(COLOR_TEXT, alpha),
    );
    regions.push(HitRegion {
        id: "thick_slider".to_string(),
        rect: Rect {
            x: bar_x,
            y: y_thick + 2,
            w: bar_w,
            h: 26,
        },
        action: UiAction::SetThickness(cfg.bar.thickness),
        is_thickness_slider: true,
    });

    // Gap & Corner radius
    let y_pair = iy + 32 + row_step * 3;
    let half_w = (c1w - 16) / 2;
    render_stepper(
        canvas,
        regions,
        anim,
        "gap",
        c1x,
        y_pair,
        half_w,
        &format!("Boşluk: {} px", cfg.bar.gap),
        alpha,
        UiAction::AdjustGap(-2),
        UiAction::AdjustGap(2),
    );
    render_stepper(
        canvas,
        regions,
        anim,
        "corner",
        c1x + half_w + 16,
        y_pair,
        half_w,
        &format!("Köşe: {} px", cfg.bar.corner_radius),
        alpha,
        UiAction::AdjustCornerRadius(-2),
        UiAction::AdjustCornerRadius(2),
    );

    // Card 2: Arka Plan ve Tipografi
    let c2_y = iy + c1_h + if compact { 8 } else { 12 };
    let c2_h = if compact { 102 } else { 112 };
    if c2_y + c2_h <= iy + ih {
        draw_card(
            canvas,
            Rect {
                x: ix,
                y: c2_y,
                w: iw,
                h: c2_h,
            },
            10.0,
            alpha,
        );
        draw_left_text(
            canvas,
            c1x,
            c2_y + 8,
            c1w,
            20,
            "Arka Plan ve Tipografi",
            with_alpha(COLOR_TEXT, alpha),
            FONT_BOLD,
            12.5,
        );
        render_segmented_row(
            canvas,
            regions,
            anim,
            "backdrop",
            c1x,
            c2_y + 32,
            c1w,
            "Arka Plan",
            cfg.bar.backdrop,
            alpha,
            &[
                ("Acrylic", Backdrop::Acrylic, UiAction::SetBackdrop(Backdrop::Acrylic)),
                ("Mica", Backdrop::Mica, UiAction::SetBackdrop(Backdrop::Mica)),
                ("Solid", Backdrop::Solid, UiAction::SetBackdrop(Backdrop::Solid)),
            ],
        );
        render_stepper(
            canvas,
            regions,
            anim,
            "font_size",
            c1x,
            c2_y + 32 + row_step,
            half_w,
            &format!("Yazı Boyutu: {:.0} pt", cfg.theme.font_size),
            alpha,
            UiAction::AdjustFontSize(-1.0),
            UiAction::AdjustFontSize(1.0),
        );
    }

    // Card 3: Renk Teması (with swatches)
    let c3_y = c2_y + c2_h + if compact { 8 } else { 12 };
    let c3_h = (iy + ih - c3_y).min(if compact { 82 } else { 94 });
    if c3_h >= 68 {
        draw_card(
            canvas,
            Rect {
                x: ix,
                y: c3_y,
                w: iw,
                h: c3_h,
            },
            10.0,
            alpha,
        );
        draw_left_text(
            canvas,
            c1x,
            c3_y + 8,
            c1w,
            20,
            "Çubuk Renk Teması",
            with_alpha(COLOR_TEXT, alpha),
            FONT_BOLD,
            12.5,
        );
        let preset_gap = 8;
        let preset_count = THEME_PRESETS.len() as i32;
        let preset_w = ((c1w - preset_gap * (preset_count - 1)) / preset_count).max(60);
        for (idx, preset) in THEME_PRESETS.iter().enumerate() {
            let px = c1x + idx as i32 * (preset_w + preset_gap);
            let pw = if idx as i32 == preset_count - 1 {
                (c1x + c1w - px).max(60)
            } else {
                preset_w
            };
            let active =
                cfg.theme.foreground == preset.foreground && cfg.theme.pill == preset.pill;
            let prect = Rect {
                x: px,
                y: c3_y + 32,
                w: pw,
                h: 34,
            };
            draw_button(
                canvas,
                regions,
                anim,
                &format!("preset_{idx}"),
                prect,
                preset.name,
                active,
                false,
                alpha,
                UiAction::ApplyThemePreset(idx),
            );
            // Color swatch dot on the left of preset label
            let swatch = Rect {
                x: prect.x + 8,
                y: prect.y + 12,
                w: 10,
                h: 10,
            };
            canvas.fill_round_rect(swatch, 5.0, with_alpha(preset.foreground, alpha));
            canvas.stroke_round_rect(swatch, 5.0, 1.0, with_alpha(COLOR_BORDER_STRONG, alpha));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_widgets_tab(
    canvas: &mut dyn SettingsPainter,
    regions: &mut Vec<HitRegion>,
    anim: &UiAnimState,
    ix: i32,
    iy: i32,
    iw: i32,
    ih: i32,
    state: &SettingsAppState,
    alpha: f32,
) {
    let compact = ih < 380;
    draw_left_text(
        canvas,
        ix,
        iy,
        iw,
        22,
        "Widget Sıralaması ve Görünürlük",
        with_alpha(COLOR_TEXT, alpha),
        FONT_BOLD,
        13.5,
    );

    let ordered = state.ordered_widgets();
    let row_start_y = iy + 28;
    let row_h = if compact { 50 } else { 58 };
    let row_step = if compact { 56 } else { 64 };

    for (idx, entry) in ordered.iter().enumerate() {
        let ry = row_start_y + idx as i32 * row_step;
        let is_selected = state.selected_widget == entry.id;
        let is_enabled = state.working_config.is_module_enabled(entry.id);
        let row_rect = Rect {
            x: ix,
            y: ry,
            w: iw,
            h: row_h,
        };
        canvas.fill_round_rect(
            row_rect,
            10.0,
            with_alpha(
                if is_selected { COLOR_ACCENT_SOFT } else { COLOR_CARD },
                alpha,
            ),
        );
        canvas.stroke_round_rect(
            row_rect,
            10.0,
            1.0,
            with_alpha(
                if is_selected {
                    COLOR_ACCENT_BORDER
                } else {
                    COLOR_BORDER
                },
                alpha,
            ),
        );

        let title_w = canvas
            .measure_text(entry.title, FONT_BOLD, 13.5)
            .w
            .clamp(1, (iw - 290).max(60));
        draw_left_text(
            canvas,
            ix + 14,
            ry + 6,
            title_w,
            20,
            entry.title,
            with_alpha(COLOR_TEXT, alpha),
            FONT_BOLD,
            13.5,
        );
        let badge_rect = Rect {
            x: ix + 14 + title_w + 10,
            y: ry + 7,
            w: 52,
            h: 18,
        };
        canvas.fill_round_rect(
            badge_rect,
            9.0,
            with_alpha(
                if is_enabled {
                    COLOR_ACCENT_SOFT
                } else {
                    COLOR_CONTROL
                },
                alpha,
            ),
        );
        canvas.stroke_round_rect(
            badge_rect,
            9.0,
            1.0,
            with_alpha(
                if is_enabled {
                    COLOR_ACCENT_BORDER
                } else {
                    COLOR_BORDER
                },
                alpha,
            ),
        );
        canvas.draw_text(
            badge_rect,
            if is_enabled { "Aktif" } else { "Kapalı" },
            with_alpha(if is_enabled { COLOR_ACCENT_HOVER } else { COLOR_MUTED }, alpha),
            FONT_UI,
            10.5,
        );
        if !compact {
            draw_left_text(
                canvas,
                ix + 14,
                ry + 29,
                (iw - 250).max(100),
                18,
                entry.description,
                with_alpha(COLOR_MUTED, alpha),
                FONT_UI,
                11.5,
            );
        }

        let btn_y = ry + (row_h - 30) / 2;
        let right = ix + iw - 12;
        let select_rect = Rect {
            x: right - 72,
            y: btn_y,
            w: 72,
            h: 30,
        };
        let toggle_rect = Rect {
            x: select_rect.x - 68,
            y: btn_y,
            w: 62,
            h: 30,
        };
        let down_rect = Rect {
            x: toggle_rect.x - 34,
            y: btn_y,
            w: 28,
            h: 30,
        };
        let up_rect = Rect {
            x: down_rect.x - 34,
            y: btn_y,
            w: 28,
            h: 30,
        };

        draw_button(
            canvas,
            regions,
            anim,
            &format!("w_up_{}", entry.id),
            up_rect,
            "↑",
            false,
            false,
            alpha,
            UiAction::MoveWidget {
                id: entry.id.to_string(),
                delta: -1,
            },
        );
        draw_button(
            canvas,
            regions,
            anim,
            &format!("w_down_{}", entry.id),
            down_rect,
            "↓",
            false,
            false,
            alpha,
            UiAction::MoveWidget {
                id: entry.id.to_string(),
                delta: 1,
            },
        );
        draw_button(
            canvas,
            regions,
            anim,
            &format!("w_toggle_{}", entry.id),
            toggle_rect,
            if is_enabled { "Açık" } else { "Kapalı" },
            is_enabled,
            false,
            alpha,
            UiAction::ToggleWidget(entry.id.to_string()),
        );
        draw_button(
            canvas,
            regions,
            anim,
            &format!("w_select_{}", entry.id),
            select_rect,
            "Ayarlar ▾",
            is_selected,
            false,
            alpha,
            UiAction::SelectWidget(entry.id.to_string()),
        );
    }

    // Selected Widget Configuration Slot Card
    let slot_y = row_start_y + ordered.len() as i32 * row_step + 8;
    let slot_h = (iy + ih - slot_y).clamp(118, 162);
    if slot_y + 118 <= iy + ih {
        draw_card(
            canvas,
            Rect {
                x: ix,
                y: slot_y,
                w: iw,
                h: slot_h,
            },
            10.0,
            alpha,
        );

        let selected_id = &state.selected_widget;
        let selected_title = KNOWN_WIDGETS
            .iter()
            .find(|w| w.id == selected_id.as_str())
            .map(|w| w.title)
            .unwrap_or(selected_id.as_str());
        draw_left_text(
            canvas,
            ix + 14,
            slot_y + 10,
            iw - 28,
            22,
            &format!("Seçili Widget Ayarları: {selected_title}"),
            with_alpha(COLOR_TEXT, alpha),
            FONT_BOLD,
            13.0,
        );

        let slot_cfg = state.working_config.widget_settings(selected_id);
        let btn_w = ((iw - 38) / 2).clamp(120, 174);
        draw_button(
            canvas,
            regions,
            anim,
            "slot_icon",
            Rect {
                x: ix + 14,
                y: slot_y + 38,
                w: btn_w,
                h: 32,
            },
            if slot_cfg.show_icon {
                "İkon Göster: Açık"
            } else {
                "İkon Göster: Kapalı"
            },
            slot_cfg.show_icon,
            false,
            alpha,
            UiAction::ToggleWidgetIcon(selected_id.clone()),
        );
        draw_button(
            canvas,
            regions,
            anim,
            "slot_label",
            Rect {
                x: ix + 24 + btn_w,
                y: slot_y + 38,
                w: btn_w,
                h: 32,
            },
            if slot_cfg.show_label {
                "Etiket Göster: Açık"
            } else {
                "Etiket Göster: Kapalı"
            },
            slot_cfg.show_label,
            false,
            alpha,
            UiAction::ToggleWidgetLabel(selected_id.clone()),
        );

        let param_y = slot_y + 80;
        if selected_id == "clock" {
            let is_12h = slot_cfg.params.get("format").map(|s| s.as_str()) == Some("12h");
            draw_left_text(
                canvas,
                ix + 14,
                param_y,
                96,
                32,
                "Saat Formatı",
                with_alpha(COLOR_MUTED, alpha),
                FONT_UI,
                12.5,
            );
            draw_button(
                canvas,
                regions,
                anim,
                "clock_24h",
                Rect {
                    x: ix + 116,
                    y: param_y,
                    w: 88,
                    h: 32,
                },
                "24 Saat",
                !is_12h,
                false,
                alpha,
                UiAction::SetClockFormat("24h".to_string()),
            );
            draw_button(
                canvas,
                regions,
                anim,
                "clock_12h",
                Rect {
                    x: ix + 212,
                    y: param_y,
                    w: 132,
                    h: 32,
                },
                "12 Saat (AM/PM)",
                is_12h,
                false,
                alpha,
                UiAction::SetClockFormat("12h".to_string()),
            );
        } else if selected_id == "battery" {
            let show_bolt = slot_cfg.params.get("show_bolt").map(|s| s.as_str()) != Some("false");
            draw_button(
                canvas,
                regions,
                anim,
                "battery_bolt",
                Rect {
                    x: ix + 14,
                    y: param_y,
                    w: 220,
                    h: 32,
                },
                if show_bolt {
                    "Şarj Sembolü (⚡): Açık"
                } else {
                    "Şarj Sembolü (⚡): Kapalı"
                },
                show_bolt,
                false,
                alpha,
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
                with_alpha(COLOR_MUTED, alpha),
                FONT_UI,
                12.5,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_about_tab(
    canvas: &mut dyn SettingsPainter,
    regions: &mut Vec<HitRegion>,
    anim: &UiAnimState,
    ix: i32,
    iy: i32,
    iw: i32,
    state: &SettingsAppState,
    alpha: f32,
) {
    let card = Rect {
        x: ix,
        y: iy,
        w: iw,
        h: 212,
    };
    draw_card(canvas, card, 10.0, alpha);
    draw_left_text(
        canvas,
        ix + 16,
        iy + 16,
        iw - 32,
        24,
        "Taskbar Hakkında ve Güncellemeler",
        with_alpha(COLOR_TEXT, alpha),
        FONT_BOLD,
        14.5,
    );
    draw_left_text(
        canvas,
        ix + 16,
        iy + 50,
        iw - 32,
        20,
        concat!("Sürüm: v", env!("CARGO_PKG_VERSION")),
        with_alpha(COLOR_MUTED, alpha),
        FONT_UI,
        13.0,
    );
    draw_left_text(
        canvas,
        ix + 16,
        iy + 78,
        iw - 32,
        20,
        &format!("Yapılandırma: {}", state.config_path.display()),
        with_alpha(COLOR_MUTED, alpha),
        FONT_UI,
        12.5,
    );
    draw_left_text(
        canvas,
        ix + 16,
        iy + 108,
        iw - 32,
        20,
        "Güncellemeler yalnızca ikili dosyaları yeniler; ayarlarınız korunur.",
        with_alpha(COLOR_MUTED, alpha),
        FONT_UI,
        12.5,
    );
    draw_button(
        canvas,
        regions,
        anim,
        "check_updates",
        Rect {
            x: ix + 16,
            y: iy + 152,
            w: 230,
            h: 36,
        },
        "Güncellemeleri Şimdi Denetle",
        false,
        true,
        alpha,
        UiAction::CheckUpdates,
    );
}

fn draw_window_controls(
    canvas: &mut dyn SettingsPainter,
    regions: &mut Vec<HitRegion>,
    anim: &UiAnimState,
    w: i32,
    maximized: bool,
    alpha: f32,
) {
    let buttons = [
        ("win_minimize", w - 126, UiAction::WindowMinimize),
        ("win_maximize", w - 84, UiAction::WindowMaximize),
        ("win_close", w - 42, UiAction::WindowClose),
    ];

    for (id, x, action) in buttons {
        let rect = Rect {
            x,
            y: 0,
            w: 42,
            h: 34,
        };
        let hover_t = anim.hover_amount(id);
        let press_t = anim.press_amount(id);
        let is_close = matches!(action, UiAction::WindowClose);
        let base_bg = [COLOR_BG[0], COLOR_BG[1], COLOR_BG[2], 0];
        let hover_bg = if is_close {
            COLOR_CLOSE_HOVER
        } else {
            COLOR_CONTROL_HOVER
        };
        let press_bg = if is_close {
            COLOR_CLOSE_PRESS
        } else {
            COLOR_CONTROL_PRESS
        };
        let bg = lerp_color(lerp_color(base_bg, hover_bg, hover_t), press_bg, press_t);
        if bg[3] > 0 {
            canvas.fill_round_rect(rect, 0.0, with_alpha(bg, alpha));
        }

        let icon_color = with_alpha(
            if is_close {
                lerp_color(COLOR_MUTED, COLOR_TEXT, hover_t.max(press_t))
            } else {
                lerp_color(COLOR_MUTED, COLOR_TEXT, hover_t * 0.85)
            },
            alpha,
        );
        let cx = rect.x as f32 + rect.w as f32 * 0.5;
        let cy = rect.y as f32 + rect.h as f32 * 0.5;

        match action {
            UiAction::WindowMinimize => {
                canvas.draw_line(cx - 5.0, cy, cx + 5.0, cy, 1.3, icon_color);
            }
            UiAction::WindowMaximize => {
                if maximized {
                    // Front square
                    canvas.draw_line(cx - 4.5, cy - 2.5, cx + 2.5, cy - 2.5, 1.2, icon_color);
                    canvas.draw_line(cx + 2.5, cy - 2.5, cx + 2.5, cy + 4.5, 1.2, icon_color);
                    canvas.draw_line(cx + 2.5, cy + 4.5, cx - 4.5, cy + 4.5, 1.2, icon_color);
                    canvas.draw_line(cx - 4.5, cy + 4.5, cx - 4.5, cy - 2.5, 1.2, icon_color);
                    // Back offset frame
                    canvas.draw_line(cx - 2.5, cy - 4.5, cx + 4.5, cy - 4.5, 1.2, icon_color);
                    canvas.draw_line(cx + 4.5, cy - 4.5, cx + 4.5, cy + 2.5, 1.2, icon_color);
                    canvas.draw_line(cx - 2.5, cy - 4.5, cx - 2.5, cy - 2.5, 1.2, icon_color);
                    canvas.draw_line(cx + 2.5, cy + 2.5, cx + 4.5, cy + 2.5, 1.2, icon_color);
                } else {
                    canvas.draw_line(cx - 4.5, cy - 4.5, cx + 4.5, cy - 4.5, 1.2, icon_color);
                    canvas.draw_line(cx + 4.5, cy - 4.5, cx + 4.5, cy + 4.5, 1.2, icon_color);
                    canvas.draw_line(cx + 4.5, cy + 4.5, cx - 4.5, cy + 4.5, 1.2, icon_color);
                    canvas.draw_line(cx - 4.5, cy + 4.5, cx - 4.5, cy - 4.5, 1.2, icon_color);
                }
            }
            UiAction::WindowClose => {
                canvas.draw_line(cx - 4.5, cy - 4.5, cx + 4.5, cy + 4.5, 1.4, icon_color);
                canvas.draw_line(cx + 4.5, cy - 4.5, cx - 4.5, cy + 4.5, 1.4, icon_color);
            }
            _ => {}
        }

        regions.push(HitRegion {
            id: id.to_string(),
            rect,
            action,
            is_thickness_slider: false,
        });
    }
}

fn render_splash_screen(
    canvas: &mut dyn SettingsPainter,
    regions: &mut Vec<HitRegion>,
    w: i32,
    h: i32,
    maximized: bool,
    anim: &UiAnimState,
) {
    canvas.fill_round_rect(Rect { x: 0, y: 0, w, h }, 0.0, COLOR_BG);
    draw_window_controls(canvas, regions, anim, w, maximized, 1.0);

    let title_rect = Rect {
        x: (w - 300) / 2,
        y: h / 2 - 48,
        w: 300,
        h: 48,
    };
    canvas.draw_text(title_rect, "taskbar", COLOR_TEXT, FONT_BOLD, 40.0);

    let sub_rect = Rect {
        x: (w - 260) / 2,
        y: h / 2 + 2,
        w: 260,
        h: 20,
    };
    canvas.draw_text(sub_rect, "Ayarlar hazırlanıyor", COLOR_MUTED, FONT_UI, 12.0);

    let bar_w = 220;
    let bar_x = (w - bar_w) / 2;
    let bar_y = h / 2 + 34;
    let track = Rect {
        x: bar_x,
        y: bar_y,
        w: bar_w,
        h: 6,
    };
    canvas.fill_round_rect(track, 3.0, COLOR_CONTROL);
    canvas.stroke_round_rect(track, 3.0, 1.0, COLOR_BORDER);

    let fill_w = ((bar_w as f32 * anim.splash_progress()).round() as i32).clamp(6, bar_w);
    canvas.fill_round_rect(
        Rect {
            x: bar_x,
            y: bar_y,
            w: fill_w,
            h: 6,
        },
        3.0,
        COLOR_ACCENT,
    );
}

pub fn render_settings_ui_animated(
    canvas: &mut dyn SettingsPainter,
    size: Size,
    state: &SettingsAppState,
    anim: &UiAnimState,
) -> Vec<HitRegion> {
    let w = size.w.max(480);
    let h = size.h.max(420);

    if anim.phase() == AppPhase::Splash {
        let mut regions = Vec::new();
        render_splash_screen(canvas, &mut regions, w, h, state.window_maximized, anim);
        return regions;
    }

    let mut regions = Vec::new();
    let alpha = anim.main_alpha();
    canvas.fill_round_rect(Rect { x: 0, y: 0, w, h }, 0.0, COLOR_BG);

    // Header
    draw_left_text(
        canvas,
        20,
        14,
        200,
        28,
        "Taskbar",
        with_alpha(COLOR_TEXT, alpha),
        FONT_BOLD,
        18.0,
    );
    draw_left_text(
        canvas,
        20,
        44,
        300,
        18,
        "Görünüm, konum ve widget yapılandırması",
        with_alpha(COLOR_MUTED, alpha),
        FONT_UI,
        11.5,
    );

    let status_pill = Rect {
        x: w - 290,
        y: 10,
        w: 152,
        h: 26,
    };
    canvas.fill_round_rect(
        status_pill,
        13.0,
        with_alpha(
            if state.live_connected {
                COLOR_ACCENT_SOFT
            } else {
                COLOR_AMBER_SOFT
            },
            alpha,
        ),
    );
    canvas.stroke_round_rect(
        status_pill,
        13.0,
        1.0,
        with_alpha(
            if state.live_connected {
                COLOR_ACCENT_BORDER
            } else {
                COLOR_AMBER_BORDER
            },
            alpha,
        ),
    );
    canvas.draw_text(
        status_pill,
        if state.live_connected {
            "● Canlı Önizleme"
        } else {
            "○ Çevrimdışı"
        },
        with_alpha(
            if state.live_connected {
                COLOR_ACCENT_HOVER
            } else {
                COLOR_TEXT_AMBER
            },
            alpha,
        ),
        FONT_UI,
        12.0,
    );
    draw_window_controls(
        canvas,
        &mut regions,
        anim,
        w,
        state.window_maximized,
        alpha,
    );

    // Sidebar
    let body_h = (h - 140).max(260);
    let sidebar_w = if w < 580 { 148 } else { 172 };
    draw_card(
        canvas,
        Rect {
            x: 16,
            y: 72,
            w: sidebar_w,
            h: body_h,
        },
        12.0,
        alpha,
    );

    let tabs = [
        ("Görünüm & Konum", SettingsTab::Appearance),
        ("Widget'lar", SettingsTab::Widgets),
        ("Güncelleme", SettingsTab::About),
    ];

    // Animated active tab indicator pill
    let indicator_rect = Rect {
        x: 24,
        y: anim.active_tab_y().round() as i32,
        w: sidebar_w - 16,
        h: 38,
    };
    canvas.fill_round_rect(indicator_rect, 8.0, with_alpha(COLOR_ACCENT_SOFT, alpha));
    canvas.stroke_round_rect(indicator_rect, 8.0, 1.0, with_alpha(COLOR_ACCENT_BORDER, alpha));
    canvas.fill_round_rect(
        Rect {
            x: indicator_rect.x + 2,
            y: indicator_rect.y + 8,
            w: 3,
            h: 22,
        },
        1.5,
        with_alpha(COLOR_ACCENT, alpha),
    );

    for (idx, (label, tab)) in tabs.iter().enumerate() {
        let btn_id = format!("tab_{idx}");
        let rect = Rect {
            x: 24,
            y: 82 + idx as i32 * 46,
            w: sidebar_w - 16,
            h: 38,
        };
        let is_active = state.tab == *tab;
        if !is_active {
            let hover = anim.hover_amount(&btn_id);
            if hover > 0.01 {
                canvas.fill_round_rect(
                    rect,
                    8.0,
                    with_alpha(COLOR_CONTROL_HOVER, alpha * hover * 0.7),
                );
            }
        }
        canvas.draw_text(
            rect,
            label,
            with_alpha(if is_active { COLOR_TEXT } else { COLOR_MUTED }, alpha),
            if is_active { FONT_BOLD } else { FONT_UI },
            12.5,
        );
        regions.push(HitRegion {
            id: btn_id,
            rect,
            action: UiAction::SelectTab(*tab),
            is_thickness_slider: false,
        });
    }

    // Main content panel
    let cx = 16 + sidebar_w + 12;
    let cy = 72;
    let cw = (w - cx - 16).max(240);
    draw_card(
        canvas,
        Rect {
            x: cx,
            y: cy,
            w: cw,
            h: body_h,
        },
        12.0,
        alpha,
    );

    let ix = cx + 14;
    let iy = cy + 14;
    let iw = (cw - 28).max(200);
    let ih = (body_h - 28).max(220);
    let content_alpha = alpha * anim.tab_alpha();

    match state.tab {
        SettingsTab::Appearance => {
            render_appearance_tab(canvas, &mut regions, anim, ix, iy, iw, ih, state, content_alpha);
        }
        SettingsTab::Widgets => {
            render_widgets_tab(canvas, &mut regions, anim, ix, iy, iw, ih, state, content_alpha);
        }
        SettingsTab::About => {
            render_about_tab(canvas, &mut regions, anim, ix, iy, iw, state, content_alpha);
        }
    }

    // Bottom footer bar
    let fy = h - 58;
    draw_card(
        canvas,
        Rect {
            x: 16,
            y: fy,
            w: w - 32,
            h: 44,
        },
        10.0,
        alpha,
    );
    let footer_status = if !state.status_text.is_empty() {
        state.status_text.as_str()
    } else if state.has_unsaved_changes() {
        "● Kaydedilmemiş değişiklikler önizleniyor"
    } else {
        "Tüm ayarlar güncel"
    };
    let status_color = if state.has_unsaved_changes() && state.status_text.is_empty() {
        COLOR_TEXT_AMBER
    } else {
        COLOR_MUTED
    };
    draw_left_text(
        canvas,
        30,
        fy + 10,
        (w - 290).max(110),
        24,
        footer_status,
        with_alpha(status_color, alpha),
        FONT_UI,
        12.0,
    );
    draw_button(
        canvas,
        &mut regions,
        anim,
        "footer_revert",
        Rect {
            x: w - 256,
            y: fy + 6,
            w: 124,
            h: 32,
        },
        "İptal / Geri Al",
        false,
        false,
        alpha,
        UiAction::Revert,
    );
    draw_button(
        canvas,
        &mut regions,
        anim,
        "footer_save",
        Rect {
            x: w - 124,
            y: fy + 6,
            w: 100,
            h: 32,
        },
        "Kaydet",
        false,
        true,
        alpha,
        UiAction::Save,
    );

    regions
}

pub fn render_settings_ui(
    canvas: &mut dyn SettingsPainter,
    size: Size,
    state: &SettingsAppState,
) -> Vec<HitRegion> {
    let tab_idx = match state.tab {
        SettingsTab::Appearance => 0,
        SettingsTab::Widgets => 1,
        SettingsTab::About => 2,
    };
    let anim = UiAnimState::settled_main(tab_idx, state.working_config.bar.thickness);
    render_settings_ui_animated(canvas, size, state, &anim)
}

pub fn hit_test_region(regions: &[HitRegion], x: i32, y: i32) -> Option<&HitRegion> {
    regions.iter().rev().find(|hr| {
        x >= hr.rect.x
            && x < hr.rect.x + hr.rect.w
            && y >= hr.rect.y
            && y < hr.rect.y + hr.rect.h
    })
}

pub fn hit_test(regions: &[HitRegion], x: i32, y: i32) -> Option<UiAction> {
    hit_test_region(regions, x, y).map(|hr| hr.action.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{Canvas, Config};
    use std::path::PathBuf;

    #[derive(Default)]
    struct RecordingCanvas {
        rects: Vec<(Rect, f32, [u8; 4])>,
        texts: Vec<String>,
        lines: Vec<(f32, f32, f32, f32)>,
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

    impl SettingsPainter for RecordingCanvas {
        fn draw_line(
            &mut self,
            x1: f32,
            y1: f32,
            x2: f32,
            y2: f32,
            _width: f32,
            _color: [u8; 4],
        ) {
            self.lines.push((x1, y1, x2, y2));
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
        assert!(canvas.texts.iter().any(|t| t == "Taskbar"));
        assert!(!canvas.texts.iter().any(|t| t.contains("Taskbar Ayarları")));
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

    #[test]
    fn splash_phase_renders_wordmark_and_blocks_main_hit_regions() {
        let state = SettingsAppState::new(
            Config::default(),
            PathBuf::from(r"C:\Temp\config.toml"),
            true,
        );
        let anim = crate::anim::UiAnimState::new(0, 40);
        let mut canvas = RecordingCanvas::default();
        let regions = render_settings_ui_animated(
            &mut canvas,
            Size { w: 780, h: 600 },
            &state,
            &anim,
        );
        assert!(canvas.texts.iter().any(|t| t == "taskbar"));
        assert!(!regions.is_empty());
        assert!(regions.iter().all(|hr| matches!(
            hr.action,
            UiAction::WindowMinimize | UiAction::WindowMaximize | UiAction::WindowClose
        )));
    }

    #[test]
    fn splash_phase_exposes_window_controls_only() {
        let state = SettingsAppState::new(
            Config::default(),
            PathBuf::from(r"C:\Temp\config.toml"),
            true,
        );
        let anim = crate::anim::UiAnimState::new(0, 40);
        let mut canvas = RecordingCanvas::default();
        let regions = render_settings_ui_animated(
            &mut canvas,
            Size { w: 780, h: 600 },
            &state,
            &anim,
        );
        assert_eq!(regions.len(), 3);
        assert!(regions.iter().any(|hr| hr.action == UiAction::WindowMinimize));
        assert!(regions.iter().any(|hr| hr.action == UiAction::WindowMaximize));
        assert!(regions.iter().any(|hr| hr.action == UiAction::WindowClose));
        assert!(!canvas.lines.is_empty());
    }

    #[test]
    fn header_controls_do_not_overlap_at_min_width() {
        let mut state = SettingsAppState::new(
            Config::default(),
            PathBuf::from(r"C:\Temp\config.toml"),
            true,
        );
        let size = Size { w: 520, h: 460 };
        let anim = crate::anim::UiAnimState::settled_main(0, 40);

        let mut canvas = RecordingCanvas::default();
        let regions = render_settings_ui_animated(&mut canvas, size, &state, &anim);
        let win_regions: Vec<&HitRegion> = regions
            .iter()
            .filter(|hr| {
                matches!(
                    hr.action,
                    UiAction::WindowMinimize | UiAction::WindowMaximize | UiAction::WindowClose
                )
            })
            .collect();
        assert_eq!(win_regions.len(), 3);
        for hr in &win_regions {
            assert!(hr.rect.x >= 390, "expected right-aligned control: {:?}", hr.rect);
            assert!(hr.rect.x + hr.rect.w <= size.w);
            assert!(hr.rect.y == 0 && hr.rect.y + hr.rect.h <= 40);
        }
        let restored_lines = canvas.lines.len();
        assert!(restored_lines >= 7);

        state.window_maximized = true;
        let mut max_canvas = RecordingCanvas::default();
        let _ = render_settings_ui_animated(&mut max_canvas, size, &state, &anim);
        assert!(max_canvas.lines.len() > restored_lines);
    }

    #[test]
    fn appearance_tab_has_thickness_slider_region_and_high_contrast_bg() {
        let state = SettingsAppState::new(
            Config::default(),
            PathBuf::from(r"C:\Temp\config.toml"),
            true,
        );
        let anim = crate::anim::UiAnimState::settled_main(0, 40);
        let mut canvas = RecordingCanvas::default();
        let regions = render_settings_ui_animated(
            &mut canvas,
            Size { w: 780, h: 600 },
            &state,
            &anim,
        );
        assert_eq!(canvas.rects.first().map(|r| r.2), Some([7, 7, 8, 255]));
        assert!(regions.iter().any(|hr| hr.is_thickness_slider));
    }

    #[test]
    fn small_window_keeps_all_hit_regions_in_bounds() {
        let mut state = SettingsAppState::new(
            Config::default(),
            PathBuf::from(r"C:\Temp\config.toml"),
            true,
        );
        let size = Size { w: 520, h: 460 };
        for (idx, tab) in [
            SettingsTab::Appearance,
            SettingsTab::Widgets,
            SettingsTab::About,
        ]
        .into_iter()
        .enumerate()
        {
            state.tab = tab;
            let anim = crate::anim::UiAnimState::settled_main(idx, 40);
            let mut canvas = RecordingCanvas::default();
            let regions = render_settings_ui_animated(&mut canvas, size, &state, &anim);
            for hr in &regions {
                assert!(
                    hr.rect.x >= 0 && hr.rect.y >= 0,
                    "negative pos on {tab:?}: {:?}",
                    hr.rect
                );
                assert!(
                    hr.rect.x + hr.rect.w <= size.w && hr.rect.y + hr.rect.h <= size.h,
                    "out of bounds on {tab:?}: {:?}",
                    hr.rect
                );
            }
        }
    }
}

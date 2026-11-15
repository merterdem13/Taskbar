use crate::config::{Align, Edge};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    pub w: i32,
    pub h: i32,
}

pub fn bar_rect(edge: Edge, screen: Rect, thickness: i32) -> Rect {
    let screen_w = screen.w.max(0);
    let screen_h = screen.h.max(0);
    let t = thickness.max(0);
    match edge {
        Edge::Top => {
            let h = t.min(screen_h);
            Rect {
                x: screen.x,
                y: screen.y,
                w: screen_w,
                h,
            }
        }
        Edge::Bottom => {
            let h = t.min(screen_h);
            Rect {
                x: screen.x,
                y: screen.y + screen_h - h,
                w: screen_w,
                h,
            }
        }
        Edge::Left => {
            let w = t.min(screen_w);
            Rect {
                x: screen.x,
                y: screen.y,
                w,
                h: screen_h,
            }
        }
        Edge::Right => {
            let w = t.min(screen_w);
            Rect {
                x: screen.x + screen_w - w,
                y: screen.y,
                w,
                h: screen_h,
            }
        }
    }
}

pub fn layout_modules(
    edge: Edge,
    bar: Size,
    items: &[Size],
    align: Align,
    gap: i32,
) -> Vec<Rect> {
    if items.is_empty() {
        return Vec::new();
    }

    let is_vertical = edge.is_vertical();
    let bar_main = (if is_vertical { bar.h } else { bar.w }).max(0);
    let gap = gap.max(0);

    let mut accepted = vec![false; items.len()];
    let mut total_main = 0i32;
    let mut accepted_count = 0usize;

    for (i, item) in items.iter().enumerate() {
        let item_main = if is_vertical { item.h } else { item.w };
        let item_cross = if is_vertical { item.w } else { item.h };
        if item_main <= 0 || item_cross <= 0 {
            continue;
        }
        let sep = if accepted_count > 0 { gap } else { 0 };
        if total_main + sep + item_main <= bar_main {
            accepted[i] = true;
            total_main += sep + item_main;
            accepted_count += 1;
        }
    }

    let mut cursor = match align {
        Align::Start => 0,
        Align::Center => (bar_main - total_main) / 2,
        Align::End => bar_main - total_main,
    };

    let max_cross = if is_vertical {
        (bar.w - 4).max(1)
    } else {
        (bar.h - 8).max(1)
    };

    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        if !accepted[i] {
            out.push(Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            });
            continue;
        }

        if is_vertical {
            let w = item.w.min(max_cross).max(1);
            let h = item.h;
            let x = (bar.w.max(0) - w) / 2;
            let y = cursor;
            out.push(Rect { x, y, w, h });
            cursor += h + gap;
        } else {
            let w = item.w;
            let h = item.h.min(max_cross).max(1);
            let x = cursor;
            let y = (bar.h.max(0) - h) / 2;
            out.push(Rect { x, y, w, h });
            cursor += w + gap;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_rect_top_bottom_left_right() {
        let screen = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        assert_eq!(
            bar_rect(Edge::Top, screen, 40),
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 40
            }
        );
        assert_eq!(
            bar_rect(Edge::Bottom, screen, 40),
            Rect {
                x: 0,
                y: 1040,
                w: 1920,
                h: 40
            }
        );
        assert_eq!(
            bar_rect(Edge::Left, screen, 40),
            Rect {
                x: 0,
                y: 0,
                w: 40,
                h: 1080
            }
        );
        assert_eq!(
            bar_rect(Edge::Right, screen, 40),
            Rect {
                x: 1880,
                y: 0,
                w: 40,
                h: 1080
            }
        );
    }

    #[test]
    fn layout_center_horizontal() {
        let items = [Size { w: 80, h: 24 }, Size { w: 80, h: 24 }];
        let rects = layout_modules(
            Edge::Top,
            Size { w: 800, h: 40 },
            &items,
            Align::Center,
            8,
        );
        assert_eq!(
            rects,
            vec![
                Rect {
                    x: 316,
                    y: 8,
                    w: 80,
                    h: 24
                },
                Rect {
                    x: 404,
                    y: 8,
                    w: 80,
                    h: 24
                },
            ]
        );
    }

    #[test]
    fn layout_start_and_end() {
        let items = [Size { w: 80, h: 24 }, Size { w: 100, h: 24 }];
        let start = layout_modules(
            Edge::Bottom,
            Size { w: 800, h: 40 },
            &items,
            Align::Start,
            8,
        );
        assert_eq!(
            start,
            vec![
                Rect {
                    x: 0,
                    y: 8,
                    w: 80,
                    h: 24
                },
                Rect {
                    x: 88,
                    y: 8,
                    w: 100,
                    h: 24
                },
            ]
        );

        let end = layout_modules(
            Edge::Bottom,
            Size { w: 800, h: 40 },
            &items,
            Align::End,
            8,
        );
        assert_eq!(
            end,
            vec![
                Rect {
                    x: 612,
                    y: 8,
                    w: 80,
                    h: 24
                },
                Rect {
                    x: 700,
                    y: 8,
                    w: 100,
                    h: 24
                },
            ]
        );
    }

    #[test]
    fn layout_vertical_for_left_edge_clamps_cross_width() {
        let items = [Size { w: 56, h: 36 }, Size { w: 30, h: 40 }];
        let rects = layout_modules(
            Edge::Left,
            Size { w: 40, h: 1080 },
            &items,
            Align::Start,
            8,
        );
        assert_eq!(
            rects,
            vec![
                Rect {
                    x: 2,
                    y: 0,
                    w: 36,
                    h: 36
                },
                Rect {
                    x: 5,
                    y: 44,
                    w: 30,
                    h: 40
                },
            ]
        );
    }

    #[test]
    fn overflowing_main_axis_items_dropped_not_overlapping() {
        let items = [
            Size { w: 100, h: 24 },
            Size { w: 100, h: 24 },
            Size { w: 100, h: 24 },
        ];
        let rects = layout_modules(
            Edge::Top,
            Size { w: 220, h: 40 },
            &items,
            Align::Center,
            8,
        );
        assert_eq!(rects.len(), 3);
        assert_eq!(
            rects[0],
            Rect {
                x: 6,
                y: 8,
                w: 100,
                h: 24
            }
        );
        assert_eq!(
            rects[1],
            Rect {
                x: 114,
                y: 8,
                w: 100,
                h: 24
            }
        );
        assert_eq!(
            rects[2],
            Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0
            }
        );
        assert!(rects[0].x + rects[0].w <= rects[1].x);
    }

    #[test]
    fn empty_items_is_empty() {
        let rects = layout_modules(Edge::Top, Size { w: 800, h: 40 }, &[], Align::Center, 8);
        assert!(rects.is_empty());
    }
}

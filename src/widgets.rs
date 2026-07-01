//! Reusable drawing primitives: bounded history buffers, sparklines and bars.

use crate::theme::Palette;
use egui::epaint::{PathShape, PathStroke};
use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::collections::VecDeque;

/// A fixed-capacity ring of recent samples for a single metric.
#[derive(Clone)]
pub struct History {
    buf: VecDeque<f32>,
    cap: usize,
}

impl History {
    pub fn new(cap: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(cap),
            cap,
        }
    }

    pub fn push(&mut self, v: f32) {
        if self.buf.len() == self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(v);
    }

    pub fn values(&self) -> Vec<f32> {
        self.buf.iter().copied().collect()
    }
}

/// Draw a filled sparkline of `values` scaled to `max`, occupying `height`.
pub fn sparkline(ui: &mut Ui, values: &[f32], max: f32, height: f32, line: Color32, fill: Color32) {
    let width = ui.available_width();
    let (resp, painter) = ui.allocate_painter(vec2(width, height), Sense::hover());
    let rect = resp.rect;
    if values.len() < 2 {
        return;
    }
    let max = max.max(1e-3);
    let n = values.len();
    let dx = rect.width() / (n as f32 - 1.0);
    let point = |i: usize, v: f32| -> Pos2 {
        let x = rect.left() + dx * i as f32;
        let y = rect.bottom() - (v / max).clamp(0.0, 1.0) * rect.height();
        pos2(x, y)
    };

    let line_pts: Vec<Pos2> = values.iter().enumerate().map(|(i, &v)| point(i, v)).collect();

    // Filled area beneath the trace.
    let mut poly = line_pts.clone();
    poly.push(pos2(rect.right(), rect.bottom()));
    poly.push(pos2(rect.left(), rect.bottom()));
    painter.add(PathShape {
        points: poly,
        closed: true,
        fill,
        stroke: PathStroke::NONE,
    });

    // The trace itself.
    painter.add(PathShape::line(line_pts, Stroke::new(1.6, line)));
}

/// A rounded progress bar filling `frac` (0..1) of its width.
pub fn bar(ui: &mut Ui, frac: f32, color: Color32, track: Color32, height: f32) {
    let width = ui.available_width();
    let (resp, painter) = ui.allocate_painter(vec2(width, height), Sense::hover());
    let rect = resp.rect;
    let radius = CornerRadius::same((height * 0.5) as u8);
    painter.rect_filled(rect, radius, track);
    let w = rect.width() * frac.clamp(0.0, 1.0);
    if w > 0.5 {
        let fill_rect = Rect::from_min_size(rect.min, vec2(w.max(height), rect.height()));
        painter.rect_filled(fill_rect, radius, color);
    }
}

/// A thin segmented row of per-core usage bars (compact CPU cluster view).
pub fn core_bars(ui: &mut Ui, cores: &[f32], pal: &Palette) {
    let n = cores.len().max(1);
    let gap = 3.0;
    let width = ui.available_width();
    let height = 26.0;
    let (resp, painter) = ui.allocate_painter(vec2(width, height), Sense::hover());
    let rect = resp.rect;
    let bar_w = ((rect.width() - gap * (n as f32 - 1.0)) / n as f32).max(1.0);
    for (i, &c) in cores.iter().enumerate() {
        let frac = (c / 100.0).clamp(0.0, 1.0);
        let x = rect.left() + i as f32 * (bar_w + gap);
        let full = Rect::from_min_size(pos2(x, rect.top()), vec2(bar_w, rect.height()));
        painter.rect_filled(full, CornerRadius::same(2), pal.track);
        let h = rect.height() * frac;
        let filled = Rect::from_min_size(
            pos2(x, rect.bottom() - h),
            vec2(bar_w, h.max(1.0)),
        );
        painter.rect_filled(filled, CornerRadius::same(2), crate::theme::usage_color(frac));
    }
}

/// Convenience: a labelled value line "Label ............ value".
pub fn kv_row(ui: &mut Ui, pal: &Palette, label: &str, value: &str, value_color: Color32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(12.0).color(pal.subtle));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value).size(12.0).color(value_color));
        });
    });
}

/// Draw a small coloured status dot (used in section headers).
pub fn dot(ui: &mut Ui, color: Color32) {
    let (resp, painter) = ui.allocate_painter(vec2(8.0, 8.0), Sense::hover());
    let c = resp.rect.center();
    painter.circle_filled(c, 3.5, color);
}

/// Vertical spacing shortcut.
pub fn gap(ui: &mut Ui, amount: f32) {
    ui.add_space(amount);
}

/// A hairline separator matching the palette.
pub fn separator(ui: &mut Ui, pal: &Palette) {
    let width = ui.available_width();
    let (resp, painter) = ui.allocate_painter(vec2(width, 1.0), Sense::hover());
    let rect = resp.rect;
    painter.hline(rect.x_range(), rect.center().y, Stroke::new(1.0, pal.separator));
}

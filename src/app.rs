//! The eframe application: a hidden, borderless popover anchored under the
//! menu-bar icon. `logic()` runs every repaint (even while hidden) and drives
//! event handling / show-hide; `ui()` paints the panel when visible.

use crate::config::{Config, TrayMetric};
use crate::metrics::Sample;
use crate::theme::{self, Palette};
use crate::tray::{ID_LOGIN, ID_OPEN, ID_PREFS, ID_QUIT, Tray};
use crate::widgets::{self, History};
use crate::{autostart, util};

use crossbeam_channel::{Receiver, Sender, unbounded};
use egui::{Align, Color32, Layout, Margin, Rect, RichText, Stroke, ViewportCommand, pos2, vec2};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

const HIST: usize = 60;
pub const WINDOW_W: f32 = 342.0;
pub const WINDOW_H: f32 = 600.0;
const OUTER_MARGIN: i8 = 10;
// Popover tail (the little bump pointing up at the menu-bar icon).
const ARROW_H: f32 = 9.0;
const ARROW_HALF: f32 = 20.0;
const TOP_INSET: f32 = ARROW_H + 3.0;

pub struct Histories {
    pub cpu: History,
    pub gpu: History,
    pub mem: History,
    pub net_rx: History,
    pub net_tx: History,
}

impl Histories {
    fn new() -> Self {
        Self {
            cpu: History::new(HIST),
            gpu: History::new(HIST),
            mem: History::new(HIST),
            net_rx: History::new(HIST),
            net_tx: History::new(HIST),
        }
    }

    fn push(&mut self, s: &Sample) {
        self.cpu.push(s.cpu_overall);
        self.gpu.push(s.gpu.unwrap_or(0.0));
        self.mem.push(s.mem_frac() * 100.0);
        self.net_rx.push(s.net_rx_bps as f32);
        self.net_tx.push(s.net_tx_bps as f32);
    }
}

pub struct PulseApp {
    sample_rx: Receiver<Sample>,
    tray_events: Receiver<TrayIconEvent>,
    menu_events: Receiver<MenuEvent>,
    tray: Tray,

    latest: Sample,
    hist: Histories,
    have_sample: bool,

    cfg: Config,
    poll_ms: Arc<AtomicU64>,

    visible: bool,
    show_prefs: bool,
    tray_rect: Option<tray_icon::Rect>,
    show_guard: u32,
    last_hide: Instant,
    // Window-space x (points) where the popover tail points — i.e. the icon centre.
    arrow_x: f32,
}

impl PulseApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        sample_rx: Receiver<Sample>,
        poll_ms: Arc<AtomicU64>,
        mut cfg: Config,
    ) -> Self {
        let ctx = cc.egui_ctx.clone();
        theme::install_fonts(&ctx);
        theme::apply_style(&ctx);

        // Forward global tray/menu events into our own channels and wake egui
        // so `logic()` runs immediately (even while the window is hidden).
        let (tray_tx, tray_events): (Sender<TrayIconEvent>, Receiver<TrayIconEvent>) = unbounded();
        let (menu_tx, menu_events): (Sender<MenuEvent>, Receiver<MenuEvent>) = unbounded();
        {
            let ctx = ctx.clone();
            TrayIconEvent::set_event_handler(Some(move |e| {
                let _ = tray_tx.send(e);
                ctx.request_repaint();
            }));
        }
        {
            let ctx = ctx.clone();
            MenuEvent::set_event_handler(Some(move |e| {
                let _ = menu_tx.send(e);
                ctx.request_repaint();
            }));
        }

        // Reconcile the persisted preference with what's actually installed.
        let login_reality = autostart::is_enabled();
        cfg.launch_at_login = login_reality;

        let tray = match Tray::new(login_reality) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[pulse] failed to create tray icon: {e}");
                std::process::exit(1);
            }
        };

        Self {
            sample_rx,
            tray_events,
            menu_events,
            tray,
            latest: Sample::default(),
            hist: Histories::new(),
            have_sample: false,
            cfg,
            poll_ms,
            visible: false,
            show_prefs: false,
            tray_rect: None,
            show_guard: 0,
            last_hide: Instant::now() - std::time::Duration::from_secs(1),
            arrow_x: WINDOW_W / 2.0,
        }
    }

    // ---- panel show / hide -------------------------------------------------

    fn show_panel(&mut self, ctx: &egui::Context) {
        // Prefer the window's real backing scale; `pixels_per_point()` can be
        // stale while the popover is still parked off-screen.
        let ppp = ctx
            .input(|i| i.viewport().native_pixels_per_point)
            .unwrap_or_else(|| ctx.pixels_per_point())
            .max(1.0);
        if let Some(rect) = self.tray_rect {
            // The tray rect (physical px, top-left origin) spans the whole status
            // item — including any "23%" title drawn to the right of the glyph.
            // The icon itself sits at the left and is ~square at the menu-bar
            // height, so its centre is `left + height/2` (this stays correct when
            // no title is shown, where width ≈ height).
            let left = rect.position.x as f32 / ppp;
            let top = rect.position.y as f32 / ppp;
            let w = rect.size.width as f32 / ppp;
            let h = rect.size.height as f32 / ppp;
            let icon_cx = left + (h * 0.5).min(w * 0.5);
            let bottom = top + h;

            let mut x = icon_cx - WINDOW_W / 2.0;
            let screen_w = ctx
                .input(|i| i.viewport().monitor_size)
                .map(|s| s.x)
                .unwrap_or(icon_cx + WINDOW_W);
            let max_x = (screen_w - WINDOW_W - 8.0).max(8.0);
            x = x.clamp(8.0, max_x);
            let y = (bottom + 2.0).max(8.0);
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(x, y)));
            // Point the tail at the icon even when the panel is clamped near an edge.
            self.arrow_x = icon_cx - x;
        } else {
            self.arrow_x = WINDOW_W / 2.0;
        }
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(WINDOW_W, WINDOW_H)));
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        self.visible = true;
        self.show_guard = 3;
    }

    fn hide_panel(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        self.visible = false;
        self.show_prefs = false;
        self.last_hide = Instant::now();
    }

    fn toggle_from_tray(&mut self, ctx: &egui::Context) {
        if self.visible {
            self.hide_panel(ctx);
        } else if self.last_hide.elapsed().as_millis() > 250 {
            // Guard against the "click tray to dismiss" race, where focus-loss
            // already hid the panel a moment earlier.
            self.show_panel(ctx);
        }
    }

    // ---- event pumps -------------------------------------------------------

    fn pump_samples(&mut self) {
        let mut got = false;
        while let Ok(s) = self.sample_rx.try_recv() {
            self.hist.push(&s);
            self.latest = s;
            got = true;
        }
        if got {
            self.have_sample = true;
            self.update_tray_title();
        }
    }

    fn update_tray_title(&self) {
        let text = match self.cfg.tray_metric {
            TrayMetric::None => None,
            TrayMetric::Cpu => Some(format!("{:.0}%", self.latest.cpu_overall)),
            TrayMetric::Gpu => self.latest.gpu.map(|g| format!("{g:.0}%")),
            TrayMetric::Mem => Some(format!("{:.0}%", self.latest.mem_frac() * 100.0)),
        };
        self.tray.set_title(text.as_deref());
    }

    fn pump_menu(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.menu_events.try_recv() {
            match ev.id.0.as_str() {
                ID_OPEN => self.show_panel(ctx),
                ID_PREFS => {
                    self.show_prefs = true;
                    self.show_panel(ctx);
                }
                ID_LOGIN => self.toggle_login(),
                ID_QUIT => {
                    self.cfg.save();
                    std::process::exit(0);
                }
                _ => {}
            }
        }
    }

    fn pump_tray(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.tray_events.try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = ev
            {
                self.tray_rect = Some(rect);
                self.toggle_from_tray(ctx);
            } else if let TrayIconEvent::Click { rect, .. } = ev {
                // Remember the icon position from any interaction for placement.
                self.tray_rect = Some(rect);
            }
        }
    }

    fn toggle_login(&mut self) {
        let want = !autostart::is_enabled();
        match autostart::set_enabled(want) {
            Ok(()) => {
                self.cfg.launch_at_login = want;
                self.cfg.save();
            }
            Err(e) => eprintln!("[pulse] launch-at-login toggle failed: {e}"),
        }
        self.tray.set_login_checked(autostart::is_enabled());
    }

    // ---- panel sections ----------------------------------------------------

    fn header(&self, ui: &mut egui::Ui, pal: &Palette) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Pulse").size(17.0).strong().color(pal.text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let chip = if self.latest.chip_name.is_empty() {
                    "Mac".to_string()
                } else {
                    self.latest.chip_name.clone()
                };
                ui.label(RichText::new(chip).size(11.0).color(pal.subtle));
            });
        });
    }

    fn cpu_card(&self, ui: &mut egui::Ui, pal: &Palette) {
        let s = &self.latest;
        let frac = (s.cpu_overall / 100.0).clamp(0.0, 1.0);
        let color = theme::usage_color(frac);
        card(ui, pal, |ui| {
            metric_head(ui, pal, "CPU", color, &format!("{:.0}%", s.cpu_overall));
            widgets::gap(ui, 6.0);
            widgets::bar(ui, frac, color, pal.track, 6.0);
            widgets::gap(ui, 6.0);
            widgets::sparkline(
                ui,
                &self.hist.cpu.values(),
                100.0,
                34.0,
                color,
                theme::usage_fill(frac),
            );
            // Efficiency / performance clusters (Apple Silicon).
            if let (Some(e), Some(p)) = (s.ecpu, s.pcpu) {
                widgets::gap(ui, 6.0);
                cluster_row(ui, pal, "E-cores", e);
                widgets::gap(ui, 4.0);
                cluster_row(ui, pal, "P-cores", p);
            }
            if !s.per_core.is_empty() {
                widgets::gap(ui, 8.0);
                ui.label(RichText::new("Per-core").size(10.5).color(pal.faint));
                widgets::gap(ui, 3.0);
                widgets::core_bars(ui, &s.per_core, pal);
            }
            widgets::gap(ui, 4.0);
            let sub = if s.cpu_freq_mhz > 0 {
                format!("{} cores · {}", s.per_core.len(), util::freq(s.cpu_freq_mhz))
            } else {
                format!("{} cores", s.per_core.len())
            };
            ui.label(RichText::new(sub).size(10.5).color(pal.faint));
        });
    }

    fn mem_card(&self, ui: &mut egui::Ui, pal: &Palette) {
        let s = &self.latest;
        let frac = s.mem_frac();
        let color = theme::usage_color(frac);
        card(ui, pal, |ui| {
            metric_head(ui, pal, "Memory", color, &format!("{:.0}%", frac * 100.0));
            widgets::gap(ui, 6.0);
            widgets::bar(ui, frac, color, pal.track, 6.0);
            widgets::gap(ui, 6.0);
            widgets::sparkline(
                ui,
                &self.hist.mem.values(),
                100.0,
                26.0,
                color,
                theme::usage_fill(frac),
            );
            widgets::gap(ui, 6.0);
            widgets::kv_row(
                ui,
                pal,
                "Used",
                &format!("{} / {}", util::bytes(s.mem_used), util::bytes(s.mem_total)),
                pal.text,
            );
            if s.swap_total > 0 {
                widgets::gap(ui, 3.0);
                let sc = theme::usage_color(s.swap_frac());
                widgets::kv_row(
                    ui,
                    pal,
                    "Swap",
                    &format!("{} / {}", util::bytes(s.swap_used), util::bytes(s.swap_total)),
                    sc,
                );
            }
        });
    }

    fn gpu_card(&self, ui: &mut egui::Ui, pal: &Palette) {
        let s = &self.latest;
        card(ui, pal, |ui| {
            if let Some(g) = s.gpu {
                let frac = (g / 100.0).clamp(0.0, 1.0);
                let color = theme::usage_color(frac);
                metric_head(ui, pal, "GPU", color, &format!("{g:.0}%"));
                widgets::gap(ui, 6.0);
                widgets::bar(ui, frac, color, pal.track, 6.0);
                widgets::gap(ui, 6.0);
                widgets::sparkline(
                    ui,
                    &self.hist.gpu.values(),
                    100.0,
                    30.0,
                    color,
                    theme::usage_fill(frac),
                );
                if s.gpu_cores > 0 {
                    widgets::gap(ui, 4.0);
                    ui.label(
                        RichText::new(format!("{}-core GPU", s.gpu_cores))
                            .size(10.5)
                            .color(pal.faint),
                    );
                }
            } else {
                metric_head(ui, pal, "GPU", pal.faint, "—");
                widgets::gap(ui, 4.0);
                ui.label(
                    RichText::new("Utilisation unavailable on this Mac")
                        .size(11.0)
                        .color(pal.subtle),
                );
            }
        });
    }

    fn power_card(&self, ui: &mut egui::Ui, pal: &Palette) {
        let s = &self.latest;
        let any = s.cpu_power.is_some() || s.cpu_temp.is_some();
        if !any {
            return;
        }
        card(ui, pal, |ui| {
            metric_head_plain(ui, pal, "Power & Thermals");
            widgets::gap(ui, 6.0);
            if let (Some(c), Some(g), Some(a)) = (s.cpu_power, s.gpu_power, s.ane_power) {
                widgets::kv_row(ui, pal, "CPU", &format!("{c:.2} W"), pal.text);
                widgets::gap(ui, 3.0);
                widgets::kv_row(ui, pal, "GPU", &format!("{g:.2} W"), pal.text);
                widgets::gap(ui, 3.0);
                widgets::kv_row(ui, pal, "ANE", &format!("{a:.2} W"), pal.text);
            }
            if let Some(p) = s.sys_power {
                widgets::gap(ui, 3.0);
                widgets::kv_row(ui, pal, "Package", &format!("{p:.2} W"), pal.accent);
            }
            if let Some(t) = s.cpu_temp {
                widgets::gap(ui, 3.0);
                widgets::kv_row(
                    ui,
                    pal,
                    "CPU temp",
                    &format!("{t:.0} °C"),
                    theme::usage_color((t / 100.0).clamp(0.0, 1.0)),
                );
            }
            if let Some(t) = s.gpu_temp {
                widgets::gap(ui, 3.0);
                widgets::kv_row(
                    ui,
                    pal,
                    "GPU temp",
                    &format!("{t:.0} °C"),
                    theme::usage_color((t / 100.0).clamp(0.0, 1.0)),
                );
            }
        });
    }

    fn disk_card(&self, ui: &mut egui::Ui, pal: &Palette) {
        let s = &self.latest;
        if s.disks.is_empty() {
            return;
        }
        card(ui, pal, |ui| {
            metric_head_plain(ui, pal, "Disk");
            for (i, d) in s.disks.iter().enumerate() {
                if i > 0 {
                    widgets::gap(ui, 8.0);
                }
                let frac = d.frac();
                let color = theme::usage_color(frac);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&d.mount).size(11.5).color(pal.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} / {}",
                                util::bytes(d.used),
                                util::bytes(d.total)
                            ))
                            .size(11.0)
                            .color(pal.subtle),
                        );
                    });
                });
                widgets::gap(ui, 4.0);
                widgets::bar(ui, frac, color, pal.track, 5.0);
            }
        });
    }

    fn net_card(&self, ui: &mut egui::Ui, pal: &Palette) {
        let s = &self.latest;
        let rx = self.hist.net_rx.values();
        let tx = self.hist.net_tx.values();
        let peak = rx
            .iter()
            .chain(tx.iter())
            .cloned()
            .fold(0.0f32, f32::max)
            .max(64.0 * 1024.0);
        card(ui, pal, |ui| {
            metric_head_plain(ui, pal, "Network");
            widgets::gap(ui, 6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("↓").size(13.0).color(pal.accent));
                ui.label(
                    RichText::new(util::rate(s.net_rx_bps))
                        .size(12.0)
                        .color(pal.text),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(util::rate(s.net_tx_bps))
                            .size(12.0)
                            .color(pal.text),
                    );
                    ui.label(RichText::new("↑").size(13.0).color(Color32::from_rgb(255, 149, 10)));
                });
            });
            widgets::gap(ui, 5.0);
            widgets::sparkline(ui, &rx, peak, 22.0, pal.accent, alpha(pal.accent, 40));
            widgets::gap(ui, 3.0);
            widgets::sparkline(
                ui,
                &tx,
                peak,
                22.0,
                Color32::from_rgb(255, 149, 10),
                Color32::from_rgba_unmultiplied(255, 149, 10, 40),
            );
        });
    }

    fn proc_card(&self, ui: &mut egui::Ui, pal: &Palette) {
        let s = &self.latest;
        if s.top_procs.is_empty() {
            return;
        }
        card(ui, pal, |ui| {
            metric_head_plain(ui, pal, "Top Processes");
            widgets::gap(ui, 4.0);
            for p in &s.top_procs {
                widgets::gap(ui, 3.0);
                ui.horizontal(|ui| {
                    let mut name = p.name.clone();
                    if name.len() > 20 {
                        name.truncate(19);
                        name.push('…');
                    }
                    ui.label(RichText::new(name).size(11.5).color(pal.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(util::bytes(p.mem))
                                .size(10.5)
                                .color(pal.faint),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(format!("{:.0}%", p.cpu))
                                .size(11.0)
                                .color(theme::usage_color((p.cpu / 100.0).clamp(0.0, 1.0))),
                        );
                    });
                });
            }
        });
    }

    fn prefs_card(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        if !self.show_prefs {
            return;
        }
        card(ui, pal, |ui| {
            metric_head_plain(ui, pal, "Preferences");
            widgets::gap(ui, 8.0);

            // Update interval.
            ui.label(RichText::new("Update interval").size(11.0).color(pal.subtle));
            widgets::gap(ui, 2.0);
            let mut ms = self.cfg.poll_interval_ms as f64 / 1000.0;
            if ui
                .add(
                    egui::Slider::new(&mut ms, 0.5..=5.0)
                        .suffix(" s")
                        .step_by(0.5),
                )
                .changed()
            {
                self.cfg.poll_interval_ms = (ms * 1000.0) as u64;
                self.poll_ms
                    .store(self.cfg.interval_ms(), Ordering::Relaxed);
                self.cfg.save();
            }

            widgets::gap(ui, 8.0);
            ui.label(RichText::new("Menu-bar readout").size(11.0).color(pal.subtle));
            widgets::gap(ui, 3.0);
            ui.horizontal(|ui| {
                for m in [
                    TrayMetric::None,
                    TrayMetric::Cpu,
                    TrayMetric::Gpu,
                    TrayMetric::Mem,
                ] {
                    let selected = self.cfg.tray_metric == m;
                    if ui.selectable_label(selected, m.label()).clicked() {
                        self.cfg.tray_metric = m;
                        self.cfg.save();
                        self.update_tray_title();
                    }
                }
            });

            widgets::gap(ui, 8.0);
            let mut login = self.cfg.launch_at_login;
            if ui.checkbox(&mut login, "Launch at Login").changed() {
                self.toggle_login();
            }
        });
    }

    fn footer(&mut self, ui: &mut egui::Ui, pal: &Palette, ctx: &egui::Context) {
        widgets::separator(ui, pal);
        widgets::gap(ui, 8.0);
        ui.horizontal(|ui| {
            if ui
                .button(RichText::new("⚙ Preferences").size(11.5))
                .clicked()
            {
                self.show_prefs = !self.show_prefs;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(RichText::new("Quit").size(11.5)).clicked() {
                    self.cfg.save();
                    std::process::exit(0);
                }
                let _ = ctx; // reserved for future actions
            });
        });
    }
}

impl eframe::App for PulseApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::TRANSPARENT.to_normalized_gamma_f32()
    }

    // Don't restore stale window/scroll state between launches.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump_samples();
        self.pump_menu(ctx);
        self.pump_tray(ctx);

        // Auto-hide when the popover loses focus or Escape is pressed.
        if self.visible {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.hide_panel(ctx);
            } else if self.show_guard > 0 {
                self.show_guard -= 1;
            } else {
                let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
                if !focused {
                    self.hide_panel(ctx);
                }
            }
            // Keep values ticking smoothly while the panel is open.
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let dark = ui.visuals().dark_mode;
        let pal = Palette::for_mode(dark);

        // Custom popover background: rounded body + an upward tail pointing at
        // the menu-bar icon, drawn behind the content.
        let full = ui.max_rect();
        let body = Rect::from_min_max(
            pos2(full.left() + OUTER_MARGIN as f32, full.top() + TOP_INSET),
            pos2(full.right() - OUTER_MARGIN as f32, full.bottom() - OUTER_MARGIN as f32),
        );
        draw_panel_bg(ui.painter(), &pal, body, self.arrow_x);

        let frame = egui::Frame::default()
            .inner_margin(Margin::same(14))
            .outer_margin(Margin {
                left: OUTER_MARGIN,
                right: OUTER_MARGIN,
                top: TOP_INSET as i8,
                bottom: OUTER_MARGIN,
            });

        frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !self.have_sample {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(RichText::new("Pulse").size(17.0).strong().color(pal.text));
                    ui.add_space(6.0);
                    ui.label(RichText::new("Sampling…").size(12.0).color(pal.subtle));
                    ui.add_space(40.0);
                });
                return;
            }

            self.header(ui, &pal);
            widgets::gap(ui, 10.0);
            // Reserve room for the footer (separator + gaps + buttons) so it
            // always stays inside the panel box, then let the scroll area fill
            // whatever height remains.
            let footer_reserve = 46.0;
            let sa_max = (ui.available_height() - footer_reserve).max(120.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(sa_max)
                .show(ui, |ui| {
                    self.cpu_card(ui, &pal);
                    widgets::gap(ui, 8.0);
                    self.mem_card(ui, &pal);
                    widgets::gap(ui, 8.0);
                    self.gpu_card(ui, &pal);
                    widgets::gap(ui, 8.0);
                    self.power_card(ui, &pal);
                    widgets::gap(ui, 8.0);
                    self.disk_card(ui, &pal);
                    widgets::gap(ui, 8.0);
                    self.net_card(ui, &pal);
                    widgets::gap(ui, 8.0);
                    self.proc_card(ui, &pal);
                    if self.show_prefs {
                        widgets::gap(ui, 8.0);
                        self.prefs_card(ui, &pal);
                    }
                });
            widgets::gap(ui, 8.0);
            let ctx = ui.ctx().clone();
            self.footer(ui, &pal, &ctx);
        });
    }
}

// ---- free helpers ----------------------------------------------------------

fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Paint the popover: soft shadow, rounded body, and an upward tail (a smooth
/// raised-cosine bump) at `arrow_x` pointing up at the menu-bar icon.
fn draw_panel_bg(painter: &egui::Painter, pal: &Palette, body: Rect, arrow_x: f32) {
    use egui::epaint::{PathShape, PathStroke, Shadow};

    let radius = theme::PANEL_RADIUS;
    let shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(75),
    };
    painter.add(shadow.as_shape(body, radius));
    painter.rect_filled(body, radius, pal.bg);

    // Tail: keep it clear of the rounded corners.
    let min_x = body.left() + 18.0 + ARROW_HALF;
    let max_x = body.right() - 18.0 - ARROW_HALF;
    let ax = if min_x <= max_x {
        arrow_x.clamp(min_x, max_x)
    } else {
        body.center().x
    };
    let n = 28;
    let mut pts = Vec::with_capacity(n + 3);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let u = 2.0 * t - 1.0;
        let x = ax - ARROW_HALF + t * 2.0 * ARROW_HALF;
        let h = ARROW_H * 0.5 * (1.0 + (std::f32::consts::PI * u).cos());
        pts.push(pos2(x, body.top() - h));
    }
    // Close a little below the top edge so the join with the body is seamless.
    pts.push(pos2(ax + ARROW_HALF, body.top() + 2.0));
    pts.push(pos2(ax - ARROW_HALF, body.top() + 2.0));
    painter.add(PathShape {
        points: pts,
        closed: true,
        fill: pal.bg,
        stroke: PathStroke::NONE,
    });
}

fn card<R>(ui: &mut egui::Ui, pal: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) {
    egui::Frame::default()
        .fill(pal.card)
        .corner_radius(theme::CARD_RADIUS)
        .stroke(Stroke::new(1.0, pal.card_stroke))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// Section header with a coloured status dot and a right-aligned big value.
fn metric_head(ui: &mut egui::Ui, pal: &Palette, title: &str, color: Color32, value: &str) {
    ui.horizontal(|ui| {
        widgets::dot(ui, color);
        ui.add_space(2.0);
        ui.label(RichText::new(title).size(12.5).strong().color(pal.text));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).size(15.0).strong().color(color));
        });
    });
}

fn metric_head_plain(ui: &mut egui::Ui, pal: &Palette, title: &str) {
    ui.label(RichText::new(title).size(12.5).strong().color(pal.text));
}

fn cluster_row(ui: &mut egui::Ui, pal: &Palette, label: &str, pct: f32) {
    let frac = (pct / 100.0).clamp(0.0, 1.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.5).color(pal.subtle));
        ui.add_space(4.0);
        ui.scope(|ui| {
            ui.set_width(ui.available_width() - 40.0);
            widgets::bar(ui, frac, theme::usage_color(frac), pal.track, 5.0);
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(format!("{pct:.0}%")).size(10.5).color(pal.subtle));
        });
    });
}

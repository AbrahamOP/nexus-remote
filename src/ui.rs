use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
// GOA Design dark tokens, shared by the native widgets and the GC-CRD-01 cards.
pub const BG: Color32 = Color32::from_rgb(10, 20, 17);
pub const SIDEBAR: Color32 = BG;
pub const TOP: Color32 = BG;
pub const CARD: Color32 = Color32::from_rgb(16, 30, 24);
pub const RAISED: Color32 = Color32::from_rgb(22, 38, 30);
pub const SUNKEN: Color32 = Color32::from_rgb(8, 16, 13);
pub const LINE: Color32 = Color32::from_rgb(39, 61, 49);
pub const TEXT: Color32 = Color32::from_rgb(230, 242, 236);
pub const MUTED: Color32 = Color32::from_rgb(154, 184, 171);
pub const SUBTLE: Color32 = Color32::from_rgb(126, 157, 142);
pub const GREEN: Color32 = Color32::from_rgb(58, 208, 160);
pub const ON_PRIMARY: Color32 = Color32::from_rgb(7, 28, 20);
pub const BLUE: Color32 = Color32::from_rgb(125, 186, 248);
pub const PURPLE: Color32 = Color32::from_rgb(176, 166, 255);
pub const GOLD: Color32 = Color32::from_rgb(239, 193, 111);
pub const DANGER: Color32 = Color32::from_rgb(250, 153, 153);

/// Composites a GOA translucent accent over its actual underlying surface.
pub fn tint(color: Color32, background: Color32, opacity: f32) -> Color32 {
    let opacity = opacity.clamp(0.0, 1.0);
    let blend = |foreground: u8, backdrop: u8| {
        (foreground as f32 * opacity + backdrop as f32 * (1.0 - opacity)).round() as u8
    };
    Color32::from_rgb(
        blend(color.r(), background.r()),
        blend(color.g(), background.g()),
        blend(color.b(), background.b()),
    )
}
#[derive(Clone, Copy)]
pub enum Icon {
    Hosts,
    Folder,
    Terminal,
    Files,
    Key,
    Code,
    Tunnel,
    Grid,
    Settings,
    Search,
    Plus,
    Star,
    Clock,
    Monitor,
    Chevron,
    Check,
    Close,
    More,
}
pub fn apply_theme(ctx: &egui::Context) {
    let mut s = (*ctx.style()).clone();
    s.visuals = egui::Visuals::dark();
    s.visuals.panel_fill = BG;
    s.visuals.window_fill = CARD;
    s.visuals.extreme_bg_color = SUNKEN;
    s.visuals.faint_bg_color = RAISED;
    s.visuals.text_edit_bg_color = Some(CARD);
    s.visuals.override_text_color = Some(TEXT);
    s.visuals.hyperlink_color = GREEN;
    s.visuals.warn_fg_color = GOLD;
    s.visuals.error_fg_color = DANGER;
    s.visuals.code_bg_color = SUNKEN;
    s.visuals.window_stroke = Stroke::new(1.0_f32, LINE);
    s.visuals.window_corner_radius = CornerRadius::same(16);
    s.visuals.menu_corner_radius = CornerRadius::same(10);
    s.visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 32,
        spread: 0,
        color: Color32::from_black_alpha(51),
    };
    s.visuals.popup_shadow = s.visuals.window_shadow;
    s.visuals.selection.bg_fill = tint(GREEN, CARD, 0.2);
    s.visuals.selection.stroke = Stroke::new(1.0_f32, GREEN);
    for w in [
        &mut s.visuals.widgets.inactive,
        &mut s.visuals.widgets.hovered,
        &mut s.visuals.widgets.active,
        &mut s.visuals.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(10);
        w.bg_stroke = Stroke::new(1.0_f32, LINE);
        w.fg_stroke.color = TEXT;
    }
    s.visuals.widgets.inactive.bg_fill = CARD;
    s.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    s.visuals.widgets.hovered.bg_fill = RAISED;
    s.visuals.widgets.hovered.weak_bg_fill = RAISED;
    s.visuals.widgets.active.bg_fill = RAISED;
    s.visuals.widgets.active.weak_bg_fill = RAISED;
    s.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, GREEN);
    s.visuals.widgets.open.bg_fill = RAISED;
    s.visuals.widgets.open.weak_bg_fill = RAISED;
    s.visuals.widgets.noninteractive.bg_fill = CARD;
    s.visuals.widgets.noninteractive.weak_bg_fill = CARD;
    s.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    s.visuals.widgets.noninteractive.fg_stroke.color = TEXT;
    s.spacing.item_spacing = Vec2::new(8.0, 8.0);
    s.spacing.button_padding = Vec2::new(16.0, 9.0);
    s.spacing.interact_size.y = 32.0;
    s.spacing.scroll.bar_width = 7.0;
    s.text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(13.0));
    s.text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(13.0));
    s.text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(11.0));
    ctx.set_style(s);
    let mut fonts = egui::FontDefinitions::default();
    for (name, data, family) in [
        (
            "Inter",
            include_bytes!("../assets/fonts/Inter.ttf").as_slice(),
            egui::FontFamily::Proportional,
        ),
        (
            "JetBrains Mono",
            include_bytes!("../assets/fonts/JetBrainsMono.ttf").as_slice(),
            egui::FontFamily::Monospace,
        ),
    ] {
        fonts.font_data.insert(
            name.into(),
            std::sync::Arc::new(egui::FontData::from_static(data)),
        );
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, name.into());
    }
    if let Ok(data) = std::fs::read("C:\\Windows\\Fonts\\seguisym.ttf") {
        fonts.font_data.insert(
            "Windows Symbols".into(),
            std::sync::Arc::new(egui::FontData::from_owned(data)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(1, "Windows Symbols".into());
    }
    ctx.set_fonts(fonts);
}
pub fn label(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).color(MUTED).size(12.0));
}
pub fn title(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(26.0).strong());
}
pub fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).size(11.0).color(MUTED).strong());
    ui.add_space(4.0);
}
pub fn primary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = Vec2::new(16.0, 9.0);
            let fill = if ui.is_enabled() {
                GREEN
            } else {
                tint(GREEN, CARD, 0.42)
            };
            ui.add(
                egui::Button::new(RichText::new(text).size(13.0).color(ON_PRIMARY).strong())
                    .fill(fill)
                    .stroke(Stroke::NONE)
                    .corner_radius(10)
                    .min_size(Vec2::new(0.0, 40.0)),
            )
        })
        .inner;
    focus_ring(ui, &response);
    response
}
pub fn outline(ui: &mut egui::Ui, text: &str) -> egui::Response {
    outline_button(ui, text, false)
}
pub fn outline_small(ui: &mut egui::Ui, text: &str) -> egui::Response {
    compact_button(ui, text)
}
/// A 32-point control whose content and border keep the same bounds in every row.
pub fn compact_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let color = if ui.is_enabled() { TEXT } else { SUBTLE };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(12.0), color);
    let width = (galley.size().x + 24.0).ceil().max(64.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), text));
    if ui.is_rect_visible(rect) {
        let fill = if response.is_pointer_button_down_on() {
            tint(GREEN, CARD, 0.12)
        } else if response.hovered() {
            RAISED
        } else {
            Color32::TRANSPARENT
        };
        let border = if response.has_focus() {
            GREEN
        } else if response.hovered() {
            tint(GREEN, LINE, 0.35)
        } else {
            LINE
        };
        ui.painter().rect_filled(rect, 8.0, fill);
        ui.painter().rect_stroke(
            rect,
            8.0,
            Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, color);
    }
    focus_ring(ui, &response);
    response
}
fn outline_button(ui: &mut egui::Ui, text: &str, small: bool) -> egui::Response {
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = if small {
                Vec2::new(11.0, 6.0)
            } else {
                Vec2::new(16.0, 9.0)
            };
            ui.add(
                egui::Button::new(
                    RichText::new(text)
                        .size(if small { 12.0 } else { 13.0 })
                        .color(TEXT),
                )
                .stroke(Stroke::new(1.0_f32, LINE))
                .corner_radius(10)
                .min_size(Vec2::new(0.0, if small { 32.0 } else { 40.0 })),
            )
        })
        .inner;
    focus_ring(ui, &response);
    response
}
fn focus_ring(ui: &egui::Ui, response: &egui::Response) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(4.0),
            12.0,
            Stroke::new(2.0_f32, GREEN),
            egui::StrokeKind::Outside,
        );
    }
}
pub fn field(ui: &mut egui::Ui, text: &str, value: &mut String) {
    label(ui, text);
    ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;
        for w in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
        ] {
            w.corner_radius = CornerRadius::same(6);
        }
        ui.add_sized(
            [ui.available_width(), 42.0],
            egui::TextEdit::singleline(value)
                .font(FontId::proportional(13.0))
                .margin(Vec2::new(12.0, 10.0)),
        );
    });
    ui.add_space(4.0);
}
pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    let mut job = egui::text::LayoutJob::simple_singleline(
        text.to_owned(),
        FontId::proportional(11.0),
        color,
    );
    if !matches!(text, "SSH" | "RDP" | "SFTP") {
        job.wrap =
            egui::text::TextWrapping::truncate_at_width((ui.available_width() - 16.0).max(1.0));
    }
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new((galley.size().x + 16.0).ceil(), 22.0),
        Sense::hover(),
    );
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, 6.0, tint(color, CARD, 0.1));
        ui.painter().rect_stroke(
            rect,
            6.0,
            Stroke::new(1.0_f32, tint(color, CARD, 0.2)),
            egui::StrokeKind::Inside,
        );
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, color);
    }
    response.on_hover_text(text);
}
pub fn proto(p: crate::model::Protocol) -> Color32 {
    match p {
        crate::model::Protocol::Ssh => BLUE,
        crate::model::Protocol::Sftp => PURPLE,
        crate::model::Protocol::Rdp => GOLD,
    }
}
pub fn proto_icon(p: crate::model::Protocol) -> Icon {
    match p {
        crate::model::Protocol::Ssh => Icon::Terminal,
        crate::model::Protocol::Sftp => Icon::Files,
        crate::model::Protocol::Rdp => Icon::Monitor,
    }
}
pub fn icon(ui: &mut egui::Ui, which: Icon, color: Color32, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_icon(ui.painter(), rect, which, color);
    response
}
pub fn icon_button(ui: &mut egui::Ui, which: Icon, tip: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), tip));
    if response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 8.0, tint(GREEN, CARD, 0.12));
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 8.0, RAISED);
    }
    paint_icon(
        ui.painter(),
        rect.shrink(8.0),
        which,
        if !ui.is_enabled() {
            SUBTLE
        } else if response.hovered() {
            TEXT
        } else {
            MUTED
        },
    );
    focus_ring(ui, &response);
    response.on_hover_text(tip)
}
pub fn nav(
    ui: &mut egui::Ui,
    which: Icon,
    text: &str,
    count: Option<usize>,
    selected: bool,
) -> bool {
    let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.0), Sense::click());
    if selected || r.hovered() {
        ui.painter().rect_filled(
            rect,
            6.0,
            if selected { tint(GREEN, BG, 0.1) } else { CARD },
        );
    }
    if selected {
        ui.painter().line_segment(
            [
                rect.left_top() + Vec2::new(1.0, 5.0),
                rect.left_bottom() + Vec2::new(1.0, -5.0),
            ],
            Stroke::new(2.0_f32, GREEN),
        );
    }
    paint_icon(
        ui.painter(),
        Rect::from_min_size(rect.min + Vec2::new(10.0, 10.0), Vec2::splat(16.0)),
        which,
        if selected { GREEN } else { MUTED },
    );
    ui.painter().text(
        Pos2::new(rect.left() + 37.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(12.0),
        if selected {
            GREEN
        } else if r.hovered() {
            TEXT
        } else {
            MUTED
        },
    );
    if let Some(count) = count {
        ui.painter().text(
            Pos2::new(rect.right() - 11.0, rect.center().y),
            Align2::RIGHT_CENTER,
            count.to_string(),
            FontId::proportional(11.0),
            MUTED,
        );
    }
    focus_ring(ui, &r);
    r.clicked()
}
pub fn empty(ui: &mut egui::Ui, which: Icon, heading: &str, description: &str) {
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        icon(ui, which, GREEN, 48.0);
        ui.add_space(15.0);
        ui.label(RichText::new(heading).size(21.0));
        ui.add_space(6.0);
        ui.label(RichText::new(description).size(13.0).color(MUTED));
        ui.add_space(16.0);
    });
}
pub fn paint_icon(p: &egui::Painter, r: Rect, which: Icon, color: Color32) {
    let point = |x: f32, y: f32| Pos2::new(r.left() + r.width() * x, r.top() + r.height() * y);
    let stroke = Stroke::new(1.7_f32, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([point(a.0, a.1), point(b.0, b.1)], stroke);
    };
    let boxx = |a: (f32, f32), b: (f32, f32)| {
        p.rect_stroke(
            Rect::from_min_max(point(a.0, a.1), point(b.0, b.1)),
            2.0,
            stroke,
            egui::StrokeKind::Inside,
        );
    };
    match which {
        Icon::Hosts => {
            boxx((0.1, 0.08), (0.9, 0.42));
            boxx((0.1, 0.57), (0.9, 0.91));
            for y in [0.25, 0.74] {
                p.circle_filled(point(0.23, y), r.width() * 0.035, color);
                line((0.43, y), (0.75, y));
            }
        }
        Icon::Folder => {
            let pts = vec![
                point(0.05, 0.22),
                point(0.42, 0.22),
                point(0.51, 0.36),
                point(0.93, 0.36),
                point(0.93, 0.85),
                point(0.05, 0.85),
            ];
            p.add(egui::Shape::convex_polygon(
                pts,
                color.gamma_multiply(0.2),
                stroke,
            ));
        }
        Icon::Terminal => {
            boxx((0.03, 0.11), (0.97, 0.91));
            line((0.22, 0.35), (0.42, 0.52));
            line((0.42, 0.52), (0.22, 0.69));
            line((0.55, 0.71), (0.78, 0.71));
        }
        Icon::Files => {
            boxx((0.12, 0.08), (0.72, 0.82));
            boxx((0.3, 0.24), (0.9, 0.95));
            line((0.42, 0.45), (0.75, 0.45));
            line((0.42, 0.62), (0.75, 0.62));
        }
        Icon::Key => {
            p.circle_stroke(point(0.32, 0.32), r.width() * 0.23, stroke);
            line((0.48, 0.5), (0.85, 0.88));
            line((0.68, 0.7), (0.82, 0.57));
            line((0.8, 0.82), (0.94, 0.69));
        }
        Icon::Code => {
            line((0.29, 0.2), (0.05, 0.5));
            line((0.05, 0.5), (0.29, 0.8));
            line((0.71, 0.2), (0.95, 0.5));
            line((0.95, 0.5), (0.71, 0.8));
            line((0.61, 0.08), (0.4, 0.92));
        }
        Icon::Tunnel => {
            boxx((0.02, 0.28), (0.24, 0.72));
            boxx((0.76, 0.28), (0.98, 0.72));
            line((0.25, 0.4), (0.75, 0.4));
            line((0.75, 0.6), (0.25, 0.6));
            line((0.64, 0.28), (0.76, 0.4));
            line((0.76, 0.4), (0.64, 0.52));
        }
        Icon::Grid => {
            for x in [0.08, 0.56] {
                for y in [0.08, 0.56] {
                    boxx((x, y), (x + 0.34, y + 0.34));
                }
            }
        }
        Icon::Settings => {
            p.circle_stroke(r.center(), r.width() * 0.21, stroke);
            p.circle_stroke(r.center(), r.width() * 0.4, stroke);
            for a in 0..8 {
                let a = a as f32 * std::f32::consts::TAU / 8.0;
                let d = Vec2::angled(a);
                p.line_segment(
                    [
                        r.center() + d * r.width() * 0.35,
                        r.center() + d * r.width() * 0.5,
                    ],
                    stroke,
                );
            }
        }
        Icon::Search => {
            p.circle_stroke(point(0.41, 0.4), r.width() * 0.29, stroke);
            line((0.62, 0.62), (0.91, 0.92));
        }
        Icon::Plus => {
            line((0.15, 0.5), (0.85, 0.5));
            line((0.5, 0.15), (0.5, 0.85));
        }
        Icon::Star => {
            let pts = (0..10)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::PI / 5.0 - std::f32::consts::FRAC_PI_2;
                    let d = if i % 2 == 0 { 0.46 } else { 0.2 };
                    r.center() + Vec2::angled(a) * r.width() * d
                })
                .collect();
            p.add(egui::Shape::closed_line(pts, stroke));
        }
        Icon::Clock => {
            p.circle_stroke(r.center(), r.width() * 0.43, stroke);
            line((0.5, 0.2), (0.5, 0.51));
            line((0.5, 0.51), (0.72, 0.65));
        }
        Icon::Monitor => {
            boxx((0.03, 0.12), (0.97, 0.72));
            line((0.5, 0.73), (0.5, 0.92));
            line((0.27, 0.94), (0.73, 0.94));
        }
        Icon::Chevron => {
            line((0.35, 0.2), (0.65, 0.5));
            line((0.65, 0.5), (0.35, 0.8));
        }
        Icon::Check => {
            line((0.1, 0.5), (0.38, 0.8));
            line((0.38, 0.8), (0.9, 0.16));
        }
        Icon::Close => {
            line((0.2, 0.2), (0.8, 0.8));
            line((0.8, 0.2), (0.2, 0.8));
        }
        Icon::More => {
            for x in [0.2, 0.5, 0.8] {
                p.circle_filled(point(x, 0.5), r.width() * 0.06, color);
            }
        }
    }
}

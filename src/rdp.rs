use crate::model::Profile;
use egui::RichText;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

#[path = "rdp_native.rs"]
mod native;
use native::{NativeRdp, OleGuard, Variant};

#[derive(Clone, Copy)]
enum RdpAction {
    Connect,
    Disconnect,
    Fit,
    Fullscreen,
    External,
}

/// A real Windows RDP child window inside the native egui application.
/// The caller must hide inactive tabs and hide this child while displaying an egui modal.
pub struct RdpSession {
    profile: Profile,
    secret: Zeroizing<String>,
    host: Option<NativeRdp>,
    parent: isize,
    visible: bool,
    auto_connect: bool,
    status: String,
    detail: String,
    last_state: i32,
    connecting_since: Option<Instant>,
    last_poll: Instant,
    desktop_size: (i32, i32),
    pending_action: Option<RdpAction>,
    show_help: bool,
    // Drop the native control before balancing this thread's OleInitialize call.
    _ole: OleGuard,
}

impl RdpSession {
    pub fn new(profile: Profile, secret: String) -> Result<Self, String> {
        profile.validate()?;
        Ok(Self {
            profile,
            secret: Zeroizing::new(secret),
            host: None,
            parent: 0,
            visible: true,
            auto_connect: true,
            status: "Préparation du bureau distant…".into(),
            detail: String::new(),
            last_state: 0,
            connecting_since: None,
            last_poll: Instant::now() - Duration::from_secs(1),
            desktop_size: (1280, 800),
            pending_action: None,
            show_help: false,
            _ole: OleGuard::new()?,
        })
    }

    pub fn set_parent_hwnd(&mut self, hwnd: isize) {
        if self.parent != hwnd {
            self.host.take();
            self.parent = hwnd;
            self.auto_connect = true;
            self.last_state = 0;
            self.connecting_since = None;
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        if let Some(host) = &self.host {
            host.visible(visible);
        }
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn connected(&self) -> bool {
        self.last_state == 1
    }

    /// Compact controls for the common session header. Rendering the controls
    /// never creates a native control or starts a connection by itself.
    pub fn controls(&mut self, ui: &mut egui::Ui) {
        self.poll();
        let available = ui.available_width();
        let connected = self.connected();
        let options =
            crate::ui::icon_button(ui, crate::ui::Icon::Settings, "Options du bureau distant");
        egui::Popup::menu(&options).show(|ui| {
            ui.set_min_width(210.0);
            if ui
                .add_enabled(
                    self.last_state == 0 && self.connecting_since.is_none(),
                    egui::Button::new("Reconnecter"),
                )
                .clicked()
            {
                self.pending_action = Some(RdpAction::Connect);
                ui.close();
            }
            if ui
                .add_enabled(
                    self.last_state != 0 || self.connecting_since.is_some(),
                    egui::Button::new("Déconnecter"),
                )
                .clicked()
            {
                self.pending_action = Some(RdpAction::Disconnect);
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(connected, egui::Button::new("Ajuster à la fenêtre"))
                .clicked()
            {
                self.pending_action = Some(RdpAction::Fit);
                ui.close();
            }
            if ui
                .add_enabled(connected, egui::Button::new("Plein écran"))
                .clicked()
            {
                self.pending_action = Some(RdpAction::Fullscreen);
                ui.close();
            }
            if ui.button("Ouvrir dans le client Windows ↗").clicked() {
                self.pending_action = Some(RdpAction::External);
                ui.close();
            }
            ui.separator();
            if ui
                .selectable_label(self.show_help, "Raccourcis clavier")
                .clicked()
            {
                self.show_help = !self.show_help;
                ui.close();
            }
        });
        if available >= 220.0 {
            if ui
                .add_enabled_ui(connected, |ui| crate::ui::compact_button(ui, "Plein écran"))
                .inner
                .clicked()
            {
                self.pending_action = Some(RdpAction::Fullscreen);
            }
        }
        if available >= 110.0 {
            if ui
                .add_enabled_ui(connected, |ui| crate::ui::compact_button(ui, "Ajuster"))
                .inner
                .on_hover_text("Adapter la résolution distante à cet onglet")
                .clicked()
            {
                self.pending_action = Some(RdpAction::Fit);
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.poll();
        if self.show_help {
            egui::Frame::new()
                .fill(crate::ui::CARD)
                .corner_radius(8)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(
                            "Ctrl+Alt+Fin : sécurité Windows · Ctrl+Alt+Pause : plein écran",
                        )
                        .size(12.0)
                        .color(crate::ui::MUTED),
                    );
                });
            ui.add_space(8.0);
        }
        if !self.detail.is_empty() {
            egui::Frame::new()
                .fill(crate::ui::CARD)
                .corner_radius(8)
                .inner_margin(8)
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&self.detail)
                                .size(12.0)
                                .color(crate::ui::MUTED),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&self.detail);
                });
            ui.add_space(8.0);
        }

        let size = ui.available_size().max(egui::vec2(1.0, 1.0));
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter().rect_filled(rect, 8.0, crate::ui::SUNKEN);
        ui.painter().rect_stroke(
            rect,
            8.0,
            egui::Stroke::new(1.0_f32, crate::ui::LINE),
            egui::StrokeKind::Inside,
        );
        // Native child windows do not inherit egui's clip rectangle. Keep them
        // inside this pane, including when the session is split or resized.
        let viewport = rect.shrink(1.0).intersect(ui.clip_rect());
        let scale = ui.ctx().pixels_per_point();
        let (x, y, width, height) = pixel_viewport(viewport, scale);
        self.desktop_size = (width.clamp(800, 8192), height.clamp(600, 8192));
        let mut action = self.pending_action.take();
        let needs_native_host =
            (self.auto_connect && self.visible) || matches!(action, Some(RdpAction::Connect));
        if self.host.is_none() && self.parent != 0 && needs_native_host {
            self.auto_connect = false;
            match NativeRdp::new(self.parent) {
                Ok(host) => {
                    self.host = Some(host);
                    if action.is_none() {
                        action = Some(RdpAction::Connect);
                    }
                }
                Err(error) => {
                    self.status = "Le bureau distant intégré est indisponible".into();
                    self.detail =
                        format!("{error} Le client Windows reste disponible dans les options RDP.");
                }
            }
        }
        if let Some(host) = &self.host {
            host.position(x, y, width, height);
            // An HWND paints above egui. Hide it temporarily while an egui menu
            // is open, so options remain visible and clickable over the desktop.
            host.visible(
                self.visible
                    && (self.connected()
                        || self.connecting_since.is_some()
                        || matches!(action, Some(RdpAction::Connect)))
                    && !egui::Popup::is_any_open(ui.ctx())
                    && viewport.width() > 1.0
                    && viewport.height() > 1.0,
            );
        }
        if !self.connected() && self.connecting_since.is_none() {
            let title = if self.parent == 0 && self.auto_connect {
                "Initialisation du bureau distant…"
            } else {
                &self.status
            };
            let mut job = egui::text::LayoutJob::simple_singleline(
                title.to_owned(),
                egui::FontId::proportional(16.0),
                crate::ui::TEXT,
            );
            job.wrap =
                egui::text::TextWrapping::truncate_at_width((viewport.width() - 48.0).max(1.0));
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            ui.painter().galley(
                viewport.center() - galley.size() * 0.5 - egui::vec2(0.0, 10.0),
                galley,
                crate::ui::TEXT,
            );
            ui.painter().text(
                viewport.center() + egui::vec2(0.0, 20.0),
                egui::Align2::CENTER_CENTER,
                "Options RDP · Reconnecter",
                egui::FontId::proportional(12.0),
                crate::ui::SUBTLE,
            );
        }
        if matches!(action, Some(RdpAction::Connect)) {
            self.auto_connect = false;
            if self.parent != 0
                && self.host.is_some()
                && let Err(error) = self.connect()
            {
                self.status = "Connexion RDP impossible".into();
                self.detail = error;
                self.connecting_since = None;
            }
        }
        if matches!(action, Some(RdpAction::Disconnect)) {
            self.auto_connect = false;
            self.disconnect();
        }
        if matches!(action, Some(RdpAction::Fit)) {
            match self.fit() {
                Ok(()) => self.detail.clear(),
                Err(error) => self.detail = error,
            }
        }
        if matches!(action, Some(RdpAction::Fullscreen)) {
            match self.fullscreen() {
                Ok(()) => self.detail.clear(),
                Err(error) => self.detail = error,
            }
        }
        if matches!(action, Some(RdpAction::External)) {
            match open_windows_client(&self.profile) {
                Ok(_) => {
                    self.detail =
                        "Client Windows ouvert. Saisissez vos identifiants dans sa fenêtre.".into();
                }
                Err(error) => {
                    self.detail = error;
                }
            }
        }
        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }

    #[cfg(test)]
    pub(crate) fn offline_preview(profile: Profile) -> Result<Self, String> {
        let mut session = Self::new(profile, String::new())?;
        session.auto_connect = false;
        session.status = "Bureau distant de démonstration".into();
        Ok(session)
    }

    fn connect(&mut self) -> Result<(), String> {
        let host = self
            .host
            .as_ref()
            .ok_or("La fenêtre RDP native n’est pas disponible.")?;
        let client = host.client();
        if client.int("Connected")? != 0 {
            return Ok(());
        }
        client.put(
            "Server",
            Variant::text(self.profile.host.trim_matches(['[', ']']))?,
        )?;
        client.put("UserName", Variant::text(&self.profile.username)?)?;
        client.put("Domain", Variant::text(&self.profile.domain)?)?;
        client.put("ColorDepth", Variant::int(32))?;
        client.put("DesktopWidth", Variant::int(self.desktop_size.0))?;
        client.put("DesktopHeight", Variant::int(self.desktop_size.1))?;
        client.put("FullScreen", Variant::boolean(false))?;
        let settings = client.child("AdvancedSettings9")?;
        settings.put("RDPPort", Variant::int(self.profile.port as i32))?;
        settings.put("EnableCredSspSupport", Variant::boolean(true))?;
        // AuthenticationLevel=2 preserves Windows warnings and user verification of server certificates.
        settings.put("AuthenticationLevel", Variant::uint(2))?;
        settings.put("NegotiateSecurityLayer", Variant::boolean(true))?;
        settings.put(
            "RedirectClipboard",
            Variant::boolean(self.profile.redirect_clipboard),
        )?;
        for name in [
            "RedirectDrives",
            "RedirectPrinters",
            "RedirectPorts",
            "RedirectSmartCards",
            "RedirectDevices",
            "RedirectPOSDevices",
            "EnableAutoReconnect",
        ] {
            settings.put(name, Variant::boolean(false))?;
        }
        for name in ["SmartSizing", "DisplayConnectionBar", "GrabFocusOnConnect"] {
            settings.put(name, Variant::boolean(true))?;
        }
        settings.put("overallConnectionTimeout", Variant::int(30))?;
        settings.put("singleConnectionTimeout", Variant::int(15))?;
        settings.put("ClearTextPassword", Variant::text(&self.secret)?)?;
        client.call("Connect", &mut [])?;
        self.status = "Connexion au bureau distant…".into();
        self.detail = "Windows peut demander de vérifier le certificat du serveur ou de saisir vos identifiants.".into();
        self.last_state = 2;
        self.connecting_since = Some(Instant::now());
        Ok(())
    }

    fn disconnect(&mut self) {
        self.detail.clear();
        if let Some(host) = &self.host {
            if let Err(error) = host.client().call("Disconnect", &mut []) {
                self.detail = error;
            }
        }
        self.last_state = 0;
        self.connecting_since = None;
        self.status = "Bureau distant déconnecté".into();
    }

    fn poll(&mut self) {
        if self.last_poll.elapsed() < Duration::from_millis(500) {
            return;
        }
        self.last_poll = Instant::now();
        let Some(host) = &self.host else {
            return;
        };
        match host.client().int("Connected") {
            Ok(state) => {
                if state == 1 && self.last_state != 1 {
                    self.status = "Session RDP ouverte".into();
                    self.detail.clear();
                    self.connecting_since = None;
                }
                if state == 0 && self.last_state != 0 {
                    let reason = host.client().int("ExtendedDisconnectReason").unwrap_or(0);
                    self.status = "Bureau distant déconnecté".into();
                    self.detail = if reason != 0 {
                        host.client().description(reason)
                    } else {
                        "Si la connexion a échoué, vérifiez l’adresse, le port, les identifiants et l’activation du Bureau à distance sur le serveur.".into()
                    };
                    self.connecting_since = None;
                }
                self.last_state = state;
                if self
                    .connecting_since
                    .is_some_and(|started| started.elapsed() > Duration::from_secs(45))
                {
                    self.disconnect();
                    self.status = "Délai de connexion RDP dépassé".into();
                    self.detail = "Vérifiez l’adresse, le port et l’activation du Bureau à distance sur le serveur.".into();
                }
            }
            Err(error) => {
                self.status = "État RDP indisponible".into();
                self.detail = error;
            }
        }
    }

    fn fit(&self) -> Result<(), String> {
        let client = self
            .host
            .as_ref()
            .ok_or("La session RDP est fermée.")?
            .client();
        client
            .child("AdvancedSettings9")?
            .put("SmartSizing", Variant::boolean(true))?;
        client.call("UpdateSessionDisplaySettings", &mut [
            Variant::uint(self.desktop_size.0 as u32), Variant::uint(self.desktop_size.1 as u32),
            Variant::uint(0), Variant::uint(0), Variant::uint(0), Variant::uint(100), Variant::uint(100),
        ]).map(|_| ()).map_err(|_| "Le serveur conserve sa résolution actuelle ; la mise à l’échelle locale reste active.".into())
    }

    fn fullscreen(&self) -> Result<(), String> {
        let client = self
            .host
            .as_ref()
            .ok_or("La session RDP est fermée.")?
            .client();
        let fullscreen = client.int("FullScreen")? != 0;
        client.put("FullScreen", Variant::boolean(!fullscreen))
    }
}

impl Drop for RdpSession {
    fn drop(&mut self) {
        self.host.take();
    }
}

fn pixel_viewport(rect: egui::Rect, scale: f32) -> (i32, i32, i32, i32) {
    let left = (rect.left() * scale).round() as i32;
    let top = (rect.top() * scale).round() as i32;
    let right = (rect.right() * scale).round() as i32;
    let bottom = (rect.bottom() * scale).round() as i32;
    (left, top, (right - left).max(1), (bottom - top).max(1))
}

fn open_windows_client(profile: &Profile) -> Result<(), String> {
    profile.validate()?;
    let system = std::env::var_os("SystemRoot").ok_or("Dossier Windows introuvable.")?;
    let program = PathBuf::from(system).join("System32").join("mstsc.exe");
    let folder = PathBuf::from(
        std::env::var_os("LOCALAPPDATA").ok_or("Dossier local utilisateur introuvable.")?,
    )
    .join("NexusRemote")
    .join("RdpSessions");
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let path = folder.join(format!("nexus-{}.rdp", uuid::Uuid::new_v4()));
    let host = profile.host.trim_matches(['[', ']']);
    let address = if host.contains(':') {
        format!("[{host}]:{}", profile.port)
    } else {
        format!("{host}:{}", profile.port)
    };
    let user = if profile.domain.is_empty() {
        profile.username.clone()
    } else {
        format!("{}\\{}", profile.domain, profile.username)
    };
    let configuration = format!(
        "screen mode id:i:1\r\nfull address:s:{address}\r\nusername:s:{user}\r\nprompt for credentials:i:1\r\nenablecredsspsupport:i:1\r\nnegotiate security layer:i:1\r\nauthentication level:i:2\r\nredirectclipboard:i:{}\r\nredirectdrives:i:0\r\nredirectprinters:i:0\r\nredirectcomports:i:0\r\n",
        u8::from(profile.redirect_clipboard)
    );
    let mut bytes = vec![0xff, 0xfe];
    bytes.extend(configuration.encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Command::new(program)
        .arg(&path)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rdp_pixel_viewport_preserves_fractional_dpi_edges() {
        let rect = egui::Rect::from_min_max(egui::pos2(13.2, 44.4), egui::pos2(424.4, 316.4));
        assert_eq!(pixel_viewport(rect, 1.25), (17, 56, 514, 340));
        let empty = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::Vec2::ZERO);
        assert_eq!(pixel_viewport(empty, 1.0), (10, 20, 1, 1));
    }

    #[test]
    fn rdp_controls_and_preview_remain_offline_and_fit_header_slots() {
        let profile = Profile {
            name: "Bureau de démonstration".into(),
            host: "192.0.2.40".into(),
            port: 3389,
            protocol: crate::model::Protocol::Rdp,
            ..Default::default()
        };
        let mut session = RdpSession::offline_preview(profile).unwrap();
        let ctx = egui::Context::default();
        crate::ui::apply_theme(&ctx);
        for width in [64.0, 120.0, 240.0] {
            for _ in 0..3 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 32.0),
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let available = ui.available_rect_before_wrap();
                                    session.controls(ui);
                                    assert!(ui.min_rect().left() >= available.left() - 1.0);
                                    assert!(ui.min_rect().right() <= available.right() + 1.0);
                                    assert!(ui.min_rect().height() <= 33.0);
                                },
                            );
                            session.ui(ui);
                        });
                    },
                );
            }
        }
        assert!(session.host.is_none());
        assert_eq!(session.parent, 0);
        assert!(!session.auto_connect);
        assert!(!session.connected());
    }
}

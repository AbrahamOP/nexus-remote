use super::*;
impl NexusApp {
    pub(super) fn palette_modal(&mut self, ctx: &egui::Context) {
        if !self.palette {
            return;
        }
        let (mut close, mut quick) = (false, false);
        let mut choice = None;
        egui::Modal::new(egui::Id::new("command_palette")).show(ctx, |ui| {
            ui.set_width(620.0);
            ui.horizontal(|ui| {
                icon(ui, Icon::Search, BLUE, 22.0);
                ui.label(RichText::new("Connexion rapide").size(20.0).strong());
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    close = icon_button(ui, Icon::Close, "Fermer").clicked();
                });
            });
            ui.add_space(10.0);
            let response = ui.add_sized(
                [ui.available_width(), 40.0],
                egui::TextEdit::singleline(&mut self.palette_text)
                    .hint_text("Rechercher ou saisir ssh://utilisateur@serveur:22")
                    .margin(Vec2::new(12.0, 9.0)),
            );
            if self.palette_focus {
                response.request_focus();
                self.palette_focus = false;
            }
            quick = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let needle = self.palette_text.to_lowercase();
            let hosts: Vec<_> = self
                .store
                .state
                .profiles
                .iter()
                .filter(|p| {
                    format!("{} {} {}", p.name, p.host, p.username)
                        .to_lowercase()
                        .contains(&needle)
                })
                .take(8)
                .cloned()
                .collect();
            section(ui, "HÔTES");
            for p in hosts {
                let row = Frame::new()
                    .fill(CARD)
                    .corner_radius(6)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            icon(ui, proto_icon(p.protocol), proto(p.protocol), 21.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&p.name).strong());
                                muted(ui, format!("{}@{}:{}", p.username, p.host, p.port));
                            });
                        });
                    });
                if row.response.interact(Sense::click()).clicked() {
                    choice = Some(p);
                }
            }
            ui.add_space(10.0);
            if !self.palette_text.trim().is_empty() {
                if primary(ui, "Se connecter à l’adresse saisie").clicked() {
                    quick = true;
                }
                muted(
                    ui,
                    "Formats : utilisateur@hôte:port · ssh:// · sftp:// · rdp://",
                );
            }
            ui.add_space(7.0);
            ui.horizontal(|ui| {
                if ui.button("Nouvel hôte").clicked() {
                    close = true;
                    self.new_profile(Protocol::Ssh);
                }
                if ui.button("Identités").clicked() {
                    close = true;
                    self.set_page(Page::Identities);
                }
                if ui.button("Snippets").clicked() {
                    close = true;
                    self.set_page(Page::Snippets);
                }
                if ui.button("Tunnels").clicked() {
                    close = true;
                    self.set_page(Page::Tunnels);
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if let Some(p) = choice {
            self.connect(p, None, ctx);
            close = true;
        } else if quick {
            match crate::ssh_config::quick_profile(&self.palette_text) {
                Ok(p) => {
                    self.connect(p, None, ctx);
                    close = true;
                }
                Err(error) => self.error = Some(error),
            }
        }
        if close {
            self.palette = false;
        }
    }
}

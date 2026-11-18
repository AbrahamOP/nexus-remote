use super::*;
impl NexusApp {
    pub(super) fn auth_modal(&mut self, ctx: &egui::Context) {
        let Some(mut auth) = self.auth.take() else {
            return;
        };
        let mut connect = false;
        let mut close = false;
        egui::Modal::new(egui::Id::new("auth")).show(ctx, |ui| {
            ui.set_width(430.0);
            ui.heading(&auth.profile.name);
            muted(
                ui,
                format!(
                    "{}@{}:{}",
                    auth.profile.username, auth.profile.host, auth.profile.port
                ),
            );
            ui.add_space(14.0);
            ui.label(if auth.profile.auth == AuthMode::PrivateKey {
                "Phrase secrète (vide si la clé n’est pas chiffrée)"
            } else {
                "Mot de passe"
            });
            let response = ui.add(
                egui::TextEdit::singleline(&mut *auth.secret)
                    .password(true)
                    .desired_width(f32::INFINITY),
            );
            if auth.first_focus {
                response.request_focus();
                auth.first_focus = false;
            }
            muted(ui, "Utilisé pour cette session uniquement.");
            ui.add_space(13.0);
            ui.horizontal(|ui| {
                if primary(ui, "Se connecter").clicked() {
                    connect = true;
                }
                if ui.button("Annuler").clicked() {
                    close = true;
                }
            });
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                connect = true;
            }
        });
        if connect {
            self.dispatch_launch(
                auth.profile.clone(),
                auth.secret.to_string(),
                auth.target.clone(),
                ctx,
            );
        } else if !close {
            self.auth = Some(auth);
        }
    }
    pub(super) fn trust_requests(&mut self) {
        while let Ok(request) = self.trust_rx.try_recv() {
            match self.store.state.known_hosts.get(&request.endpoint) {
                Some(fingerprint) if fingerprint == &request.fingerprint => {
                    let _ = request.reply.send(true);
                }
                Some(previous) => {
                    let _ = request.reply.send(false);
                    self.error = Some(format!(
                        "La clé SSH de {} a changé. Connexion refusée.\n\nEmpreinte enregistrée :\n{}\n\nEmpreinte présentée :\n{}\n\nVérifiez auprès du serveur avant de retirer l’ancienne clé dans les paramètres.",
                        request.endpoint, previous, request.fingerprint
                    ));
                }
                None => self.trust_queue.push_back(request),
            }
        }
    }
    pub(super) fn trust_modal(&mut self, ctx: &egui::Context) {
        let Some(request) = self.trust_queue.pop_front() else {
            return;
        };
        let mut answer = None;
        // A previous concurrent connection may just have recorded this endpoint.
        if let Some(known) = self.store.state.known_hosts.get(&request.endpoint) {
            let _ = request.reply.send(known == &request.fingerprint);
            return;
        }
        egui::Modal::new(egui::Id::new("host_key")).show(ctx, |ui| {
            ui.set_width(530.0);
            ui.heading("Vérifier la clé SSH");
            ui.label(format!("Première connexion à {}", request.endpoint));
            ui.add_space(12.0);
            muted(ui, format!("Algorithme : {}", request.algorithm));
            ui.label(
                RichText::new(&request.fingerprint)
                    .monospace()
                    .color(ACCENT),
            );
            ui.add_space(13.0);
            ui.label(
                "Comparez cette empreinte avec celle du serveur avant de lui faire confiance.",
            );
            ui.horizontal(|ui| {
                if ui.button("Refuser").clicked() {
                    answer = Some(false);
                }
                if primary(ui, "Faire confiance et mémoriser").clicked() {
                    answer = Some(true);
                }
            });
        });
        match answer {
            Some(true) => {
                let previous = self.store.state.clone();
                self.store
                    .state
                    .known_hosts
                    .insert(request.endpoint.clone(), request.fingerprint.clone());
                if self.save() {
                    let _ = request.reply.send(true);
                } else {
                    self.store.state = previous;
                    let _ = request.reply.send(false);
                }
            }
            Some(false) => {
                let _ = request.reply.send(false);
            }
            None => self.trust_queue.push_front(request),
        }
    }
    pub(super) fn confirmation_modals(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.delete.clone() {
            let name = self
                .store
                .state
                .profiles
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.name.as_str())
                .unwrap_or("cette connexion");
            let mut answer = None;
            egui::Modal::new(egui::Id::new("delete_profile")).show(ctx, |ui| {
                ui.set_width(400.0);
                ui.heading("Supprimer la connexion ?");
                ui.label(format!("La connexion « {name} » sera retirée du carnet."));
                ui.horizontal(|ui| {
                    if ui.button("Annuler").clicked() {
                        answer = Some(false);
                    }
                    if ui.button("Supprimer").clicked() {
                        answer = Some(true);
                    }
                });
            });
            if let Some(yes) = answer {
                if yes {
                    let old = self.store.state.clone();
                    self.store.state.remove_profile(&id);
                    if !self.save() {
                        self.store.state = old;
                    } else {
                        self.transient.remove(&id);
                        for rule in old.tunnel_rules.iter().filter(|rule| rule.profile_id == id) {
                            self.active_tunnels.remove(&rule.id);
                        }
                    }
                }
                self.delete = None;
            }
        }
        if let Some(endpoint) = self.forget.clone() {
            let mut answer = None;
            egui::Modal::new(egui::Id::new("forget_key")).show(ctx,|ui|{ui.set_width(440.0);ui.heading("Retirer l’empreinte SSH ?");ui.label(&endpoint);ui.label("La prochaine connexion vous demandera de vérifier à nouveau la clé du serveur.");ui.horizontal(|ui|{if ui.button("Annuler").clicked(){answer=Some(false);}if ui.button("Retirer").clicked(){answer=Some(true);}});});
            if let Some(yes) = answer {
                if yes {
                    let old = self.store.state.clone();
                    self.store.state.known_hosts.remove(&endpoint);
                    if !self.save() {
                        self.store.state = old;
                    }
                }
                self.forget = None;
            }
        }
    }
}

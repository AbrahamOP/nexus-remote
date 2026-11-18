use super::*;
impl NexusApp {
    pub(super) fn new_group(&mut self) {
        self.group_editor = Some(GroupEditor {
            group: GroupConfig::default(),
            port: String::new(),
            error: String::new(),
            old_name: None,
        });
    }
    pub(super) fn new_workspace_from_tabs(&mut self) {
        let mut ids: Vec<String> = self
            .tabs
            .iter()
            .filter(|t| {
                self.store
                    .state
                    .profiles
                    .iter()
                    .any(|p| p.id == t.profile.id)
            })
            .map(|t| t.profile.id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        self.workspace_editor = Some(WorkspaceEditor {
            workspace: Workspace {
                name: "Mon espace".into(),
                profile_ids: ids,
                ..Default::default()
            },
            error: String::new(),
        });
    }
    pub(super) fn identities_page(&mut self, ui: &mut egui::Ui) {
        self.page_heading(
            ui,
            "Identités",
            "Réutilisez un utilisateur, un mot de passe ou une clé sur plusieurs hôtes.",
        );
        if primary(ui, "+  Nouvelle identité").clicked() {
            self.identity_editor = Some(IdentityEditor {
                identity: Identity::default(),
                secret: Zeroizing::new(String::new()),
                remember: false,
                error: String::new(),
            });
        }
        ui.add_space(18.0);
        if self.store.state.identities.is_empty() {
            empty(
                ui,
                Icon::Key,
                "Un identifiant pour plusieurs machines",
                "Créez une identité, puis sélectionnez-la dans vos hôtes ou groupes.",
            );
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for identity in self.store.state.identities.clone() {
                ui.push_id(&identity.id, |ui| {
                    Frame::new()
                        .fill(CARD)
                        .corner_radius(16)
                        .inner_margin(17)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                icon(ui, Icon::Key, PURPLE, 32.0);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&identity.name).strong().size(16.0));
                                    muted(
                                        ui,
                                        format!(
                                            "{} · {}",
                                            identity.username,
                                            if identity.auth == AuthMode::PrivateKey {
                                                "Clé privée SSH"
                                            } else {
                                                "Mot de passe"
                                            }
                                        ),
                                    );
                                });
                                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button("Supprimer").clicked() {
                                        self.entity_delete =
                                            Some(EntityDelete::Identity(identity.id.clone()));
                                    }
                                    if ui.button("Modifier").clicked() {
                                        match identity
                                            .encrypted_secret
                                            .as_ref()
                                            .map(|s| storage::unprotect(s))
                                            .transpose()
                                        {
                                            Ok(secret) => {
                                                self.identity_editor = Some(IdentityEditor {
                                                    remember: identity.encrypted_secret.is_some(),
                                                    identity: identity.clone(),
                                                    secret: Zeroizing::new(
                                                        secret
                                                            .or_else(|| {
                                                                self.transient
                                                                    .get(&format!(
                                                                        "identity:{}",
                                                                        identity.id
                                                                    ))
                                                                    .map(|s| s.to_string())
                                                            })
                                                            .unwrap_or_default(),
                                                    ),
                                                    error: String::new(),
                                                });
                                            }
                                            Err(e) => self.error = Some(e),
                                        }
                                    }
                                });
                            });
                        });
                    ui.add_space(9.0);
                });
            }
        });
    }
    pub(super) fn snippets_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.page_heading(
            ui,
            "Snippets",
            "Gardez vos commandes utiles à portée de main, puis choisissez où les envoyer.",
        );
        ui.horizontal(|ui| {
            if primary(ui, "+  Nouveau snippet").clicked() {
                self.snippet_editor = Some(SnippetEditor {
                    snippet: Snippet::default(),
                    error: String::new(),
                });
            }
            ui.menu_button("Exemples", |ui| {
                for (name, command) in [
                    ("État du serveur", "uptime"),
                    ("Espace disque", "df -h"),
                    ("Mémoire", "free -h"),
                    ("Services en échec", "systemctl --failed"),
                ] {
                    if ui.button(name).clicked() {
                        self.snippet_editor = Some(SnippetEditor {
                            snippet: Snippet {
                                name: name.into(),
                                command: command.into(),
                                ..Default::default()
                            },
                            error: String::new(),
                        });
                        ui.close();
                    }
                }
            });
        });
        ui.add_space(16.0);
        if self.store.state.snippets.is_empty() {
            empty(
                ui,
                Icon::Code,
                "Vos commandes, sans les retaper",
                "Enregistrez un snippet ou partez d’un exemple.",
            );
        }
        egui::ScrollArea::vertical().show(ui,|ui|{for snippet in self.store.state.snippets.clone(){ui.push_id(&snippet.id,|ui|{Frame::new().fill(CARD).corner_radius(16).inner_margin(24).show(ui,|ui|{ui.set_width(ui.available_width());ui.horizontal(|ui|{icon(ui,Icon::Code,BLUE,26.0);ui.label(RichText::new(&snippet.name).size(16.0).strong());badge(ui,&snippet.group,MUTED);ui.with_layout(egui::Layout::right_to_left(Align::Center),|ui|{if ui.small_button("Supprimer").clicked(){self.entity_delete=Some(EntityDelete::Snippet(snippet.id.clone()));}if ui.small_button("Modifier").clicked(){self.snippet_editor=Some(SnippetEditor{snippet:snippet.clone(),error:String::new()});}});});ui.add_space(8.0);Frame::new().fill(TOP).corner_radius(6).inner_margin(12).show(ui,|ui|{ui.add(egui::Label::new(RichText::new(&snippet.command).monospace().color(GREEN)).wrap());});ui.add_space(9.0);
            ui.horizontal(|ui|{if ui.button("Copier").clicked(){ctx.copy_text(snippet.command.clone());}ui.menu_button("Insérer dans…",|ui|{let sessions:Vec<_>=self.tabs.iter().enumerate().filter(|(_,t)|matches!(&t.view,SessionView::Ssh(s)if s.connected())).map(|(i,t)|(i,t.title.clone())).collect();if sessions.is_empty(){muted(ui,"Ouvrez d’abord une session SSH.");}for(index,title)in sessions{if ui.button(title).clicked(){if let SessionView::Ssh(s)=&mut self.tabs[index].view{s.insert_text(&snippet.command);}self.active=Some(index);self.secondary=None;self.page=Page::Session;ui.close();}}});if ui.button("Exécuter…").clicked(){let targets=self.active.filter(|i|matches!(self.tabs.get(*i).map(|t|&t.view),Some(SessionView::Ssh(s))if s.connected())).and_then(|i|self.tabs.get(i).map(|t|t.id.clone())).into_iter().collect();self.command_run=Some(CommandRun{command:snippet.command.clone(),targets});}});
        });ui.add_space(12.0);});}});
    }
    pub(super) fn tunnels_page(&mut self, ui: &mut egui::Ui) {
        self.page_heading(
            ui,
            "Tunnels SSH",
            "Accédez à un service à travers une connexion SSH locale ou distante.",
        );
        if primary(ui, "+  Nouveau tunnel").clicked() {
            let mut rule = TunnelRule::default();
            if let Some(p) = self
                .store
                .state
                .profiles
                .iter()
                .find(|p| p.protocol != Protocol::Rdp)
            {
                rule.profile_id = p.id.clone();
            }
            self.tunnel_editor = Some(TunnelEditor {
                local_port: rule.local_port.to_string(),
                destination_port: rule.destination_port.to_string(),
                rule,
                error: String::new(),
            });
        }
        ui.add_space(18.0);
        if self.store.state.tunnel_rules.is_empty() {
            empty(
                ui,
                Icon::Tunnel,
                "Reliez vos services en toute simplicité",
                "Exemple : localhost:5433 vers la base de données d’un serveur SSH.",
            );
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for rule in self.store.state.tunnel_rules.clone() {
                let running = self
                    .active_tunnels
                    .get(&rule.id)
                    .is_some_and(|s| s.is_running());
                let status = self
                    .active_tunnels
                    .get(&rule.id)
                    .map(|s| s.status().to_string())
                    .unwrap_or("Arrêté".into());
                ui.push_id(&rule.id, |ui| {
                    Frame::new()
                        .fill(CARD)
                        .corner_radius(16)
                        .inner_margin(24)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                icon(ui, Icon::Tunnel, if running { GREEN } else { PURPLE }, 30.0);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&rule.name).size(16.0).strong());
                                    muted(ui, rule.kind.label());
                                });
                                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                    if running {
                                        if ui.button("Arrêter").clicked() {
                                            self.active_tunnels.remove(&rule.id);
                                        }
                                    } else if primary(ui, "Démarrer").clicked() {
                                        if let Some(p) = self
                                            .store
                                            .state
                                            .profiles
                                            .iter()
                                            .find(|p| p.id == rule.profile_id)
                                            .cloned()
                                        {
                                            self.launch_queue
                                                .push_back((p, LaunchTarget::Tunnel(rule.clone())));
                                        }
                                    }
                                    if ui
                                        .add_enabled(!running, egui::Button::new("Modifier"))
                                        .clicked()
                                    {
                                        self.tunnel_editor = Some(TunnelEditor {
                                            rule: rule.clone(),
                                            local_port: rule.local_port.to_string(),
                                            destination_port: rule.destination_port.to_string(),
                                            error: String::new(),
                                        });
                                    }
                                    if ui
                                        .add_enabled(!running, egui::Button::new("Supprimer"))
                                        .clicked()
                                    {
                                        self.entity_delete =
                                            Some(EntityDelete::Tunnel(rule.id.clone()));
                                    }
                                });
                            });
                            ui.add_space(8.0);
                            ui.label(RichText::new(rule.description()).monospace().color(BLUE));
                            muted(ui, status);
                            if rule.kind == TunnelKind::Remote {
                                ui.colored_label(
                                    GOLD,
                                    "Attention : le serveur SSH peut exposer ce port sur son réseau si GatewayPorts=yes. Vérifiez sa configuration avant de démarrer.",
                                );
                            }
                        });
                    ui.add_space(12.0);
                });
            }
        });
    }
    pub(super) fn workspaces_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.page_heading(
            ui,
            "Espaces de travail",
            "Rouvrez ensemble les hôtes d’un projet, puis partagez votre vue en deux panneaux.",
        );
        if primary(ui, "+  Nouvel espace").clicked() {
            self.new_workspace_from_tabs();
        }
        ui.add_space(18.0);
        if self.store.state.workspaces.is_empty() {
            empty(
                ui,
                Icon::Grid,
                "Retrouvez votre environnement",
                "Sélectionnez plusieurs hôtes et enregistrez-les dans un espace de travail.",
            );
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for workspace in self.store.state.workspaces.clone() {
                Frame::new()
                    .fill(CARD)
                    .corner_radius(16)
                    .inner_margin(24)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            icon(ui, Icon::Grid, PURPLE, 32.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&workspace.name).size(17.0).strong());
                                muted(ui, format!("{} hôte(s)", workspace.profile_ids.len()));
                            });
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                if primary(ui, "Ouvrir l’espace").clicked() {
                                    for id in &workspace.profile_ids {
                                        if let Some(p) = self
                                            .store
                                            .state
                                            .profiles
                                            .iter()
                                            .find(|p| &p.id == id)
                                            .cloned()
                                        {
                                            self.connect(p, None, ctx);
                                        }
                                    }
                                }
                                if ui.button("Modifier").clicked() {
                                    self.workspace_editor = Some(WorkspaceEditor {
                                        workspace: workspace.clone(),
                                        error: String::new(),
                                    });
                                }
                                if ui.button("Supprimer").clicked() {
                                    self.entity_delete =
                                        Some(EntityDelete::Workspace(workspace.id.clone()));
                                }
                            });
                        });
                        ui.add_space(8.0);
                        ui.horizontal_wrapped(|ui| {
                            for id in &workspace.profile_ids {
                                if let Some(p) =
                                    self.store.state.profiles.iter().find(|p| &p.id == id)
                                {
                                    badge(ui, &p.name, proto(p.protocol));
                                }
                            }
                        });
                    });
                ui.add_space(12.0);
            }
        });
    }
    pub(super) fn known_hosts_page(&mut self, ui: &mut egui::Ui) {
        self.page_heading(
            ui,
            "Empreintes SSH",
            "Les identités de serveur que vous avez vérifiées lors de vos connexions.",
        );
        if self.store.state.known_hosts.is_empty() {
            empty(
                ui,
                Icon::Check,
                "Aucune empreinte enregistrée",
                "La première connexion à un serveur vous demandera de vérifier sa clé.",
            );
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (endpoint, fingerprint) in self.store.state.known_hosts.clone() {
                Frame::new()
                    .fill(CARD)
                    .corner_radius(8)
                    .inner_margin(16)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            icon(ui, Icon::Check, GREEN, 23.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&endpoint).strong());
                                ui.label(
                                    RichText::new(&fingerprint)
                                        .monospace()
                                        .color(MUTED)
                                        .size(12.0),
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("Retirer").clicked() {
                                    self.forget = Some(endpoint);
                                }
                                if ui.button("Copier").clicked() {
                                    ui.ctx().copy_text(fingerprint);
                                }
                            });
                        });
                    });
                ui.add_space(9.0);
            }
        });
    }
    pub(super) fn settings_page(&mut self, ui: &mut egui::Ui) {
        self.page_heading(
            ui,
            "Préférences",
            "Personnalisez votre terminal et gérez vos données locales.",
        );
        egui::ScrollArea::vertical().show(ui, |ui| {
            section(ui, "TERMINAL");
            Frame::new()
                .fill(CARD)
                .corner_radius(16)
                .inner_margin(20)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label("Thème de couleurs");
                    ui.horizontal(|ui| {
                        for (theme, name) in [
                            ("nexus", "Nexus"),
                            ("dracula", "Dracula"),
                            ("solarized", "Solarized"),
                        ] {
                            ui.selectable_value(&mut self.settings_theme, theme.into(), name);
                        }
                    });
                    ui.add_space(10.0);
                    ui.add(
                        egui::Slider::new(&mut self.settings_font, 10.0..=24.0)
                            .text("Taille du texte")
                            .suffix(" px"),
                    );
                    ui.add_space(8.0);
                    Frame::new()
                        .fill(if self.settings_theme == "dracula" {
                            Color32::from_rgb(40, 42, 54)
                        } else if self.settings_theme == "solarized" {
                            Color32::from_rgb(0, 43, 54)
                        } else {
                            TOP
                        })
                        .corner_radius(7)
                        .inner_margin(15)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("admin@serveur:~$ uptime")
                                    .monospace()
                                    .size(self.settings_font)
                                    .color(GREEN),
                            );
                            ui.label(
                                RichText::new("14:30:01 up 12 days, 3 users, load average: 0.02")
                                    .monospace()
                                    .size(self.settings_font)
                                    .color(TEXT),
                            );
                        });
                    if primary(ui, "Appliquer au terminal").clicked() {
                        let old = self.store.state.clone();
                        self.store.state.preferences.terminal_font_size = self.settings_font;
                        self.store.state.preferences.terminal_theme = self.settings_theme.clone();
                        if self.save() {
                            for tab in &mut self.tabs {
                                if let SessionView::Ssh(s) = &mut tab.view {
                                    s.set_preferences(self.settings_font, &self.settings_theme);
                                }
                            }
                            self.status = "Préférences enregistrées.".into();
                        } else {
                            self.store.state = old;
                        }
                    }
                });
            ui.add_space(17.0);
            section(ui, "DONNÉES");
            Frame::new()
                .fill(CARD)
                .corner_radius(16)
                .inner_margin(20)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label("Coffre personnel local");
                    muted(
                        ui,
                        "Les mots de passe enregistrés sont chiffrés pour votre compte Windows.",
                    );
                    ui.label(
                        RichText::new(self.store.data_dir.display().to_string())
                            .monospace()
                            .size(11.0)
                            .color(MUTED),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Ouvrir le dossier").clicked() {
                            let _ = std::fs::create_dir_all(&self.store.data_dir);
                            let _ = std::process::Command::new("explorer.exe")
                                .arg(&self.store.data_dir)
                                .spawn();
                        }
                        if ui.button("Exporter les hôtes sans secrets").clicked() {
                            self.export();
                        }
                        if ui.button("Importer JSON").clicked() {
                            self.import();
                        }
                    });
                });
            ui.add_space(17.0);
            section(ui, "RACCOURCIS");
            for (key, help) in [
                ("Ctrl K", "Connexion rapide et recherche"),
                ("Ctrl N", "Nouvel hôte dans le coffre"),
                ("Ctrl F", "Rechercher dans le coffre"),
                ("Ctrl Maj F", "Rechercher dans un terminal"),
                ("Ctrl Maj C / V", "Copier / coller dans un terminal"),
                ("Ctrl Alt Fin", "Écran de sécurité du bureau RDP"),
            ] {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [165.0, 25.0],
                        egui::Label::new(RichText::new(key).monospace().color(BLUE)),
                    );
                    muted(ui, help);
                });
            }
            ui.add_space(15.0);
            muted(
                ui,
                concat!(
                    "Nexus Remote ",
                    env!("CARGO_PKG_VERSION"),
                    " · Rust natif · SSH / SFTP / RDP"
                ),
            );
        });
    }
    pub(super) fn entities_modals(&mut self, ctx: &egui::Context) {
        self.identity_modal(ctx);
        self.group_modal(ctx);
        self.snippet_modal(ctx);
        self.tunnel_modal(ctx);
        self.workspace_modal(ctx);
        self.entity_delete_modal(ctx);
        self.command_modal(ctx);
    }
    fn identity_modal(&mut self, ctx: &egui::Context) {
        let Some(mut e) = self.identity_editor.take() else {
            return;
        };
        let (mut save, mut close) = (false, false);
        egui::Modal::new(egui::Id::new("identity_editor")).show(ctx, |ui| {
            ui.set_width(490.0);
            ui.heading("Identité réutilisable");
            ui.add_space(12.0);
            field(ui, "Nom", &mut e.identity.name);
            field(ui, "Utilisateur", &mut e.identity.username);
            ui.horizontal(|ui| {
                for (mode, title) in [
                    (AuthMode::Password, "Mot de passe"),
                    (AuthMode::PrivateKey, "Clé SSH"),
                ] {
                    if ui
                        .selectable_label(e.identity.auth == mode, title)
                        .clicked()
                    {
                        e.identity.auth = mode;
                        e.secret.zeroize();
                    }
                }
            });
            if e.identity.auth == AuthMode::PrivateKey {
                field(
                    ui,
                    "Chemin de la clé privée",
                    &mut e.identity.private_key_path,
                );
                if ui.button("Choisir une clé…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_file() {
                        e.identity.private_key_path = p.display().to_string();
                    }
                }
            }
            muted(
                ui,
                if e.identity.auth == AuthMode::PrivateKey {
                    "Phrase secrète de la clé"
                } else {
                    "Mot de passe"
                },
            );
            ui.add_sized(
                [ui.available_width(), 34.0],
                egui::TextEdit::singleline(&mut *e.secret).password(true),
            );
            ui.checkbox(
                &mut e.remember,
                "Enregistrer le secret sur ce compte Windows",
            );
            muted(
                ui,
                "Sans cette option, le secret reste en mémoire jusqu’à la fermeture.",
            );
            if !e.error.is_empty() {
                ui.colored_label(Color32::LIGHT_RED, &e.error);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                save = primary(ui, "Enregistrer").clicked();
                close = ui.button("Annuler").clicked();
            });
        });
        if save {
            let result = (|| {
                e.identity.validate()?;
                if e.identity.auth == AuthMode::PrivateKey
                    && !std::path::Path::new(&e.identity.private_key_path).is_file()
                {
                    return Err("Le fichier de clé est introuvable.".into());
                }
                e.identity.encrypted_secret = if e.remember {
                    Some(storage::protect(&e.secret)?)
                } else {
                    None
                };
                Ok::<(), String>(())
            })();
            match result {
                Err(error) => e.error = error,
                Ok(()) => {
                    let old = self.store.state.clone();
                    if let Some(saved) = self
                        .store
                        .state
                        .identities
                        .iter_mut()
                        .find(|i| i.id == e.identity.id)
                    {
                        *saved = e.identity.clone();
                    } else {
                        self.store.state.identities.push(e.identity.clone());
                    }
                    if self.save() {
                        let key = format!("identity:{}", e.identity.id);
                        if !e.remember && !e.secret.is_empty() {
                            self.transient
                                .insert(key, Zeroizing::new(e.secret.to_string()));
                        } else {
                            self.transient.remove(&key);
                        }
                        close = true;
                    } else {
                        self.store.state = old;
                    }
                }
            }
        }
        if !close {
            self.identity_editor = Some(e);
        }
    }
    fn group_modal(&mut self, ctx: &egui::Context) {
        let Some(mut e) = self.group_editor.take() else {
            return;
        };
        let (mut save, mut close) = (false, false);
        egui::Modal::new(egui::Id::new("group_editor")).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.heading("Groupe d’hôtes");
            field(ui, "Nom", &mut e.group.name);
            field(ui, "Description", &mut e.group.description);
            section(ui, "VALEURS PAR DÉFAUT POUR LES NOUVEAUX HÔTES");
            field(
                ui,
                "Utilisateur (facultatif)",
                &mut e.group.default_username,
            );
            field(ui, "Port (facultatif)", &mut e.port);
            egui::ComboBox::from_id_salt("group_identity")
                .selected_text(
                    e.group
                        .default_identity_id
                        .as_ref()
                        .and_then(|id| self.store.state.identities.iter().find(|i| &i.id == id))
                        .map(|i| i.name.as_str())
                        .unwrap_or("Aucune identité"),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut e.group.default_identity_id, None, "Aucune identité");
                    for identity in &self.store.state.identities {
                        ui.selectable_value(
                            &mut e.group.default_identity_id,
                            Some(identity.id.clone()),
                            &identity.name,
                        );
                    }
                });
            if !e.error.is_empty() {
                ui.colored_label(Color32::LIGHT_RED, &e.error);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                save = primary(ui, "Enregistrer").clicked();
                close = ui.button("Annuler").clicked();
            });
        });
        if save {
            let result = (|| {
                e.group.default_port = if e.port.trim().is_empty() {
                    None
                } else {
                    Some(e.port.parse::<u16>().map_err(|_| "Port invalide.")?)
                };
                e.group.validate()
            })();
            match result {
                Err(error) => e.error = error,
                Ok(()) => {
                    let old = self.store.state.clone();
                    if let Some(name) = &e.old_name {
                        for p in &mut self.store.state.profiles {
                            if &p.group == name {
                                p.group = e.group.name.clone();
                            }
                        }
                    }
                    if let Some(g) = self
                        .store
                        .state
                        .groups
                        .iter_mut()
                        .find(|g| g.id == e.group.id)
                    {
                        *g = e.group.clone();
                    } else {
                        self.store.state.groups.push(e.group.clone());
                    }
                    if self.save() {
                        close = true;
                    } else {
                        self.store.state = old;
                    }
                }
            }
        }
        if !close {
            self.group_editor = Some(e);
        }
    }
    fn snippet_modal(&mut self, ctx: &egui::Context) {
        let Some(mut e) = self.snippet_editor.take() else {
            return;
        };
        let (mut save, mut close) = (false, false);
        egui::Modal::new(egui::Id::new("snippet_editor")).show(ctx, |ui| {
            ui.set_width(540.0);
            ui.heading("Snippet");
            field(ui, "Nom", &mut e.snippet.name);
            field(ui, "Groupe", &mut e.snippet.group);
            muted(ui, "Commande");
            ui.add(
                egui::TextEdit::multiline(&mut e.snippet.command)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(7)
                    .desired_width(f32::INFINITY),
            );
            if !e.error.is_empty() {
                ui.colored_label(Color32::LIGHT_RED, &e.error);
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                save = primary(ui, "Enregistrer").clicked();
                close = ui.button("Annuler").clicked();
            });
        });
        if save {
            match e.snippet.validate() {
                Err(error) => e.error = error,
                Ok(()) => {
                    let old = self.store.state.clone();
                    if let Some(s) = self
                        .store
                        .state
                        .snippets
                        .iter_mut()
                        .find(|s| s.id == e.snippet.id)
                    {
                        *s = e.snippet.clone();
                    } else {
                        self.store.state.snippets.push(e.snippet.clone());
                    }
                    if self.save() {
                        close = true;
                    } else {
                        self.store.state = old;
                    }
                }
            }
        }
        if !close {
            self.snippet_editor = Some(e);
        }
    }
    fn tunnel_modal(&mut self, ctx: &egui::Context) {
        let Some(mut e) = self.tunnel_editor.take() else {
            return;
        };
        let (mut save, mut close) = (false, false);
        egui::Modal::new(egui::Id::new("tunnel_editor")).show(ctx, |ui| {
            ui.set_width(530.0);
            ui.heading("Tunnel SSH");
            field(ui, "Nom", &mut e.rule.name);
            egui::ComboBox::from_id_salt("tunnel_host")
                .width(470.0)
                .selected_text(
                    self.store
                        .state
                        .profiles
                        .iter()
                        .find(|p| p.id == e.rule.profile_id)
                        .map(|p| p.name.as_str())
                        .unwrap_or("Choisir un hôte SSH"),
                )
                .show_ui(ui, |ui| {
                    for p in self
                        .store
                        .state
                        .profiles
                        .iter()
                        .filter(|p| p.protocol != Protocol::Rdp)
                    {
                        ui.selectable_value(&mut e.rule.profile_id, p.id.clone(), &p.name);
                    }
                });
            ui.horizontal(|ui| {
                ui.selectable_value(&mut e.rule.kind, TunnelKind::Local, "Local · -L");
                ui.selectable_value(&mut e.rule.kind, TunnelKind::Remote, "Distant · -R");
            });
            if e.rule.kind == TunnelKind::Local {
                muted(ui, "Un port de ce PC sera relié à un service accessible depuis le serveur.");
            } else {
                ui.colored_label(
                    GOLD,
                    "Le serveur décide de l’adresse réellement écoutée. Avec GatewayPorts=yes, ce port peut être accessible sur son réseau. Vérifiez GatewayPorts=no ou clientspecified avant de démarrer.",
                );
            }
            field(ui, "Adresse d’écoute demandée (loopback)", &mut e.rule.local_host);
            field(ui, "Port d’écoute", &mut e.local_port);
            field(ui, "Adresse de destination", &mut e.rule.destination_host);
            field(ui, "Port de destination", &mut e.destination_port);
            if !e.error.is_empty() {
                ui.colored_label(Color32::LIGHT_RED, &e.error);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                save = primary(ui, "Enregistrer").clicked();
                close = ui.button("Annuler").clicked();
            });
        });
        if save {
            let result = (|| {
                e.rule.local_port = e
                    .local_port
                    .parse::<u16>()
                    .map_err(|_| "Port d’écoute invalide.")?;
                e.rule.destination_port = e
                    .destination_port
                    .parse::<u16>()
                    .map_err(|_| "Port de destination invalide.")?;
                e.rule.validate()
            })();
            match result {
                Err(error) => e.error = error,
                Ok(()) => {
                    let old = self.store.state.clone();
                    if let Some(t) = self
                        .store
                        .state
                        .tunnel_rules
                        .iter_mut()
                        .find(|t| t.id == e.rule.id)
                    {
                        *t = e.rule.clone();
                    } else {
                        self.store.state.tunnel_rules.push(e.rule.clone());
                    }
                    if self.save() {
                        close = true;
                    } else {
                        self.store.state = old;
                    }
                }
            }
        }
        if !close {
            self.tunnel_editor = Some(e);
        }
    }
    fn workspace_modal(&mut self, ctx: &egui::Context) {
        let Some(mut e) = self.workspace_editor.take() else {
            return;
        };
        let (mut save, mut close) = (false, false);
        egui::Modal::new(egui::Id::new("workspace_editor")).show(ctx, |ui| {
            ui.set_width(510.0);
            ui.heading("Espace de travail");
            field(ui, "Nom", &mut e.workspace.name);
            muted(ui, "Choisissez les hôtes à ouvrir ensemble.");
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| {
                    for p in &self.store.state.profiles {
                        let mut selected = e.workspace.profile_ids.contains(&p.id);
                        if ui
                            .checkbox(
                                &mut selected,
                                format!("{}  ·  {}", p.name, p.protocol.label()),
                            )
                            .changed()
                        {
                            if selected {
                                e.workspace.profile_ids.push(p.id.clone());
                            } else {
                                e.workspace.profile_ids.retain(|id| id != &p.id);
                            }
                        }
                    }
                });
            if !e.error.is_empty() {
                ui.colored_label(Color32::LIGHT_RED, &e.error);
            }
            ui.horizontal(|ui| {
                save = primary(ui, "Enregistrer").clicked();
                close = ui.button("Annuler").clicked();
            });
        });
        if save {
            match e.workspace.validate() {
                Err(error) => e.error = error,
                Ok(()) => {
                    let old = self.store.state.clone();
                    if let Some(w) = self
                        .store
                        .state
                        .workspaces
                        .iter_mut()
                        .find(|w| w.id == e.workspace.id)
                    {
                        *w = e.workspace.clone();
                    } else {
                        self.store.state.workspaces.push(e.workspace.clone());
                    }
                    if self.save() {
                        close = true;
                    } else {
                        self.store.state = old;
                    }
                }
            }
        }
        if !close {
            self.workspace_editor = Some(e);
        }
    }
    fn entity_delete_modal(&mut self, ctx: &egui::Context) {
        let Some(target) = self.entity_delete.take() else {
            return;
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::new("entity_delete")).show(ctx, |ui| {
            ui.set_width(430.0);
            ui.heading("Supprimer cet élément ?");
            ui.label(match &target {
                EntityDelete::Group(_) => {
                    "Les hôtes seront conservés et déplacés dans le groupe Personnel."
                }
                EntityDelete::Identity(_) => {
                    "Une identité utilisée par un hôte ou un groupe doit d’abord en être détachée."
                }
                _ => "Cet élément sera retiré du coffre local.",
            });
            ui.horizontal(|ui| {
                if ui.button("Annuler").clicked() {
                    answer = Some(false);
                }
                if ui.button("Supprimer").clicked() {
                    answer = Some(true);
                }
            });
        });
        match answer {
            Some(false) => (),
            None => self.entity_delete = Some(target),
            Some(true) => {
                let old = self.store.state.clone();
                match &target {
                    EntityDelete::Identity(id) => {
                        self.store.state.identities.retain(|i| &i.id != id);
                    }
                    EntityDelete::Snippet(id) => self.store.state.snippets.retain(|s| &s.id != id),
                    EntityDelete::Tunnel(id) => {
                        self.store.state.tunnel_rules.retain(|r| &r.id != id);
                    }
                    EntityDelete::Workspace(id) => {
                        self.store.state.workspaces.retain(|w| &w.id != id)
                    }
                    EntityDelete::Group(id) => {
                        if let Some(group) = self.store.state.groups.iter().find(|g| &g.id == id) {
                            let name = group.name.clone();
                            for p in &mut self.store.state.profiles {
                                if p.group == name {
                                    p.group = if name == "Personnel" {
                                        "Sans groupe"
                                    } else {
                                        "Personnel"
                                    }
                                    .into();
                                }
                            }
                        }
                        self.store.state.groups.retain(|g| &g.id != id);
                    }
                }
                if !self.save() {
                    self.store.state = old;
                } else {
                    match target {
                        EntityDelete::Identity(id) => {
                            self.transient.remove(&format!("identity:{id}"));
                        }
                        EntityDelete::Tunnel(id) => {
                            self.active_tunnels.remove(&id);
                        }
                        _ => (),
                    }
                }
            }
        }
    }
    fn command_modal(&mut self, ctx: &egui::Context) {
        let Some(mut run) = self.command_run.take() else {
            return;
        };
        // Session indices can move when another tab closes. Keep immutable tab IDs
        // throughout selection and resolve them only at the moment of execution.
        let candidates: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(_, t)| matches!(&t.view,SessionView::Ssh(s)if s.connected()))
            .map(|(i, t)| {
                (
                    t.id.clone(),
                    format!(
                        "{}  ·  {}@{}:{}  ·  session {}",
                        t.title,
                        t.profile.username,
                        t.profile.host,
                        t.profile.port,
                        i + 1
                    ),
                )
            })
            .collect();
        run.targets
            .retain(|id| candidates.iter().any(|(candidate, _)| candidate == id));
        let (mut execute, mut close) = (false, false);
        egui::Modal::new(egui::Id::new("run_command")).show(ctx, |ui| {
            ui.set_width(550.0);
            ui.heading("Exécuter le snippet");
            ui.label("La commande sera envoyée avec Entrée aux sessions cochées.");
            Frame::new()
                .fill(TOP)
                .corner_radius(6)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.label(RichText::new(&run.command).monospace().color(GREEN));
                });
            if candidates.is_empty() {
                muted(ui, "Aucune session SSH connectée.");
            }
            for (id, title) in &candidates {
                let mut selected = run.targets.contains(id);
                ui.push_id(id, |ui| {
                    if ui.checkbox(&mut selected, title).changed() {
                        if selected {
                            run.targets.push(id.clone());
                        } else {
                            run.targets.retain(|candidate| candidate != id);
                        }
                    }
                });
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                execute = ui
                    .add_enabled(
                        !run.targets.is_empty(),
                        egui::Button::new("Exécuter sur les sessions sélectionnées").fill(GREEN),
                    )
                    .clicked();
                close = ui.button("Annuler").clicked();
            });
        });
        if execute {
            let mut activated = None;
            for id in &run.targets {
                if let Some(index) = self.tabs.iter().position(|tab| &tab.id == id) {
                    if let SessionView::Ssh(session) = &mut self.tabs[index].view {
                        match session.run_command(&run.command) {
                            Ok(()) => {
                                activated.get_or_insert(index);
                            }
                            Err(error) => self.error = Some(error),
                        }
                    }
                }
            }
            if let Some(index) = activated {
                self.active = Some(index);
                self.page = Page::Session;
            }
        } else if !close {
            self.command_run = Some(run);
        }
    }

    pub(super) fn pending_close_modal(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_close.take() else {
            return;
        };
        let (message,confirm)=match &pending{
            PendingClose::App=>("Un ou plusieurs fichiers SFTP ont des modifications non enregistrées. Quitter l’application sans les enregistrer ?".to_string(),"Quitter sans enregistrer"),
            PendingClose::Tab{id,reconnect}=>{
                let Some(tab)=self.tabs.iter().find(|tab|&tab.id==id)else{return};
                (format!("Le fichier ouvert dans « {} » contient des modifications non enregistrées.",tab.title),if *reconnect{"Reconnecter sans enregistrer"}else{"Fermer sans enregistrer"})
            }
        };
        let (mut confirmed, mut cancel) = (false, false);
        let modal = egui::Modal::new(egui::Id::new("unsaved_sftp_close")).show(ctx, |ui| {
            ui.set_width(510.0);
            ui.heading("Modifications non enregistrées");
            ui.add_space(8.0);
            ui.label(message);
            ui.add_space(14.0);
            ui.horizontal_wrapped(|ui| {
                cancel = primary(ui, "Annuler et revenir au fichier").clicked();
                confirmed = ui.button(confirm).clicked();
            });
        });
        if cancel || modal.should_close() {
            let index=match &pending{
                PendingClose::Tab{id,..}=>self.tabs.iter().position(|tab|&tab.id==id),
                PendingClose::App=>self.tabs.iter().position(|tab|matches!(&tab.view,SessionView::Sftp(session)if session.has_unsaved_changes())),
            };
            if let Some(index) = index {
                self.active = Some(index);
                self.secondary = None;
                self.page = Page::Session;
            }
        } else if confirmed {
            match pending {
                PendingClose::Tab { id, reconnect } => self.finish_tab_close(&id, reconnect, ctx),
                PendingClose::App => {
                    self.app_quit_confirmed = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        } else {
            self.pending_close = Some(pending);
        }
    }
}

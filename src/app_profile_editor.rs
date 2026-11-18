use super::*;
impl NexusApp {
    pub(super) fn editor_modal(&mut self, ctx: &egui::Context) {
        let Some(mut e) = self.editor.take() else {
            return;
        };
        let (mut save, mut close) = (false, false);
        egui::Modal::new(egui::Id::new("profile_editor")).show(ctx,|ui|{ui.set_width(540.0);ui.horizontal(|ui|{icon(ui,proto_icon(e.profile.protocol),proto(e.profile.protocol),26.0);ui.heading(if e.is_edit{"Modifier l’hôte"}else{"Nouvel hôte"});});ui.add_space(10.0);
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-185.0).min(625.0)).show(ui,|ui|{
                ui.horizontal(|ui|{for p in[Protocol::Ssh,Protocol::Sftp,Protocol::Rdp]{if ui.selectable_label(e.profile.protocol==p,p.label()).clicked(){if e.port==e.profile.protocol.port().to_string(){e.port=p.port().to_string();}if(e.profile.protocol==Protocol::Rdp)!=(p==Protocol::Rdp){e.secret.zeroize();e.profile.identity_id=None;}e.profile.protocol=p;}}});ui.add_space(10.0);
                field(ui,"Nom de l’hôte",&mut e.profile.name);ui.horizontal(|ui|{ui.vertical(|ui|{ui.set_width(400.0);field(ui,"Adresse IP ou nom d’hôte",&mut e.profile.host);});ui.vertical(|ui|{ui.set_width(105.0);field(ui,"Port",&mut e.port);});});
                ui.horizontal(|ui|{ui.vertical(|ui|{ui.set_width(340.0);field(ui,"Groupe",&mut e.profile.group);});ui.menu_button("Groupes existants",|ui|{let mut names:Vec<_>=self.store.state.groups.iter().map(|g|g.name.clone()).chain(self.store.state.profiles.iter().map(|p|p.group.clone())).collect();names.sort();names.dedup();for name in names{if ui.button(&name).clicked(){e.profile.group=name.clone();self.store.state.apply_group_defaults(&mut e.profile);if let Some(port)=self.store.state.groups.iter().find(|g|g.name==name).and_then(|g|g.default_port){e.port=port.to_string();}ui.close();}}});});
                field(ui,"Étiquettes (séparées par des virgules)",&mut e.tags);
                section(ui,"IDENTIFIANTS");let identities:Vec<_>=self.store.state.identities.iter().filter(|i|e.profile.protocol!=Protocol::Rdp||i.auth==AuthMode::Password).cloned().collect();
                egui::ComboBox::from_id_salt("profile_identity").width(490.0).selected_text(e.profile.identity_id.as_ref().and_then(|id|identities.iter().find(|i|&i.id==id)).map(|i|i.name.as_str()).unwrap_or("Identifiants propres à cet hôte")).show_ui(ui,|ui|{ui.selectable_value(&mut e.profile.identity_id,None,"Identifiants propres à cet hôte");for identity in &identities{ui.selectable_value(&mut e.profile.identity_id,Some(identity.id.clone()),&identity.name);}});ui.add_space(6.0);
                if e.profile.identity_id.is_none(){field(ui,"Utilisateur",&mut e.profile.username);
                    if e.profile.protocol!=Protocol::Rdp{ui.horizontal(|ui|{for(mode,label)in[(AuthMode::Password,"Mot de passe"),(AuthMode::PrivateKey,"Clé SSH")]{if ui.selectable_label(e.profile.auth==mode,label).clicked(){e.profile.auth=mode;e.secret.zeroize();}}});if e.profile.auth==AuthMode::PrivateKey{field(ui,"Clé privée",&mut e.profile.private_key_path);if ui.button("Choisir un fichier de clé…").clicked(){if let Some(path)=rfd::FileDialog::new().set_title("Clé privée SSH").pick_file(){e.profile.private_key_path=path.display().to_string();}}}}
                    muted(ui,if e.profile.protocol!=Protocol::Rdp&&e.profile.auth==AuthMode::PrivateKey{"Phrase secrète (facultative)"}else{"Mot de passe (facultatif)"});ui.add_sized([ui.available_width(),34.0],egui::TextEdit::singleline(&mut *e.secret).password(true));ui.checkbox(&mut e.remember,"Mémoriser le secret sur ce compte Windows");
                }else{muted(ui,"L’utilisateur et l’authentification viennent de l’identité sélectionnée.");}
                egui::CollapsingHeader::new("Paramètres avancés et notes").show(ui, |ui| {
                    if e.profile.protocol == Protocol::Rdp {
                        field(ui, "Domaine Windows (facultatif)", &mut e.profile.domain);
                        ui.checkbox(&mut e.profile.redirect_clipboard, "Partager le presse-papiers");
                        ui.checkbox(
                            &mut e.profile.allow_unverified_rdp_certificate,
                            "Autoriser un certificat RDP non vérifié",
                        );
                        muted(
                            ui,
                            "Permet de poursuivre après l’avertissement Windows si le serveur ne peut pas être authentifié. Vérifiez d’abord son identité : un faux serveur pourrait recevoir vos identifiants.",
                        );
                    } else {
                        field(ui, "Dossier SFTP initial", &mut e.profile.remote_path);
                    }
                    muted(ui, "Notes");
                    ui.add(
                        egui::TextEdit::multiline(&mut e.profile.description)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );
                });
                ui.checkbox(&mut e.profile.favorite, "Ajouter aux favoris");
            });if !e.error.is_empty(){ui.colored_label(Color32::LIGHT_RED,&e.error);}ui.add_space(10.0);ui.horizontal(|ui|{save=primary(ui,"Enregistrer l’hôte").clicked();close=ui.button("Annuler").clicked();});
        });
        if save {
            let result = (|| {
                e.profile.port = e
                    .port
                    .parse::<u16>()
                    .map_err(|_| "Port invalide : entre 1 et 65535.")?;
                e.profile.host = e.profile.host.trim().trim_matches(['[', ']']).into();
                e.profile.name = e.profile.name.trim().into();
                e.profile.username = e.profile.username.trim().into();
                e.profile.group = e.profile.group.trim().into();
                if e.profile.group.is_empty() {
                    e.profile.group = "Personnel".into();
                }
                e.profile.tags = e
                    .tags
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
                let mut seen = std::collections::HashSet::new();
                e.profile.tags.retain(|tag| seen.insert(tag.clone()));
                if e.profile.protocol == Protocol::Rdp {
                    e.profile.auth = AuthMode::Password;
                }
                e.profile.encrypted_secret = if e.profile.identity_id.is_some() || !e.remember {
                    None
                } else {
                    Some(storage::protect(&e.secret)?)
                };
                let resolved = self.store.state.resolved_profile(&e.profile)?;
                if resolved.protocol != Protocol::Rdp
                    && resolved.auth == AuthMode::PrivateKey
                    && !std::path::Path::new(&resolved.private_key_path).is_file()
                {
                    return Err("Le fichier de clé privée est introuvable.".into());
                }
                Ok::<(), String>(())
            })();
            match result {
                Err(error) => e.error = error,
                Ok(()) => {
                    let old = self.store.state.clone();
                    if let Some(p) = self
                        .store
                        .state
                        .profiles
                        .iter_mut()
                        .find(|p| p.id == e.profile.id)
                    {
                        *p = e.profile.clone();
                    } else {
                        self.store.state.profiles.push(e.profile.clone());
                    }
                    if self.save() {
                        if e.profile.identity_id.is_none() && !e.remember && !e.secret.is_empty() {
                            self.transient
                                .insert(e.profile.id.clone(), Zeroizing::new(e.secret.to_string()));
                        } else {
                            self.transient.remove(&e.profile.id);
                        }
                        self.selected = Some(e.profile.id.clone());
                        self.status = "Hôte enregistré.".into();
                        close = true;
                    } else {
                        self.store.state = old;
                    }
                }
            }
        }
        if !close {
            self.editor = Some(e);
        }
    }
}

use crate::ui::{BLUE, CARD, GOLD, Icon, LINE, MUTED, RAISED, SIDEBAR, TEXT, paint_icon};
use crate::{model::Profile, net::TrustBroker};
use eframe::egui;
use ssh2::{ErrorCode, FileStat, OpenFlags, OpenType, RenameFlags, Sftp};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroize;

#[derive(Clone, Debug)]
struct Entry {
    name: String,
    path: String,
    directory: bool,
    link: bool,
    regular: bool,
    actionable: bool,
    size: u64,
}

#[derive(Clone)]
enum Command {
    Local(PathBuf),
    Remote(String),
    Upload {
        source: PathBuf,
        directory: String,
        replace: bool,
    },
    Download {
        source: String,
        target: PathBuf,
        replace: bool,
    },
    Mkdir {
        directory: String,
        name: String,
    },
    Rename {
        source: String,
        directory: String,
        name: String,
    },
    Delete {
        source: String,
    },
    ReadText {
        source: String,
    },
    SaveText {
        source: String,
        original: String,
        text: String,
    },
    Stop,
}

enum Event {
    Connected,
    Local {
        directory: String,
        entries: Vec<Entry>,
    },
    Remote {
        directory: String,
        entries: Vec<Entry>,
    },
    Progress {
        done: u64,
        total: u64,
    },
    Confirm {
        message: String,
        command: Command,
    },
    Finished(Result<String, String>),
    TextLoaded {
        source: String,
        text: String,
    },
    TextSaved {
        source: String,
        text: String,
    },
}

struct Confirm {
    message: String,
    command: Command,
}

enum NameAction {
    NewFolder,
    Rename(String),
}

struct NameDialog {
    action: NameAction,
    name: String,
    error: String,
}

struct RemoteEditor {
    source: String,
    original: String,
    text: String,
    confirm_close: bool,
}
const MAX_EDITOR_BYTES: usize = 1024 * 1024;

enum FileMenuAction {
    OpenLocal(PathBuf),
    OpenRemote(String),
    Upload(PathBuf),
    Download(Entry),
    EditText(String),
    Rename { path: String, name: String },
    Delete { path: String, name: String },
    CopyPath(String),
}

/// All remote I/O belongs to one worker, so the egui frame never waits on SSH.
pub struct SftpSession {
    tx: Sender<Command>,
    rx: Receiver<Event>,
    cancelled: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    status: String,
    connected: bool,
    busy: bool,
    local_directory: String,
    local_input: String,
    remote_directory: String,
    remote_input: String,
    local_entries: Vec<Entry>,
    remote_entries: Vec<Entry>,
    local_selected: Option<usize>,
    remote_selected: Option<usize>,
    progress: Option<(u64, u64)>,
    confirmation: Option<Confirm>,
    name_dialog: Option<NameDialog>,
    id: String,
    filter: String,
    show_hidden: bool,
    editor: Option<RemoteEditor>,
}

impl SftpSession {
    pub fn new(profile: Profile, secret: String, trust: TrustBroker, ctx: egui::Context) -> Self {
        let (tx, commands) = mpsc::channel();
        let (events, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let local = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("C:\\"));
        let remote = if profile.remote_path.trim().is_empty() {
            ".".to_owned()
        } else {
            profile.remote_path.clone()
        };
        let id = uuid::Uuid::new_v4().to_string();
        let (worker_cancel, worker_stop) = (cancelled.clone(), stopped.clone());
        let initial_local = local.clone();
        thread::spawn(move || {
            let (panic_events, panic_ctx) = (events.clone(), ctx.clone());
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                worker(
                    profile,
                    secret,
                    trust,
                    ctx,
                    commands,
                    events,
                    worker_cancel,
                    worker_stop,
                    initial_local,
                )
            }))
            .is_err()
            {
                emit(&panic_events, &panic_ctx, Event::Finished(Err("Session interrompue : réponse serveur ou nom de fichier non pris en charge. Fermez cet onglet puis reconnectez-vous.".into())));
            }
        });
        Self {
            tx,
            rx,
            cancelled,
            stopped,
            status: "Connexion SFTP…".into(),
            connected: false,
            busy: true,
            local_directory: local.to_string_lossy().into_owned(),
            local_input: local.to_string_lossy().into_owned(),
            remote_directory: remote.clone(),
            remote_input: remote,
            local_entries: Vec::new(),
            remote_entries: Vec::new(),
            local_selected: None,
            remote_selected: None,
            progress: None,
            confirmation: None,
            name_dialog: None,
            id,
            filter: String::new(),
            show_hidden: true,
            editor: None,
        }
    }

    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn connected(&self) -> bool {
        self.connected
    }
    pub fn has_modal(&self) -> bool {
        self.confirmation.is_some()
            || self.name_dialog.is_some()
            || self.editor.as_ref().is_some_and(|e| e.confirm_close)
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|editor| editor.text != editor.original)
    }

    fn send(&mut self, command: Command, label: &str) {
        if self.busy {
            return;
        }
        self.cancelled.store(false, Ordering::Release);
        self.busy = true;
        self.progress = None;
        self.status = label.to_owned();
        if self.tx.send(command).is_err() {
            self.busy = false;
            self.connected = false;
            self.status = "Session fermée. Fermez cet onglet puis reconnectez-vous.".into();
        }
    }

    fn poll(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Connected => self.connected = true,
                Event::Local { directory, entries } => {
                    self.local_directory = directory.clone();
                    self.local_input = directory;
                    self.local_entries = entries;
                    self.local_selected = None;
                }
                Event::Remote { directory, entries } => {
                    self.remote_directory = directory.clone();
                    self.remote_input = directory;
                    self.remote_entries = entries;
                    self.remote_selected = None;
                }
                Event::Progress { done, total } => self.progress = Some((done, total)),
                Event::Confirm { message, command } => {
                    self.confirmation = Some(Confirm { message, command })
                }
                Event::Finished(result) => {
                    self.busy = false;
                    self.progress = None;
                    self.status = result.unwrap_or_else(|e| format!("SFTP : {e}"));
                    self.remote_input = self.remote_directory.clone();
                    self.local_input = self.local_directory.clone();
                }
                Event::TextLoaded { source, text } => {
                    self.editor = Some(RemoteEditor {
                        source,
                        original: text.clone(),
                        text,
                        confirm_close: false,
                    })
                }
                Event::TextSaved { source, text } => {
                    if let Some(editor) = &mut self.editor {
                        if editor.source == source {
                            editor.original = text.clone();
                            editor.text = text;
                        }
                    }
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.poll();
        ui.push_id(self.id.clone(), |ui| {
            if self.editor.is_some() { self.editor_ui(ui); return; }
            ui.horizontal(|ui|{ui.label(egui::RichText::new("SFTP").strong().color(crate::ui::PURPLE));ui.label(egui::RichText::new("Fichiers locaux et distants").weak());});
            ui.add_space(8.0);
            let ready = self.connected && !self.busy && self.confirmation.is_none() && self.name_dialog.is_none();
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(ready, egui::Button::new("Envoyer →")).clicked() { self.choose_upload(); }
                if ui.add_enabled(ready, egui::Button::new("← Télécharger")).clicked() { self.choose_download(); }
                let editable=self.remote_selected.and_then(|i|self.remote_entries.get(i)).is_some_and(|e|e.actionable&&e.regular&&!e.link&&e.size<=MAX_EDITOR_BYTES as u64);
                if ui.add_enabled(ready&&editable,egui::Button::new("Modifier le texte")).on_hover_text("Éditeur UTF-8 · fichiers ordinaires de 1 Mo maximum").clicked(){if let Some(entry)=self.remote_selected.and_then(|i|self.remote_entries.get(i)){self.send(Command::ReadText{source:entry.path.clone()},"Lecture du fichier texte…");}}
                if ui.add_enabled(ready, egui::Button::new("Nouveau dossier distant")).clicked() {
                    self.name_dialog = Some(NameDialog { action: NameAction::NewFolder, name: String::new(), error: String::new() });
                }
                let selected_actionable = self.remote_selected.and_then(|i| self.remote_entries.get(i)).is_some_and(|entry| entry.actionable);
                if ui.add_enabled(ready && selected_actionable, egui::Button::new("Renommer")).clicked() {
                    if let Some(entry) = self.remote_selected.and_then(|i| self.remote_entries.get(i)) {
                        self.name_dialog = Some(NameDialog { action: NameAction::Rename(entry.path.clone()), name: entry.name.clone(), error: String::new() });
                    }
                }
                if ui.add_enabled(ready && selected_actionable, egui::Button::new("Supprimer")).clicked() {
                    if let Some(entry) = self.remote_selected.and_then(|i| self.remote_entries.get(i)) {
                        self.confirmation = Some(Confirm {
                            message: format!("Supprimer définitivement « {} » sur le serveur ?\n\nLes dossiers doivent être vides. Aucun effacement récursif.", entry.name),
                            command: Command::Delete { source: entry.path.clone() },
                        });
                    }
                }
                if self.busy && ui.button("Annuler").clicked() {
                    self.cancelled.store(true, Ordering::Release);
                    self.status = "Annulation demandée…".into();
                }
            });
            ui.horizontal(|ui|{ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filtrer les fichiers…").desired_width((ui.available_width()-180.0).max(100.0)));ui.checkbox(&mut self.show_hidden,"Fichiers masqués");});
            ui.add_space(10.0);
            ui.separator();
            let pane_height=(ui.available_height()-if self.progress.is_some(){70.0}else{42.0}).max(180.0);
            ui.columns(2, |columns| {
                egui::Frame::new().fill(SIDEBAR).stroke(egui::Stroke::new(1.0_f32,LINE)).corner_radius(8).inner_margin(10).show(&mut columns[0],|ui|{
                    ui.set_min_height(pane_height-20.0);
                    self.local_pane(ui,pane_height-20.0);
                });
                egui::Frame::new().fill(SIDEBAR).stroke(egui::Stroke::new(1.0_f32,LINE)).corner_radius(8).inner_margin(10).show(&mut columns[1],|ui|{
                    ui.set_min_height(pane_height-20.0);
                    self.remote_pane(ui,pane_height-20.0);
                });
            });
            ui.separator();
            if let Some((done, total)) = self.progress {
                let fraction = if total == 0 { 0.0 } else { (done as f32 / total as f32).min(1.0) };
                ui.add(egui::ProgressBar::new(fraction).text(format!("{} / {}", size_label(done), size_label(total))));
            }
            ui.horizontal_wrapped(|ui| {
                if self.busy { ui.spinner(); }
                ui.label(&self.status);
            });
        });
        self.dialogs(ui.ctx());
    }

    fn editor_ui(&mut self, ui: &mut egui::Ui) {
        let Some(mut editor) = self.editor.take() else {
            return;
        };
        let dirty = editor.text != editor.original;
        let mut close = false;
        let mut save = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(if dirty {
                    "● Fichier modifié"
                } else {
                    "Éditeur distant"
                })
                .strong()
                .color(crate::ui::PURPLE),
            );
            if ui
                .add_enabled(
                    !self.busy && dirty,
                    egui::Button::new("Enregistrer sur le serveur"),
                )
                .clicked()
            {
                save = true;
            }
            if ui
                .add_enabled(!self.busy, egui::Button::new("Revenir aux fichiers"))
                .clicked()
            {
                if dirty {
                    editor.confirm_close = true;
                } else {
                    close = true;
                }
            }
            if self.busy && ui.button("Annuler l’opération").clicked() {
                self.cancelled.store(true, Ordering::Release);
            }
        });
        ui.label(
            egui::RichText::new(&editor.source)
                .monospace()
                .small()
                .weak(),
        );
        ui.separator();
        egui::ScrollArea::both()
            .id_salt(("sftp-editor", &self.id))
            .max_height((ui.available_height() - 64.0).max(100.0))
            .show(ui, |ui| {
                ui.add_enabled(
                    !self.busy,
                    egui::TextEdit::multiline(&mut editor.text)
                        .code_editor()
                        .desired_width(ui.available_width())
                        .desired_rows(28)
                        .lock_focus(true),
                );
            });
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "UTF-8 · {} lignes · {}",
                    editor.text.lines().count(),
                    size_label(editor.text.len() as u64)
                ))
                .small()
                .weak(),
            );
            if self.busy {
                ui.spinner();
            }
            ui.label(&self.status);
        });
        if save {
            if editor.text.len() > MAX_EDITOR_BYTES {
                self.status = "Le texte dépasse la limite de 1 Mo.".into();
            } else {
                self.confirmation = Some(Confirm {
                    message: format!(
                        "Enregistrer vos modifications dans « {} » ?\n\nLa version distante sera comparée à celle qui a été ouverte. Une modification concurrente bloquera l’enregistrement.",
                        editor.source
                    ),
                    command: Command::SaveText {
                        source: editor.source.clone(),
                        original: editor.original.clone(),
                        text: editor.text.clone(),
                    },
                });
            }
        }
        if editor.confirm_close {
            egui::Modal::new(egui::Id::new(("sftp-editor-close",&self.id))).show(ui.ctx(),|ui|{
                ui.set_max_width(430.0);ui.heading("Conserver vos modifications ?");ui.label("Les modifications de ce fichier n’ont pas encore été enregistrées sur le serveur.");
                ui.horizontal(|ui|{if ui.button("Continuer l’édition").clicked(){editor.confirm_close=false;}if ui.button("Abandonner les modifications").clicked(){close=true;}});
            });
        }
        if !close {
            self.editor = Some(editor);
        }
    }

    fn local_pane(&mut self, ui: &mut egui::Ui, pane_height: f32) {
        let top = ui.cursor().top();
        ui.horizontal(|ui| {
            crate::ui::icon(ui, Icon::Monitor, BLUE, 17.0);
            ui.label(egui::RichText::new("Cet ordinateur").size(13.0).strong());
        });
        let mut navigate = None;
        ui.add_enabled_ui(
            !self.busy && self.confirmation.is_none() && self.name_dialog.is_none(),
            |ui| {
                ui.horizontal(|ui| {
                    if ui.button("↑").on_hover_text("Dossier parent").clicked() {
                        navigate = Path::new(&self.local_directory)
                            .parent()
                            .map(Path::to_path_buf);
                    }
                    if ui.button("Parcourir…").clicked() {
                        navigate = rfd::FileDialog::new()
                            .set_directory(&self.local_directory)
                            .pick_folder();
                    }
                    if ui
                        .button("↻")
                        .on_hover_text("Actualiser ce dossier")
                        .clicked()
                    {
                        navigate = Some(PathBuf::from(&self.local_directory));
                    }
                });
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.local_input)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Vec2::new(9.0, 7.0)),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    navigate = Some(PathBuf::from(&self.local_input));
                }
            },
        );
        file_table_header(ui);
        let visible: Vec<usize> = self
            .local_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry_visible(entry, &self.filter, self.show_hidden))
            .map(|(i, _)| i)
            .collect();
        let height = (pane_height - (ui.cursor().top() - top) - 24.0).max(60.0);
        let ready = self.connected
            && !self.busy
            && self.confirmation.is_none()
            && self.name_dialog.is_none();
        let mut menu_action = None;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical()
                .id_salt(("local", &self.id))
                .auto_shrink([false, false])
                .max_height(height)
                .min_scrolled_height(height)
                .show_rows(ui, 34.0, visible.len(), |ui, rows| {
                    for visible_row in rows {
                        let i = visible[visible_row];
                        let entry = &self.local_entries[i];
                        ui.push_id(("local-entry", &entry.path), |ui| {
                            let response = file_row(ui, entry, self.local_selected == Some(i));
                            if response.clicked() || response.secondary_clicked() {
                                self.local_selected = Some(i);
                                ui.ctx().request_repaint();
                            }
                            if response.double_clicked() && ready && entry.directory && !entry.link
                            {
                                navigate = Some(PathBuf::from(&entry.path));
                            }
                            response.on_hover_text(&entry.path).context_menu(|ui| {
                                ui.set_min_width(190.0);
                                if entry.directory
                                    && !entry.link
                                    && ui
                                        .add_enabled(ready, egui::Button::new("Ouvrir le dossier"))
                                        .clicked()
                                {
                                    menu_action =
                                        Some(FileMenuAction::OpenLocal(PathBuf::from(&entry.path)));
                                    ui.close();
                                }
                                if entry.regular
                                    && !entry.link
                                    && ui
                                        .add_enabled(
                                            ready,
                                            egui::Button::new("Envoyer vers le serveur"),
                                        )
                                        .clicked()
                                {
                                    menu_action =
                                        Some(FileMenuAction::Upload(PathBuf::from(&entry.path)));
                                    ui.close();
                                }
                                if ui.button("Copier le chemin").clicked() {
                                    menu_action =
                                        Some(FileMenuAction::CopyPath(entry.path.clone()));
                                    ui.close();
                                }
                            });
                        });
                    }
                });
        });
        if let Some(action) = menu_action {
            match action {
                FileMenuAction::OpenLocal(path) => navigate = Some(path),
                FileMenuAction::Upload(source) => self.upload_file(source),
                FileMenuAction::CopyPath(path) => ui.ctx().copy_text(path),
                _ => unreachable!("local menu action"),
            }
        }
        ui.label(
            egui::RichText::new(format!("{} élément(s)", visible.len()))
                .size(11.0)
                .color(MUTED),
        );
        if let Some(path) = navigate {
            self.send(Command::Local(path), "Lecture du dossier local…");
        }
    }

    fn remote_pane(&mut self, ui: &mut egui::Ui, pane_height: f32) {
        let top = ui.cursor().top();
        ui.horizontal(|ui| {
            crate::ui::icon(ui, Icon::Hosts, BLUE, 17.0);
            ui.label(egui::RichText::new("Serveur distant").size(13.0).strong());
        });
        let mut navigate = None;
        ui.add_enabled_ui(
            self.connected
                && !self.busy
                && self.confirmation.is_none()
                && self.name_dialog.is_none(),
            |ui| {
                ui.horizontal(|ui| {
                    if ui.button("↑").on_hover_text("Dossier parent").clicked() {
                        navigate = Some(remote_parent(&self.remote_directory));
                    }
                    if ui.button("↻ Actualiser").clicked() {
                        navigate = Some(self.remote_directory.clone());
                    }
                });
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.remote_input)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Vec2::new(9.0, 7.0)),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    navigate = Some(self.remote_input.clone());
                }
            },
        );
        file_table_header(ui);
        let visible: Vec<usize> = self
            .remote_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry_visible(entry, &self.filter, self.show_hidden))
            .map(|(i, _)| i)
            .collect();
        let height = (pane_height - (ui.cursor().top() - top) - 24.0).max(60.0);
        let ready = self.connected
            && !self.busy
            && self.confirmation.is_none()
            && self.name_dialog.is_none();
        let mut menu_action = None;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical()
                .id_salt(("remote", &self.id))
                .auto_shrink([false, false])
                .max_height(height)
                .min_scrolled_height(height)
                .show_rows(ui, 34.0, visible.len(), |ui, rows| {
                    for visible_row in rows {
                        let i = visible[visible_row];
                        let entry = &self.remote_entries[i];
                        ui.push_id(("remote-entry", &entry.path), |ui| {
                            let response = file_row(ui, entry, self.remote_selected == Some(i));
                            if response.clicked() || response.secondary_clicked() {
                                self.remote_selected = Some(i);
                                ui.ctx().request_repaint();
                            }
                            if response.double_clicked()
                                && ready
                                && entry.directory
                                && !entry.link
                                && entry.actionable
                            {
                                navigate = Some(entry.path.clone());
                            }
                            response.on_hover_text(if entry.actionable {
                                entry.path.clone()
                            } else {
                                format!(
                                    "{}\nNom non pris en charge pour les opérations sous Windows.",
                                    entry.path
                                )
                            }).context_menu(|ui| {
                                ui.set_min_width(190.0);
                                if entry.directory
                                    && !entry.link
                                    && entry.actionable
                                    && ui.add_enabled(ready, egui::Button::new("Ouvrir le dossier")).clicked()
                                {
                                    menu_action = Some(FileMenuAction::OpenRemote(entry.path.clone()));
                                    ui.close();
                                }
                                if entry.regular && !entry.link && entry.actionable {
                                    if ui.add_enabled(ready && valid_local_filename(&entry.name), egui::Button::new("Télécharger…")).clicked() {
                                        menu_action = Some(FileMenuAction::Download(entry.clone()));
                                        ui.close();
                                    }
                                    if ui.add_enabled(ready && entry.size <= MAX_EDITOR_BYTES as u64, egui::Button::new("Modifier le texte")).clicked() {
                                        menu_action = Some(FileMenuAction::EditText(entry.path.clone()));
                                        ui.close();
                                    }
                                }
                                if entry.actionable {
                                    ui.separator();
                                    if ui.add_enabled(ready, egui::Button::new("Renommer…")).clicked() {
                                        menu_action = Some(FileMenuAction::Rename { path: entry.path.clone(), name: entry.name.clone() });
                                        ui.close();
                                    }
                                    if ui.add_enabled(ready, egui::Button::new("Supprimer…")).clicked() {
                                        menu_action = Some(FileMenuAction::Delete { path: entry.path.clone(), name: entry.name.clone() });
                                        ui.close();
                                    }
                                }
                                ui.separator();
                                if ui.button("Copier le chemin").clicked() {
                                    menu_action = Some(FileMenuAction::CopyPath(entry.path.clone()));
                                    ui.close();
                                }
                            });
                        });
                    }
                });
        });
        if let Some(action) = menu_action {
            match action {
                FileMenuAction::OpenRemote(path) => navigate = Some(path),
                FileMenuAction::Download(entry) => self.download_file(entry),
                FileMenuAction::EditText(source) => {
                    self.send(Command::ReadText { source }, "Lecture du fichier texte…");
                }
                FileMenuAction::Rename { path, name } => {
                    self.name_dialog = Some(NameDialog {
                        action: NameAction::Rename(path),
                        name,
                        error: String::new(),
                    });
                }
                FileMenuAction::Delete { path, name } => {
                    self.confirmation = Some(Confirm {
                        message: format!(
                            "Supprimer définitivement « {name} » sur le serveur ?\n\nLes dossiers doivent être vides. Aucun effacement récursif."
                        ),
                        command: Command::Delete { source: path },
                    });
                }
                FileMenuAction::CopyPath(path) => ui.ctx().copy_text(path),
                _ => unreachable!("remote menu action"),
            }
        }
        ui.label(
            egui::RichText::new(format!("{} élément(s)", visible.len()))
                .size(11.0)
                .color(MUTED),
        );
        if let Some(path) = navigate {
            self.send(Command::Remote(path), "Lecture du dossier distant…");
        }
    }

    fn choose_upload(&mut self) {
        let selected = self
            .local_selected
            .and_then(|i| self.local_entries.get(i))
            .filter(|entry| entry.regular && !entry.link)
            .map(|entry| PathBuf::from(&entry.path));
        let source = selected.or_else(|| {
            rfd::FileDialog::new()
                .set_title("Fichier à envoyer")
                .set_directory(&self.local_directory)
                .pick_file()
        });
        if let Some(source) = source {
            self.upload_file(source);
        }
    }

    fn upload_file(&mut self, source: PathBuf) {
        self.send(
            Command::Upload {
                source,
                directory: self.remote_directory.clone(),
                replace: false,
            },
            "Préparation de l’envoi…",
        );
    }

    fn choose_download(&mut self) {
        let Some(entry) = self
            .remote_selected
            .and_then(|i| self.remote_entries.get(i))
            .cloned()
        else {
            self.status = "Sélectionnez un fichier distant.".into();
            return;
        };
        self.download_file(entry);
    }

    fn download_file(&mut self, entry: Entry) {
        if !entry.regular || entry.link || !entry.actionable {
            self.status = "Choisissez un fichier ordinaire avec un nom pris en charge. Les dossiers et liens ne sont pas transférés automatiquement.".into();
            return;
        }
        if !valid_local_filename(&entry.name) {
            self.status = "Ce nom distant n’est pas utilisable sous Windows. Renommez le fichier sur le serveur.".into();
            return;
        }
        if let Some(target) = rfd::FileDialog::new()
            .set_title("Télécharger le fichier")
            .set_directory(&self.local_directory)
            .set_file_name(&entry.name)
            .save_file()
        {
            self.send(
                Command::Download {
                    source: entry.path,
                    target,
                    replace: false,
                },
                "Préparation du téléchargement…",
            );
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let mut confirmed = None;
        let mut close = false;
        if let Some(pending) = &self.confirmation {
            egui::Modal::new(egui::Id::new(("sftp-confirm", &self.id))).show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.heading("Confirmer l’opération");
                ui.label(&pending.message);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Annuler").clicked() {
                        close = true;
                    }
                    if ui.button("Confirmer").clicked() {
                        confirmed = Some(pending.command.clone());
                        close = true;
                    }
                });
            });
        }
        if close {
            self.confirmation = None;
        }
        if let Some(command) = confirmed {
            self.send(command, "Opération SFTP…");
        }
        let mut name_command = None;
        let mut close_name = false;
        if let Some(dialog) = &mut self.name_dialog {
            egui::Window::new("Nom de l’élément distant").id(egui::Id::new(("sftp-name", &self.id))).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
                ui.set_min_width(340.0);
                ui.add(egui::TextEdit::singleline(&mut dialog.name).desired_width(f32::INFINITY));
                if !dialog.error.is_empty() { ui.colored_label(egui::Color32::LIGHT_RED, &dialog.error); }
                ui.horizontal(|ui| {
                    if ui.button("Annuler").clicked() { close_name = true; }
                    if ui.button("Valider").clicked() {
                        if !valid_remote_name(&dialog.name) { dialog.error = "Utilisez un nom simple, sans / ni \\ ni caractères de contrôle.".into(); }
                        else {
                            name_command = Some(match &dialog.action {
                                NameAction::NewFolder => Command::Mkdir { directory: self.remote_directory.clone(), name: dialog.name.clone() },
                                NameAction::Rename(source) => Command::Rename { source: source.clone(), directory: self.remote_directory.clone(), name: dialog.name.clone() },
                            });
                            close_name = true;
                        }
                    }
                });
            });
        }
        if close_name {
            self.name_dialog = None;
        }
        if let Some(command) = name_command {
            self.send(command, "Opération SFTP…");
        }
    }
}

impl Drop for SftpSession {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        self.cancelled.store(true, Ordering::Release);
        let _ = self.tx.send(Command::Stop);
        // The worker owns the session and closes it after the current bounded I/O completes.
    }
}

fn emit(events: &Sender<Event>, ctx: &egui::Context, event: Event) {
    let _ = events.send(event);
    ctx.request_repaint();
}

#[allow(clippy::too_many_arguments)]
fn worker(
    profile: Profile,
    secret: String,
    trust: TrustBroker,
    ctx: egui::Context,
    commands: Receiver<Command>,
    events: Sender<Event>,
    cancelled: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    initial_local: PathBuf,
) {
    let mut secret = zeroize::Zeroizing::new(secret);
    if let Ok((directory, entries)) = read_local(&initial_local) {
        emit(&events, &ctx, Event::Local { directory, entries });
    }
    let connected = crate::net::connect(&profile, &secret, &trust, &cancelled);
    secret.zeroize();
    let session = match connected {
        Ok(session) => session,
        Err(error) => {
            emit(&events, &ctx, Event::Finished(Err(error)));
            return;
        }
    };
    if stopped.load(Ordering::Acquire) {
        return;
    }
    session.set_timeout(15_000);
    let sftp = match session.sftp() {
        Ok(sftp) => sftp,
        Err(error) => {
            emit(&events, &ctx, Event::Finished(Err(error.to_string())));
            return;
        }
    };
    emit(&events, &ctx, Event::Connected);
    let initial = if profile.remote_path.trim().is_empty() {
        "."
    } else {
        &profile.remote_path
    };
    let result = read_remote(&sftp, initial, &cancelled).map(|(directory, entries)| {
        emit(&events, &ctx, Event::Remote { directory, entries });
        "SFTP connecté. Sélectionnez un fichier à transférer.".into()
    });
    emit(&events, &ctx, Event::Finished(result));
    while !stopped.load(Ordering::Acquire) {
        let command = match commands.recv_timeout(Duration::from_millis(200)) {
            Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Ok(command) => command,
        };
        let result = run_command(&sftp, command, &events, &ctx, &cancelled);
        emit(&events, &ctx, Event::Finished(result));
    }
    drop(sftp);
    let _ = session.disconnect(None, "Session fermée", None);
}

fn run_command(
    sftp: &Sftp,
    command: Command,
    events: &Sender<Event>,
    ctx: &egui::Context,
    cancelled: &AtomicBool,
) -> Result<String, String> {
    check_cancel(cancelled)?;
    // libssh2's Windows wrapper normalizes backslashes. Refuse them before
    // operation dispatch so a Unix filename can never alias a different path.
    let source = match &command {
        Command::Remote(path)
        | Command::Delete { source: path }
        | Command::Download { source: path, .. }
        | Command::ReadText { source: path }
        | Command::SaveText { source: path, .. } => Some(path.as_str()),
        Command::Rename { source, .. } => Some(source.as_str()),
        _ => None,
    };
    if source.is_some_and(|path| !valid_remote_path(path)) {
        return Err("Ce chemin distant n’est pas pris en charge sous Windows.".into());
    }
    if !matches!(&command, Command::Remote(_))
        && source.is_some_and(|path| !valid_remote_item_path(path))
    {
        return Err("Chemin d’élément distant invalide.".into());
    }
    let directory = match &command {
        Command::Upload { directory, .. }
        | Command::Mkdir { directory, .. }
        | Command::Rename { directory, .. } => Some(directory.as_str()),
        _ => None,
    };
    if directory.is_some_and(|path| !valid_remote_path(path)) {
        return Err("Dossier distant invalide.".into());
    }
    match command {
        Command::Local(path) => {
            let (directory, entries) = read_local(&path)?;
            emit(events, ctx, Event::Local { directory, entries });
            Ok("Dossier local actualisé.".into())
        }
        Command::Remote(path) => refresh_remote(sftp, &path, events, ctx, cancelled),
        Command::ReadText { source } => {
            let text = read_remote_text(sftp, &source, cancelled)?;
            emit(events, ctx, Event::TextLoaded { source, text });
            Ok("Fichier texte ouvert. Les modifications restent locales jusqu’à confirmation de l’enregistrement.".into())
        }
        Command::SaveText {
            source,
            original,
            text,
        } => {
            save_remote_text(sftp, &source, &original, &text, cancelled)?;
            emit(
                events,
                ctx,
                Event::TextSaved {
                    source: source.clone(),
                    text,
                },
            );
            let _ = refresh_remote(sftp, &remote_parent(&source), events, ctx, cancelled);
            Ok("Fichier enregistré sur le serveur.".into())
        }
        Command::Upload {
            source,
            directory,
            replace,
        } => {
            let filename = source
                .file_name()
                .and_then(|x| x.to_str())
                .ok_or("Nom de fichier local non pris en charge.")?;
            if !valid_remote_name(filename) {
                return Err("Nom de fichier non pris en charge sur le serveur.".into());
            }
            let target = remote_join(&directory, filename);
            if let Some(stat) = remote_stat(sftp, &target)? {
                if !stat.is_file() {
                    return Err(
                        "La destination est un dossier, un lien ou un fichier spécial.".into(),
                    );
                }
                if !replace {
                    emit(
                        events,
                        ctx,
                        Event::Confirm {
                            message: format!(
                                "Le fichier distant « {filename} » existe déjà. Le remplacer ?"
                            ),
                            command: Command::Upload {
                                source,
                                directory,
                                replace: true,
                            },
                        },
                    );
                    return Ok("Confirmation requise pour remplacer le fichier distant.".into());
                }
            }
            upload(sftp, &source, &target, replace, events, ctx, cancelled)?;
            let _ = refresh_remote(sftp, &directory, events, ctx, cancelled);
            Ok(format!("Envoyé : {filename}"))
        }
        Command::Download {
            source,
            target,
            replace,
        } => {
            match fs::symlink_metadata(&target) {
                Ok(metadata) => {
                    if !metadata.is_file() || local_is_link(&metadata) {
                        return Err("La destination locale est un dossier ou un lien.".into());
                    }
                    if !replace {
                        emit(
                            events,
                            ctx,
                            Event::Confirm {
                                message: format!(
                                    "Le fichier local « {} » existe déjà. Le remplacer ?",
                                    target.display()
                                ),
                                command: Command::Download {
                                    source,
                                    target,
                                    replace: true,
                                },
                            },
                        );
                        return Ok("Confirmation requise pour remplacer le fichier local.".into());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
            download(sftp, &source, &target, replace, events, ctx, cancelled)?;
            if let Some(parent) = target.parent() {
                if let Ok((directory, entries)) = read_local(parent) {
                    emit(events, ctx, Event::Local { directory, entries });
                }
            }
            Ok(format!("Téléchargé : {}", target.display()))
        }
        Command::Mkdir { directory, name } => {
            if !valid_remote_name(&name) {
                return Err("Nom distant invalide.".into());
            }
            let path = remote_join(&directory, &name);
            if remote_stat(sftp, &path)?.is_some() {
                return Err("Ce nom existe déjà.".into());
            }
            sftp.mkdir(Path::new(&path), 0o755)
                .map_err(|e| e.to_string())?;
            let _ = refresh_remote(sftp, &directory, events, ctx, cancelled);
            Ok(format!("Dossier créé : {name}"))
        }
        Command::Rename {
            source,
            directory,
            name,
        } => {
            if !valid_remote_name(&name) {
                return Err("Nom distant invalide.".into());
            }
            let target = remote_join(&directory, &name);
            if remote_stat(sftp, &target)?.is_some() {
                return Err("Ce nom existe déjà. Aucun remplacement effectué.".into());
            }
            sftp.rename(
                Path::new(&source),
                Path::new(&target),
                Some(RenameFlags::empty()),
            )
            .map_err(|e| e.to_string())?;
            let _ = refresh_remote(sftp, &directory, events, ctx, cancelled);
            Ok(format!("Élément renommé : {name}"))
        }
        Command::Delete { source } => {
            let stat = sftp.lstat(Path::new(&source)).map_err(|e| e.to_string())?;
            if stat.is_dir() {
                sftp.rmdir(Path::new(&source))
            } else {
                sftp.unlink(Path::new(&source))
            }
            .map_err(|e| e.to_string())?;
            let _ = refresh_remote(sftp, &remote_parent(&source), events, ctx, cancelled);
            Ok("Élément distant supprimé.".into())
        }
        Command::Stop => Ok("Session fermée.".into()),
    }
}

fn read_local(path: &Path) -> Result<(String, Vec<Entry>), String> {
    let directory = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for item in fs::read_dir(&directory).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        let metadata = match fs::symlink_metadata(item.path()) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        entries.push(Entry {
            name: item.file_name().to_string_lossy().into_owned(),
            path: item.path().to_string_lossy().into_owned(),
            directory: metadata.is_dir(),
            link: local_is_link(&metadata),
            regular: metadata.is_file(),
            actionable: true,
            size: metadata.len(),
        });
    }
    sort_entries(&mut entries);
    let display = directory.to_string_lossy().into_owned();
    // Windows canonical paths use an extended prefix; keep it for I/O, strip only for display
    // where ordinary drive/UNC paths can be reconstructed without losing their meaning.
    let display = if let Some(unc) = display.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{unc}")
    } else {
        display
            .strip_prefix("\\\\?\\")
            .unwrap_or(&display)
            .to_owned()
    };
    Ok((display, entries))
}

fn read_remote(
    sftp: &Sftp,
    path: &str,
    cancelled: &AtomicBool,
) -> Result<(String, Vec<Entry>), String> {
    if !valid_remote_path(path) {
        return Err("Chemin distant invalide.".into());
    }
    check_cancel(cancelled)?;
    let directory = sftp
        .realpath(Path::new(path))
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .into_owned();
    if !valid_remote_path(&directory) {
        return Err("Le chemin résolu n’est pas pris en charge sous Windows.".into());
    }
    let mut handle = sftp
        .opendir(Path::new(&directory))
        .map_err(|e| e.to_string())?;
    // A server can keep returning directory entries indefinitely. Bound both
    // the result held in memory and the number of entries we accept from it.
    const MAX_REMOTE_ENTRIES: usize = 20_000;
    let mut entries = Vec::new();
    let mut seen_entries = 0usize;
    loop {
        check_cancel(cancelled)?;
        match handle.readdir() {
            Ok((name, stat)) => {
                if seen_entries >= MAX_REMOTE_ENTRIES {
                    return Err(
                        "Ce dossier distant contient plus de 20 000 entrées. Ouvrez un dossier plus précis pour éviter de saturer la mémoire."
                            .into(),
                    );
                }
                seen_entries += 1;
                let name = name.to_string_lossy();
                if name.len() > 255 {
                    return Err(
                        "Le serveur a renvoyé un nom de fichier trop long (255 octets maximum)."
                            .into(),
                    );
                }
                if name == "." || name == ".." {
                    continue;
                }
                let name = name.into_owned();
                let actionable = valid_remote_name(&name);
                entries.push(Entry {
                    path: remote_join(&directory, &name),
                    name,
                    directory: stat.is_dir(),
                    link: stat.file_type().is_symlink(),
                    regular: stat.is_file(),
                    actionable,
                    size: stat.size.unwrap_or(0),
                });
            }
            Err(error) if error.code() == ErrorCode::Session(-16) => break, // LIBSSH2_ERROR_FILE: end of directory
            Err(error) => return Err(error.to_string()),
        }
    }
    sort_entries(&mut entries);
    Ok((directory, entries))
}

fn refresh_remote(
    sftp: &Sftp,
    path: &str,
    events: &Sender<Event>,
    ctx: &egui::Context,
    cancelled: &AtomicBool,
) -> Result<String, String> {
    let (directory, entries) = read_remote(sftp, path, cancelled)?;
    let count = entries.len();
    emit(events, ctx, Event::Remote { directory, entries });
    Ok(format!("{count} élément(s) sur le serveur."))
}

fn remote_stat(sftp: &Sftp, path: &str) -> Result<Option<FileStat>, String> {
    match sftp.lstat(Path::new(path)) {
        Ok(stat) => Ok(Some(stat)),
        Err(error) if matches!(error.code(), ErrorCode::SFTP(2) | ErrorCode::SFTP(10)) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn decode_editor_text(bytes: Vec<u8>) -> Result<String, String> {
    if bytes.len() > MAX_EDITOR_BYTES {
        return Err("L’éditeur accepte les fichiers de 1 Mo maximum.".into());
    }
    if bytes.contains(&0) {
        return Err(
            "Ce fichier semble binaire. Téléchargez-le pour l’ouvrir dans une application adaptée."
                .into(),
        );
    }
    String::from_utf8(bytes).map_err(|_| {
        "Ce fichier n’est pas encodé en UTF-8. Téléchargez-le pour conserver son encodage.".into()
    })
}

fn read_remote_text(sftp: &Sftp, source: &str, cancelled: &AtomicBool) -> Result<String, String> {
    check_cancel(cancelled)?;
    if !valid_remote_item_path(source) {
        return Err("Chemin de fichier distant invalide.".into());
    }
    let stat = sftp.lstat(Path::new(source)).map_err(|e| e.to_string())?;
    if !stat.is_file() || stat.file_type().is_symlink() {
        return Err("L’éditeur accepte uniquement les fichiers ordinaires, sans lien.".into());
    }
    if stat.size.unwrap_or(0) > MAX_EDITOR_BYTES as u64 {
        return Err("Le fichier dépasse 1 Mo.".into());
    }
    let mut file = sftp.open(Path::new(source)).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 16384];
    loop {
        check_cancel(cancelled)?;
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        if bytes.len() + count > MAX_EDITOR_BYTES {
            return Err("Le fichier dépasse 1 Mo.".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    decode_editor_text(bytes)
}

fn save_remote_text(
    sftp: &Sftp,
    target: &str,
    original: &str,
    text: &str,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    if text.len() > MAX_EDITOR_BYTES || text.contains('\0') {
        return Err("Texte trop volumineux ou contenant un caractère nul.".into());
    }
    check_cancel(cancelled)?;
    if read_remote_text(sftp, target, cancelled)? != original {
        return Err("Le fichier a changé sur le serveur depuis son ouverture. Aucun remplacement effectué. Conservez vos modifications puis rechargez la version distante.".into());
    }
    let stat = sftp.lstat(Path::new(target)).map_err(|e| e.to_string())?;
    let temporary = remote_join(&remote_parent(target), &unique_name("editor"));
    let backup = remote_join(&remote_parent(target), &unique_name("backup"));
    let result = (|| {
        let mut file = sftp
            .open_mode(
                Path::new(&temporary),
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUSIVE,
                (stat.perm.unwrap_or(0o600) & 0o777) as i32,
                OpenType::File,
            )
            .map_err(|e| e.to_string())?;
        for chunk in text.as_bytes().chunks(16384) {
            check_cancel(cancelled)?;
            file.write_all(chunk).map_err(|e| e.to_string())?;
        }
        file.close().map_err(|e| e.to_string())?;
        check_cancel(cancelled)?;
        // Check again immediately before committing, after staging every new byte.
        if read_remote_text(sftp, target, cancelled)? != original {
            return Err("Conflit : le fichier a changé pendant l’enregistrement. Aucun remplacement effectué.".into());
        }
        sftp.rename(
            Path::new(target),
            Path::new(&backup),
            Some(RenameFlags::empty()),
        )
        .map_err(|e| e.to_string())?;
        if let Err(error) = sftp.rename(
            Path::new(&temporary),
            Path::new(target),
            Some(RenameFlags::empty()),
        ) {
            if sftp
                .rename(
                    Path::new(&backup),
                    Path::new(target),
                    Some(RenameFlags::empty()),
                )
                .is_err()
            {
                return Err(format!(
                    "Enregistrement interrompu ({error}). Original conservé : {backup}"
                ));
            }
            return Err(error.to_string());
        }
        sftp.unlink(Path::new(&backup)).map_err(|e| {
            format!(
                "Nouvelle version enregistrée ; copie de l’original conservée dans {backup} : {e}"
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = sftp.unlink(Path::new(&temporary));
    }
    result
}

fn upload(
    sftp: &Sftp,
    source: &Path,
    target: &str,
    replace: bool,
    events: &Sender<Event>,
    ctx: &egui::Context,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source).map_err(|e| e.to_string())?;
    if !metadata.is_file() || local_is_link(&metadata) {
        return Err("Sélectionnez directement un fichier local ordinaire, sans lien.".into());
    }
    let temporary = remote_join(&remote_parent(target), &unique_name("upload"));
    let result = (|| {
        let mut input = File::open(source).map_err(|e| e.to_string())?;
        let mut output = sftp
            .open_mode(
                Path::new(&temporary),
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUSIVE,
                0o600,
                OpenType::File,
            )
            .map_err(|e| e.to_string())?;
        transfer(
            &mut input,
            &mut output,
            metadata.len(),
            events,
            ctx,
            cancelled,
        )?;
        output.close().map_err(|e| e.to_string())?;
        check_cancel(cancelled)?;
        if let Some(current) = remote_stat(sftp, target)? {
            if !replace {
                return Err(
                    "Un fichier est apparu à destination. Aucun remplacement effectué.".into(),
                );
            }
            if !current.is_file() {
                return Err("La destination a changé de type. Aucun remplacement effectué.".into());
            }
            let backup = remote_join(&remote_parent(target), &unique_name("backup"));
            sftp.rename(
                Path::new(target),
                Path::new(&backup),
                Some(RenameFlags::empty()),
            )
            .map_err(|e| e.to_string())?;
            if let Err(error) = sftp.rename(
                Path::new(&temporary),
                Path::new(target),
                Some(RenameFlags::empty()),
            ) {
                if sftp
                    .rename(
                        Path::new(&backup),
                        Path::new(target),
                        Some(RenameFlags::empty()),
                    )
                    .is_err()
                {
                    return Err(format!(
                        "Remplacement interrompu ({error}). Original conservé : {backup}"
                    ));
                }
                return Err(error.to_string());
            }
            sftp.unlink(Path::new(&backup)).map_err(|e| {
                format!("Fichier envoyé. Ancienne version conservée dans {backup} : {e}")
            })?;
        } else {
            sftp.rename(
                Path::new(&temporary),
                Path::new(target),
                Some(RenameFlags::empty()),
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    let _ = sftp.unlink(Path::new(&temporary));
    result
}

fn download(
    sftp: &Sftp,
    source: &str,
    target: &Path,
    replace: bool,
    events: &Sender<Event>,
    ctx: &egui::Context,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let stat = sftp.lstat(Path::new(source)).map_err(|e| e.to_string())?;
    if !stat.is_file() {
        return Err("Le fichier distant est un lien, un dossier ou un fichier spécial.".into());
    }
    let parent = target.parent().ok_or("Dossier de destination invalide.")?;
    let temporary = parent.join(unique_name("download"));
    let result = (|| {
        let mut input = sftp.open(Path::new(source)).map_err(|e| e.to_string())?;
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        transfer(
            &mut input,
            &mut output,
            stat.size.unwrap_or(0),
            events,
            ctx,
            cancelled,
        )?;
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        check_cancel(cancelled)?;
        install_local(&temporary, target, replace)
    })();
    let _ = fs::remove_file(&temporary);
    result
}

/// Install a completed same-directory file without ever silently replacing a race-created target.
fn install_local(temporary: &Path, target: &Path, replace: bool) -> Result<(), String> {
    let existing = match fs::symlink_metadata(target) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    if let Some(metadata) = existing {
        if !replace {
            return Err("Un fichier est apparu à destination. Aucun remplacement effectué.".into());
        }
        if !metadata.is_file() || local_is_link(&metadata) {
            return Err("La destination locale a changé de type.".into());
        }
        let backup = target
            .parent()
            .ok_or("Dossier local invalide.")?
            .join(unique_name("backup"));
        move_no_replace(target, &backup)?;
        if let Err(error) = move_no_replace(temporary, target) {
            if let Err(restore) = move_no_replace(&backup, target) {
                return Err(format!(
                    "Installation impossible ({error}). Original conservé dans {} ({restore}).",
                    backup.display()
                ));
            }
            return Err(format!(
                "Installation impossible, original restauré : {error}"
            ));
        }
        fs::remove_file(&backup).map_err(|e| {
            format!(
                "Téléchargement terminé. Ancienne version conservée dans {} : {e}",
                backup.display()
            )
        })?;
    } else {
        move_no_replace(temporary, target)
            .map_err(|e| format!("Installation impossible, aucune destination remplacée : {e}"))?;
    }
    Ok(())
}

fn move_no_replace(source: &Path, target: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW},
            core::PCWSTR,
        };
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        // Omitting MOVEFILE_REPLACE_EXISTING makes concurrent destination creation fail.
        unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(target.as_ptr()),
                MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        fs::hard_link(source, target).map_err(|e| e.to_string())?;
        fs::remove_file(source).map_err(|e| e.to_string())
    }
}

fn transfer(
    input: &mut impl Read,
    output: &mut impl Write,
    total: u64,
    events: &Sender<Event>,
    ctx: &egui::Context,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut done = 0_u64;
    let mut last_update = Instant::now();
    loop {
        check_cancel(cancelled)?;
        let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        check_cancel(cancelled)?;
        output
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        done += count as u64;
        if last_update.elapsed() > Duration::from_millis(100) {
            emit(events, ctx, Event::Progress { done, total });
            last_update = Instant::now();
        }
    }
    output.flush().map_err(|e| e.to_string())?;
    emit(events, ctx, Event::Progress { done, total });
    Ok(())
}

fn check_cancel(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Acquire) {
        Err("Opération annulée. Aucun fichier partiel n’a été installé à destination.".into())
    } else {
        Ok(())
    }
}

fn sort_entries(entries: &mut [Entry]) {
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}
fn valid_remote_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control())
}
fn valid_remote_path(path: &str) -> bool {
    !path.is_empty() && path.len() <= 4096 && !path.chars().any(|c| c == '\\' || c.is_control())
}
fn valid_remote_item_path(path: &str) -> bool {
    valid_remote_path(path)
        && path != "/"
        && path
            .split('/')
            .filter(|part| !part.is_empty())
            .all(valid_remote_name)
}
fn valid_local_filename(name: &str) -> bool {
    valid_remote_name(name)
        && !name.ends_with([' ', '.'])
        && !name.chars().any(|c| "<>:\"|?*".contains(c))
        && !matches!(
            name.split('.')
                .next()
                .unwrap_or("")
                .to_ascii_uppercase()
                .as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
}
fn remote_join(directory: &str, name: &str) -> String {
    format!("{}/{}", directory.trim_end_matches('/'), name)
}
fn remote_parent(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit_once('/')
        .map(|(parent, _)| {
            if parent.is_empty() {
                "/".to_owned()
            } else {
                parent.to_owned()
            }
        })
        .unwrap_or_else(|| "/".into())
}
fn unique_name(kind: &str) -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    format!(
        ".nexus-{kind}-{}-{}-{}.part",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}
fn local_is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
fn size_label(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} Go", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} Mo", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} Ko", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} o")
    }
}
fn entry_visible(entry: &Entry, filter: &str, show_hidden: bool) -> bool {
    (show_hidden || !entry.name.starts_with('.'))
        && (filter.is_empty() || entry.name.to_lowercase().contains(&filter.to_lowercase()))
}

fn file_columns(rect: egui::Rect) -> (f32, f32, Option<f32>) {
    let type_x = (rect.width() >= 260.0).then_some(rect.right() - 90.0);
    let size_right = type_x.map(|x| x - 14.0).unwrap_or(rect.right() - 12.0);
    let name_right = (size_right - 75.0).max(rect.left() + 48.0);
    (name_right, size_right, type_x)
}

fn file_table_header(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(
        egui::Vec2::new(ui.available_width(), 31.0),
        egui::Sense::hover(),
    );
    let (_, size_right, type_x) = file_columns(rect);
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, CARD);
    let font = egui::FontId::proportional(11.0);
    painter.text(
        egui::pos2(rect.left() + 12.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "NOM",
        font.clone(),
        MUTED,
    );
    painter.text(
        egui::pos2(size_right, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        "TAILLE",
        font.clone(),
        MUTED,
    );
    if let Some(x) = type_x {
        painter.text(
            egui::pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "TYPE",
            font,
            MUTED,
        );
    }
}

fn file_row(ui: &mut egui::Ui, entry: &Entry, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::Vec2::new(ui.available_width(), 34.0),
        egui::Sense::click(),
    );
    let (name_right, size_right, type_x) = file_columns(rect);
    let painter = ui.painter().with_clip_rect(rect);
    if selected {
        painter.rect_filled(rect, 4.0, BLUE.gamma_multiply(0.18));
    } else if response.hovered() {
        painter.rect_filled(rect, 4.0, RAISED);
    }
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 18.0, rect.center().y),
        egui::Vec2::splat(17.0),
    );
    paint_icon(
        &painter,
        icon_rect,
        if entry.directory && !entry.link {
            Icon::Folder
        } else {
            Icon::Files
        },
        if entry.link || !entry.actionable {
            MUTED
        } else if entry.directory {
            GOLD
        } else {
            BLUE
        },
    );
    let name_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 36.0, rect.top()),
        egui::pos2(name_right, rect.bottom()),
    );
    painter.with_clip_rect(name_rect).text(
        egui::pos2(name_rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        &entry.name,
        egui::FontId::proportional(13.0),
        if ui.is_enabled() && entry.actionable {
            TEXT
        } else {
            MUTED
        },
    );
    painter.text(
        egui::pos2(size_right, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        if entry.directory {
            "—".into()
        } else {
            size_label(entry.size)
        },
        egui::FontId::proportional(12.0),
        MUTED,
    );
    if let Some(x) = type_x {
        painter.text(
            egui::pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            if entry.link {
                "Lien"
            } else if entry.directory {
                "Dossier"
            } else if entry.regular {
                "Fichier"
            } else {
                "Spécial"
            },
            egui::FontId::proportional(12.0),
            MUTED,
        );
    }
    painter.line_segment(
        [
            egui::pos2(rect.left(), rect.bottom()),
            egui::pos2(rect.right(), rect.bottom()),
        ],
        egui::Stroke::new(1.0_f32, LINE.gamma_multiply(0.25)),
    );
    response
}

#[cfg(test)]
pub(crate) fn loopback_transfer_test(session: &ssh2::Session, local: &Path) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let sftp = session.sftp().map_err(|e| e.to_string())?;
    let remote = format!("/roundtrip-{}", uuid::Uuid::new_v4());
    sftp.mkdir(Path::new(&remote), 0o700)
        .map_err(|e| e.to_string())?;
    let source = local.join("source-été.bin");
    let target = remote_join(&remote, "source-été.bin");
    let downloaded = local.join("downloaded.bin");
    let original: Vec<u8> = (0..200_000_u32).map(|i| ((i * 31) % 251) as u8).collect();
    fs::write(&source, &original).map_err(|e| e.to_string())?;
    let (events, _rx) = mpsc::channel();
    let ctx = egui::Context::default();
    let cancel = AtomicBool::new(false);
    upload(&sftp, &source, &target, false, &events, &ctx, &cancel)?;
    let (_, listing) = read_remote(&sftp, &remote, &cancel)?;
    assert!(listing.iter().any(|entry| entry.name == "source-été.bin"
        && entry.size == original.len() as u64
        && entry.regular));
    download(&sftp, &target, &downloaded, false, &events, &ctx, &cancel)?;
    assert_eq!(
        Sha256::digest(fs::read(&downloaded).unwrap()),
        Sha256::digest(&original)
    );

    let replacement = b"replacement bytes - UTF8: \xc3\xa9";
    fs::write(&source, replacement).unwrap();
    assert!(upload(&sftp, &source, &target, false, &events, &ctx, &cancel).is_err());
    let mut preserved = Vec::new();
    sftp.open(Path::new(&target))
        .unwrap()
        .read_to_end(&mut preserved)
        .unwrap();
    assert_eq!(
        preserved, original,
        "unconfirmed replacement must preserve remote bytes"
    );
    upload(&sftp, &source, &target, true, &events, &ctx, &cancel)?;
    download(&sftp, &target, &downloaded, true, &events, &ctx, &cancel)?;
    assert_eq!(fs::read(&downloaded).unwrap(), replacement);
    assert!(
        download(
            &sftp,
            &remote_join(&remote, "fixture-link"),
            &local.join("link.bin"),
            false,
            &events,
            &ctx,
            &cancel
        )
        .is_err()
    );

    run_command(
        &sftp,
        Command::Mkdir {
            directory: remote.clone(),
            name: "empty-folder".into(),
        },
        &events,
        &ctx,
        &cancel,
    )?;
    run_command(
        &sftp,
        Command::Rename {
            source: target.clone(),
            directory: remote.clone(),
            name: "renamed.bin".into(),
        },
        &events,
        &ctx,
        &cancel,
    )?;
    let renamed = remote_join(&remote, "renamed.bin");
    assert!(remote_stat(&sftp, &target)?.is_none());
    assert!(remote_stat(&sftp, &renamed)?.is_some());
    assert!(
        run_command(
            &sftp,
            Command::Delete {
                source: remote.clone()
            },
            &events,
            &ctx,
            &cancel
        )
        .is_err(),
        "nonempty directory deletion must fail"
    );
    cancel.store(true, Ordering::Release);
    let cancelled_path = remote_join(&remote, "cancelled.bin");
    assert!(
        upload(
            &sftp,
            &source,
            &cancelled_path,
            false,
            &events,
            &ctx,
            &cancel
        )
        .is_err()
    );
    assert!(remote_stat(&sftp, &cancelled_path)?.is_none());
    cancel.store(false, Ordering::Release);
    let edited = remote_join(&remote, "configuration-été.txt");
    {
        let mut file = sftp.create(Path::new(&edited)).map_err(|e| e.to_string())?;
        file.write_all("initial=été\n".as_bytes())
            .map_err(|e| e.to_string())?;
        file.close().map_err(|e| e.to_string())?;
    }
    let initial_text = read_remote_text(&sftp, &edited, &cancel)?;
    assert_eq!(initial_text, "initial=été\n");
    save_remote_text(&sftp, &edited, &initial_text, "updated=✓\n", &cancel)?;
    assert_eq!(read_remote_text(&sftp, &edited, &cancel)?, "updated=✓\n");
    {
        let mut file = sftp.create(Path::new(&edited)).map_err(|e| e.to_string())?;
        file.write_all(b"external change")
            .map_err(|e| e.to_string())?;
        file.close().map_err(|e| e.to_string())?;
    }
    assert!(save_remote_text(&sftp, &edited, "updated=✓\n", "do not overwrite", &cancel).is_err());
    assert_eq!(
        read_remote_text(&sftp, &edited, &cancel)?,
        "external change"
    );
    sftp.unlink(Path::new(&edited)).map_err(|e| e.to_string())?;
    assert!(
        !read_remote(&sftp, &remote, &cancel)?
            .1
            .iter()
            .any(|entry| entry.name.starts_with(".nexus-")),
        "transfer staging files must be cleaned up"
    );
    run_command(
        &sftp,
        Command::Delete { source: renamed },
        &events,
        &ctx,
        &cancel,
    )?;
    run_command(
        &sftp,
        Command::Delete {
            source: remote_join(&remote, "empty-folder"),
        },
        &events,
        &ctx,
        &cancel,
    )?;
    run_command(
        &sftp,
        Command::Delete {
            source: remote.clone(),
        },
        &events,
        &ctx,
        &cancel,
    )?;
    assert!(remote_stat(&sftp, &remote)?.is_none());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu_fixture() -> (SftpSession, Receiver<Command>) {
        let (tx, commands) = mpsc::channel();
        let (_events, rx) = mpsc::channel();
        let session = SftpSession {
            tx,
            rx,
            cancelled: Arc::new(AtomicBool::new(false)),
            stopped: Arc::new(AtomicBool::new(false)),
            status: "SFTP connecté".into(),
            connected: true,
            busy: false,
            local_directory: r"C:\fixture".into(),
            local_input: r"C:\fixture".into(),
            remote_directory: "/fixture".into(),
            remote_input: "/fixture".into(),
            local_entries: ["local-menu.txt", "other-local.txt"]
                .into_iter()
                .map(|name| Entry {
                    name: name.into(),
                    path: format!(r"C:\fixture\{name}"),
                    directory: false,
                    link: false,
                    regular: true,
                    actionable: true,
                    size: 12,
                })
                .collect(),
            remote_entries: ["remote-menu.txt", "other-remote.txt"]
                .into_iter()
                .map(|name| Entry {
                    name: name.into(),
                    path: format!("/fixture/{name}"),
                    directory: false,
                    link: false,
                    regular: true,
                    actionable: true,
                    size: 12,
                })
                .collect(),
            local_selected: None,
            remote_selected: None,
            progress: None,
            confirmation: None,
            name_dialog: None,
            id: "sftp-menu-fixture".into(),
            filter: String::new(),
            show_hidden: true,
            editor: None,
        };
        (session, commands)
    }

    fn menu_frame(
        session: &mut SftpSession,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::Vec2::new(1200.0, 800.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| session.ui(ui));
            },
        )
    }

    fn text_position(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + egui::vec2(8.0, text.galley.size().y * 0.5))
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().find_map(|shape| text_position(shape, label))
            }
            _ => None,
        }
    }

    fn has_text(shape: &egui::epaint::Shape, label: &str) -> bool {
        match shape {
            egui::epaint::Shape::Text(text) => text.galley.job.text.contains(label),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().any(|shape| has_text(shape, label)),
            _ => false,
        }
    }

    #[test]
    fn right_click_opens_the_target_file_menu_without_starting_a_transfer() {
        for (target, menu_label, remote) in [
            ("local-menu.txt", "Envoyer vers le serveur", false),
            ("remote-menu.txt", "Télécharger", true),
        ] {
            let (mut session, commands) = menu_fixture();
            let ctx = egui::Context::default();
            let mut output = menu_frame(&mut session, &ctx, Vec::new());
            for _ in 0..2 {
                output = menu_frame(&mut session, &ctx, Vec::new());
            }
            let position = output
                .shapes
                .iter()
                .find_map(|clipped| text_position(&clipped.shape, target))
                .expect("fixture file row must be visible");
            for pressed in [true, false] {
                let _ = menu_frame(
                    &mut session,
                    &ctx,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Secondary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            output = menu_frame(
                &mut session,
                &ctx,
                vec![egui::Event::PointerMoved(position)],
            );
            assert!(
                egui::Popup::is_any_open(&ctx),
                "menu did not open for {target}"
            );
            assert!(
                output
                    .shapes
                    .iter()
                    .any(|clipped| has_text(&clipped.shape, menu_label)),
                "expected {menu_label} in the menu for {target}"
            );
            assert_eq!(session.local_selected, (!remote).then_some(0));
            assert_eq!(session.remote_selected, remote.then_some(0));
            assert!(!session.busy, "right click must not start a transfer");

            // The menu must act on the clicked row even if a selection changes
            // while the popup is open.
            let action_label = if remote {
                "Supprimer…"
            } else {
                "Envoyer vers le serveur"
            };
            let action_position = output
                .shapes
                .iter()
                .find_map(|clipped| text_position(&clipped.shape, action_label))
                .expect("menu action must be visible");
            if remote {
                session.remote_selected = Some(1);
            } else {
                session.local_selected = Some(1);
            }
            for pressed in [true, false] {
                let _ = menu_frame(
                    &mut session,
                    &ctx,
                    vec![
                        egui::Event::PointerMoved(action_position),
                        egui::Event::PointerButton {
                            pos: action_position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            if remote {
                let confirm = session
                    .confirmation
                    .as_ref()
                    .expect("delete needs confirmation");
                assert!(
                    matches!(&confirm.command, Command::Delete { source } if source == "/fixture/remote-menu.txt")
                );
                assert!(
                    commands.try_recv().is_err(),
                    "delete must not run before confirmation"
                );
            } else {
                assert!(
                    matches!(commands.try_recv(), Ok(Command::Upload { source, .. }) if source == PathBuf::from(r"C:\fixture\local-menu.txt"))
                );
            }
        }
    }

    fn sandbox() -> PathBuf {
        let root = std::env::current_dir()
            .expect("cwd")
            .join("target")
            .join("sftp-tests")
            .join(unique_name("test"));
        fs::create_dir_all(&root).expect("create test directory");
        root
    }

    #[test]
    fn local_install_preserves_unapproved_target_and_installs_complete_bytes() {
        let root = sandbox();
        let source = root.join("complete.part");
        let target = root.join("target.txt");
        fs::write(&source, b"completed download").unwrap();
        fs::write(&target, b"original").unwrap();
        assert!(install_local(&source, &target, false).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
        install_local(&source, &target, true).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"completed download");
        assert!(!source.exists());
    }

    #[test]
    fn cancellation_prevents_transfer_writes() {
        let (tx, _rx) = mpsc::channel();
        let ctx = egui::Context::default();
        let cancel = AtomicBool::new(true);
        let mut input = std::io::Cursor::new(b"secret bytes");
        let mut output = Vec::new();
        assert!(transfer(&mut input, &mut output, 12, &tx, &ctx, &cancel).is_err());
        assert!(output.is_empty());
    }
    #[test]
    fn editor_rejects_binary_non_utf8_and_oversized_content() {
        assert_eq!(
            decode_editor_text("été ✓\n".as_bytes().to_vec()).unwrap(),
            "été ✓\n"
        );
        assert!(decode_editor_text(vec![0, 1, 2]).is_err());
        assert!(decode_editor_text(vec![255]).is_err());
        assert!(decode_editor_text(vec![b'a'; MAX_EDITOR_BYTES + 1]).is_err());
    }
    #[test]
    fn hidden_file_and_text_filters_preserve_directory_entries() {
        let mut entry = Entry {
            name: ".config".into(),
            path: "/.config".into(),
            directory: true,
            link: false,
            regular: false,
            actionable: true,
            size: 0,
        };
        assert!(!entry_visible(&entry, "", false));
        assert!(entry_visible(&entry, "CONFIG", true));
        entry.name = "rapport-été.txt".into();
        assert!(entry_visible(&entry, "ÉTÉ", false));
        assert!(!entry_visible(&entry, "missing", true));
    }

    #[test]
    fn traversal_and_windows_device_names_are_rejected() {
        for bad in [".", "..", "../hello", "a/b", "a\\b", "a\n"] {
            assert!(!valid_remote_name(bad), "{bad:?}");
        }
        for bad in [
            "CON",
            "NUL.txt",
            "com1.log",
            "LPT9",
            "file:stream",
            "file.",
            "file ",
        ] {
            assert!(!valid_local_filename(bad), "{bad:?}");
        }
        assert!(valid_remote_name("rapport été.txt"));
        assert!(valid_local_filename("rapport été.txt"));
        assert_eq!(remote_parent("/home/user"), "/home");
        assert_eq!(remote_parent("/home"), "/");
        assert_eq!(remote_join("/", "test"), "/test");
        assert!(!valid_remote_item_path("/parent/../other"));
        assert!(!valid_remote_item_path("/parent/a\\b"));
        assert!(!valid_remote_item_path("/"));
    }
}

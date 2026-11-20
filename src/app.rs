use crate::{
    model::{AuthMode, GroupConfig, Identity, Profile, Protocol, Snippet, Workspace},
    net::{HostKeyRequest, TrustBroker},
    rdp::RdpSession,
    sftp::SftpSession,
    storage::{self, Store},
    terminal::TerminalSession,
    tunnels::{TunnelKind, TunnelRule, TunnelSession},
    ui::{self as design, *},
};
use egui::{Align, Color32, Frame, Margin, RichText, Sense, Stroke, Vec2};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::Duration,
};
use zeroize::{Zeroize, Zeroizing};
#[path = "app_dialogs.rs"]
mod dialogs;
#[path = "app_entities.rs"]
mod entities;
#[cfg(test)]
#[path = "app_layout_tests.rs"]
mod layout_tests;
#[path = "app_palette.rs"]
mod palette;
#[path = "app_profile_editor.rs"]
mod profile_editor;
#[cfg(test)]
#[path = "app_session_input_tests.rs"]
mod session_input_tests;
#[cfg(test)]
#[path = "app_session_layout_tests.rs"]
mod session_layout_tests;
const ACCENT: Color32 = GREEN;
fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    design::label(ui, text);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Hosts,
    Sftp,
    Identities,
    Snippets,
    Tunnels,
    Workspaces,
    KnownHosts,
    Settings,
    Session,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum HostFilter {
    All,
    Favorites,
    Recent,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
    Recent,
    Group,
}
enum SessionView {
    Ssh(TerminalSession),
    Sftp(SftpSession),
    Rdp(RdpSession),
}
struct SessionTab {
    id: String,
    title: String,
    profile: Profile,
    protocol: Protocol,
    view: SessionView,
}
impl SessionTab {
    fn status(&self) -> &str {
        match &self.view {
            SessionView::Ssh(s) => s.status(),
            SessionView::Sftp(s) => s.status(),
            SessionView::Rdp(s) => s.status(),
        }
    }
    fn has_modal(&self) -> bool {
        match &self.view {
            SessionView::Ssh(s) => s.has_modal(),
            SessionView::Sftp(s) => s.has_modal(),
            _ => false,
        }
    }
    fn connected(&self) -> bool {
        match &self.view {
            SessionView::Ssh(session) => session.connected(),
            SessionView::Sftp(session) => session.connected(),
            SessionView::Rdp(session) => session.connected(),
        }
    }
}
struct Editor {
    profile: Profile,
    port: String,
    tags: String,
    secret: Zeroizing<String>,
    remember: bool,
    error: String,
    is_edit: bool,
}
#[derive(Clone)]
enum LaunchTarget {
    Session { title: Option<String> },
    Tunnel { rule: TunnelRule, confirmed: bool },
}
struct TabRename {
    id: String,
    title: String,
    error: String,
    first_focus: bool,
}
struct AuthPrompt {
    profile: Profile,
    secret: Zeroizing<String>,
    first_focus: bool,
    target: LaunchTarget,
}
struct IdentityEditor {
    identity: Identity,
    secret: Zeroizing<String>,
    remember: bool,
    error: String,
}
struct GroupEditor {
    group: GroupConfig,
    port: String,
    error: String,
    old_name: Option<String>,
}
struct SnippetEditor {
    snippet: Snippet,
    error: String,
}
struct TunnelEditor {
    rule: TunnelRule,
    local_port: String,
    destination_port: String,
    error: String,
}
struct WorkspaceEditor {
    workspace: Workspace,
    error: String,
}
enum EntityDelete {
    Identity(String),
    Snippet(String),
    Group(String),
    Tunnel(String),
    Workspace(String),
}
struct CommandRun {
    command: String,
    targets: Vec<String>,
}
enum PendingClose {
    Tab { id: String, reconnect: bool },
    App,
}

pub struct NexusApp {
    store: Store,
    startup_error: Option<String>,
    status: String,
    error: Option<String>,
    page: Page,
    filter: Option<Protocol>,
    host_filter: HostFilter,
    group: Option<String>,
    search: String,
    sort: Sort,
    selected: Option<String>,
    focus_search: bool,
    editor: Option<Editor>,
    auth: Option<AuthPrompt>,
    delete: Option<String>,
    forget: Option<String>,
    identity_editor: Option<IdentityEditor>,
    group_editor: Option<GroupEditor>,
    snippet_editor: Option<SnippetEditor>,
    tunnel_editor: Option<TunnelEditor>,
    remote_tunnel_confirmation: Option<(Profile, TunnelRule)>,
    workspace_editor: Option<WorkspaceEditor>,
    entity_delete: Option<EntityDelete>,
    command_run: Option<CommandRun>,
    pending_close: Option<PendingClose>,
    tab_rename: Option<TabRename>,
    last_clicked_tab: Option<String>,
    app_quit_confirmed: bool,
    tabs: Vec<SessionTab>,
    active: Option<usize>,
    secondary: Option<usize>,
    split_horizontal: bool,
    hwnd: isize,
    trust: TrustBroker,
    trust_rx: Receiver<HostKeyRequest>,
    trust_queue: VecDeque<HostKeyRequest>,
    launch_queue: VecDeque<(Profile, LaunchTarget)>,
    transient: HashMap<String, Zeroizing<String>>,
    active_tunnels: HashMap<String, TunnelSession>,
    palette: bool,
    palette_text: String,
    palette_focus: bool,
    settings_font: f32,
    settings_theme: String,
}
impl NexusApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        design::apply_theme(&cc.egui_ctx);
        let hwnd = cc
            .window_handle()
            .ok()
            .and_then(|h| match h.as_raw() {
                RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
                _ => None,
            })
            .unwrap_or(0);
        let args: Vec<String> = std::env::args().collect();
        let data_dir = args
            .iter()
            .position(|x| x == "--data-dir")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from);
        let loaded = data_dir.map(Store::load_at).unwrap_or_else(Store::load);
        let (store, startup_error) = match loaded {
            Ok(s) => (s, None),
            Err(e) => (Store::empty(), Some(e)),
        };
        Self::from_store(store, startup_error, hwnd)
    }
    fn from_store(store: Store, startup_error: Option<String>, hwnd: isize) -> Self {
        let (tx, rx) = mpsc::channel();
        let settings_font = store.state.preferences.terminal_font_size;
        let settings_theme = store.state.preferences.terminal_theme.clone();
        Self {
            store,
            startup_error,
            status: "Prêt".into(),
            error: None,
            page: Page::Hosts,
            filter: None,
            host_filter: HostFilter::All,
            group: None,
            search: String::new(),
            sort: Sort::Name,
            selected: None,
            focus_search: false,
            editor: None,
            auth: None,
            delete: None,
            forget: None,
            identity_editor: None,
            group_editor: None,
            snippet_editor: None,
            tunnel_editor: None,
            remote_tunnel_confirmation: None,
            workspace_editor: None,
            entity_delete: None,
            command_run: None,
            pending_close: None,
            tab_rename: None,
            last_clicked_tab: None,
            app_quit_confirmed: false,
            tabs: Vec::new(),
            active: None,
            secondary: None,
            split_horizontal: false,
            hwnd,
            trust: TrustBroker { requests: tx },
            trust_rx: rx,
            trust_queue: VecDeque::new(),
            launch_queue: VecDeque::new(),
            transient: HashMap::new(),
            active_tunnels: HashMap::new(),
            palette: false,
            palette_text: String::new(),
            palette_focus: false,
            settings_font,
            settings_theme,
        }
    }
    #[cfg(test)]
    pub fn offline_session_preview(ctx: &egui::Context, rdp: bool, split: bool) -> Self {
        design::apply_theme(ctx);
        let mut app = session_layout_tests::offline_sessions();
        for (tab, title) in app.tabs.iter_mut().zip([
            "Linux · Développement",
            "Linux · Maintenance",
            "Windows · Bureau",
        ]) {
            tab.title = title.into();
            tab.profile.name = title.into();
        }
        app.active = Some(if rdp { 2 } else { 0 });
        app.secondary = if split { Some(2) } else { None };
        app
    }
    fn save(&mut self) -> bool {
        if self.startup_error.is_some() {
            self.error = Some(
                "Le carnet n’a pas pu être chargé. Le fichier existant a été conservé.".into(),
            );
            return false;
        }
        match self.store.save() {
            Ok(()) => true,
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }
    fn set_page(&mut self, page: Page) {
        self.page = page;
        self.search.clear();
    }
    fn has_modal(&self) -> bool {
        self.editor.is_some()
            || self.auth.is_some()
            || self.delete.is_some()
            || self.forget.is_some()
            || self.error.is_some()
            || self.startup_error.is_some()
            || self.identity_editor.is_some()
            || self.group_editor.is_some()
            || self.snippet_editor.is_some()
            || self.tunnel_editor.is_some()
            || self.remote_tunnel_confirmation.is_some()
            || self.workspace_editor.is_some()
            || self.entity_delete.is_some()
            || self.command_run.is_some()
            || self.pending_close.is_some()
            || self.tab_rename.is_some()
            || self.palette
            || !self.trust_queue.is_empty()
    }
    fn pane_modal(&self) -> bool {
        self.tabs.iter().any(SessionTab::has_modal)
    }
    fn poll_hidden_ssh_sessions(&mut self) {
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            let rendered = self.page == Page::Session
                && (self.active == Some(index) || self.secondary == Some(index));
            if !rendered {
                if let SessionView::Ssh(session) = &mut tab.view {
                    session.poll_updates();
                }
            }
        }
    }
    fn new_profile(&mut self, protocol: Protocol) {
        let mut p = Profile {
            protocol,
            port: protocol.port(),
            group: self.group.clone().unwrap_or("Personnel".into()),
            ..Default::default()
        };
        self.store.state.apply_group_defaults(&mut p);
        if protocol == Protocol::Rdp
            && p.identity_id.as_ref().is_some_and(|id| {
                self.store
                    .state
                    .identities
                    .iter()
                    .any(|identity| &identity.id == id && identity.auth == AuthMode::PrivateKey)
            })
        {
            p.identity_id = None;
        }
        if let Some(g) = self.store.state.groups.iter().find(|g| g.name == p.group) {
            if let Some(port) = g.default_port {
                p.port = port;
            }
        }
        self.editor = Some(Editor {
            port: p.port.to_string(),
            profile: p,
            tags: String::new(),
            secret: Zeroizing::new(String::new()),
            remember: false,
            error: String::new(),
            is_edit: false,
        });
    }
    fn edit_profile(&mut self, p: Profile) {
        let secret = match p.encrypted_secret.as_ref().map(|s| storage::unprotect(s)) {
            Some(Ok(s)) => s,
            Some(Err(e)) => {
                self.error = Some(e);
                return;
            }
            None => self
                .transient
                .get(&p.id)
                .map(|s| s.to_string())
                .unwrap_or_default(),
        };
        self.editor = Some(Editor {
            port: p.port.to_string(),
            tags: p.tags.join(", "),
            remember: p.encrypted_secret.is_some(),
            profile: p,
            secret: Zeroizing::new(secret),
            error: String::new(),
            is_edit: true,
        });
    }
    fn connect(&mut self, p: Profile, protocol: Option<Protocol>, _ctx: &egui::Context) {
        self.connect_with_title(p, protocol, None);
    }
    fn connect_with_title(
        &mut self,
        mut p: Profile,
        protocol: Option<Protocol>,
        title: Option<String>,
    ) {
        if let Some(protocol) = protocol {
            p.protocol = protocol;
        }
        self.launch_queue
            .push_back((p, LaunchTarget::Session { title }));
    }
    fn pump_launch(&mut self, ctx: &egui::Context) {
        if self.has_modal() || self.pane_modal() {
            return;
        }
        let Some((original, target)) = self.launch_queue.pop_front() else {
            return;
        };
        if let LaunchTarget::Tunnel {
            rule,
            confirmed: false,
        } = &target
        {
            if rule.kind == TunnelKind::Remote {
                self.remote_tunnel_confirmation = Some((original, rule.clone()));
                ctx.request_repaint();
                return;
            }
        }
        #[cfg(test)]
        if self.hwnd == 0 {
            self.launch_queue.clear();
            return;
        }
        let p = match self.store.state.resolved_profile(&original) {
            Ok(p) => p,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let key = original
            .identity_id
            .as_ref()
            .map(|id| format!("identity:{id}"))
            .unwrap_or_else(|| original.id.clone());
        let cached = p
            .encrypted_secret
            .as_ref()
            .map(|s| storage::unprotect(s))
            .or_else(|| self.transient.get(&key).map(|s| Ok(s.to_string())));
        match cached {
            Some(Ok(secret)) => self.dispatch_launch(p, secret, target, ctx),
            None if p.protocol == Protocol::Rdp => {
                self.dispatch_launch(p, String::new(), target, ctx)
            }
            other => {
                if let Some(Err(e)) = other {
                    self.status = e;
                }
                self.auth = Some(AuthPrompt {
                    profile: p,
                    secret: Zeroizing::new(String::new()),
                    first_focus: true,
                    target,
                });
            }
        }
    }
    fn dispatch_launch(
        &mut self,
        p: Profile,
        secret: String,
        target: LaunchTarget,
        ctx: &egui::Context,
    ) {
        match target {
            LaunchTarget::Session { title } => self.start_session(p, secret, title, ctx),
            LaunchTarget::Tunnel { rule, confirmed } => {
                if rule.kind == TunnelKind::Remote && !confirmed {
                    self.error =
                        Some("Ce tunnel distant doit être confirmé avant chaque démarrage.".into());
                    return;
                }
                let id = rule.id.clone();
                match TunnelSession::new(p, secret, rule, self.trust.clone(), ctx.clone()) {
                    Ok(s) => {
                        self.active_tunnels.insert(id, s);
                        self.status = "Ouverture du tunnel SSH…".into();
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        }
    }
    fn start_session(
        &mut self,
        profile: Profile,
        secret: String,
        title: Option<String>,
        ctx: &egui::Context,
    ) {
        let view = match profile.protocol {
            Protocol::Ssh => {
                let mut s =
                    TerminalSession::new(profile.clone(), secret, self.trust.clone(), ctx.clone());
                s.set_preferences(
                    self.store.state.preferences.terminal_font_size,
                    &self.store.state.preferences.terminal_theme,
                );
                SessionView::Ssh(s)
            }
            Protocol::Sftp => SessionView::Sftp(SftpSession::new(
                profile.clone(),
                secret,
                self.trust.clone(),
                ctx.clone(),
            )),
            Protocol::Rdp => match RdpSession::new(profile.clone(), secret) {
                Ok(mut s) => {
                    s.set_parent_hwnd(self.hwnd);
                    SessionView::Rdp(s)
                }
                Err(e) => {
                    self.error = Some(e);
                    return;
                }
            },
        };
        let original = self
            .store
            .state
            .profiles
            .iter()
            .find(|p| p.id == profile.id)
            .cloned()
            .unwrap_or_else(|| profile.clone());
        self.store.state.mark_used(&profile.id);
        if self.store.state.profiles.iter().any(|p| p.id == profile.id) {
            self.save();
        }
        self.tabs.push(SessionTab {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.unwrap_or_else(|| profile.name.clone()),
            profile: original,
            protocol: profile.protocol,
            view,
        });
        self.active = Some(self.tabs.len() - 1);
        self.page = Page::Session;
    }
    fn has_unsaved_changes(&self) -> bool {
        self.tabs.iter().any(
            |tab| matches!(&tab.view,SessionView::Sftp(session) if session.has_unsaved_changes()),
        )
    }

    fn request_tab_close(&mut self, index: usize, reconnect: bool, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let id = tab.id.clone();
        if matches!(&tab.view,SessionView::Sftp(session) if session.has_unsaved_changes()) {
            self.pending_close = Some(PendingClose::Tab { id, reconnect });
        } else {
            self.finish_tab_close(&id, reconnect, ctx);
        }
    }

    fn finish_tab_close(&mut self, id: &str, reconnect: bool, _ctx: &egui::Context) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let profile = self.tabs[index].profile.clone();
        let protocol = self.tabs[index].protocol;
        let title = self.tabs[index].title.clone();
        self.close_tab(index);
        if reconnect {
            self.connect_with_title(profile, Some(protocol), Some(title));
        }
    }

    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        self.active = match self.active {
            Some(i) if i == index => {
                if self.tabs.is_empty() {
                    None
                } else {
                    Some(index.min(self.tabs.len() - 1))
                }
            }
            Some(i) if i > index => Some(i - 1),
            v => v,
        };
        self.secondary = match self.secondary {
            Some(i) if i == index => None,
            Some(i) if i > index => Some(i - 1),
            v => v,
        };
        if self.active == self.secondary {
            if let Some(right) = self.secondary {
                self.active = (0..self.tabs.len()).find(|candidate| *candidate != right);
                if self.active.is_none() {
                    self.active = Some(right);
                    self.secondary = None;
                }
            }
        }
        if self.active.is_none() {
            self.page = Page::Hosts;
        }
    }
    fn activate_tab(&mut self, index: usize) {
        let Some(tab) = self.tabs.get_mut(index) else {
            return;
        };
        if self.secondary != Some(index) {
            self.active = Some(index);
        }
        self.page = Page::Session;
        if let SessionView::Ssh(session) = &mut tab.view {
            session.request_input_focus();
        }
    }
    fn begin_tab_rename(&mut self, id: &str) {
        if let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) {
            self.tab_rename = Some(TabRename {
                id: id.to_owned(),
                title: tab.title.clone(),
                error: String::new(),
                first_focus: true,
            });
        }
    }
    fn tab_rename_modal(&mut self, ctx: &egui::Context) {
        let Some(mut rename) = self.tab_rename.take() else {
            return;
        };
        let mut save = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("rename_session_tab")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("Renommer l’onglet");
            ui.label("Ce nom appartient à la session ; l’hôte enregistré garde son nom.");
            let response = ui.add(
                egui::TextEdit::singleline(&mut rename.title)
                    .desired_width(f32::INFINITY)
                    .hint_text("Nom de l’onglet"),
            );
            if rename.first_focus {
                response.request_focus();
                rename.first_focus = false;
            }
            if !rename.error.is_empty() {
                ui.colored_label(DANGER, &rename.error);
            }
            ui.horizontal(|ui| {
                cancel = ui.button("Annuler").clicked();
                save = primary(ui, "Enregistrer").clicked();
            });
            save |= ui.input(|input| input.key_pressed(egui::Key::Enter));
            cancel |= ui.input(|input| input.key_pressed(egui::Key::Escape));
        });
        if cancel {
            return;
        }
        if save {
            let title = rename.title.trim();
            if title.is_empty() || title.chars().count() > 80 || title.chars().any(char::is_control)
            {
                rename.error =
                    "Saisissez un nom de 1 à 80 caractères, sans caractère de contrôle.".into();
            } else if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == rename.id) {
                tab.title = title.to_owned();
                return;
            } else {
                return;
            }
        }
        self.tab_rename = Some(rename);
    }
    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("main_header")
            .exact_height(if self.page == Page::Session {
                56.0
            } else {
                64.0
            })
            .frame(
                Frame::new()
                    .fill(TOP)
                    .inner_margin(Margin::symmetric(18, 9)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    Frame::new()
                        .fill(GREEN)
                        .corner_radius(8)
                        .inner_margin(Margin::symmetric(9, 4))
                        .show(ui, |ui| {
                            ui.label(RichText::new("N").size(20.0).strong().color(TOP));
                        });
                    ui.label(RichText::new("nexus").size(20.0).strong());
                    ui.add_space(25.0);
                    for (page, caption) in [
                        (Page::Hosts, "Coffre"),
                        (Page::Sftp, "SFTP"),
                        (Page::Workspaces, "Espaces de travail"),
                    ] {
                        if ui
                            .add(
                                egui::Button::selectable(self.page == page, caption)
                                    .min_size(Vec2::new(0.0, 34.0)),
                            )
                            .clicked()
                        {
                            self.set_page(page);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .button(
                                RichText::new("Rechercher / connexion rapide   Ctrl K")
                                    .color(MUTED),
                            )
                            .clicked()
                        {
                            self.palette = true;
                            self.palette_focus = true;
                        }
                        if icon_button(ui, Icon::Plus, "Nouvelle connexion").clicked() {
                            self.new_profile(Protocol::Ssh);
                        }
                    });
                });
            });
    }
    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("vault_sidebar")
            .exact_width(244.0)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(SIDEBAR)
                    .inner_margin(Margin::symmetric(14, 20)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    Frame::new()
                        .fill(RAISED)
                        .corner_radius(8)
                        .inner_margin(8)
                        .show(ui, |ui| {
                            icon(ui, Icon::Hosts, BLUE, 19.0);
                        });
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Coffre personnel").strong());
                        muted(ui, "Enregistré sur ce PC");
                    });
                });
                ui.add_space(22.0);
                for (page, which, name, count) in [
                    (
                        Page::Hosts,
                        Icon::Hosts,
                        "Hôtes",
                        Some(self.store.state.profiles.len()),
                    ),
                    (
                        Page::Identities,
                        Icon::Key,
                        "Identités",
                        Some(self.store.state.identities.len()),
                    ),
                    (
                        Page::Snippets,
                        Icon::Code,
                        "Snippets",
                        Some(self.store.state.snippets.len()),
                    ),
                    (
                        Page::Tunnels,
                        Icon::Tunnel,
                        "Tunnels SSH",
                        Some(self.store.state.tunnel_rules.len()),
                    ),
                    (Page::KnownHosts, Icon::Check, "Empreintes SSH", None),
                ] {
                    if nav(ui, which, name, count, self.page == page) {
                        self.set_page(page);
                        self.host_filter = HostFilter::All;
                        self.group = None;
                    }
                }
                ui.add_space(15.0);
                section(ui, "ACCÈS RAPIDE");
                for (filter, which, name) in [
                    (HostFilter::Favorites, Icon::Star, "Favoris"),
                    (HostFilter::Recent, Icon::Clock, "Récents"),
                ] {
                    if nav(
                        ui,
                        which,
                        name,
                        None,
                        self.page == Page::Hosts && self.host_filter == filter,
                    ) {
                        self.set_page(Page::Hosts);
                        self.host_filter = filter;
                        self.group = None;
                    }
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    muted(ui, "GROUPES");
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Plus, "Nouveau groupe").clicked() {
                            self.new_group();
                        }
                    });
                });
                let mut groups: Vec<String> = self
                    .store
                    .state
                    .profiles
                    .iter()
                    .map(|p| p.group.clone())
                    .chain(self.store.state.groups.iter().map(|g| g.name.clone()))
                    .collect();
                groups.sort();
                groups.dedup();
                egui::ScrollArea::vertical()
                    .id_salt("group_navigation")
                    .max_height((ui.available_height() - 115.0).max(35.0))
                    .show(ui, |ui| {
                        for group in groups {
                            let count = self
                                .store
                                .state
                                .profiles
                                .iter()
                                .filter(|p| p.group == group)
                                .count();
                            if nav(
                                ui,
                                Icon::Folder,
                                &group,
                                Some(count),
                                self.page == Page::Hosts && self.group.as_ref() == Some(&group),
                            ) {
                                self.set_page(Page::Hosts);
                                self.group = Some(group);
                                self.host_filter = HostFilter::All;
                            }
                        }
                    });
                ui.add_space((ui.available_height() - 85.0).max(8.0));
                if nav(
                    ui,
                    Icon::Settings,
                    "Préférences",
                    None,
                    self.page == Page::Settings,
                ) {
                    self.set_page(Page::Settings);
                }
                ui.add_space(7.0);
                ui.label(
                    RichText::new(format!("Nexus Remote {}", env!("CARGO_PKG_VERSION")))
                        .size(11.0)
                        .color(SUBTLE),
                );
                ui.scope(|ui| {
                    ui.spacing_mut().interact_size.y = 0.0;
                    ui.horizontal(|ui| {
                        let (dot, _) = ui.allocate_exact_size(Vec2::new(8.0, 14.0), Sense::hover());
                        ui.painter().circle_filled(dot.center(), 2.5, GREEN);
                        ui.label(
                            RichText::new("Coffre local · secrets chiffrés")
                                .size(11.0)
                                .color(MUTED),
                        );
                    });
                });
            });
    }
    fn page_heading(&mut self, ui: &mut egui::Ui, title_text: &str, subtitle: &str) {
        muted(ui, format!("Coffre personnel  /  {title_text}"));
        ui.add_space(10.0);
        title(ui, title_text);
        muted(ui, subtitle);
        ui.add_space(18.0);
    }
    fn library(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let heading = if self.page == Page::Sftp {
            "Fichiers SFTP".into()
        } else if let Some(g) = &self.group {
            g.clone()
        } else {
            match self.host_filter {
                HostFilter::All => "Hôtes",
                HostFilter::Favorites => "Favoris",
                HostFilter::Recent => "Récents",
            }
            .into()
        };
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                title(ui, &heading);
                muted(
                    ui,
                    "Vos machines, vos groupes et vos accès, au même endroit.",
                );
                ui.add_space(16.0);
            });
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if primary(ui, "+  Nouvel hôte").clicked() {
                    self.new_profile(if self.page == Page::Sftp {
                        Protocol::Sftp
                    } else {
                        Protocol::Ssh
                    });
                }
                egui::containers::menu::MenuButton::from_button(
                    egui::Button::new("Importer").min_size(Vec2::new(84.0, 40.0)),
                )
                .ui(ui, |ui| {
                    if ui.button("Fichier JSON Nexus").clicked() {
                        self.import();
                        ui.close();
                    }
                    if ui.button("Configuration OpenSSH").clicked() {
                        self.import_ssh();
                        ui.close();
                    }
                    if ui.button("Exporter les hôtes").clicked() {
                        self.export();
                        ui.close();
                    }
                });
            });
        });
        let toolbar_width = ui.available_width();
        let (toolbar, _) = ui.allocate_exact_size(Vec2::new(toolbar_width, 40.0), Sense::hover());
        let toggle_rect = egui::Rect::from_min_size(
            egui::pos2(toolbar.right() - 114.0, toolbar.top() + 2.0),
            Vec2::new(114.0, 36.0),
        );
        let protocol_rect = egui::Rect::from_min_size(
            egui::pos2(toggle_rect.left() - 132.0, toggle_rect.top()),
            Vec2::new(120.0, 36.0),
        );
        let search_rect = egui::Rect::from_min_max(
            egui::pos2(toolbar.left(), toggle_rect.top()),
            egui::pos2(protocol_rect.left() - 12.0, toggle_rect.bottom()),
        );
        let response = ui.put(
            search_rect,
            egui::TextEdit::singleline(&mut self.search)
                .desired_width(search_rect.width())
                .hint_text("Rechercher un hôte, un utilisateur ou une étiquette…")
                .margin(Vec2::new(12.0, 8.0)),
        );
        if self.focus_search {
            response.request_focus();
            self.focus_search = false;
        }
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(protocol_rect)
                .layout(egui::Layout::left_to_right(Align::Center)),
            |ui| {
                ui.spacing_mut().interact_size.y = 36.0;
                egui::ComboBox::from_id_salt("filter_protocol")
                    .width(114.0)
                    .selected_text(self.filter.map(|p| p.label()).unwrap_or("Protocoles"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.filter, None, "Tous les protocoles");
                        for protocol in [Protocol::Ssh, Protocol::Sftp, Protocol::Rdp] {
                            ui.selectable_value(&mut self.filter, Some(protocol), protocol.label());
                        }
                    });
            },
        );
        ui.painter().rect_filled(toggle_rect, 9.0, SUNKEN);
        ui.painter().rect_stroke(
            toggle_rect,
            9.0,
            Stroke::new(1.0_f32, LINE),
            egui::StrokeKind::Inside,
        );
        let previous = self.store.state.preferences.host_view_list;
        ui.scope_builder(egui::UiBuilder::new().max_rect(toggle_rect), |ui| {
            ui.spacing_mut().button_padding = Vec2::new(6.0, 4.0);
            ui.spacing_mut().interact_size.y = 28.0;
            for (index, (list, caption)) in
                [(false, "Grille"), (true, "Liste")].into_iter().enumerate()
            {
                let button_rect = egui::Rect::from_min_size(
                    toggle_rect.min + Vec2::new(3.0 + index as f32 * 55.0, 3.0),
                    Vec2::new(53.0, 30.0),
                );
                if ui
                    .put(
                        button_rect,
                        egui::Button::selectable(
                            self.store.state.preferences.host_view_list == list,
                            RichText::new(caption).size(12.0),
                        )
                        .stroke(Stroke::NONE)
                        .corner_radius(6),
                    )
                    .clicked()
                {
                    self.store.state.preferences.host_view_list = list;
                }
            }
        });
        if previous != self.store.state.preferences.host_view_list {
            self.save();
        }
        ui.add_space(16.0);
        if self.group.is_some() && compact_button(ui, "‹  Tous les groupes").clicked() {
            self.group = None;
        }
        let needle = self.search.to_lowercase();
        let mut profiles: Vec<_> = self
            .store
            .state
            .profiles
            .iter()
            .filter(|p| {
                (self.page != Page::Sftp || p.protocol != Protocol::Rdp)
                    && self.group.as_ref().is_none_or(|g| g == &p.group)
                    && self.filter.is_none_or(|x| x == p.protocol)
                    && (self.host_filter != HostFilter::Favorites || p.favorite)
                    && (self.host_filter != HostFilter::Recent || p.last_used.is_some())
                    && format!(
                        "{} {} {} {} {}",
                        p.name,
                        p.host,
                        p.username,
                        p.group,
                        p.tags.join(" ")
                    )
                    .to_lowercase()
                    .contains(&needle)
            })
            .cloned()
            .collect();
        match self.sort {
            Sort::Name => profiles.sort_by_key(|p| p.name.to_lowercase()),
            Sort::Group => {
                profiles.sort_by_key(|p| (p.group.to_lowercase(), p.name.to_lowercase()))
            }
            Sort::Recent => profiles.sort_by_key(|p| std::cmp::Reverse(p.last_used.unwrap_or(0))),
        }
        if self.host_filter == HostFilter::Recent {
            profiles.sort_by_key(|p| std::cmp::Reverse(p.last_used.unwrap_or(0)));
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.group.is_none() && needle.is_empty() && self.host_filter == HostFilter::All
                {
                    self.group_tiles(ui);
                }
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("HÔTES  ·  {}", profiles.len()))
                            .size(11.0)
                            .color(SUBTLE),
                    );
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("sort_hosts")
                            .width(160.0)
                            .selected_text(match self.sort {
                                Sort::Name => "Nom",
                                Sort::Recent => "Dernière utilisation",
                                Sort::Group => "Groupe",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.sort, Sort::Name, "Nom");
                                ui.selectable_value(
                                    &mut self.sort,
                                    Sort::Recent,
                                    "Dernière utilisation",
                                );
                                ui.selectable_value(&mut self.sort, Sort::Group, "Groupe");
                            });
                    });
                });
                ui.add_space(8.0);
                if profiles.is_empty() {
                    empty(
                        ui,
                        Icon::Hosts,
                        if self.store.state.profiles.is_empty() {
                            "Ajoutez votre premier hôte"
                        } else {
                            "Aucun hôte trouvé"
                        },
                        "SSH, SFTP ou RDP : choisissez une machine pour commencer.",
                    );
                    ui.vertical_centered(|ui| {
                        if primary(ui, "+  Ajouter un hôte").clicked() {
                            self.new_profile(Protocol::Ssh);
                        }
                    });
                    return;
                }
                if self.store.state.preferences.host_view_list {
                    for p in profiles {
                        self.host_row(ui, &p, ctx);
                        ui.add_space(5.0);
                    }
                } else {
                    let available = ui.available_width();
                    let gap = 16.0;
                    let cols = ((available + gap) / (296.0 + gap)).floor().max(1.0) as usize;
                    let card_width = (available - gap * (cols - 1) as f32) / cols as f32;
                    for row in profiles.chunks(cols) {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for profile in row {
                                self.host_tile(ui, profile, card_width, ctx);
                            }
                        });
                        ui.add_space(8.0);
                    }
                }
            });
    }
    fn group_tiles(&mut self, ui: &mut egui::Ui) {
        let mut names: Vec<String> = self
            .store
            .state
            .profiles
            .iter()
            .map(|p| p.group.clone())
            .chain(self.store.state.groups.iter().map(|g| g.name.clone()))
            .collect();
        names.sort();
        names.dedup();
        if names.is_empty() {
            return;
        }
        section(ui, "GROUPES");
        ui.horizontal_wrapped(|ui| {
            for name in names {
                let count = self
                    .store
                    .state
                    .profiles
                    .iter()
                    .filter(|p| p.group == name)
                    .count();
                let config = self
                    .store
                    .state
                    .groups
                    .iter()
                    .find(|g| g.name == name)
                    .cloned();
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(176.0, 56.0), Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &name)
                });
                ui.painter().rect_filled(
                    rect,
                    10.0,
                    if response.hovered() { RAISED } else { CARD },
                );
                ui.painter().rect_stroke(
                    rect,
                    10.0,
                    Stroke::new(
                        1.0_f32,
                        if response.hovered() {
                            tint(GREEN, LINE, 0.4)
                        } else {
                            LINE
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
                paint_icon(
                    ui.painter(),
                    egui::Rect::from_min_size(rect.min + Vec2::new(14.0, 18.0), Vec2::splat(20.0)),
                    Icon::Folder,
                    MUTED,
                );
                let mut job = egui::text::LayoutJob::simple_singleline(
                    name.clone(),
                    egui::FontId::proportional(13.0),
                    TEXT,
                );
                job.wrap = egui::text::TextWrapping::truncate_at_width(114.0);
                let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                ui.painter()
                    .galley(rect.min + Vec2::new(46.0, 11.0), galley, TEXT);
                ui.painter().text(
                    rect.min + Vec2::new(46.0, 32.0),
                    egui::Align2::LEFT_TOP,
                    if count == 1 {
                        "1 hôte".into()
                    } else {
                        format!("{count} hôtes")
                    },
                    egui::FontId::proportional(11.0),
                    SUBTLE,
                );
                let response = response.on_hover_text(&name);
                if response.clicked() {
                    self.group = Some(name.clone());
                    self.selected = None;
                }
                response.context_menu(|ui| {
                    if ui.button("Modifier le groupe").clicked() {
                        let group = config.clone().unwrap_or_else(|| GroupConfig {
                            name: name.clone(),
                            ..Default::default()
                        });
                        self.group_editor = Some(GroupEditor {
                            port: group
                                .default_port
                                .map(|p| p.to_string())
                                .unwrap_or_default(),
                            group,
                            old_name: Some(name.clone()),
                            error: String::new(),
                        });
                        ui.close();
                    }
                    if let Some(group) = &config {
                        if ui.button("Supprimer le groupe").clicked() {
                            self.entity_delete = Some(EntityDelete::Group(group.id.clone()));
                            ui.close();
                        }
                    }
                });
            }
        });
        ui.add_space(8.0);
    }
    fn host_tile(&mut self, ui: &mut egui::Ui, p: &Profile, width: f32, ctx: &egui::Context) {
        let card_response = ui.scope_builder(
            egui::UiBuilder::new().id_salt(&p.id).sense(Sense::click()),
            |ui| {
                let selected = self.selected.as_ref() == Some(&p.id);
                let card = Frame::new()
                    .fill(if selected {
                        tint(GREEN, CARD, 0.045)
                    } else {
                        CARD
                    })
                    .corner_radius(14)
                    .stroke(Stroke::new(
                        1.0_f32,
                        if selected {
                            tint(GREEN, LINE, 0.65)
                        } else {
                            LINE
                        },
                    ))
                    .inner_margin(16);
                let content_width = (width - card.total_margin().sum().x).max(180.0);
                card.show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(content_width);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.horizontal_top(|ui| {
                            Frame::new()
                                .fill(tint(proto(p.protocol), CARD, 0.1))
                                .corner_radius(10)
                                .inner_margin(8)
                                .show(ui, |ui| {
                                    icon(ui, proto_icon(p.protocol), proto(p.protocol), 20.0);
                                });
                            let text_width = (content_width - 96.0).max(75.0);
                            ui.allocate_ui_with_layout(
                                Vec2::new(text_width, 52.0),
                                egui::Layout::top_down(Align::Min),
                                |ui| {
                                    ui.set_min_width(text_width);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&p.name).size(14.0).strong(),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(&p.name);
                                    ui.add_space(5.0);
                                    let endpoint = if p.host.contains(':') {
                                        format!("[{}]:{}", p.host, p.port)
                                    } else {
                                        format!("{}:{}", p.host, p.port)
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&endpoint)
                                                .font(egui::FontId::monospace(11.0))
                                                .color(MUTED),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(endpoint);
                                    ui.add_space(5.0);
                                    let user = p
                                        .identity_id
                                        .as_ref()
                                        .and_then(|id| {
                                            self.store
                                                .state
                                                .identities
                                                .iter()
                                                .find(|identity| &identity.id == id)
                                        })
                                        .map(|identity| identity.username.as_str())
                                        .unwrap_or(&p.username);
                                    let account = if user.is_empty() {
                                        p.group.clone()
                                    } else {
                                        format!("{}  ·  {}", user, p.group)
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&account).size(11.0).color(SUBTLE),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(
                                        if p.description.is_empty() {
                                            account
                                        } else {
                                            format!("{account}\n\n{}", p.description)
                                        },
                                    );
                                },
                            );
                            ui.with_layout(egui::Layout::right_to_left(Align::Min), |ui| {
                                badge(ui, p.protocol.label(), proto(p.protocol));
                            });
                        });
                        ui.add_space(14.0);
                        let (divider, _) =
                            ui.allocate_exact_size(Vec2::new(content_width, 1.0), Sense::hover());
                        ui.painter().line_segment(
                            [divider.left_center(), divider.right_center()],
                            Stroke::new(1.0_f32, LINE),
                        );
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let favorite = icon_button(
                                ui,
                                Icon::Star,
                                if p.favorite {
                                    "Retirer des favoris"
                                } else {
                                    "Ajouter aux favoris"
                                },
                            );
                            if p.favorite {
                                paint_icon(
                                    ui.painter(),
                                    favorite.rect.shrink(8.0),
                                    Icon::Star,
                                    GOLD,
                                );
                            }
                            if favorite.clicked() {
                                let old = self.store.state.clone();
                                if let Some(saved) = self
                                    .store
                                    .state
                                    .profiles
                                    .iter_mut()
                                    .find(|saved| saved.id == p.id)
                                {
                                    saved.favorite = !saved.favorite;
                                }
                                if !self.save() {
                                    self.store.state = old;
                                }
                            }
                            self.host_menu(ui, p);
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                if compact_button(
                                    ui,
                                    if self.page == Page::Sftp {
                                        "Ouvrir SFTP"
                                    } else {
                                        "Ouvrir →"
                                    },
                                )
                                .clicked()
                                {
                                    self.connect(
                                        p.clone(),
                                        if self.page == Page::Sftp {
                                            Some(Protocol::Sftp)
                                        } else {
                                            None
                                        },
                                        ctx,
                                    );
                                }
                                if p.protocol != Protocol::Rdp
                                    && self.page != Page::Sftp
                                    && compact_button(ui, "SFTP").clicked()
                                {
                                    self.connect(p.clone(), Some(Protocol::Sftp), ctx);
                                }
                            });
                        });
                    });
                });
            },
        );
        let response = card_response.response;
        if response.hovered() {
            ui.painter().rect_stroke(
                response.rect,
                14.0,
                Stroke::new(1.0_f32, tint(GREEN, LINE, 0.45)),
                egui::StrokeKind::Inside,
            );
        }
        if response.clicked() {
            self.selected = Some(p.id.clone());
        }
        if response.double_clicked() {
            self.connect(
                p.clone(),
                if self.page == Page::Sftp {
                    Some(Protocol::Sftp)
                } else {
                    None
                },
                ctx,
            );
        }
        response.context_menu(|ui| {
            if ui.button("Modifier").clicked() {
                self.edit_profile(p.clone());
                ui.close();
            }
            if ui.button("Supprimer").clicked() {
                self.delete = Some(p.id.clone());
                ui.close();
            }
        });
    }

    fn host_row(&mut self, ui: &mut egui::Ui, p: &Profile, ctx: &egui::Context) {
        let row_response = ui.scope_builder(
            egui::UiBuilder::new().id_salt(&p.id).sense(Sense::click()),
            |ui| {
                Frame::new()
                    .fill(CARD)
                    .corner_radius(7)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            icon(ui, proto_icon(p.protocol), proto(p.protocol), 25.0);
                            ui.add_space(7.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&p.name).strong());
                                muted(ui, format!("{}@{}:{}", p.username, p.host, p.port));
                            });
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                self.host_menu(ui, p);
                                if ui.button("Ouvrir").clicked() {
                                    self.connect(
                                        p.clone(),
                                        if self.page == Page::Sftp {
                                            Some(Protocol::Sftp)
                                        } else {
                                            None
                                        },
                                        ctx,
                                    );
                                }
                                badge(ui, p.protocol.label(), proto(p.protocol));
                                muted(ui, &p.group);
                            });
                        });
                    });
            },
        );
        let response = row_response.response;
        if response.clicked() {
            self.selected = Some(p.id.clone());
        }
        if response.double_clicked() {
            self.connect(p.clone(), None, ctx);
        }
    }
    fn host_menu(&mut self, ui: &mut egui::Ui, p: &Profile) {
        let response = icon_button(ui, Icon::More, "Actions de l’hôte");
        egui::Popup::menu(&response).show(|ui| {
            if ui.button("Modifier l’hôte").clicked() {
                self.edit_profile(p.clone());
                ui.close();
            }
            if ui.button("Dupliquer").clicked() {
                let mut copy = p.clone();
                copy.id = uuid::Uuid::new_v4().to_string();
                copy.name.push_str(" (copie)");
                self.edit_profile(copy);
                ui.close();
            }
            if ui
                .button(RichText::new("Supprimer").color(Color32::LIGHT_RED))
                .clicked()
            {
                self.delete = Some(p.id.clone());
                ui.close();
            }
        });
    }
    fn inspector(&mut self, ctx: &egui::Context) {
        if !matches!(self.page, Page::Hosts | Page::Sftp) || ctx.content_rect().width() < 1240.0 {
            return;
        }
        let Some(p) = self
            .selected
            .as_ref()
            .and_then(|id| self.store.state.profiles.iter().find(|p| &p.id == id))
            .cloned()
        else {
            return;
        };
        egui::SidePanel::right("host_details")
            .exact_width(282.0)
            .resizable(false)
            .frame(Frame::new().fill(SIDEBAR).inner_margin(20))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    muted(ui, "DÉTAILS DE L’HÔTE");
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, "Fermer les détails").clicked() {
                            self.selected = None;
                        }
                    });
                });
                ui.add_space(23.0);
                icon(ui, proto_icon(p.protocol), proto(p.protocol), 42.0);
                ui.add_space(12.0);
                ui.label(RichText::new(&p.name).size(21.0).strong());
                ui.add_space(5.0);
                muted(ui, &p.host);
                ui.add_space(22.0);
                if primary(ui, "Se connecter").clicked() {
                    self.connect(p.clone(), None, ctx);
                }
                if p.protocol != Protocol::Rdp && ui.button("Parcourir les fichiers SFTP").clicked()
                {
                    self.connect(p.clone(), Some(Protocol::Sftp), ctx);
                }
                ui.add_space(18.0);
                for (name, value) in [
                    ("PROTOCOLE", p.protocol.label().to_string()),
                    ("UTILISATEUR", p.username.clone()),
                    ("PORT", p.port.to_string()),
                    ("GROUPE", p.group.clone()),
                ] {
                    section(ui, name);
                    ui.label(value);
                }
                if !p.description.is_empty() {
                    section(ui, "NOTES");
                    ui.label(&p.description);
                }
                if !p.tags.is_empty() {
                    section(ui, "ÉTIQUETTES");
                    ui.horizontal_wrapped(|ui| {
                        for tag in &p.tags {
                            badge(ui, tag, MUTED);
                        }
                    });
                }
                ui.add_space(20.0);
                if ui.button("Modifier l’hôte").clicked() {
                    self.edit_profile(p);
                }
            });
    }
    fn session_tabs(&mut self, ctx: &egui::Context) {
        if self.tabs.is_empty() {
            return;
        }
        egui::TopBottomPanel::top("sessions")
            .exact_height(48.0)
            .frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(12, 6)))
            .show(ctx, |ui| {
                let available = ui.available_rect_before_wrap();
                let add_rect = egui::Rect::from_min_size(
                    egui::pos2(available.right() - 32.0, available.top() + 2.0),
                    Vec2::splat(32.0),
                );
                let split_width = if self.tabs.len() >= 2 { 116.0 } else { 0.0 };
                let split_rect = egui::Rect::from_min_size(
                    egui::pos2(add_rect.left() - 8.0 - split_width, available.top() + 2.0),
                    Vec2::new(split_width, 32.0),
                );
                let tabs_rect = egui::Rect::from_min_max(
                    available.min,
                    egui::pos2(
                        if split_width > 0.0 {
                            split_rect.left() - 8.0
                        } else {
                            add_rect.left() - 8.0
                        },
                        available.bottom(),
                    ),
                );
                let mut close = None;
                let mut reconnect = None;
                let mut activate = None;
                let mut rename = None;
                let mut clicked_tab = None;
                let mut show_left = None;
                let mut show_right = None;
                let mut unsplit = false;
                ui.scope_builder(egui::UiBuilder::new().max_rect(tabs_rect), |ui| {
                    egui::ScrollArea::horizontal()
                        .id_salt("session_tab_strip")
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                for (index, tab) in self.tabs.iter().enumerate() {
                                    ui.push_id(&tab.id, |ui| {
                                        let primary_tab = self.active == Some(index);
                                        let right_tab = self.secondary == Some(index);
                                        let selected = self.page == Page::Session
                                            && (primary_tab || right_tab);
                                        let title_width = ui
                                            .painter()
                                            .layout_no_wrap(
                                                tab.title.clone(),
                                                egui::FontId::proportional(12.0),
                                                TEXT,
                                            )
                                            .size()
                                            .x;
                                        let width = (title_width + 72.0).clamp(156.0, 236.0);
                                        let (rect, response) = ui.allocate_exact_size(
                                            Vec2::new(width, 36.0),
                                            Sense::click(),
                                        );
                                        response.widget_info(|| {
                                            egui::WidgetInfo::selected(
                                                egui::WidgetType::SelectableLabel,
                                                ui.is_enabled(),
                                                selected,
                                                &tab.title,
                                            )
                                        });
                                        if selected || response.hovered() {
                                            ui.painter().rect_filled(
                                                rect,
                                                8.0,
                                                if selected {
                                                    tint(
                                                        if right_tab { BLUE } else { GREEN },
                                                        CARD,
                                                        0.10,
                                                    )
                                                } else {
                                                    CARD
                                                },
                                            );
                                        }
                                        if selected {
                                            ui.painter().line_segment(
                                                [
                                                    rect.left_bottom() + Vec2::new(10.0, -1.0),
                                                    rect.right_bottom() + Vec2::new(-10.0, -1.0),
                                                ],
                                                Stroke::new(
                                                    2.0_f32,
                                                    if right_tab { BLUE } else { GREEN },
                                                ),
                                            );
                                        }
                                        paint_icon(
                                            ui.painter(),
                                            egui::Rect::from_min_size(
                                                rect.min + Vec2::new(11.0, 10.0),
                                                Vec2::splat(16.0),
                                            ),
                                            proto_icon(tab.protocol),
                                            proto(tab.protocol),
                                        );
                                        let mut job = egui::text::LayoutJob::simple_singleline(
                                            tab.title.clone(),
                                            egui::FontId::proportional(12.0),
                                            if selected { TEXT } else { MUTED },
                                        );
                                        job.wrap = egui::text::TextWrapping::truncate_at_width(
                                            width - 68.0,
                                        );
                                        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                                        ui.painter().galley(
                                            egui::pos2(
                                                rect.left() + 36.0,
                                                rect.center().y - galley.size().y * 0.5,
                                            ),
                                            galley,
                                            TEXT,
                                        );
                                        let close_rect = egui::Rect::from_center_size(
                                            egui::pos2(rect.right() - 18.0, rect.center().y),
                                            Vec2::splat(28.0),
                                        );
                                        let close_response = ui.interact(
                                            close_rect,
                                            response.id.with("close"),
                                            Sense::click(),
                                        );
                                        close_response.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::WidgetType::Button,
                                                ui.is_enabled(),
                                                format!("Fermer {}", tab.title),
                                            )
                                        });
                                        if close_response.hovered() {
                                            ui.painter().rect_filled(close_rect, 6.0, RAISED);
                                        }
                                        paint_icon(
                                            ui.painter(),
                                            close_rect.shrink(8.0),
                                            Icon::Close,
                                            if close_response.hovered() {
                                                TEXT
                                            } else {
                                                SUBTLE
                                            },
                                        );
                                        if close_response.clicked() {
                                            close = Some(index);
                                        }
                                        close_response.on_hover_text("Fermer la session");
                                        if response.clicked() {
                                            if response.double_clicked()
                                                && self.last_clicked_tab.as_deref()
                                                    == Some(tab.id.as_str())
                                            {
                                                rename = Some(tab.id.clone());
                                            } else {
                                                activate = Some(index);
                                            }
                                            clicked_tab = Some(tab.id.clone());
                                        }
                                        response
                                            .on_hover_text(format!(
                                                "{} · {}:{}",
                                                tab.title, tab.profile.host, tab.profile.port
                                            ))
                                            .context_menu(|ui| {
                                                if ui.button("Renommer l’onglet…").clicked() {
                                                    rename = Some(tab.id.clone());
                                                    ui.close();
                                                }
                                                ui.separator();
                                                if !primary_tab
                                                    && ui.button("Afficher à gauche").clicked()
                                                {
                                                    show_left = Some(index);
                                                    ui.close();
                                                }
                                                if !primary_tab
                                                    && ui.button("Afficher à droite").clicked()
                                                {
                                                    show_right = Some(index);
                                                    ui.close();
                                                }
                                                if right_tab
                                                    && ui.button("Retirer du partage").clicked()
                                                {
                                                    unsplit = true;
                                                    ui.close();
                                                }
                                                ui.separator();
                                                if ui.button("Reconnecter").clicked() {
                                                    reconnect = Some(index);
                                                    ui.close();
                                                }
                                                if ui.button("Fermer la session").clicked() {
                                                    close = Some(index);
                                                    ui.close();
                                                }
                                            });
                                    });
                                }
                            });
                        });
                });
                if split_width > 0.0 {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(split_rect), |ui| {
                        let response = ui.add_sized(
                            [split_width, 32.0],
                            egui::Button::selectable(self.secondary.is_some(), "Côte à côte ▾"),
                        );
                        egui::Popup::menu(&response).show(|ui| {
                            ui.set_min_width(220.0);
                            ui.label("Afficher à droite");
                            for (index, tab) in self.tabs.iter().enumerate() {
                                if self.active != Some(index) && ui.button(&tab.title).clicked() {
                                    show_right = Some(index);
                                    ui.close();
                                }
                            }
                            if self.secondary.is_some() {
                                ui.separator();
                                if ui.button("Une seule vue").clicked() {
                                    unsplit = true;
                                    ui.close();
                                }
                            }
                        });
                    });
                }
                ui.scope_builder(egui::UiBuilder::new().max_rect(add_rect), |ui| {
                    if icon_button(ui, Icon::Plus, "Nouvelle session · Ctrl+K").clicked() {
                        self.palette = true;
                        self.palette_focus = true;
                    }
                });
                if ctx.input(|input| input.pointer.any_click()) {
                    self.last_clicked_tab = clicked_tab;
                }
                if let Some(index) = close {
                    self.request_tab_close(index, false, ctx);
                } else if let Some(index) = reconnect {
                    self.request_tab_close(index, true, ctx);
                } else if let Some(id) = rename {
                    self.begin_tab_rename(&id);
                } else if let Some(index) = show_right {
                    if self.active.is_none() {
                        self.active = (0..self.tabs.len()).find(|other| *other != index);
                    }
                    if self.active != Some(index) {
                        self.secondary = Some(index);
                        self.split_horizontal = false;
                        self.activate_tab(index);
                    }
                } else if let Some(index) = show_left {
                    if self.secondary == Some(index) {
                        self.secondary = self.active;
                    }
                    self.activate_tab(index);
                } else if unsplit {
                    self.secondary = None;
                    if let Some(index) = self.active {
                        self.activate_tab(index);
                    }
                } else if let Some(index) = activate {
                    self.activate_tab(index);
                }
            });
    }

    fn session_area(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(active) = self.active.filter(|index| *index < self.tabs.len()) else {
            self.page = Page::Hosts;
            return;
        };
        let other = self
            .secondary
            .filter(|index| *index < self.tabs.len() && *index != active);
        if let Some(other) = other {
            let size = ui.available_size();
            if self.split_horizontal {
                let height = (size.y - 10.0) / 2.0;
                for index in [active, other] {
                    ui.allocate_ui_with_layout(
                        Vec2::new(size.x, height),
                        egui::Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_clip_rect(ui.max_rect());
                            self.render_session(ui, index, ctx);
                        },
                    );
                    if index == active {
                        ui.add_space(2.0);
                    }
                }
            } else {
                ui.columns(2, |columns| {
                    for (pane, column) in columns.iter_mut().enumerate() {
                        column.set_clip_rect(column.max_rect());
                        self.render_session(column, if pane == 0 { active } else { other }, ctx);
                    }
                });
            }
        } else {
            self.render_session(ui, active, ctx);
        }
    }

    fn session_pane_header(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        ctx: &egui::Context,
    ) -> bool {
        let Some(tab) = self.tabs.get(index) else {
            return false;
        };
        let id = tab.id.clone();
        let profile = tab.profile.clone();
        let protocol = tab.protocol;
        let status = tab.status().to_owned();
        let connected = tab.connected();
        let endpoint = if profile.host.contains(':') {
            format!("[{}]:{}", profile.host, profile.port)
        } else {
            format!("{}:{}", profile.host, profile.port)
        };
        let title = if self.secondary.is_some() {
            tab.title.clone()
        } else {
            format!("{} · {}", tab.title, endpoint)
        };
        let width = ui.available_width();
        let control_width = match protocol {
            Protocol::Ssh => {
                if width >= 900.0 {
                    160.0
                } else {
                    72.0
                }
            }
            Protocol::Rdp => {
                if width >= 900.0 {
                    240.0
                } else {
                    120.0
                }
            }
            Protocol::Sftp => 0.0,
        };
        let switch_width = if protocol != Protocol::Rdp {
            if width >= 900.0 { 112.0 } else { 32.0 }
        } else {
            0.0
        };
        let (bar, _) = ui.allocate_exact_size(Vec2::new(width, 36.0), Sense::hover());
        let session_menu_rect = egui::Rect::from_min_size(
            egui::pos2(bar.right() - 76.0, bar.top() + 2.0),
            Vec2::new(76.0, 32.0),
        );
        let switch_right = session_menu_rect.left() - 8.0;
        let switch_rect = egui::Rect::from_min_size(
            egui::pos2(switch_right - switch_width, bar.top() + 2.0),
            Vec2::new(switch_width, 32.0),
        );
        let controls_right = if switch_width > 0.0 {
            switch_rect.left() - 8.0
        } else {
            switch_right
        };
        let controls_rect = egui::Rect::from_min_size(
            egui::pos2(controls_right - control_width, bar.top() + 2.0),
            Vec2::new(control_width, 32.0),
        );
        let info_rect = egui::Rect::from_min_max(
            bar.min,
            egui::pos2(controls_rect.left() - 12.0, bar.bottom()),
        );
        paint_icon(
            ui.painter(),
            egui::Rect::from_min_size(info_rect.min + Vec2::new(2.0, 8.0), Vec2::splat(20.0)),
            proto_icon(protocol),
            proto(protocol),
        );
        let text_rect =
            egui::Rect::from_min_max(info_rect.min + Vec2::new(32.0, 0.0), info_rect.max);
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(text_rect)
                .layout(egui::Layout::top_down(Align::Min)),
            |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.add(egui::Label::new(RichText::new(&title).size(12.0).strong()).truncate())
                    .on_hover_text(endpoint);
                ui.add(
                    egui::Label::new(RichText::new(&status).size(10.0).color(if connected {
                        GREEN
                    } else {
                        MUTED
                    }))
                    .truncate(),
                )
                .on_hover_text(&status);
            },
        );
        if control_width > 0.0 {
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(controls_rect)
                    .layout(egui::Layout::left_to_right(Align::Center)),
                |ui| match &mut self.tabs[index].view {
                    SessionView::Ssh(session) => session.controls(ui),
                    SessionView::Rdp(session) => session.controls(ui),
                    SessionView::Sftp(_) => {}
                },
            );
        }
        let mut switch = false;
        if switch_width > 0.0 {
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(switch_rect)
                    .layout(egui::Layout::right_to_left(Align::Center)),
                |ui| {
                    let (which, label) = if protocol == Protocol::Ssh {
                        (Icon::Files, "Fichiers SFTP")
                    } else {
                        (Icon::Terminal, "Terminal SSH")
                    };
                    switch = if switch_width > 32.0 {
                        compact_button(ui, label).clicked()
                    } else {
                        icon_button(ui, which, label).clicked()
                    };
                },
            );
        }
        let mut reconnect = false;
        let mut close = false;
        let mut snippets = false;
        let mut workspace = false;
        let mut rename = false;
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(session_menu_rect)
                .layout(egui::Layout::right_to_left(Align::Center)),
            |ui| {
                let response = compact_button(ui, "Session");
                egui::Popup::menu(&response).show(|ui| {
                    ui.set_min_width(210.0);
                    if protocol != Protocol::Rdp && ui.button("Reconnecter").clicked() {
                        reconnect = true;
                        ui.close();
                    }
                    if protocol != Protocol::Rdp && ui.button("Bibliothèque de snippets").clicked()
                    {
                        snippets = true;
                        ui.close();
                    }
                    if ui.button("Enregistrer l’espace de travail…").clicked() {
                        workspace = true;
                        ui.close();
                    }
                    if ui.button("Renommer l’onglet…").clicked() {
                        rename = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Une seule vue").clicked() {
                        self.secondary = None;
                        ui.close();
                    }
                    ui.menu_button("Partager avec…", |ui| {
                        for (other, tab) in self.tabs.iter().enumerate() {
                            if other != index && ui.button(&tab.title).clicked() {
                                self.active = Some(index);
                                self.secondary = Some(other);
                                ui.close();
                            }
                        }
                    });
                    if self.secondary.is_some() {
                        ui.checkbox(&mut self.split_horizontal, "Empiler les vues verticalement");
                    }
                    ui.separator();
                    if ui
                        .button(RichText::new("Fermer cette session").color(DANGER))
                        .clicked()
                    {
                        close = true;
                        ui.close();
                    }
                });
            },
        );
        if reconnect || close {
            self.request_tab_close(index, reconnect, ctx);
            return false;
        }
        if switch {
            self.connect(
                profile,
                Some(if protocol == Protocol::Ssh {
                    Protocol::Sftp
                } else {
                    Protocol::Ssh
                }),
                ctx,
            );
        }
        if snippets {
            self.page = Page::Snippets;
        }
        if workspace {
            self.new_workspace_from_tabs();
        }
        if rename {
            self.begin_tab_rename(&id);
        }
        self.page == Page::Session && self.tabs.get(index).is_some_and(|tab| tab.id == id)
    }

    fn render_session(&mut self, ui: &mut egui::Ui, index: usize, ctx: &egui::Context) {
        if index >= self.tabs.len() {
            return;
        }
        let id = self.tabs[index].id.clone();
        ui.push_id(&id, |ui| {
            let frame = Frame::new()
                .fill(CARD)
                .stroke(Stroke::new(1.0_f32, LINE))
                .corner_radius(12)
                .inner_margin(8);
            let content_size =
                (ui.available_size() - frame.total_margin().sum()).max(Vec2::splat(1.0));
            frame.show(ui, |ui| {
                ui.set_min_size(content_size);
                let enabled = !self.has_modal() && !self.pane_modal();
                let keep_rendering = ui
                    .add_enabled_ui(enabled, |ui| self.session_pane_header(ui, index, ctx))
                    .inner;
                if !keep_rendering {
                    return;
                }
                ui.add_space(6.0);
                let enabled =
                    !self.has_modal() && !self.pane_modal() && !egui::Popup::is_any_open(ui.ctx());
                let Some(tab) = self.tabs.get_mut(index) else {
                    return;
                };
                ui.add_enabled_ui(enabled, |ui| match &mut tab.view {
                    SessionView::Ssh(session) => session.ui(ui),
                    SessionView::Sftp(session) => session.ui(ui),
                    SessionView::Rdp(session) => {
                        session.set_visible(enabled);
                        session.ui(ui);
                    }
                });
            });
        });
    }

    fn export(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("Nexus-hotes.json")
            .add_filter("JSON", &["json"])
            .save_file()
        {
            match self.store.export(&path) {
                Ok(()) => self.status = "Hôtes exportés sans secrets.".into(),
                Err(e) => self.error = Some(e),
            }
        }
    }
    fn import(&mut self) {
        if self.startup_error.is_some() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
        {
            match self.store.import(&path) {
                Ok(n) => self.status = format!("{n} hôte(s) importé(s)."),
                Err(e) => self.error = Some(e),
            }
        }
    }
    fn import_ssh(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Importer une configuration OpenSSH")
            .pick_file()
        {
            let result = (|| {
                if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 5_000_000 {
                    return Err("Fichier trop volumineux.".into());
                }
                let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                crate::ssh_config::parse(
                    &text,
                    &PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default()),
                )
            })();
            match result {
                Ok(profiles) => {
                    let old = self.store.state.clone();
                    let count = profiles.len();
                    self.store.state.profiles.extend(profiles);
                    if self.save() {
                        self.status = format!("{count} hôte(s) importé(s) depuis OpenSSH.");
                    } else {
                        self.store.state = old;
                    }
                }
                Err(e) => self.error = Some(e),
            }
        }
    }
}
impl eframe::App for NexusApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|input| input.viewport().close_requested())
            && !self.app_quit_confirmed
            && self.has_unsaved_changes()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.pending_close.is_none() {
                self.pending_close = Some(PendingClose::App);
            }
        }
        self.trust_requests();
        self.poll_hidden_ssh_sessions();
        for tunnel in self.active_tunnels.values_mut() {
            tunnel.poll();
        }
        let modal = self.has_modal() || self.pane_modal();
        if !modal && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::K)) {
            self.palette = true;
            self.palette_focus = true;
        }
        if !modal && self.page != Page::Session {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
                self.new_profile(Protocol::Ssh);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F)) {
                self.focus_search = true;
            }
        }
        self.header(ctx);
        if self.page != Page::Session {
            egui::TopBottomPanel::bottom("status_bar")
                .exact_height(27.0)
                .frame(
                    Frame::new()
                        .fill(TOP)
                        .inner_margin(Margin::symmetric(16, 5)),
                )
                .show(ctx, |ui| {
                    ui.spacing_mut().interact_size.y = 0.0;
                    ui.horizontal(|ui| {
                        let text = self
                            .active
                            .and_then(|i| self.tabs.get(i))
                            .filter(|_| self.page == Page::Session)
                            .map(|t| t.status())
                            .unwrap_or(&self.status);
                        ui.add(
                            egui::Label::new(RichText::new(text).size(11.0).color(MUTED))
                                .truncate(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            muted(
                                ui,
                                format!(
                                    "{} session(s)   ·   {} tunnel(s)",
                                    self.tabs.len(),
                                    self.active_tunnels
                                        .values()
                                        .filter(|s| s.is_running())
                                        .count()
                                ),
                            );
                        });
                    });
                });
        }
        if self.page != Page::Session {
            self.sidebar(ctx);
            self.inspector(ctx);
        }
        self.session_tabs(ctx);
        // Apply tab activation before hiding native children: a focused RDP
        // descendant must yield the Windows keyboard focus in this same frame.
        let modal = self.has_modal() || self.pane_modal();
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            if let SessionView::Rdp(session) = &mut tab.view {
                session.set_visible(
                    self.page == Page::Session
                        && (self.active == Some(index) || self.secondary == Some(index))
                        && !modal,
                );
            }
        }
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(BG)
                    .inner_margin(if self.page == Page::Session { 14 } else { 26 }),
            )
            .show(ctx, |ui| {
                let enabled = !self.has_modal();
                ui.add_enabled_ui(enabled, |ui| match self.page {
                    Page::Hosts | Page::Sftp => self.library(ui, ctx),
                    Page::Session => self.session_area(ui, ctx),
                    Page::Identities => self.identities_page(ui),
                    Page::Snippets => self.snippets_page(ui, ctx),
                    Page::Tunnels => self.tunnels_page(ui),
                    Page::Workspaces => self.workspaces_page(ui, ctx),
                    Page::KnownHosts => self.known_hosts_page(ui),
                    Page::Settings => self.settings_page(ui),
                });
            });
        self.editor_modal(ctx);
        self.tab_rename_modal(ctx);
        self.auth_modal(ctx);
        self.entities_modals(ctx);
        self.confirmation_modals(ctx);
        self.palette_modal(ctx);
        self.trust_modal(ctx);
        if let Some(message) = self.error.clone() {
            let mut close = false;
            egui::Modal::new(egui::Id::new("error")).show(ctx, |ui| {
                ui.set_width(500.0);
                ui.heading("Nexus Remote");
                ui.label(message);
                if primary(ui, "Fermer").clicked() {
                    close = true;
                }
            });
            if close {
                self.error = None;
            }
        }
        if let Some(message) = &self.startup_error {
            egui::Modal::new(egui::Id::new("startup_error")).show(ctx, |ui| {
                ui.set_width(540.0);
                ui.heading("Le carnet n’a pas pu être chargé");
                ui.label(message);
                ui.label(
                    "Le fichier existant est conservé. Fermez l’application avant de le corriger.",
                );
                if primary(ui, "Fermer l’application").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        }
        self.pending_close_modal(ctx);
        if self.page != Page::Session || self.has_modal() || self.pane_modal() {
            for t in &mut self.tabs {
                if let SessionView::Rdp(s) = &mut t.view {
                    s.set_visible(false);
                }
            }
        }
        if !self.app_quit_confirmed {
            self.pump_launch(ctx);
        }
        if !self.tabs.is_empty() || !self.active_tunnels.is_empty() || !self.launch_queue.is_empty()
        {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }
}

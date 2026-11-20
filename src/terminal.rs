use crate::{
    model::Profile,
    net::{self, TrustBroker},
};
use egui::{Align2, Color32, Event, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, Vec2};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroize;

const BG: Color32 = crate::ui::SUNKEN;
const FG: Color32 = crate::ui::TEXT;
const MAX_PASTE: usize = 1024 * 1024;
const MAX_UPDATES_PER_POLL: usize = 64;
const MAX_OUTPUT_BYTES_PER_POLL: usize = 128 * 1024;
const OUTPUT_CHUNK_BYTES: usize = 2048;
const MAX_POLL_TIME: Duration = Duration::from_millis(3);

enum Command {
    Input(Vec<u8>),
    Resize(u16, u16),
}
enum Update {
    Connected,
    Output(Vec<u8>),
    Closed(String),
}

pub struct TerminalSession {
    parser: vt100::Parser,
    commands: SyncSender<Command>,
    updates: Receiver<Update>,
    cancel: Arc<AtomicBool>,
    status: String,
    connected: bool,
    closed_update_received: bool,
    size: (u16, u16),
    selection: Option<((u16, u16), (u16, u16))>,
    pending_paste: Option<String>,
    query_tail: VecDeque<u8>,
    pending_output: Option<(Vec<u8>, usize)>,
    first_focus: bool,
    focus_requested: bool,
    font_size: f32,
    theme: String,
    search_open: bool,
    search: String,
    focus_search: bool,
    #[cfg(test)]
    _offline_commands: Option<Receiver<Command>>,
    #[cfg(test)]
    offline_updates: Option<SyncSender<Update>>,
}

impl TerminalSession {
    pub fn new(profile: Profile, secret: String, trust: TrustBroker, ctx: egui::Context) -> Self {
        let (commands, receiver) = mpsc::sync_channel(256);
        let (sender, updates) = mpsc::sync_channel(128);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        thread::spawn(move || {
            let result = worker(
                profile,
                secret,
                trust,
                &worker_cancel,
                receiver,
                &sender,
                &ctx,
            );
            let message = if worker_cancel.load(Ordering::Relaxed) {
                "Session déconnectée.".into()
            } else {
                result
                    .err()
                    .unwrap_or_else(|| "Session fermée par le serveur.".into())
            };
            let _ = sender.send(Update::Closed(message));
            ctx.request_repaint();
        });
        Self {
            parser: vt100::Parser::new(30, 100, 5000),
            commands,
            updates,
            cancel,
            status: "Connexion SSH · vérification du serveur…".into(),
            connected: false,
            closed_update_received: false,
            size: (30, 100),
            selection: None,
            pending_paste: None,
            query_tail: VecDeque::new(),
            pending_output: None,
            first_focus: true,
            focus_requested: false,
            font_size: 14.0,
            theme: "nexus".into(),
            search_open: false,
            search: String::new(),
            focus_search: false,
            #[cfg(test)]
            _offline_commands: None,
            #[cfg(test)]
            offline_updates: None,
        }
    }

    #[cfg(test)]
    fn offline_pair() -> (Self, Receiver<Command>) {
        let (commands, receiver) = mpsc::sync_channel(256);
        let (update_sender, updates) = mpsc::sync_channel(128);
        (
            Self {
                parser: vt100::Parser::new(24, 80, 100),
                commands,
                updates,
                cancel: Arc::new(AtomicBool::new(false)),
                status: "Fixture".into(),
                connected: true,
                closed_update_received: false,
                size: (24, 80),
                selection: None,
                pending_paste: None,
                query_tail: VecDeque::new(),
                pending_output: None,
                first_focus: true,
                focus_requested: false,
                font_size: 14.0,
                theme: "nexus".into(),
                search_open: false,
                search: String::new(),
                focus_search: false,
                _offline_commands: None,
                offline_updates: Some(update_sender),
            },
            receiver,
        )
    }

    /// In-memory session for layout tests; no worker, file, or socket is opened.
    #[cfg(test)]
    pub(crate) fn offline_preview() -> Self {
        let (mut terminal, receiver) = Self::offline_pair();
        terminal._offline_commands = Some(receiver);
        terminal.status = "Connecté · SSH chiffré".into();
        terminal.parser.process(
            b"\x1b[32mNexus Remote\x1b[0m\r\nSession de demonstration\r\n\r\nops@laboratoire:~$ ",
        );
        terminal
    }
    #[cfg(test)]
    pub(crate) fn take_offline_input(&mut self) -> Vec<u8> {
        let mut input = Vec::new();
        if let Some(receiver) = &self._offline_commands {
            while let Ok(command) = receiver.try_recv() {
                if let Command::Input(bytes) = command {
                    input.extend(bytes);
                }
            }
        }
        input
    }

    #[cfg(test)]
    pub(crate) fn inject_offline_output(&mut self, bytes: &[u8]) {
        assert!(
            self.offline_updates
                .as_ref()
                .expect("offline terminal update sender")
                .try_send(Update::Output(bytes.to_vec()))
                .is_ok(),
            "offline terminal update queue is full"
        );
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn connected(&self) -> bool {
        self.connected
    }
    /// Explicit session activation may replace a tab button's keyboard focus,
    /// while a visible terminal search field keeps receiving its own text.
    pub fn request_input_focus(&mut self) {
        if self.search_open {
            self.focus_search = true;
            self.focus_requested = false;
        } else {
            self.focus_requested = true;
        }
    }

    pub fn has_modal(&self) -> bool {
        self.pending_paste.is_some()
    }

    /// The currently displayed screen, including the selected scrollback page.
    pub fn screen_text(&self) -> String {
        self.parser.screen().contents()
    }

    /// Insert a snippet as typed text. Multiline insertion keeps the paste confirmation.
    pub fn insert_text(&mut self, text: &str) {
        self.prepare_paste(text.to_owned());
    }

    /// Execute a command after the application has obtained explicit confirmation.
    pub fn run_command(&mut self, text: &str) -> Result<(), String> {
        if !self.connected {
            return Err("La session SSH n’est pas connectée.".into());
        }
        if text.len() > MAX_PASTE {
            return Err("Commande supérieure à 1 Mo.".into());
        }
        let safe: String = text
            .chars()
            .filter(|c| !c.is_control() || matches!(c, '\r' | '\n' | '\t'))
            .collect();
        if safe.trim().is_empty() {
            return Err("La commande est vide.".into());
        }
        let mut bytes = safe.replace("\r\n", "\n").replace('\n', "\r").into_bytes();
        if !bytes.ends_with(b"\r") {
            bytes.push(b'\r');
        }
        self.input(bytes);
        Ok(())
    }

    pub fn set_preferences(&mut self, font_size: f32, theme: &str) {
        self.font_size = if font_size.is_finite() {
            font_size.clamp(10.0, 26.0)
        } else {
            14.0
        };
        self.theme = match theme {
            "dracula" | "solarized" => theme,
            _ => "nexus",
        }
        .into();
    }

    fn palette(&self) -> (Color32, Color32, Color32) {
        match self.theme.as_str() {
            "dracula" => (
                Color32::from_rgb(40, 42, 54),
                Color32::from_rgb(248, 248, 242),
                Color32::from_rgb(189, 147, 249),
            ),
            "solarized" => (
                Color32::from_rgb(0, 43, 54),
                Color32::from_rgb(131, 148, 150),
                Color32::from_rgb(42, 161, 152),
            ),
            _ => (BG, FG, crate::ui::GREEN),
        }
    }

    fn send(&mut self, command: Command) {
        if !self.connected {
            return;
        }
        match self.commands.try_send(command) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.status = "Envoi saturé. Attendez la réponse du serveur.".into()
            }
            Err(TrySendError::Disconnected(_)) => {
                self.connected = false;
                self.status = "Session déconnectée.".into();
            }
        }
    }

    /// Drain SSH output even when the tab is hidden. The per-call budget keeps
    /// several busy tmux sessions from monopolizing the UI thread.
    pub fn poll_updates(&mut self) {
        let started = Instant::now();
        let mut processed = 0;
        let mut received = 0;
        while processed < MAX_OUTPUT_BYTES_PER_POLL && started.elapsed() < MAX_POLL_TIME {
            if self.pending_output.is_none() {
                if received >= MAX_UPDATES_PER_POLL {
                    break;
                }
                let update = match self.updates.try_recv() {
                    Ok(update) => update,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if !self.closed_update_received {
                            self.connected = false;
                            self.status = "Le processus de la session SSH s’est interrompu.".into();
                            self.closed_update_received = true;
                        }
                        break;
                    }
                };
                received += 1;
                match update {
                    Update::Connected => {
                        self.connected = true;
                        self.status = "Connecté · SSH chiffré".into();
                        self.send(Command::Resize(self.size.1, self.size.0));
                        continue;
                    }
                    Update::Output(data) => self.pending_output = Some((data, 0)),
                    Update::Closed(message) => {
                        self.connected = false;
                        self.status = message;
                        self.closed_update_received = true;
                        continue;
                    }
                }
            }
            let Some((data, mut offset)) = self.pending_output.take() else {
                continue;
            };
            let end = data
                .len()
                .min(offset + (MAX_OUTPUT_BYTES_PER_POLL - processed).min(OUTPUT_CHUNK_BYTES));
            self.process_output_chunk(&data[offset..end]);
            processed += end - offset;
            offset = end;
            if offset < data.len() {
                self.pending_output = Some((data, offset));
                continue;
            }
        }
    }

    fn process_output_chunk(&mut self, bytes: &[u8]) {
        // Feed the parser in slices, stopping at each terminal query so a DSR
        // reply observes the exact cursor position at that point in the stream.
        // query_tail also recognizes requests split across worker messages.
        let mut start = 0;
        for (index, &byte) in bytes.iter().enumerate() {
            self.query_tail.push_back(byte);
            if self.query_tail.len() > 8 {
                self.query_tail.pop_front();
            }
            let query = {
                let tail = self.query_tail.make_contiguous();
                if tail.ends_with(b"\x1b[6n") {
                    1
                } else if tail.ends_with(b"\x1b[5n") {
                    2
                } else if tail.ends_with(b"\x1b[c") || tail.ends_with(b"\x1b[0c") {
                    3
                } else {
                    0
                }
            };
            if query == 0 {
                continue;
            }
            self.parser.process(&bytes[start..=index]);
            start = index + 1;
            if query == 1 {
                let (row, col) = self.parser.screen().cursor_position();
                self.send(Command::Input(
                    format!("\x1b[{};{}R", row + 1, col + 1).into_bytes(),
                ));
            } else if query == 2 {
                self.send(Command::Input(b"\x1b[0n".to_vec()));
            } else {
                self.send(Command::Input(b"\x1b[?1;2c".to_vec()));
            }
        }
        if start < bytes.len() {
            self.parser.process(&bytes[start..]);
        }
    }

    /// Compact controls for the shared session header. The narrow form uses 72 px;
    /// a slot of at least 144 px also exposes selection copying directly.
    pub fn controls(&mut self, ui: &mut egui::Ui) {
        let show_copy = ui.available_width() >= 144.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
            ui.spacing_mut().interact_size.y = 32.0;
            if crate::ui::icon_button(ui, crate::ui::Icon::Search, "Rechercher · Ctrl+Maj+F")
                .clicked()
            {
                self.search_open = !self.search_open;
                self.focus_search = self.search_open;
            }
            if show_copy
                && ui
                    .add_enabled_ui(self.selection.is_some(), |ui| {
                        crate::ui::compact_button(ui, "Copier")
                    })
                    .inner
                    .clicked()
            {
                ui.ctx().copy_text(self.selected_text());
            }
            let more = crate::ui::icon_button(ui, crate::ui::Icon::More, "Options du terminal");
            egui::Popup::menu(&more).show(|ui| {
                ui.set_min_width(228.0);
                ui.label(
                    egui::RichText::new("TAILLE DU TEXTE")
                        .size(10.0)
                        .color(crate::ui::SUBTLE),
                );
                ui.horizontal(|ui| {
                    if crate::ui::compact_button(ui, "−")
                        .on_hover_text("Réduire la police")
                        .clicked()
                    {
                        self.font_size = (self.font_size - 1.0).max(10.0);
                    }
                    if crate::ui::compact_button(ui, &format!("{} px", self.font_size as u32))
                        .on_hover_text("Rétablir la taille de 14 px")
                        .clicked()
                    {
                        self.font_size = 14.0;
                    }
                    if crate::ui::compact_button(ui, "+")
                        .on_hover_text("Agrandir la police")
                        .clicked()
                    {
                        self.font_size = (self.font_size + 1.0).min(26.0);
                    }
                });
                ui.separator();
                if ui
                    .add_enabled(
                        self.selection.is_some(),
                        egui::Button::new("Copier la sélection"),
                    )
                    .clicked()
                {
                    ui.ctx().copy_text(self.selected_text());
                    ui.close();
                }
                if ui.button("Copier l’écran").clicked() {
                    ui.ctx().copy_text(self.screen_text());
                    ui.close();
                }
                if ui.button("Exporter l’écran…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("session-ssh.txt")
                        .add_filter("Texte", &["txt"])
                        .save_file()
                    {
                        if let Err(error) = std::fs::write(path, self.screen_text()) {
                            self.status = format!("Export impossible : {error}");
                        }
                    }
                    ui.close();
                }
                if ui.button("Revenir à la dernière ligne").clicked() {
                    self.parser.set_scrollback(0);
                    self.selection = None;
                    ui.close();
                }
                ui.separator();
                ui.menu_button("Thème du terminal", |ui| {
                    for (theme, title) in [
                        ("nexus", "Nexus sombre"),
                        ("dracula", "Dracula"),
                        ("solarized", "Solarized sombre"),
                    ] {
                        if ui.selectable_label(self.theme == theme, title).clicked() {
                            self.theme = theme.into();
                            ui.close();
                        }
                    }
                });
                ui.menu_button("Raccourcis clavier", |ui| {
                    for (keys, action) in [
                        ("Ctrl+C", "Interrompre la commande"),
                        ("Ctrl+Maj+C", "Copier la sélection"),
                        ("Ctrl+V", "Coller"),
                        ("Ctrl+Maj+F", "Rechercher"),
                        ("Maj+Page précédente/suivante", "Parcourir l’historique"),
                    ] {
                        ui.horizontal(|ui| {
                            ui.monospace(keys);
                            ui.label(action);
                        });
                    }
                });
                ui.separator();
                if ui
                    .add_enabled(
                        self.connected,
                        egui::Button::new(
                            egui::RichText::new("Déconnecter").color(crate::ui::DANGER),
                        ),
                    )
                    .clicked()
                {
                    self.cancel.store(true, Ordering::Relaxed);
                    self.connected = false;
                    self.status = "Déconnexion…".into();
                    ui.close();
                }
            });
        });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.poll_updates();
        let accepts_input = ui.is_enabled()
            && ui.memory(|memory| memory.is_above_modal_layer(ui.layer_id()))
            && !egui::Popup::is_any_open(ui.ctx());
        if accepts_input
            && ui.input_mut(|input| input.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::F))
        {
            self.search_open = !self.search_open;
            self.focus_search = self.search_open;
        }
        if self.search_open {
            let (bar, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.0), Sense::hover());
            let close_rect = Rect::from_min_size(
                Pos2::new(bar.right() - 32.0, bar.top() + 2.0),
                Vec2::splat(32.0),
            );
            let matches_rect = Rect::from_min_max(
                Pos2::new(close_rect.left() - 98.0, bar.top()),
                Pos2::new(close_rect.left() - 8.0, bar.bottom()),
            );
            let input_rect =
                Rect::from_min_max(bar.min, Pos2::new(matches_rect.left() - 8.0, bar.bottom()));
            let response = ui.put(
                input_rect,
                egui::TextEdit::singleline(&mut self.search)
                    .desired_width(input_rect.width())
                    .hint_text("Rechercher dans l’écran affiché…")
                    .margin(Vec2::new(10.0, 8.0)),
            );
            if self.focus_search {
                response.request_focus();
                self.focus_search = false;
            }
            let matches = if self.search.is_empty() {
                0
            } else {
                self.screen_text()
                    .to_lowercase()
                    .matches(&self.search.to_lowercase())
                    .count()
            };
            ui.painter().with_clip_rect(matches_rect).text(
                matches_rect.right_center(),
                Align2::RIGHT_CENTER,
                format!("{matches} résultat{}", if matches == 1 { "" } else { "s" }),
                FontId::proportional(11.0),
                crate::ui::MUTED,
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(close_rect), |ui| {
                if crate::ui::icon_button(ui, crate::ui::Icon::Close, "Fermer la recherche")
                    .clicked()
                {
                    self.search_open = false;
                    self.search.clear();
                }
            });
            ui.add_space(8.0);
        }
        let font = FontId::monospace(self.font_size);
        let char_width = ui.fonts_mut(|f| f.glyph_width(&font, 'M')).max(5.0);
        let char_height = ui.fonts_mut(|f| f.row_height(&font)).max(10.0) + 2.0;
        let available = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(available.x.max(10.0), available.y.max(60.0)),
            Sense::click_and_drag(),
        );
        let grid = rect.shrink(12.0);
        let cols = ((grid.width() / char_width).floor() as u16).clamp(2, 400);
        let rows = ((grid.height() / char_height).floor() as u16).clamp(1, 200);
        if (rows, cols) != self.size {
            self.size = (rows, cols);
            self.parser.set_size(rows, cols);
            self.selection = None;
            self.send(Command::Resize(cols, rows));
        }
        let may_autofocus =
            self.first_focus && !self.search_open && !ui.ctx().wants_keyboard_input();
        let requested_focus = self.focus_requested && accepts_input && !self.search_open;
        let pressed = response.is_pointer_button_down_on()
            && ui.input(|input| input.pointer.button_pressed(egui::PointerButton::Primary));
        // A sibling pane is rendered later in the frame. Do not forward its
        // same-frame typing using this terminal's previous-frame focus.
        let clicked_elsewhere = ui.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| !rect.contains(pos))
        });
        // A tab click may contain both press and release in one frame. Keep its
        // activation request pending until the cross-pane click guard is clear.
        if accepts_input
            && !clicked_elsewhere
            && (pressed || response.clicked() || may_autofocus || requested_focus)
        {
            response.request_focus();
            self.first_focus = false;
            self.focus_requested = false;
            if response.clicked() {
                self.selection = None;
            }
        }
        if clicked_elsewhere {
            response.surrender_focus();
        }
        if response.has_focus() {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
        }
        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                let cell = position_to_cell(pos, grid, char_width, char_height, self.size);
                self.selection = Some((cell, cell));
                response.request_focus();
            }
        }
        if response.dragged() {
            if let (Some((start, _)), Some(pos)) = (self.selection, response.interact_pointer_pos())
            {
                self.selection = Some((
                    start,
                    position_to_cell(pos, grid, char_width, char_height, self.size),
                ));
            }
        }
        if response.hovered() {
            let wheel = ui.input(|i| i.smooth_scroll_delta.y);
            if wheel.abs() > 1.0 && !self.parser.screen().alternate_screen() {
                let current = self.parser.screen().scrollback();
                let next = if wheel > 0.0 {
                    current.saturating_add((wheel / char_height).ceil() as usize)
                } else {
                    current.saturating_sub((-wheel / char_height).ceil() as usize)
                };
                self.parser.set_scrollback(next);
                self.selection = None;
            }
        }
        let painter = ui.painter().with_clip_rect(rect);
        let (background, foreground, cursor_color) = self.palette();
        painter.rect_filled(rect, 8.0, background);
        let ordered_selection = self
            .selection
            .map(|(a, b)| if a <= b { (a, b) } else { (b, a) });
        let screen = self.parser.screen();
        let mut glyphs = Vec::new();
        for row in 0..rows {
            let matches_search = self.search_open
                && !self.search.is_empty()
                && screen
                    .contents_between(row, 0, row, cols)
                    .to_lowercase()
                    .contains(&self.search.to_lowercase());
            if matches_search {
                painter.rect_filled(
                    Rect::from_min_size(
                        grid.min + Vec2::new(0.0, row as f32 * char_height),
                        Vec2::new(grid.width(), char_height),
                    ),
                    0.0,
                    Color32::from_rgba_unmultiplied(220, 173, 76, 40),
                );
            }
            for col in 0..cols {
                let Some(cell) = screen.cell(row, col) else {
                    continue;
                };
                let top = grid.min + Vec2::new(col as f32 * char_width, row as f32 * char_height);
                let bounds = Rect::from_min_size(top, Vec2::new(char_width, char_height));
                let mut fg = terminal_color(cell.fgcolor(), foreground, cell.bold());
                let mut bg = terminal_color(cell.bgcolor(), background, false);
                if cell.inverse() {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if let Some((start, end)) = ordered_selection {
                    if (row, col) >= start && (row, col) <= end {
                        bg = crate::ui::tint(crate::ui::GREEN, background, 0.25);
                    }
                }
                if bg != background {
                    painter.rect_filled(bounds, 0.0, bg);
                }
                let contents = cell.contents();
                if !cell.is_wide_continuation() && !contents.is_empty() {
                    glyphs.push((top, contents, fg, cell.bold(), cell.underline()));
                }
            }
        }
        // Paint backgrounds first so the continuation cell of a wide Unicode
        // glyph cannot paint over the right half of that glyph.
        for (top, contents, fg, bold, underline) in glyphs {
            painter.text(top, Align2::LEFT_TOP, &contents, font.clone(), fg);
            if bold {
                painter.text(
                    top + Vec2::new(0.35, 0.0),
                    Align2::LEFT_TOP,
                    &contents,
                    font.clone(),
                    fg,
                );
            }
            if underline {
                painter.line_segment(
                    [
                        top + Vec2::new(0.0, char_height - 2.0),
                        top + Vec2::new(char_width, char_height - 2.0),
                    ],
                    Stroke::new(1.0_f32, fg),
                );
            }
        }
        if response.has_focus()
            && self.connected
            && !screen.hide_cursor()
            && screen.scrollback() == 0
        {
            let (row, col) = screen.cursor_position();
            if col < cols && row < rows {
                let pos = grid.min + Vec2::new(col as f32 * char_width, row as f32 * char_height);
                painter.rect_filled(
                    Rect::from_min_size(pos, Vec2::new(2.0, char_height)),
                    0.0,
                    cursor_color,
                );
            }
        }
        if accepts_input
            && !clicked_elsewhere
            && response.has_focus()
            && self.pending_paste.is_none()
        {
            let events = ui.input(|i| i.events.clone());
            let modifiers = ui.input(|i| i.modifiers);
            for event in events {
                match event {
                    Event::Copy => {
                        if modifiers.ctrl && !modifiers.shift {
                            self.send(Command::Input(vec![3]));
                        } else if self.selection.is_some() {
                            ui.ctx().copy_text(self.selected_text());
                        }
                    }
                    Event::Cut => self.send(Command::Input(vec![24])),
                    Event::Paste(text) => self.prepare_paste(text),
                    Event::Text(text) if !modifiers.ctrl || modifiers.alt => {
                        let bytes = if modifiers.alt && !modifiers.ctrl {
                            format!("\x1b{text}").into_bytes()
                        } else {
                            text.into_bytes()
                        };
                        self.input(bytes);
                    }
                    Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        if modifiers.ctrl && modifiers.shift && key == Key::C {
                            ui.ctx().copy_text(self.selected_text());
                        } else if modifiers.shift
                            && matches!(key, Key::PageUp | Key::PageDown)
                            && !self.parser.screen().alternate_screen()
                        {
                            let current = self.parser.screen().scrollback();
                            let offset = if key == Key::PageUp {
                                current.saturating_add(self.size.0 as usize)
                            } else {
                                current.saturating_sub(self.size.0 as usize)
                            };
                            self.parser.set_scrollback(offset);
                            self.selection = None;
                        } else if let Some(bytes) =
                            key_bytes(key, modifiers, self.parser.screen().application_cursor())
                        {
                            self.input(bytes);
                        }
                    }
                    _ => {}
                }
            }
        }
        self.paste_dialog(ui.ctx());
        ui.ctx().request_repaint_after(Duration::from_millis(40));
    }

    fn input(&mut self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        self.parser.set_scrollback(0);
        self.selection = None;
        self.send(Command::Input(bytes));
    }

    fn prepare_paste(&mut self, text: String) {
        if text.len() > MAX_PASTE {
            self.status = "Collage refusé : texte supérieur à 1 Mo.".into();
            return;
        }
        let safe: String = text
            .chars()
            .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
            .collect();
        if safe.contains(['\n', '\r']) {
            self.pending_paste = Some(safe);
        } else {
            self.paste(safe);
        }
    }
    fn paste(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        let text = text.replace("\r\n", "\n").replace('\n', "\r");
        let text = if self.parser.screen().bracketed_paste() {
            format!("\x1b[200~{text}\x1b[201~")
        } else {
            text
        };
        self.input(text.into_bytes());
    }
    fn paste_dialog(&mut self, ctx: &egui::Context) {
        let Some(text) = self.pending_paste.as_ref() else {
            return;
        };
        let preview: String = text.chars().take(400).collect();
        let mut accept = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new((
            "terminal-paste",
            self.cancel.as_ref() as *const _ as usize,
        )))
        .show(ctx, |ui| {
            ui.set_max_width(500.0);
            ui.heading("Coller plusieurs lignes ?");
            ui.label("Ce texte peut exécuter plusieurs commandes sur le serveur.");
            ui.add_space(8.0);
            ui.monospace(preview);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                accept = ui.button("Coller dans cette session").clicked();
                cancel = ui.button("Annuler").clicked();
            });
        });
        if accept {
            if let Some(text) = self.pending_paste.take() {
                self.paste(text);
            }
        } else if cancel {
            self.pending_paste = None;
        }
    }
    fn selected_text(&self) -> String {
        let Some((a, b)) = self.selection else {
            return String::new();
        };
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        self.parser
            .screen()
            .contents_between(a.0, a.1, b.0, b.1.saturating_add(1).min(self.size.1))
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn worker(
    profile: Profile,
    mut secret: String,
    trust: TrustBroker,
    cancel: &AtomicBool,
    commands: Receiver<Command>,
    updates: &SyncSender<Update>,
    ctx: &egui::Context,
) -> Result<(), String> {
    let result = net::connect(&profile, &secret, &trust, cancel);
    // Minimize lifetime of the owned secret. No terminal output or error includes it.
    secret.zeroize();
    let session = result?;
    if cancel.load(Ordering::Relaxed) {
        return Ok(());
    }
    let mut channel = session.channel_session().map_err(|e| e.to_string())?;
    channel
        .request_pty("xterm-256color", None, Some((100, 30, 0, 0)))
        .map_err(|e| e.to_string())?;
    channel.shell().map_err(|e| e.to_string())?;
    channel
        .handle_extended_data(ssh2::ExtendedData::Merge)
        .map_err(|e| e.to_string())?;
    session.set_blocking(false);
    updates
        .send(Update::Connected)
        .map_err(|_| "Session fermée.".to_owned())?;
    ctx.request_repaint();
    let mut queue: VecDeque<(Vec<u8>, usize)> = VecDeque::new();
    let mut resize = None;
    let mut resize_in_progress = None;
    let mut buffer = [0u8; 32768];
    let mut keepalive = Instant::now();
    while !cancel.load(Ordering::Relaxed) {
        let mut progress = false;
        for _ in 0..64 {
            match commands.try_recv() {
                Ok(Command::Input(data)) => {
                    if !data.is_empty() {
                        queue.push_back((data, 0));
                    }
                }
                Ok(Command::Resize(cols, rows)) => resize = Some((cols, rows)),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            }
        }
        if queue
            .iter()
            .map(|(bytes, offset)| bytes.len() - offset)
            .sum::<usize>()
            > 4 * MAX_PASTE
        {
            return Err("Trop de données en attente d’envoi. Session fermée.".into());
        }
        if resize_in_progress.is_none() {
            resize_in_progress = resize.take();
        }
        if let Some((cols, rows)) = resize_in_progress {
            match channel.request_pty_size(cols as u32, rows as u32, None, None) {
                Ok(()) => {
                    resize_in_progress = None;
                    progress = true;
                }
                Err(error) if error.code() == ssh2::ErrorCode::Session(-37) => {}
                Err(error) => {
                    return Err(format!(
                        "Redimensionnement du terminal impossible : {error}"
                    ));
                }
            }
        }
        if let Some((data, offset)) = queue.front_mut() {
            match channel.write(&data[*offset..]) {
                Ok(0) => return Err("Le serveur a fermé le canal SSH.".into()),
                Ok(count) => {
                    *offset += count;
                    progress = true;
                    if *offset == data.len() {
                        queue.pop_front();
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(format!("Écriture SSH impossible : {error}")),
            }
        }
        for _ in 0..8 {
            match channel.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    progress = true;
                    updates
                        .send(Update::Output(buffer[..count].to_vec()))
                        .map_err(|_| "Session fermée.".to_owned())?;
                    ctx.request_repaint();
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(format!("Lecture SSH interrompue : {error}")),
            }
        }
        if channel.eof() {
            break;
        }
        if keepalive.elapsed() > Duration::from_secs(20) {
            let _ = session.keepalive_send();
            keepalive = Instant::now();
        }
        if !progress {
            thread::sleep(Duration::from_millis(15));
        }
    }
    let _ = channel.close();
    Ok(())
}

fn position_to_cell(
    pos: Pos2,
    grid: Rect,
    width: f32,
    height: f32,
    size: (u16, u16),
) -> (u16, u16) {
    (
        (((pos.y - grid.min.y).max(0.0) / height) as u16).min(size.0 - 1),
        (((pos.x - grid.min.x).max(0.0) / width) as u16).min(size.1 - 1),
    )
}

fn key_bytes(key: Key, m: Modifiers, application_cursor: bool) -> Option<Vec<u8>> {
    if m.ctrl && !m.alt {
        let control = match key {
            Key::A => Some(1),
            Key::B => Some(2),
            Key::C => Some(3),
            Key::D => Some(4),
            Key::E => Some(5),
            Key::F => Some(6),
            Key::G => Some(7),
            Key::H => Some(8),
            Key::I => Some(9),
            Key::J => Some(10),
            Key::K => Some(11),
            Key::L => Some(12),
            Key::M => Some(13),
            Key::N => Some(14),
            Key::O => Some(15),
            Key::P => Some(16),
            Key::Q => Some(17),
            Key::R => Some(18),
            Key::S => Some(19),
            Key::T => Some(20),
            Key::U => Some(21),
            Key::V => None,
            Key::W => Some(23),
            Key::X => Some(24),
            Key::Y => Some(25),
            Key::Z => Some(26),
            Key::Space => Some(0),
            _ => None,
        };
        if let Some(value) = control {
            return Some(vec![value]);
        }
    }
    let modifier = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.ctrl);
    let suffix = match key {
        Key::ArrowUp => Some('A'),
        Key::ArrowDown => Some('B'),
        Key::ArrowRight => Some('C'),
        Key::ArrowLeft => Some('D'),
        Key::Home => Some('H'),
        Key::End => Some('F'),
        _ => None,
    };
    if let Some(suffix) = suffix {
        return Some(
            if modifier > 1 {
                format!("\x1b[1;{modifier}{suffix}")
            } else if application_cursor {
                format!("\x1bO{suffix}")
            } else {
                format!("\x1b[{suffix}")
            }
            .into_bytes(),
        );
    }
    let text = match key {
        Key::Enter => "\r".to_owned(),
        Key::Backspace => {
            if m.alt {
                "\x1b\x7f".into()
            } else {
                "\x7f".into()
            }
        }
        Key::Tab => {
            if m.shift {
                "\x1b[Z".into()
            } else {
                "\t".into()
            }
        }
        Key::Escape => "\x1b".into(),
        Key::F1 | Key::F2 | Key::F3 | Key::F4 => {
            let c = match key {
                Key::F1 => 'P',
                Key::F2 => 'Q',
                Key::F3 => 'R',
                _ => 'S',
            };
            if modifier == 1 {
                format!("\x1bO{c}")
            } else {
                format!("\x1b[1;{modifier}{c}")
            }
        }
        Key::Insert
        | Key::Delete
        | Key::PageUp
        | Key::PageDown
        | Key::F5
        | Key::F6
        | Key::F7
        | Key::F8
        | Key::F9
        | Key::F10
        | Key::F11
        | Key::F12 => {
            let n = match key {
                Key::Insert => 2,
                Key::Delete => 3,
                Key::PageUp => 5,
                Key::PageDown => 6,
                Key::F5 => 15,
                Key::F6 => 17,
                Key::F7 => 18,
                Key::F8 => 19,
                Key::F9 => 20,
                Key::F10 => 21,
                Key::F11 => 23,
                _ => 24,
            };
            if modifier == 1 {
                format!("\x1b[{n}~")
            } else {
                format!("\x1b[{n};{modifier}~")
            }
        }
        _ => return None,
    };
    Some(text.into_bytes())
}

fn terminal_color(color: vt100::Color, default: Color32, bold: bool) -> Color32 {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        vt100::Color::Idx(mut index) => {
            if bold && index < 8 {
                index += 8;
            }
            const BASE: [[u8; 3]; 16] = [
                [24, 34, 48],
                [255, 113, 136],
                [114, 214, 174],
                [236, 196, 130],
                [128, 180, 255],
                [198, 160, 255],
                [110, 215, 223],
                [219, 228, 240],
                [116, 132, 156],
                [255, 162, 176],
                [156, 234, 203],
                [248, 223, 173],
                [176, 210, 255],
                [218, 192, 255],
                [165, 237, 241],
                [255, 255, 255],
            ];
            let [r, g, b] = if index < 16 {
                BASE[index as usize]
            } else if index < 232 {
                let n = index - 16;
                let c = |x: u8| if x == 0 { 0 } else { 55 + x * 40 };
                [c(n / 36), c((n / 6) % 6), c(n % 6)]
            } else {
                let v = 8 + (index - 232) * 10;
                [v, v, v]
            };
            Color32::from_rgb(r, g, b)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn offline_terminal() -> (TerminalSession, Receiver<Command>) {
        TerminalSession::offline_pair()
    }

    #[test]
    fn hidden_terminal_poll_drains_output_and_answers_split_dsr() {
        let (mut terminal, commands) = offline_terminal();
        let (sender, updates) = mpsc::sync_channel(1);
        terminal.updates = updates;
        terminal.offline_updates = Some(sender.clone());
        terminal.inject_offline_output(b"tmux-ready\x1b[");
        assert!(
            matches!(
                sender.try_send(Update::Output(b"6n".to_vec())),
                Err(TrySendError::Full(_))
            ),
            "the bounded update channel must apply backpressure"
        );

        terminal.poll_updates(); // No terminal UI or selected tab is rendered.
        assert!(terminal.screen_text().contains("tmux-ready"));
        assert!(sender.try_send(Update::Output(b"6n".to_vec())).is_ok());
        terminal.poll_updates();
        assert!(
            matches!(commands.try_recv(), Ok(Command::Input(bytes)) if bytes == b"\x1b[1;11R"),
            "DSR reply must reflect the cursor after the split output"
        );
        assert!(terminal.connected());
    }

    #[test]
    fn large_hidden_output_is_resumed_without_losing_the_tail() {
        let (mut terminal, commands) = offline_terminal();
        let mut output = vec![b'x'; MAX_OUTPUT_BYTES_PER_POLL + OUTPUT_CHUNK_BYTES];
        output.extend_from_slice(b"\r\nEND-OF-TMUX\x1b[6n");
        terminal.inject_offline_output(&output);
        terminal.poll_updates();
        assert!(
            terminal.pending_output.is_some(),
            "one poll must be bounded"
        );
        let mut polls = 1;
        for _ in 0..31 {
            if terminal.pending_output.is_none() {
                break;
            }
            terminal.poll_updates();
            polls += 1;
        }
        assert!(
            terminal.pending_output.is_none(),
            "output must drain within 32 bounded polls, needed at least {polls}"
        );
        assert!(terminal.screen_text().contains("END-OF-TMUX"));
        assert!(
            matches!(commands.try_recv(), Ok(Command::Input(bytes)) if bytes.starts_with(b"\x1b[") && bytes.ends_with(b"R"))
        );
    }

    #[test]
    fn missing_worker_close_is_visible_without_overwriting_a_reported_reason() {
        let (mut missing, _) = offline_terminal();
        missing.offline_updates = None;
        missing.poll_updates();
        assert!(!missing.connected());
        assert!(missing.status().contains("s’est interrompu"));

        let (mut reported, _) = offline_terminal();
        let sender = reported.offline_updates.take().unwrap();
        assert!(
            sender
                .send(Update::Closed("Connexion fermée par le serveur.".into()))
                .is_ok()
        );
        drop(sender);
        reported.poll_updates();
        assert_eq!(reported.status(), "Connexion fermée par le serveur.");
        assert!(!reported.connected());
    }

    #[test]
    fn initial_search_keeps_focus_and_never_forwards_its_text() {
        let (mut terminal, receiver) = offline_terminal();
        terminal.search_open = true;
        terminal.focus_search = true;
        let ctx = egui::Context::default();
        let screen = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(700.0, 500.0)));
        for pass in 0..2 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: screen,
                    events: if pass == 1 {
                        vec![Event::Text("local-search-fixture".into())]
                    } else {
                        Vec::new()
                    },
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| terminal.ui(ui));
                },
            );
        }
        assert_eq!(terminal.search, "local-search-fixture");
        while let Ok(command) = receiver.try_recv() {
            assert!(
                !matches!(command, Command::Input(_)),
                "Search text must stay local"
            );
        }
    }
    #[test]
    fn control_and_application_keys_are_encoded() {
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        assert_eq!(key_bytes(Key::C, ctrl, false).unwrap(), vec![3]);
        assert_eq!(
            key_bytes(Key::ArrowUp, Modifiers::NONE, true).unwrap(),
            b"\x1bOA"
        );
        assert_eq!(
            key_bytes(Key::ArrowRight, ctrl, false).unwrap(),
            b"\x1b[1;5C"
        );
        assert_eq!(
            key_bytes(Key::F12, Modifiers::NONE, false).unwrap(),
            b"\x1b[24~"
        );
    }
    #[test]
    fn snippet_execution_is_explicit_and_preferences_are_bounded() {
        let (mut terminal, receiver) = offline_terminal();
        terminal.insert_text("first\nsecond");
        assert!(terminal.pending_paste.is_some());
        assert!(receiver.try_recv().is_err());
        terminal.pending_paste = None;
        terminal.run_command("printf 'fixture'\nwhoami").unwrap();
        assert!(
            matches!(receiver.try_recv().unwrap(),Command::Input(bytes) if bytes==b"printf 'fixture'\rwhoami\r")
        );
        terminal.set_preferences(f32::NAN, "unrecognized");
        assert_eq!(terminal.font_size, 14.0);
        assert_eq!(terminal.theme, "nexus");
        terminal.set_preferences(50.0, "dracula");
        assert_eq!(terminal.font_size, 26.0);
        terminal.connected = false;
        assert!(terminal.run_command("echo fixture").is_err());
    }
    #[test]
    fn parser_preserves_utf8_colors_and_alternate_screen() {
        let mut parser = vt100::Parser::new(24, 80, 100);
        for byte in "\x1b[32mé✓".as_bytes() {
            parser.process(&[*byte]);
        }
        assert!(parser.screen().contents().contains("é✓"));
        assert_eq!(
            parser.screen().cell(0, 0).unwrap().fgcolor(),
            vt100::Color::Idx(2)
        );
        parser.process(b"\x1b[?1049hOTHER\x1b[?1049l");
        assert!(parser.screen().contents().contains("é✓"));
    }
    #[test]
    fn paste_requires_multiline_confirmation_and_removes_escape_controls() {
        let (mut terminal, receiver) = offline_terminal();
        terminal.prepare_paste("first\nsecond".into());
        assert!(terminal.pending_paste.is_some());
        assert!(receiver.try_recv().is_err());
        terminal.pending_paste = None;
        terminal.prepare_paste("\x1b\0".into());
        assert!(receiver.try_recv().is_err());
        terminal.parser.process(b"\x1b[?2004h");
        terminal.prepare_paste("\x1b[201~echo".into());
        match receiver.try_recv().unwrap() {
            Command::Input(bytes) => assert_eq!(bytes, b"\x1b[200~[201~echo\x1b[201~"),
            _ => panic!("Expected a paste input"),
        }
    }
    #[test]
    fn disabled_terminal_never_forwards_modal_text() {
        let (mut terminal, receiver) = offline_terminal();
        let ctx = egui::Context::default();
        let screen = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)));
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| terminal.ui(ui));
            },
        );
        while receiver.try_recv().is_ok() {}
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                events: vec![Event::Text("typed-fixture".into())],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| terminal.ui(ui));
            },
        );
        assert!(
            matches!(receiver.try_recv().unwrap(),Command::Input(bytes) if bytes==b"typed-fixture")
        );
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                events: vec![Event::Text("modal-secret-fixture".into())],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_enabled_ui(false, |ui| terminal.ui(ui));
                });
            },
        );
        while let Ok(command) = receiver.try_recv() {
            assert!(!matches!(command, Command::Input(_)));
        }
    }

    #[test]
    fn clicking_a_later_sibling_editor_never_sends_its_text_to_ssh() {
        let (mut terminal, receiver) = offline_terminal();
        let ctx = egui::Context::default();
        let screen = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 650.0)));
        let mut text = String::new();
        let mut field = Rect::NOTHING;
        for _ in 0..2 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: screen,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.columns(2, |columns| {
                            terminal.ui(&mut columns[0]);
                            field = columns[1]
                                .add_sized([400.0, 200.0], egui::TextEdit::multiline(&mut text))
                                .rect;
                        });
                    });
                },
            );
        }
        while receiver.try_recv().is_ok() {}
        let position = field.center();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                events: vec![
                    Event::PointerMoved(position),
                    Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                    Event::Text("sibling-editor-fixture".into()),
                    Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.columns(2, |columns| {
                        terminal.ui(&mut columns[0]);
                        columns[1].add_sized([400.0, 200.0], egui::TextEdit::multiline(&mut text));
                    });
                });
            },
        );
        assert!(
            text.contains("sibling-editor-fixture"),
            "fixture typing must reach its intended editor"
        );
        while let Ok(command) = receiver.try_recv() {
            assert!(
                !matches!(command, Command::Input(_)),
                "sibling editor text must never reach SSH"
            );
        }
    }

    #[test]
    #[ignore = "Requires the isolated loopback SSH fixture and NEXUS_TEST_PORT/FINGERPRINT."]
    fn terminal_worker_loopback_utf8_input_resize_and_cancel() {
        let port: u16 = std::env::var("NEXUS_TEST_PORT")
            .expect("NEXUS_TEST_PORT missing")
            .parse()
            .unwrap();
        let expected_fingerprint =
            std::env::var("NEXUS_TEST_FINGERPRINT").expect("NEXUS_TEST_FINGERPRINT missing");
        let profile = Profile {
            name: "Loopback terminal fixture".into(),
            host: "127.0.0.1".into(),
            port,
            username: "testuser".into(),
            ..Default::default()
        };
        let (requests, trust_rx) = mpsc::channel();
        let (commands, command_rx) = mpsc::sync_channel(256);
        let (updates, update_rx) = mpsc::sync_channel(128);
        let (finished, finished_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        struct CancelOnDrop(Arc<AtomicBool>);
        impl Drop for CancelOnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
            }
        }
        let _guard = CancelOnDrop(cancel.clone());
        let worker_cancel = cancel.clone();
        let handle = thread::spawn(move || {
            let result = worker(
                profile,
                "fixture-password".into(),
                TrustBroker { requests },
                &worker_cancel,
                command_rx,
                &updates,
                &egui::Context::default(),
            );
            let _ = finished.send(result);
        });
        let request = trust_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("No host identity request");
        assert_eq!(request.endpoint, format!("[127.0.0.1]:{port}"));
        assert_eq!(request.fingerprint, expected_fingerprint);
        request.reply.send(true).unwrap();
        let mut output = Vec::new();
        let mut parser = vt100::Parser::new(30, 100, 100);
        let receive_until = |needle: &str, output: &mut Vec<u8>, parser: &mut vt100::Parser| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !String::from_utf8_lossy(output).contains(needle) {
                match update_rx
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .expect("Terminal output timed out")
                {
                    Update::Connected => {}
                    Update::Output(bytes) => {
                        output.extend_from_slice(&bytes);
                        parser.process(&bytes);
                    }
                    Update::Closed(error) => panic!("Terminal closed unexpectedly: {error}"),
                }
            }
        };
        receive_until("été ✓", &mut output, &mut parser);
        assert!(parser.screen().contents().contains("NEXUS-FIXTURE"));
        assert_eq!(
            parser.screen().cell(0, 0).unwrap().fgcolor(),
            vt100::Color::Idx(2)
        );
        commands.send(Command::Resize(91, 27)).unwrap();
        commands
            .send(Command::Input("NEXUS_WORKER_é✓\r".as_bytes().to_vec()))
            .unwrap();
        receive_until("ECHO:NEXUS_WORKER_é✓", &mut output, &mut parser);
        if let Ok(stats_path) = std::env::var("NEXUS_TEST_STATS") {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let stats = std::fs::read_to_string(&stats_path)
                    .ok()
                    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
                let resized = stats
                    .as_ref()
                    .and_then(|value| value["resize"].as_array())
                    .is_some_and(|changes| {
                        changes
                            .iter()
                            .any(|change| change == &serde_json::json!([91, 27]))
                    });
                if resized {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Fixture never received the worker PTY resize"
                );
                thread::sleep(Duration::from_millis(30));
            }
        }
        commands.send(Command::Input(vec![3])).unwrap();
        receive_until("^C", &mut output, &mut parser);
        cancel.store(true, Ordering::Relaxed);
        finished_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("Canceled worker did not stop")
            .expect("Worker error");
        handle.join().unwrap();
    }
}

use super::*;

/// Session views with generated display data only: no worker, socket, native RDP
/// host, existing catalog, credentials or filesystem access is involved.
pub(super) fn offline_sessions() -> NexusApp {
    let profiles: Vec<_> = [
        (
            "Session A · Développement Linux avec un nom volontairement long",
            Protocol::Ssh,
        ),
        (
            "Session B · Maintenance Linux et outils de diagnostic",
            Protocol::Ssh,
        ),
        ("Session C · Bureau Windows de démonstration", Protocol::Rdp),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (name, protocol))| Profile {
        id: format!("session-layout-profile-{index}"),
        name: name.into(),
        host: format!("192.0.2.{}", index + 20),
        port: protocol.port(),
        username: "administrateur-de-demonstration".into(),
        protocol,
        ..Default::default()
    })
    .collect();
    let store = Store {
        state: storage::State {
            version: 1,
            profiles: profiles.clone(),
            ..Default::default()
        },
        data_dir: std::env::temp_dir()
            .join(format!("nexus-session-layout-{}", uuid::Uuid::new_v4())),
    };
    let mut app = NexusApp::from_store(store, None, 0);
    for (index, profile) in profiles.into_iter().enumerate() {
        let protocol = profile.protocol;
        let view = if protocol == Protocol::Rdp {
            SessionView::Rdp(
                RdpSession::offline_preview(profile.clone()).expect("Offline RDP fixture"),
            )
        } else {
            SessionView::Ssh(TerminalSession::offline_preview())
        };
        app.tabs.push(SessionTab {
            id: format!("session-layout-tab-{index}"),
            title: profile.name.clone(),
            profile,
            protocol,
            view,
        });
    }
    app.page = Page::Session;
    app.active = Some(0);
    app
}

fn render_sessions(
    app: &mut NexusApp,
    ctx: &egui::Context,
    width: f32,
    events: Vec<egui::Event>,
) -> (egui::FullOutput, egui::Rect, egui::Rect) {
    let height = if width <= 1040.0 { 700.0 } else { 850.0 };
    let mut bounds = None;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(width, height),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            app.header(ctx);
            app.session_tabs(ctx);
            bounds = Some(
                egui::CentralPanel::default()
                    .frame(Frame::new().fill(BG).inner_margin(14))
                    .show(ctx, |ui| {
                        let available = ui.available_rect_before_wrap();
                        let enabled = !app.has_modal();
                        ui.add_enabled_ui(enabled, |ui| app.session_area(ui, ctx));
                        (available, ui.min_rect())
                    })
                    .inner,
            );
            app.tab_rename_modal(ctx);
        },
    );
    let (available, used) = bounds.expect("The session panel must be rendered");
    (output, available, used)
}

fn assert_session_bounds(app: &NexusApp, available: egui::Rect, used: egui::Rect, width: f32) {
    assert!(
        used.left() >= available.left() - 1.0
            && used.right() <= available.right() + 1.0
            && used.top() >= available.top() - 1.0
            && used.bottom() <= available.bottom() + 1.0,
        "Session controls/body overflow at {width}px, active={:?}, secondary={:?}, stacked={}: available={available:?}, used={used:?}",
        app.active,
        app.secondary,
        app.split_horizontal,
    );
    assert!(
        !app.store.data_dir.exists(),
        "Rendering a session must not save the catalog"
    );
    assert!(
        app.launch_queue.is_empty(),
        "Rendering must not queue any new connection"
    );
    assert!(
        app.auth.is_none(),
        "Offline session rendering must not request credentials"
    );
}

#[test]
fn ssh_and_rdp_session_controls_stay_inside_the_panel_after_resizing() {
    let mut app = offline_sessions();
    let ctx = egui::Context::default();
    design::apply_theme(&ctx);
    for active in [0, 2] {
        app.active = Some(active);
        app.secondary = None;
        for width in [1920.0, 1040.0, 1320.0, 1040.0] {
            for pass in 0..3 {
                let (_, available, used) = render_sessions(&mut app, &ctx, width, Vec::new());
                if pass == 2 {
                    assert_session_bounds(&app, available, used, width);
                }
            }
        }
    }
}

#[test]
fn split_session_controls_keep_both_panes_inside_the_panel() {
    for stacked in [false, true] {
        for secondary in [1, 2] {
            let mut app = offline_sessions();
            app.secondary = Some(secondary);
            app.split_horizontal = stacked;
            let ctx = egui::Context::default();
            design::apply_theme(&ctx);
            for width in [1320.0, 1040.0, 1920.0] {
                for pass in 0..3 {
                    let (_, available, used) = render_sessions(&mut app, &ctx, width, Vec::new());
                    if pass == 2 {
                        assert_session_bounds(&app, available, used, width);
                    }
                }
            }
        }
    }
}

fn find_tab_title(shape: &egui::epaint::Shape, needle: &str, candidates: &mut Vec<egui::Rect>) {
    match shape {
        egui::epaint::Shape::Text(text)
            if text.galley.job.text.contains(needle) && text.pos.y < 140.0 =>
        {
            candidates.push(egui::Rect::from_min_size(text.pos, text.galley.size()));
        }
        egui::epaint::Shape::Vec(shapes) => {
            for shape in shapes {
                find_tab_title(shape, needle, candidates);
            }
        }
        _ => {}
    }
}

#[test]
fn selecting_an_existing_session_tab_does_not_reconnect_or_save_it() {
    let mut app = offline_sessions();
    let ctx = egui::Context::default();
    design::apply_theme(&ctx);
    let mut output = render_sessions(&mut app, &ctx, 1320.0, Vec::new()).0;
    for _ in 0..2 {
        output = render_sessions(&mut app, &ctx, 1320.0, Vec::new()).0;
    }
    let mut titles = Vec::new();
    for shape in &output.shapes {
        find_tab_title(&shape.shape, "Session B", &mut titles);
    }
    titles.sort_by(|a, b| a.top().total_cmp(&b.top()));
    let title = titles
        .first()
        .expect("The second session title must appear in its tab");
    let position = title.center();
    for pressed in [true, false] {
        let _ = render_sessions(
            &mut app,
            &ctx,
            1320.0,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    let (_, available, used) = render_sessions(&mut app, &ctx, 1320.0, Vec::new());
    assert_eq!(
        app.active,
        Some(1),
        "The clicked session must become active"
    );
    assert_eq!(
        app.tabs.len(),
        3,
        "Tab selection must reuse the existing session"
    );
    assert_session_bounds(&app, available, used, 1320.0);
}

#[test]
fn hidden_ssh_tabs_continue_draining_output_without_being_rendered() {
    let mut app = offline_sessions();
    let hidden = match &mut app.tabs[1].view {
        SessionView::Ssh(terminal) => terminal,
        _ => panic!("Expected hidden SSH tab"),
    };
    hidden.inject_offline_output(b"hidden-tmux-output");
    assert!(!hidden.screen_text().contains("hidden-tmux-output"));
    app.poll_hidden_ssh_sessions();
    let hidden = match &app.tabs[1].view {
        SessionView::Ssh(terminal) => terminal,
        _ => unreachable!(),
    };
    assert!(hidden.screen_text().contains("hidden-tmux-output"));

    app.set_page(Page::Hosts);
    if let SessionView::Ssh(terminal) = &mut app.tabs[0].view {
        terminal.inject_offline_output(b"vault-tmux-output");
    }
    app.poll_hidden_ssh_sessions();
    if let SessionView::Ssh(terminal) = &app.tabs[0].view {
        assert!(terminal.screen_text().contains("vault-tmux-output"));
    }
    assert_eq!(app.active, Some(0));
}

#[test]
fn split_survives_click_on_right_tab_and_visit_to_vault() {
    let mut app = offline_sessions();
    app.secondary = Some(1);
    let ctx = egui::Context::default();
    design::apply_theme(&ctx);
    let mut output = render_sessions(&mut app, &ctx, 1320.0, Vec::new()).0;
    for _ in 0..2 {
        output = render_sessions(&mut app, &ctx, 1320.0, Vec::new()).0;
    }
    let mut titles = Vec::new();
    for shape in &output.shapes {
        find_tab_title(&shape.shape, "Session B", &mut titles);
    }
    let position = titles.first().expect("Right tab must be visible").center();
    for pressed in [true, false] {
        let _ = render_sessions(
            &mut app,
            &ctx,
            1320.0,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    assert_eq!((app.active, app.secondary), (Some(0), Some(1)));
    app.set_page(Page::Hosts);
    assert_eq!((app.active, app.secondary), (Some(0), Some(1)));
    app.set_page(Page::Session);
    let (_, available, used) = render_sessions(&mut app, &ctx, 1040.0, Vec::new());
    assert_session_bounds(&app, available, used, 1040.0);
}

#[test]
fn closing_left_session_keeps_two_distinct_split_panes() {
    let mut app = offline_sessions();
    app.secondary = Some(1);
    app.close_tab(0);
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.secondary, Some(0));
    assert_eq!(app.active, Some(1));

    app.close_tab(1);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.active, Some(0));
    assert_eq!(app.secondary, None);
}

#[test]
fn split_button_is_visible_and_opens_a_session_chooser() {
    let mut app = offline_sessions();
    let ctx = egui::Context::default();
    design::apply_theme(&ctx);
    let mut output = render_sessions(&mut app, &ctx, 1320.0, Vec::new()).0;
    for _ in 0..2 {
        output = render_sessions(&mut app, &ctx, 1320.0, Vec::new()).0;
    }
    let mut labels = Vec::new();
    for shape in &output.shapes {
        find_tab_title(&shape.shape, "Côte à côte", &mut labels);
    }
    let position = labels
        .first()
        .expect("The split button must be visible beside the tabs")
        .center();
    for pressed in [true, false] {
        let _ = render_sessions(
            &mut app,
            &ctx,
            1320.0,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    assert!(egui::Popup::is_any_open(&ctx));
    assert_eq!((app.active, app.secondary), (Some(0), None));
    assert!(app.launch_queue.is_empty());
}

#[test]
fn tab_rename_changes_only_the_session_label_and_reconnect_carries_it() {
    let mut app = offline_sessions();
    let ctx = egui::Context::default();
    design::apply_theme(&ctx);
    let id = app.tabs[0].id.clone();
    let original_host_name = app.tabs[0].profile.name.clone();
    app.begin_tab_rename(&id);
    assert!(app.has_modal(), "Rename must block terminal input");
    app.tab_rename.as_mut().unwrap().title = "tmux · projet".into();
    for _ in 0..2 {
        let _ = render_sessions(&mut app, &ctx, 1320.0, Vec::new());
    }
    let enter = egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let _ = render_sessions(&mut app, &ctx, 1320.0, vec![enter]);
    assert!(app.tab_rename.is_none());
    assert_eq!(app.tabs[0].title, "tmux · projet");
    assert_eq!(app.tabs[0].profile.name, original_host_name);
    assert_eq!(app.store.state.profiles[0].name, original_host_name);
    assert!(!app.store.data_dir.exists());

    app.finish_tab_close(&id, true, &ctx);
    assert_eq!(app.tabs.len(), 2);
    assert!(matches!(
        app.launch_queue.front(),
        Some((_, LaunchTarget::Session { title: Some(title) })) if title == "tmux · projet"
    ));
    assert!(!app.store.data_dir.exists());
}

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
                        app.session_area(ui, ctx);
                        (available, ui.min_rect())
                    })
                    .inner,
            );
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

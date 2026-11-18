use super::*;

const WIDTH: f32 = 1320.0;

fn fixture() -> (NexusApp, egui::Context) {
    let app = session_layout_tests::offline_sessions();
    let ctx = egui::Context::default();
    design::apply_theme(&ctx);
    (app, ctx)
}

fn render(app: &mut NexusApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(WIDTH, 850.0),
            )),
            events,
            focused: true,
            ..Default::default()
        },
        |ctx| {
            app.header(ctx);
            app.session_tabs(ctx);
            egui::CentralPanel::default()
                .frame(Frame::new().fill(BG).inner_margin(14))
                .show(ctx, |ui| {
                    let enabled = !app.has_modal();
                    ui.add_enabled_ui(enabled, |ui| app.session_area(ui, ctx));
                });
        },
    )
}

fn input(app: &mut NexusApp, index: usize) -> Vec<u8> {
    match &mut app.tabs[index].view {
        SessionView::Ssh(terminal) => terminal.take_offline_input(),
        _ => panic!("Expected an offline SSH fixture"),
    }
}

fn settle(app: &mut NexusApp, ctx: &egui::Context) -> egui::FullOutput {
    let mut output = render(app, ctx, Vec::new());
    for _ in 0..2 {
        output = render(app, ctx, Vec::new());
    }
    input(app, 0);
    input(app, 1);
    output
}

fn collect_canvas(shape: &egui::epaint::Shape, rects: &mut Vec<egui::Rect>) {
    match shape {
        egui::epaint::Shape::Rect(rect)
            if rect.fill == SUNKEN
                && rect.corner_radius == egui::CornerRadius::same(8)
                && rect.rect.width() > 200.0
                && rect.rect.height() > 80.0 =>
        {
            rects.push(rect.rect)
        }
        egui::epaint::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_canvas(shape, rects);
            }
        }
        _ => {}
    }
}

fn canvases(output: &egui::FullOutput) -> Vec<egui::Rect> {
    let mut rects = Vec::new();
    for shape in &output.shapes {
        collect_canvas(&shape.shape, &mut rects);
    }
    rects.sort_by(|a, b| {
        a.left()
            .total_cmp(&b.left())
            .then(a.top().total_cmp(&b.top()))
    });
    rects
}

fn click(app: &mut NexusApp, ctx: &egui::Context, position: egui::Pos2) -> egui::FullOutput {
    let mut output = None;
    for pressed in [true, false] {
        output = Some(render(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        ));
    }
    output.unwrap()
}

fn collect_title(shape: &egui::epaint::Shape, needle: &str, rects: &mut Vec<egui::Rect>) {
    match shape {
        egui::epaint::Shape::Text(text)
            if text.pos.y < 140.0 && text.galley.job.text.contains(needle) =>
        {
            rects.push(egui::Rect::from_min_size(text.pos, text.galley.size()));
        }
        egui::epaint::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_title(shape, needle, rects);
            }
        }
        _ => {}
    }
}

fn tab_position(output: &egui::FullOutput, needle: &str) -> egui::Pos2 {
    let mut rects = Vec::new();
    for shape in &output.shapes {
        collect_title(&shape.shape, needle, &mut rects);
    }
    rects
        .first()
        .expect("The requested tab must be displayed")
        .center()
}

fn shape_has_text(shape: &egui::epaint::Shape, needle: &str) -> bool {
    match shape {
        egui::epaint::Shape::Text(text) => text.galley.job.text.contains(needle),
        egui::epaint::Shape::Vec(shapes) => {
            shapes.iter().any(|shape| shape_has_text(shape, needle))
        }
        _ => false,
    }
}

fn output_has_text(output: &egui::FullOutput, needle: &str) -> bool {
    output
        .shapes
        .iter()
        .any(|shape| shape_has_text(&shape.shape, needle))
}

fn collect_session_menu_label(shape: &egui::epaint::Shape, rects: &mut Vec<egui::Rect>) {
    match shape {
        egui::epaint::Shape::Text(text) if text.galley.job.text == "Session" => {
            rects.push(egui::Rect::from_min_size(text.pos, text.galley.size()));
        }
        egui::epaint::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_session_menu_label(shape, rects);
            }
        }
        _ => {}
    }
}

fn session_menu_position(output: &egui::FullOutput) -> egui::Pos2 {
    let mut rects = Vec::new();
    for shape in &output.shapes {
        collect_session_menu_label(&shape.shape, &mut rects);
    }
    rects
        .first()
        .expect("The common session menu must be displayed")
        .center()
}

fn assert_no_side_effects(app: &NexusApp) {
    assert!(
        !app.store.data_dir.exists(),
        "Input routing must not save the catalog"
    );
    assert!(
        app.launch_queue.is_empty(),
        "Input routing must not reconnect a host"
    );
    assert!(
        app.auth.is_none(),
        "Offline input must not request credentials"
    );
}

#[test]
fn actual_session_frame_keeps_initial_terminal_keyboard_focus() {
    let (mut app, ctx) = fixture();
    settle(&mut app, &ctx);
    render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("saisie-initiale".into())],
    );
    assert_eq!(input(&mut app, 0), b"saisie-initiale");
    assert!(input(&mut app, 1).is_empty());
    assert_no_side_effects(&app);
}

#[test]
fn actual_session_canvas_receives_text_after_a_mouse_click() {
    let (mut app, ctx) = fixture();
    let output = settle(&mut app, &ctx);
    let rect = canvases(&output)[0];
    click(&mut app, &ctx, rect.center());
    render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("saisie-apres-clic".into())],
    );
    assert_eq!(input(&mut app, 0), b"saisie-apres-clic");
    assert!(input(&mut app, 1).is_empty());
    assert_no_side_effects(&app);
}

#[test]
fn returning_to_an_ssh_tab_and_clicking_its_canvas_restores_input() {
    let (mut app, ctx) = fixture();
    let output = settle(&mut app, &ctx);
    let output = click(&mut app, &ctx, tab_position(&output, "Session B"));
    assert_eq!(app.active, Some(1));
    let rect = canvases(&output)[0];
    click(&mut app, &ctx, rect.center());
    render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("seconde-session".into())],
    );
    assert!(input(&mut app, 0).is_empty());
    assert_eq!(input(&mut app, 1), b"seconde-session");
    let output = render(&mut app, &ctx, Vec::new());
    let output = click(&mut app, &ctx, tab_position(&output, "Session A"));
    click(&mut app, &ctx, canvases(&output)[0].center());
    render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("retour-session".into())],
    );
    assert_eq!(input(&mut app, 0), b"retour-session");
    assert!(input(&mut app, 1).is_empty());
    assert_no_side_effects(&app);
}

fn search_shortcut() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::F,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
    }
}

#[test]
fn returning_from_rdp_to_ssh_restores_typing_without_an_extra_canvas_click() {
    let (mut app, ctx) = fixture();
    let output = settle(&mut app, &ctx);
    let output = click(&mut app, &ctx, tab_position(&output, "Session C"));
    assert_eq!(app.active, Some(2), "The RDP tab must become active");
    assert!(input(&mut app, 0).is_empty());
    let output = click(&mut app, &ctx, tab_position(&output, "Session A"));
    assert_eq!(
        app.active,
        Some(0),
        "The original SSH tab must become active"
    );
    assert_eq!(canvases(&output).len(), 1);
    // Selecting the tab is the explicit focus action. The user must not have
    // to click the terminal a second time before typing ordinary text.
    render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("retour-de-rdp".into())],
    );
    assert_eq!(input(&mut app, 0), b"retour-de-rdp");
    assert!(input(&mut app, 1).is_empty());
    assert_no_side_effects(&app);
}

#[test]
fn batched_rdp_to_ssh_tab_click_restores_typing_on_the_next_frame() {
    let (mut app, ctx) = fixture();
    let output = settle(&mut app, &ctx);
    let output = click(&mut app, &ctx, tab_position(&output, "Session C"));
    assert_eq!(app.active, Some(2));
    let position = tab_position(&output, "Session A");
    // Native event batching can deliver both edges of a click in one egui
    // frame. The tab activates while any_pressed is still true for that frame.
    render(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    assert_eq!(app.active, Some(0));
    render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("retour-clic-groupe".into())],
    );
    assert_eq!(input(&mut app, 0), b"retour-clic-groupe");
    assert!(input(&mut app, 1).is_empty());
    assert_no_side_effects(&app);
}

#[test]
fn returning_from_another_ssh_tab_restores_typing_without_a_canvas_click() {
    let (mut app, ctx) = fixture();
    let output = settle(&mut app, &ctx);
    let output = click(&mut app, &ctx, tab_position(&output, "Session B"));
    assert_eq!(app.active, Some(1));
    input(&mut app, 0);
    input(&mut app, 1);
    click(&mut app, &ctx, tab_position(&output, "Session A"));
    assert_eq!(app.active, Some(0));
    render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("retour-de-ssh".into())],
    );
    assert_eq!(input(&mut app, 0), b"retour-de-ssh");
    assert!(input(&mut app, 1).is_empty());
    assert_no_side_effects(&app);
}

#[test]
fn returning_to_ssh_with_search_open_keeps_text_in_search_instead_of_the_shell() {
    let (mut app, ctx) = fixture();
    settle(&mut app, &ctx);
    render(&mut app, &ctx, vec![search_shortcut()]);
    let output = render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("premiere-recherche".into())],
    );
    assert!(
        output_has_text(&output, "premiere-recherche"),
        "The search field must contain its input"
    );
    assert!(
        input(&mut app, 0).is_empty(),
        "Search input must never reach SSH"
    );
    let output = click(&mut app, &ctx, tab_position(&output, "Session C"));
    assert_eq!(app.active, Some(2));
    click(&mut app, &ctx, tab_position(&output, "Session A"));
    assert_eq!(app.active, Some(0));
    let output = render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("recherche-reprise".into())],
    );
    assert!(
        output_has_text(&output, "recherche-reprise"),
        "Returning to an SSH search must restore the search field focus"
    );
    assert!(
        input(&mut app, 0).is_empty(),
        "Explicit tab activation must not redirect search text to SSH"
    );
    assert!(input(&mut app, 1).is_empty());
    assert_no_side_effects(&app);
}

#[test]
fn a_session_menu_after_ssh_reactivation_never_forwards_its_text_to_the_shell() {
    let (mut app, ctx) = fixture();
    let output = settle(&mut app, &ctx);
    let output = click(&mut app, &ctx, tab_position(&output, "Session C"));
    let output = click(&mut app, &ctx, tab_position(&output, "Session A"));
    assert_eq!(app.active, Some(0));
    click(&mut app, &ctx, session_menu_position(&output));
    render(&mut app, &ctx, Vec::new());
    assert!(
        egui::Popup::is_any_open(&ctx),
        "The real session menu must be open for this safety check"
    );
    render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("texte-menu-confidentiel-fixture".into())],
    );
    assert!(
        input(&mut app, 0).is_empty(),
        "A menu must block SSH text even after explicit tab reactivation"
    );
    assert!(input(&mut app, 1).is_empty());
    let output = render(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    click(&mut app, &ctx, canvases(&output)[0].center());
    render(&mut app, &ctx, vec![egui::Event::Text("apres-menu".into())]);
    assert_eq!(input(&mut app, 0), b"apres-menu");
    assert_no_side_effects(&app);
}

#[test]
fn closing_search_then_clicking_the_canvas_restores_terminal_input() {
    let (mut app, ctx) = fixture();
    settle(&mut app, &ctx);
    render(&mut app, &ctx, vec![search_shortcut()]);
    render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("texte-recherche".into())],
    );
    assert!(
        input(&mut app, 0).is_empty(),
        "Search text must not reach SSH"
    );
    let output = render(&mut app, &ctx, vec![search_shortcut()]);
    click(&mut app, &ctx, canvases(&output)[0].center());
    render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("saisie-restauree".into())],
    );
    assert_eq!(input(&mut app, 0), b"saisie-restauree");
    assert_no_side_effects(&app);
}

#[test]
fn split_ssh_views_send_text_only_to_the_clicked_terminal() {
    let (mut app, ctx) = fixture();
    app.secondary = Some(1);
    let output = settle(&mut app, &ctx);
    let rects = canvases(&output);
    assert_eq!(rects.len(), 2);
    click(&mut app, &ctx, rects[1].center());
    render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("pane-droite".into())],
    );
    assert!(input(&mut app, 0).is_empty());
    assert_eq!(input(&mut app, 1), b"pane-droite");
    let output = render(&mut app, &ctx, Vec::new());
    click(&mut app, &ctx, canvases(&output)[0].center());
    render(
        &mut app,
        &ctx,
        vec![egui::Event::Text("pane-gauche".into())],
    );
    assert_eq!(input(&mut app, 0), b"pane-gauche");
    assert!(input(&mut app, 1).is_empty());
    assert_no_side_effects(&app);
}

#[test]
fn pending_primary_focus_does_not_capture_same_frame_secondary_pane_typing() {
    let (mut app, ctx) = fixture();
    app.secondary = Some(1);
    let output = settle(&mut app, &ctx);
    let rects = canvases(&output);
    assert_eq!(rects.len(), 2);
    // Both panes have already been used. Correct routing must not depend on the
    // secondary terminal still having its one-time initial autofocus available.
    click(&mut app, &ctx, rects[1].center());
    click(&mut app, &ctx, rects[0].center());
    input(&mut app, 0);
    input(&mut app, 1);
    match &mut app.tabs[0].view {
        SessionView::Ssh(terminal) => terminal.request_input_focus(),
        _ => unreachable!(),
    }
    let position = rects[1].center();
    render(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text("texte-panneau-secondaire".into()),
        ],
    );
    assert!(
        input(&mut app, 0).is_empty(),
        "A pending focus request must not override the user's click in another pane"
    );
    assert_eq!(input(&mut app, 1), b"texte-panneau-secondaire");
    assert_no_side_effects(&app);
}

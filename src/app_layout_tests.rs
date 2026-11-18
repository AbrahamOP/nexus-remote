use super::*;

fn offline_catalog() -> NexusApp {
    let profiles = [
        ("SSH de démonstration avec un nom volontairement long", Protocol::Ssh),
        ("Bureau Windows", Protocol::Rdp),
        ("Sauvegardes", Protocol::Sftp),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (name, protocol))| Profile {
        id: format!("layout-fixture-{index}"),
        name: name.into(),
        host: format!("192.0.2.{}", index + 10),
        port: protocol.port(),
        username: "administrateur-de-demonstration".into(),
        group: if index == 0 { "Laboratoire" } else { "Personnel" }.into(),
        protocol,
        description: if index == 0 {
            "Une note de démonstration assez longue pour vérifier que le contenu d'une carte ne repousse pas ses boutons hors de la fenêtre.".into()
        } else {
            String::new()
        },
        tags: if index == 0 { vec!["démonstration".into(), "infrastructure".into()] } else { Vec::new() },
        ..Default::default()
    })
    .collect();
    let store = Store {
        state: storage::State {
            version: 1,
            profiles,
            ..Default::default()
        },
        // No data is read or written. A unique absent directory also makes an
        // accidental save during a no-input render detectable below.
        data_dir: std::env::temp_dir().join(format!("nexus-layout-{}", uuid::Uuid::new_v4())),
    };
    NexusApp::from_store(store, None, 0)
}

fn assert_catalog_stays_in_panel(app: &mut NexusApp, ctx: &egui::Context, width: f32) {
    // Settle first-pass font/layout measurements, exactly as the native app does.
    for pass in 0..3 {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(width, 850.0))),
                ..Default::default()
            },
            |ctx| {
                app.header(ctx);
                app.sidebar(ctx);
                app.inspector(ctx);
                let (available, used) = egui::CentralPanel::default()
                    .frame(Frame::new().fill(BG).inner_margin(26))
                    .show(ctx, |ui| {
                        let available = ui.available_rect_before_wrap();
                        app.library(ui, ctx);
                        (available, ui.min_rect())
                    })
                    .inner;
                if pass == 2 {
                    assert!(
                        used.left() >= available.left() - 1.0 && used.right() <= available.right() + 1.0,
                        "Catalog overflow at viewport {width}, selected={}, list={}: available={available:?}, used={used:?}",
                        app.selected.is_some(),
                        app.store.state.preferences.host_view_list,
                    );
                }
            },
        );
    }
    assert!(
        !app.store.data_dir.exists(),
        "Rendering a catalog must not save it"
    );
    assert!(
        app.tabs.is_empty() && app.launch_queue.is_empty(),
        "Rendering must not launch a connection"
    );
}

#[test]
fn catalog_grid_keeps_its_right_margin_after_resizing_and_inspection() {
    for inspect in [false, true] {
        let mut app = offline_catalog();
        if inspect {
            app.selected = Some(app.store.state.profiles[0].id.clone());
        }
        let ctx = egui::Context::default();
        design::apply_theme(&ctx);
        // Reuse a Context across resizes so stale column measurements cannot hide
        // an overflow that a fresh Context would miss.
        for width in [1920.0, 1040.0, 1320.0, 1040.0] {
            assert_catalog_stays_in_panel(&mut app, &ctx, width);
        }
    }
}

#[test]
fn catalog_list_keeps_its_right_margin_with_long_profile_text() {
    for inspect in [false, true] {
        let mut app = offline_catalog();
        app.store.state.preferences.host_view_list = true;
        if inspect {
            app.selected = Some(app.store.state.profiles[0].id.clone());
        }
        let ctx = egui::Context::default();
        design::apply_theme(&ctx);
        for width in [1040.0, 1320.0, 1920.0] {
            assert_catalog_stays_in_panel(&mut app, &ctx, width);
        }
    }
}

fn render_catalog_input(
    app: &mut NexusApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1320.0, 850.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            app.header(ctx);
            app.sidebar(ctx);
            app.inspector(ctx);
            egui::CentralPanel::default()
                .frame(Frame::new().fill(BG).inner_margin(26))
                .show(ctx, |ui| app.library(ui, ctx));
        },
    )
}

fn collect_host_card_rects(shape: &egui::epaint::Shape, cards: &mut Vec<egui::Rect>) {
    match shape {
        egui::epaint::Shape::Rect(shape)
            if shape.corner_radius == egui::CornerRadius::same(14)
                && shape.fill == CARD
                && shape.rect.width() >= 250.0
                && (100.0..=200.0).contains(&shape.rect.height()) =>
        {
            cards.push(shape.rect);
        }
        egui::epaint::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_host_card_rects(shape, cards);
            }
        }
        _ => {}
    }
}

fn shape_contains_text(shape: &egui::epaint::Shape, text: &str) -> bool {
    match shape {
        egui::epaint::Shape::Text(shape) => shape.galley.job.text.contains(text),
        egui::epaint::Shape::Vec(shapes) => {
            shapes.iter().any(|shape| shape_contains_text(shape, text))
        }
        _ => false,
    }
}

#[test]
fn host_card_menu_receives_click_without_selecting_or_launching_its_card() {
    let mut app = offline_catalog();
    let ctx = egui::Context::default();
    design::apply_theme(&ctx);
    for _ in 0..2 {
        let _ = render_catalog_input(&mut app, &ctx, Vec::new());
    }
    let output = render_catalog_input(&mut app, &ctx, Vec::new());
    let mut cards = Vec::new();
    for clipped in &output.shapes {
        collect_host_card_rects(&clipped.shape, &mut cards);
    }
    cards.sort_by(|a, b| {
        a.top()
            .total_cmp(&b.top())
            .then(a.left().total_cmp(&b.left()))
    });
    let card = cards
        .first()
        .copied()
        .expect("The catalog must render a host card");
    // Two 32px icon buttons in the footer: 16px inner padding, 1px border,
    // then the favorite button and 4px gap before the menu button.
    let menu_center = egui::pos2(card.left() + 69.0, card.bottom() - 33.0);
    assert!(card.contains(menu_center));
    for pressed in [true, false] {
        let _ = render_catalog_input(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(menu_center),
                egui::Event::PointerButton {
                    pos: menu_center,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    let output = render_catalog_input(&mut app, &ctx, vec![egui::Event::PointerMoved(menu_center)]);
    assert!(
        egui::Popup::is_any_open(&ctx),
        "The host actions menu must open"
    );
    assert!(
        output
            .shapes
            .iter()
            .any(|clipped| shape_contains_text(&clipped.shape, "Modifier l’hôte")),
        "The host-specific edit action must be visible",
    );
    assert!(
        app.selected.is_none(),
        "The card background must not steal its menu click"
    );
    assert!(
        app.tabs.is_empty() && app.launch_queue.is_empty(),
        "A menu click must not launch a connection"
    );
    assert!(
        !app.store.data_dir.exists(),
        "A menu click must not save the catalog"
    );
}

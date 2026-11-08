//! Comprehensive integration tests for strata_ui components and primitives.

use strata_dom::{Attribute, NodeBlueprint, Signal, Tag};
use strata_icons::prelude::IconName;
use strata_primitives::{Point, Rect, Size};
use strata_ui::prelude::*;

#[test]
fn test_headless_primitives() {
    // FocusTrap
    let mut trap = FocusTrap::new(4);
    assert_eq!(trap.active_index, 0);
    assert_eq!(trap.next(), 1);
    assert_eq!(trap.next(), 2);
    assert_eq!(trap.next(), 3);
    assert_eq!(trap.next(), 0); // Wraps around
    assert_eq!(trap.prev(), 3); // Wraps backward

    // RovingIndex
    let mut roving = RovingIndex::new(3, Orientation::Horizontal);
    assert_eq!(roving.handle_key("ArrowRight"), Some(1));
    assert_eq!(roving.handle_key("ArrowRight"), Some(2));
    assert_eq!(roving.handle_key("ArrowRight"), Some(0)); // Wraps
    assert_eq!(roving.handle_key("ArrowLeft"), Some(2));
    assert_eq!(roving.handle_key("Home"), Some(0));
    assert_eq!(roving.handle_key("End"), Some(2));

    // FloatingAnchor
    let anchor = Rect::from_xywh(100.0, 100.0, 80.0, 40.0);
    let floating_size = Size::new(120.0, 60.0);
    let bottom_pos = FloatingAnchor::compute_position(anchor, floating_size, Placement::Bottom, 8.0);
    assert_eq!(bottom_pos.y, 148.0); // 100 + 40 + 8
    assert_eq!(bottom_pos.x, 80.0); // 100 + (80 - 120)/2 = 80

    let flipped = FloatingAnchor::flip_if_overflow(
        Point::new(950.0, 500.0),
        Size::new(100.0, 50.0),
        Size::new(1000.0, 800.0),
    );
    assert_eq!(flipped.x, 900.0); // Clamped so 900 + 100 <= 1000

    // DismissableLayer
    assert!(DismissableLayer::should_dismiss_on_escape("Escape"));
    assert!(DismissableLayer::should_dismiss_on_escape("Esc"));
    assert!(!DismissableLayer::should_dismiss_on_escape("Enter"));
    assert!(DismissableLayer::should_dismiss_on_outside_click(
        Point::new(10.0, 10.0),
        Rect::from_xywh(50.0, 50.0, 100.0, 100.0),
    ));
}

#[test]
fn test_button_variants_and_loading() {
    let btn_primary = Button(
        Some(ButtonVariant::Primary),
        Some(ButtonSize::Md),
        None,
        None,
        Some("shadow-lg"),
        None,
        Some(IconName::Search),
        None,
        None,
    );
    match &btn_primary.root {
        NodeBlueprint::Element { tag, classes, attributes, children } => {
            assert_eq!(*tag, Tag::Button);
            assert!(classes.iter().any(|c| c.contains("bg-indigo-600")));
            assert!(classes.iter().any(|c| c.contains("shadow-lg")));
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "role" && value == "button")));
            assert_eq!(children.len(), 1); // prefix icon
        }
        _ => panic!("Expected Element"),
    }

    let btn_loading = Button(
        Some(ButtonVariant::Destructive),
        None,
        Some(true),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    match &btn_loading.root {
        NodeBlueprint::Element { classes, attributes, .. } => {
            assert!(classes.iter().any(|c| c.contains("bg-rose-600")));
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "aria-disabled" && value == "true")));
        }
        _ => panic!("Expected Element"),
    }

    let icon_btn = IconButton(
        IconName::Settings,
        Some(ButtonVariant::Ghost),
        Some(ButtonSize::Sm),
        Some(true),
        Some("Settings"),
        None,
        None,
        None,
    );
    match &icon_btn.root {
        NodeBlueprint::Element { classes, attributes, .. } => {
            assert!(classes.iter().any(|c| c.contains("rounded-full")));
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "title" && value == "Settings")));
        }
        _ => panic!("Expected Element"),
    }
}

#[test]
fn test_form_controls_and_signals() {
    let text_sig = Signal::new(String::from("admin@strata.dev"));
    let input_view = Input(
        Some("Email"),
        Some("Enter your email"),
        Some(text_sig.clone()),
        Some(IconName::Mail),
        None,
        Some("We respect your privacy"),
        Some(false),
        None,
        None,
        None,
    );
    match &input_view.root {
        NodeBlueprint::Element { tag, children, .. } => {
            assert_eq!(*tag, Tag::Div);
            assert!(children.len() >= 2); // label + input wrapper + helper
        }
        _ => panic!("Expected Element"),
    }

    let pass_sig = Signal::new(String::from("secret123"));
    let show_pass = Signal::new(false);
    let pass_view = PasswordInput(
        Some("Password"),
        None,
        Some(pass_sig),
        Some(show_pass.clone()),
        None,
        None,
    );
    assert!(!show_pass.get());
    match &pass_view.root {
        NodeBlueprint::Element { children, .. } => {
            assert_eq!(children.len(), 2);
        }
        _ => panic!("Expected Element"),
    }

    let num_sig = Signal::new(42);
    let num_view = NumberInput(
        Some("Quantity"),
        num_sig.clone(),
        Some(0),
        Some(100),
        Some(5),
        None,
        None,
    );
    assert_eq!(num_sig.get(), 42);
    match &num_view.root {
        NodeBlueprint::Element { tag, .. } => assert_eq!(*tag, Tag::Div),
        _ => panic!("Expected Element"),
    }

    let chk_sig = Signal::new(true);
    let chk_view = Checkbox(
        Some("Accept Terms"),
        chk_sig.clone(),
        None,
        None,
    );
    assert!(chk_sig.get());
    match &chk_view.root {
        NodeBlueprint::Element { tag, .. } => assert_eq!(*tag, Tag::Label),
        _ => panic!("Expected Element"),
    }

    let switch_sig = Signal::new(false);
    let switch_view = Switch(
        Some("Dark Mode"),
        Some("Enable dark theme interface"),
        switch_sig.clone(),
        None,
    );
    assert!(!switch_sig.get());
    match &switch_view.root {
        NodeBlueprint::Element { tag, .. } => assert_eq!(*tag, Tag::Div),
        _ => panic!("Expected Element"),
    }

    let slider_sig = Signal::new(75.0);
    let slider_view = Slider(0.0, 100.0, slider_sig, None, None);
    match &slider_view.root {
        NodeBlueprint::Element { tag, .. } => assert_eq!(*tag, Tag::Div),
        _ => panic!("Expected Element"),
    }
}

#[test]
fn test_data_display_components() {
    let card = Card(
        Some(vec![
            CardHeader(Some("Metrics Overview"), Some("Real-time telemetry"), None, None).root,
            CardContent(Some(vec![
                Stat("Cluster CPU", "84.2%", Some(12.5), Some("16 Cores"), None).root,
            ]), None).root,
            CardFooter(Some(vec![
                Badge(Some(BadgeVariant::Success), None, None).root,
            ]), None).root,
        ]),
        None,
    );
    match &card.root {
        NodeBlueprint::Element { tag, children, .. } => {
            assert_eq!(*tag, Tag::Div);
            assert_eq!(children.len(), 3);
        }
        _ => panic!("Expected Element"),
    }

    let active_tab = Signal::new(1);
    let tabs = Tabs(vec!["Overview", "Deployments", "Audit Logs"], active_tab.clone(), None);
    assert_eq!(active_tab.get(), 1);
    match &tabs.root {
        NodeBlueprint::Element { tag, children, .. } => {
            assert_eq!(*tag, Tag::Div);
            assert_eq!(children.len(), 1); // 1 Fragment child with mapped tab buttons
            match &children[0] {
                NodeBlueprint::Fragment(btns) => assert_eq!(btns.len(), 3),
                _ => panic!("Expected Fragment"),
            }
        }
        _ => panic!("Expected Element"),
    }

    let accord_open = Signal::new(true);
    let accordion = AccordionItem(
        "How does Strata eliminate Chromium?",
        accord_open.clone(),
        None,
        None,
    );
    assert!(accord_open.get());
    match &accordion.root {
        NodeBlueprint::Element { children, .. } => {
            assert_eq!(children.len(), 2); // button + content
        }
        _ => panic!("Expected Element"),
    }
}

#[test]
fn test_feedback_and_toast_queue() {
    Toast::clear();
    assert_eq!(Toast::get_all().len(), 0);

    let id1 = Toast::success("Deployment created successfully!");
    let id2 = Toast::error("Connection lost to cluster");
    assert_eq!(Toast::get_all().len(), 2);

    let all = Toast::get_all();
    assert_eq!(all[0].kind, ToastKind::Success);
    assert_eq!(all[1].kind, ToastKind::Error);

    Toast::dismiss(id1);
    assert_eq!(Toast::get_all().len(), 1);
    assert_eq!(Toast::get_all()[0].id, id2);

    Toast::clear();
    assert_eq!(Toast::get_all().len(), 0);

    let modal_open = Signal::new(false);
    let closed_modal = Modal(modal_open.clone(), "Confirm Action", None, None, None);
    assert!(matches!(&closed_modal.root, NodeBlueprint::Fragment(children) if children.is_empty()));

    modal_open.set(true);
    let open_modal = Modal(modal_open, "Confirm Action", Some("Are you sure?"), None, None);
    assert!(matches!(&open_modal.root, NodeBlueprint::Element { .. }));

    let alert = Alert(
        Some(AlertVariant::Warning),
        "Memory Limit Exceeded",
        Some("Node us-east-01 is nearing 90% capacity"),
        None,
        None,
    );
    match &alert.root {
        NodeBlueprint::Element { classes, .. } => {
            assert!(classes.iter().any(|c| c.contains("bg-amber-500/10")));
        }
        _ => panic!("Expected Element"),
    }
}

#[test]
fn test_navigation_and_composites() {
    let sidebar_items = vec![
        SidebarItemData {
            label: "Dashboard",
            icon: IconName::Activity,
            href: Some("/dashboard"),
            badge: None,
            is_active: true,
        },
        SidebarItemData {
            label: "Clusters",
            icon: IconName::Server,
            href: Some("/clusters"),
            badge: Some("4"),
            is_active: false,
        },
    ];
    let sidebar = Sidebar(sidebar_items, None, None, None, None);
    match &sidebar.root {
        NodeBlueprint::Element { tag, .. } => assert_eq!(*tag, Tag::Aside),
        _ => panic!("Expected Element"),
    }

    let breadcrumbs = vec![
        BreadcrumbItemData { label: "Home", href: Some("/"), is_current: false },
        BreadcrumbItemData { label: "Clusters", href: Some("/clusters"), is_current: false },
        BreadcrumbItemData { label: "us-east-01", href: None, is_current: true },
    ];
    let bc = Breadcrumb(breadcrumbs, None);
    match &bc.root {
        NodeBlueprint::Element { tag, children, .. } => {
            assert_eq!(*tag, Tag::Nav);
            assert_eq!(children.len(), 1);
            match &children[0] {
                NodeBlueprint::Fragment(items) => assert_eq!(items.len(), 3),
                _ => panic!("Expected Fragment"),
            }
        }
        _ => panic!("Expected Element"),
    }

    let page_sig = Signal::new(1);
    let pag = Pagination(10, page_sig.clone(), None);
    match &pag.root {
        NodeBlueprint::Element { children, .. } => {
            assert_eq!(children.len(), 3); // prev + label + next
        }
        _ => panic!("Expected Element"),
    }

    // DataTable test
    struct ServerNode {
        name: &'static str,
        cores: u32,
        status: &'static str,
    }

    let nodes = vec![
        ServerNode { name: "edge-01", cores: 32, status: "healthy" },
        ServerNode { name: "edge-02", cores: 64, status: "degraded" },
    ];

    let columns = vec![
        TableColumnDef::new("name", "Node Name", |n: &ServerNode| n.name.to_string()),
        TableColumnDef::new("cores", "CPU Cores", |n: &ServerNode| n.cores.to_string()),
        TableColumnDef::new("status", "Health Status", |n: &ServerNode| n.status.to_string()),
    ];

    let dt = DataTable(columns, &nodes, None);
    match &dt.root {
        NodeBlueprint::Element { tag, .. } => assert_eq!(*tag, Tag::Div),
        _ => panic!("Expected Element"),
    }
}

#[test]
fn test_canvas2d_and_scroll_view_components() {
    use std::rc::Rc;
    use strata_style::types::Color;

    // 1. Canvas2D Component
    let drawn = Rc::new(std::cell::Cell::new(false));
    let drawn_clone = drawn.clone();

    let canvas = Canvas2D(
        Some(400.0),
        Some(300.0),
        Some("shadow-2xl"),
        Some(Rc::new(move |ctx| {
            ctx.set_fill_style(Color::rgb(255, 0, 0));
            ctx.fill_rect(10.0, 10.0, 100.0, 100.0);
            drawn_clone.set(true);
        })),
    );

    assert!(drawn.get(), "Canvas2D on_draw closure must have been executed");
    match &canvas.root {
        NodeBlueprint::Element { tag, .. } => assert_eq!(*tag, Tag::Div),
        _ => panic!("Expected Element"),
    }

    // 2. Scrollbar Component
    let scrollbar = Scrollbar(
        Some(ScrollbarOrientation::Vertical),
        Some(0.5),
        Some(0.2),
        Some(1.0),
        None,
    );
    match &scrollbar.root {
        NodeBlueprint::Element { tag, .. } => assert_eq!(*tag, Tag::Div),
        _ => panic!("Expected Element"),
    }

    // 3. ScrollView Container
    let scroll_view = ScrollView(
        Some(vec![canvas.root]),
        Some("h-96 w-full"),
        Some(true),
    );
    match &scroll_view.root {
        NodeBlueprint::Element { tag, children, .. } => {
            assert_eq!(*tag, Tag::Div);
            assert!(!children.is_empty());
        }
        _ => panic!("Expected Element"),
    }
}

#[test]
fn test_hybrid_3d_and_gpu_surface_components() {
    use strata_dom::Tag;
    use strata_render::scene3d::Scene3D;

    // 1. HudCard Component
    let hud_card = HudCard(
        "Orbital Altitude",
        Some("w-48"),
        None,
    );
    match &hud_card.root {
        NodeBlueprint::Element { tag, classes, .. } => {
            assert_eq!(*tag, Tag::Div);
            assert!(classes.iter().any(|c| c.contains("backdrop-blur-md")));
        }
        _ => panic!("Expected Element"),
    }

    // 2. SceneViewport with 2D HUD children
    let scene = Scene3D::new();
    let viewport = SceneViewport(
        Some(scene),
        Some(800.0),
        Some(600.0),
        Some("border-neutral-700"),
        None,
        Some(vec![hud_card.root]),
    );
    match &viewport.root {
        NodeBlueprint::Element { tag, children, .. } => {
            assert_eq!(*tag, Tag::Div);
            assert!(children.len() >= 2, "Viewport must contain canvas backing and HUD overlay");
        }
        _ => panic!("Expected Element"),
    }

    // 3. GpuSurface Component
    let gpu_surface = GpuSurface(
        Some(640.0),
        Some(480.0),
        Some("rounded-2xl"),
        None,
    );
    match &gpu_surface.root {
        NodeBlueprint::Element { tag, classes, .. } => {
            assert_eq!(*tag, Tag::Div);
            assert!(classes.iter().any(|c| c.contains("bg-neutral-950")));
        }
        _ => panic!("Expected Element"),
    }
}

#[test]
fn test_menu_and_dropdown_components() {
    use std::cell::Cell;
    use std::rc::Rc;
    let clicked = Rc::new(Cell::new(false));
    let clicked_clone = clicked.clone();

    let item = MenuItem(
        "New Tab".to_string(),
        Some(IconName::Plus),
        Some("Ctrl+T".to_string()),
        None,
        None,
        Some(Rc::new(move || {
            clicked_clone.set(true);
        })),
    );

    match &item.root {
        NodeBlueprint::Element { tag, classes, .. } => {
            assert_eq!(*tag, Tag::Button);
            assert!(classes.iter().any(|c| c.contains("hover:bg-blue-600/30")));
        }
        _ => panic!("Expected Button Element"),
    }

    let menu = DropdownMenu(
        true,
        None,
        vec![
            item.root,
            MenuSeparator().root,
            MenuSectionHeader("Tools".to_string()).root,
        ],
    );

    match &menu.root {
        NodeBlueprint::Element { tag, children, .. } => {
            assert_eq!(*tag, Tag::Div);
            match &children[0] {
                NodeBlueprint::Fragment(items) => assert_eq!(items.len(), 3),
                _ => assert_eq!(children.len(), 3),
            }
        }
        _ => panic!("Expected Div Element"),
    }

    let closed_menu = DropdownMenu(false, None, vec![]);
    match &closed_menu.root {
        NodeBlueprint::Element { classes, .. } => {
            assert!(classes.iter().any(|c| c.contains("hidden")));
        }
        _ => panic!("Expected Div Element"),
    }
}

#[test]
fn test_modal_and_drawer_focus_trap_and_escape_dismissal() {
    // 1. Modal open and closed states, attributes, and keydown listeners
    let modal_open = Signal::new(true);
    let modal_view = Modal(
        modal_open.clone(),
        "Delete Project",
        Some("This action cannot be undone."),
        Some(vec![
            Button(
                Some(ButtonVariant::Destructive),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .root,
        ]),
        None,
    );

    match &modal_view.root {
        NodeBlueprint::Element { attributes, .. } => {
            // Verify ARIA dialog attributes and focus trap
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "role" && value == "dialog")));
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "aria-modal" && value == "true")));
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "tabindex" && value == "0")));
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "data-focus-trap" && value == "true")));
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "aria-labelledby" && value.starts_with("strata-modal-title-"))));
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "aria-describedby" && value.starts_with("strata-modal-desc-"))));

            // Find keydown listener and simulate Escape
            let keydown_attr = attributes.iter().find(|a| {
                matches!(
                    a,
                    Attribute::EventListener {
                        event: strata_dom::EventType::KeyDown,
                        ..
                    }
                )
            });
            assert!(keydown_attr.is_some(), "Modal must have keydown event listener");

            if let Some(Attribute::EventListener { handler, .. }) = keydown_attr {
                // Test Tab key navigation (does not dismiss)
                let tab_ev = strata_dom::Event {
                    event_type: strata_dom::EventType::KeyDown,
                    key: "Tab".to_string(),
                    ..Default::default()
                };
                handler(tab_ev);
                assert!(modal_open.get(), "Tab key must not close modal");

                // Test Shift+Tab key navigation (does not dismiss)
                let shift_tab_ev = strata_dom::Event {
                    event_type: strata_dom::EventType::KeyDown,
                    key: "Tab".to_string(),
                    shift_key: true,
                    ..Default::default()
                };
                handler(shift_tab_ev);
                assert!(modal_open.get(), "Shift+Tab key must not close modal");

                // Test Escape key dismissal
                let esc_ev = strata_dom::Event {
                    event_type: strata_dom::EventType::KeyDown,
                    key: "Escape".to_string(),
                    ..Default::default()
                };
                handler(esc_ev);
                assert!(!modal_open.get(), "Escape key must dismiss modal");
            }
        }
        _ => panic!("Expected Element root for open Modal"),
    }

    // 2. Drawer open and closed states, attributes, and keydown listeners
    let drawer_open = Signal::new(true);
    let drawer_view = Drawer(
        drawer_open.clone(),
        Some("right"),
        Some("Navigation Drawer"),
        Some(vec![
            Button(
                Some(ButtonVariant::Secondary),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .root,
        ]),
        None,
    );

    match &drawer_view.root {
        NodeBlueprint::Element { attributes, .. } => {
            // Verify ARIA dialog attributes and focus trap
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "role" && value == "dialog")));
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "aria-modal" && value == "true")));
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "tabindex" && value == "0")));
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "data-focus-trap" && value == "true")));
            assert!(attributes.iter().any(|a| matches!(a, Attribute::Property { key, value } if key == "aria-labelledby" && value.starts_with("strata-drawer-title-"))));

            // Find keydown listener and simulate Escape
            let keydown_attr = attributes.iter().find(|a| {
                matches!(
                    a,
                    Attribute::EventListener {
                        event: strata_dom::EventType::KeyDown,
                        ..
                    }
                )
            });
            assert!(keydown_attr.is_some(), "Drawer must have keydown event listener");

            if let Some(Attribute::EventListener { handler, .. }) = keydown_attr {
                let esc_ev = strata_dom::Event {
                    event_type: strata_dom::EventType::KeyDown,
                    key: "Escape".to_string(),
                    ..Default::default()
                };
                handler(esc_ev);
                assert!(!drawer_open.get(), "Escape key must dismiss drawer");
            }
        }
        _ => panic!("Expected Element root for open Drawer"),
    }
}



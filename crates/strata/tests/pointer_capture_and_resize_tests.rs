use std::cell::RefCell;
use std::rc::Rc;
use strata::AppRuntime;
use strata_dom::Event;
use strata_macro::rsx;
use strata_platform::{MouseButton, PlatformEvent};

type EventLog = Rc<RefCell<Vec<(&'static str, f64, f64)>>>;
type PointerButtonLog = Rc<RefCell<Vec<(&'static str, f64, i16)>>>;
type CancelLog = Rc<RefCell<Vec<(String, Option<String>)>>>;

#[test]
fn test_pointer_capture_tracks_fast_drag_and_release_outside_element() {
    let log: EventLog = Rc::new(RefCell::new(Vec::new()));
    let log_down = Rc::clone(&log);
    let log_move = Rc::clone(&log);
    let log_up = Rc::clone(&log);

    // A narrow 10px SplitPane divider at x=195..205
    let view = rsx! {
        <div class="w-[800px] h-[400px] flex">
            <div class="w-[195px] h-full" />
            <div
                class="w-[10px] h-full"
                onmousedown={move |ev: Event| {
                    ev.set_pointer_capture(ev.pointer_id);
                    log_down.borrow_mut().push(("down", ev.client_x, ev.client_y));
                }}
                onmousemove={move |ev: Event| {
                    log_move.borrow_mut().push(("move", ev.client_x, ev.client_y));
                }}
                onmouseup={move |ev: Event| {
                    log_up.borrow_mut().push(("up", ev.client_x, ev.client_y));
                }}
            />
            <div class="flex-1 h-full" />
        </div>
    };

    let mut runtime = AppRuntime::new(view, 800.0, 400.0, 1.0);
    runtime.tick_frame();

    // Press inside the 10px divider
    assert!(runtime.dispatch_pointer_down(200.0, 100.0));
    // Fast drag far outside divider bounds (x=600, y=350)
    assert!(runtime.dispatch_pointer_move(600.0, 350.0));
    // Release far outside divider bounds
    assert!(runtime.dispatch_pointer_up(600.0, 350.0));
    // Subsequent move after release must NOT fire on the divider
    assert!(!runtime.dispatch_pointer_move(700.0, 350.0));

    let events = log.borrow();
    assert_eq!(
        *events,
        vec![
            ("down", 200.0, 100.0),
            ("move", 600.0, 350.0),
            ("up", 600.0, 350.0),
        ]
    );
}

#[test]
fn test_implicit_drag_capture_and_cross_dock_drop_a_to_b() {
    let events: Rc<RefCell<Vec<(&'static str, f64)>>> = Rc::new(RefCell::new(Vec::new()));
    let ev_a_down = Rc::clone(&events);
    let ev_a_move = Rc::clone(&events);
    let ev_a_up = Rc::clone(&events);
    let ev_b_up = Rc::clone(&events);

    let view = rsx! {
        <div class="w-[800px] h-[400px] flex">
            <div
                id="dock-a"
                class="w-[200px] h-full"
                onmousedown={move |ev: Event| {
                    ev_a_down.borrow_mut().push(("a-down", ev.client_x));
                }}
                onmousemove={move |ev: Event| {
                    ev_a_move.borrow_mut().push(("a-move", ev.client_x));
                }}
                onmouseup={move |ev: Event| {
                    ev_a_up.borrow_mut().push(("a-up", ev.client_x));
                }}
            />
            <div
                id="dock-b"
                class="w-[200px] h-full"
                onmouseup={move |ev: Event| {
                    ev_b_up.borrow_mut().push(("b-drop", ev.client_x));
                }}
            />
        </div>
    };

    let mut runtime = AppRuntime::new(view, 800.0, 400.0, 1.0);
    runtime.tick_frame();

    // Start drag in Dock A (without calling set_pointer_capture)
    runtime.step_event(&PlatformEvent::MouseDown {
        x: 50.0,
        y: 50.0,
        button: MouseButton::Left,
    });
    // Move over Dock B -> Dock A still receives onmousemove
    runtime.step_event(&PlatformEvent::MouseMove { x: 300.0, y: 50.0 });
    // Release over Dock B -> Dock B receives drop AND Dock A receives mouseup
    runtime.step_event(&PlatformEvent::MouseUp {
        x: 300.0,
        y: 50.0,
        button: MouseButton::Left,
    });

    let recorded = events.borrow();
    assert!(recorded.contains(&("a-down", 50.0)));
    assert!(recorded.contains(&("a-move", 300.0)));
    assert!(recorded.contains(&("b-drop", 300.0)));
    assert!(recorded.contains(&("a-up", 300.0)));
}

#[test]
fn test_onresize_and_query_element_rect_report_laid_out_dimensions() {
    let observed: Rc<RefCell<Vec<(f64, f64)>>> = Rc::new(RefCell::new(Vec::new()));
    let observed_cb = Rc::clone(&observed);

    let view = rsx! {
        <div class="w-full h-full flex">
            <div class="w-[200px] h-full" />
            <div
                id="tab-group"
                class="flex-1 h-full"
                onresize={move |ev: Event| {
                    observed_cb.borrow_mut().push((ev.width, ev.height));
                }}
            />
        </div>
    };

    let mut runtime = AppRuntime::new(view, 1000.0, 600.0, 1.0);
    assert_eq!(observed.borrow().as_slice(), &[(800.0, 600.0)]);

    let rect = runtime
        .query_element_rect("tab-group")
        .expect("tab-group rect should be measurable");
    assert!((rect.width - 800.0).abs() < 0.5);
    assert!((rect.height - 600.0).abs() < 0.5);

    // Ticking again with unchanged size must NOT re-fire onresize
    runtime.tick_frame();
    assert_eq!(observed.borrow().len(), 1);

    // Resizing the window from 1000x600 to 640x480 must fire onresize with (440, 480)
    assert!(runtime.step_event(&PlatformEvent::WindowResized {
        width: 640,
        height: 480,
    }));
    assert_eq!(
        observed.borrow().as_slice(),
        &[(800.0, 600.0), (440.0, 480.0)]
    );
    let rect2 = runtime.query_element_rect("tab-group").unwrap();
    assert!((rect2.width - 440.0).abs() < 0.5);
    assert!((rect2.height - 480.0).abs() < 0.5);
}

#[test]
fn test_pointer_capture_survives_sibling_overlay_insert_and_remove_by_id() {
    let show_overlay = strata_dom::Signal::new(false);
    let log: Rc<RefCell<Vec<(&'static str, f64)>>> = Rc::new(RefCell::new(Vec::new()));

    let build_view = {
        let show_overlay = show_overlay.clone();
        let log = Rc::clone(&log);
        move || {
            let mut children = Vec::new();
            if show_overlay.get() {
                let log_ov_move = Rc::clone(&log);
                let log_ov_up = Rc::clone(&log);
                children.push(strata_dom::NodeBlueprint::Element {
                    tag: strata_dom::Tag::Div,
                    classes: vec!["w-[100px]".into(), "h-full".into()].into(),
                    attributes: vec![
                        strata_dom::Attribute::Property {
                            key: "id".into(),
                            value: "snap-preview".into(),
                        },
                        strata_dom::Attribute::event_listener(
                            strata_dom::EventType::MouseMove,
                            move |ev: Event| {
                                log_ov_move
                                    .borrow_mut()
                                    .push(("overlay-move", ev.client_x));
                            },
                        ),
                        strata_dom::Attribute::event_listener(
                            strata_dom::EventType::MouseUp,
                            move |ev: Event| {
                                log_ov_up.borrow_mut().push(("overlay-up", ev.client_x));
                            },
                        ),
                    ],
                    children: Vec::new(),
                });
            }

            let log_down = Rc::clone(&log);
            let log_move = Rc::clone(&log);
            let log_up = Rc::clone(&log);
            children.push(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["w-[100px]".into(), "h-full".into()].into(),
                attributes: vec![
                    strata_dom::Attribute::Property {
                        key: "id".into(),
                        value: "split-divider".into(),
                    },
                    strata_dom::Attribute::event_listener(
                        strata_dom::EventType::MouseDown,
                        move |ev: Event| {
                            ev.set_pointer_capture(ev.pointer_id);
                            log_down.borrow_mut().push(("divider-down", ev.client_x));
                        },
                    ),
                    strata_dom::Attribute::event_listener(
                        strata_dom::EventType::MouseMove,
                        move |ev: Event| {
                            log_move.borrow_mut().push(("divider-move", ev.client_x));
                        },
                    ),
                    strata_dom::Attribute::event_listener(
                        strata_dom::EventType::MouseUp,
                        move |ev: Event| {
                            log_up.borrow_mut().push(("divider-up", ev.client_x));
                        },
                    ),
                ],
                children: Vec::new(),
            });

            strata_dom::View::new(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["w-[800px]".into(), "h-[400px]".into(), "flex".into()].into(),
                attributes: Vec::new(),
                children,
            })
        }
    };

    let mut runtime = AppRuntime::new(build_view(), 800.0, 400.0, 1.0);
    runtime.tick_frame();

    // 1. Press on #split-divider at index [0]
    assert!(runtime.dispatch_pointer_down(50.0, 100.0));
    assert_eq!(
        runtime.pointer_capture_target.as_ref().map(|c| c.path.as_slice()),
        Some(&[0usize][..])
    );

    // 2. Insert #snap-preview BEFORE #split-divider -> shifts divider from [0] to [1]
    show_overlay.set(true);
    runtime.update_view(build_view());
    assert_eq!(
        runtime.pointer_capture_target.as_ref().map(|c| c.path.as_slice()),
        Some(&[1usize][..])
    );

    // 3. Move pointer over (20, 100) (physically over #snap-preview) -> MUST route to #split-divider!
    assert!(runtime.dispatch_pointer_move(20.0, 100.0));

    // 4. Remove #snap-preview mid-drag -> shifts divider back from [1] to [0]
    show_overlay.set(false);
    runtime.update_view(build_view());
    assert_eq!(
        runtime.pointer_capture_target.as_ref().map(|c| c.path.as_slice()),
        Some(&[0usize][..])
    );

    // 5. Continue move and release far outside divider -> MUST still hit #split-divider!
    assert!(runtime.dispatch_pointer_move(500.0, 200.0));
    assert!(runtime.dispatch_pointer_up(500.0, 200.0));
    assert!(runtime.pointer_capture_target.is_none());

    assert_eq!(
        *log.borrow(),
        vec![
            ("divider-down", 50.0),
            ("divider-move", 20.0),
            ("divider-move", 500.0),
            ("divider-up", 500.0),
        ]
    );
}

#[test]
fn test_pointer_capture_survives_sibling_shift_by_key_and_ancestor_key() {
    let show_ghost = strata_dom::Signal::new(false);
    let log: Rc<RefCell<Vec<(&'static str, f64)>>> = Rc::new(RefCell::new(Vec::new()));

    // Unkeyed descendant header inside a keyed ancestor panel, while a sibling drag-ghost
    // overlay is conditionally inserted/removed before the keyed panel.
    let build_view = {
        let show_ghost = show_ghost.clone();
        let log = Rc::clone(&log);
        move || {
            let mut root_children = Vec::new();
            if show_ghost.get() {
                root_children.push(strata_dom::NodeBlueprint::Element {
                    tag: strata_dom::Tag::Div,
                    classes: vec!["w-[120px]".into(), "h-full".into()].into(),
                    attributes: vec![strata_dom::Attribute::Property {
                        key: "key".into(),
                        value: "drag-ghost".into(),
                    }],
                    children: Vec::new(),
                });
            }

            let log_down = Rc::clone(&log);
            let log_move = Rc::clone(&log);
            let log_up = Rc::clone(&log);
            let unkeyed_header = strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["w-full".into(), "h-[40px]".into()].into(),
                attributes: vec![
                    strata_dom::Attribute::event_listener(
                        strata_dom::EventType::MouseDown,
                        move |ev: Event| {
                            ev.set_pointer_capture(ev.pointer_id);
                            log_down.borrow_mut().push(("header-down", ev.client_x));
                        },
                    ),
                    strata_dom::Attribute::event_listener(
                        strata_dom::EventType::MouseMove,
                        move |ev: Event| {
                            log_move.borrow_mut().push(("header-move", ev.client_x));
                        },
                    ),
                    strata_dom::Attribute::event_listener(
                        strata_dom::EventType::MouseUp,
                        move |ev: Event| {
                            log_up.borrow_mut().push(("header-up", ev.client_x));
                        },
                    ),
                ],
                children: Vec::new(),
            };

            root_children.push(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["w-[200px]".into(), "h-[200px]".into()].into(),
                attributes: vec![strata_dom::Attribute::Property {
                    key: "key".into(),
                    value: "floating-panel-1".into(),
                }],
                children: vec![unkeyed_header],
            });

            strata_dom::View::new(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["w-[800px]".into(), "h-[400px]".into(), "flex".into()].into(),
                attributes: Vec::new(),
                children: root_children,
            })
        }
    };

    let mut runtime = AppRuntime::new(build_view(), 800.0, 400.0, 1.0);
    runtime.tick_frame();

    // Press inside unkeyed header at [0, 0]
    assert!(runtime.dispatch_pointer_down(40.0, 20.0));
    assert_eq!(
        runtime.pointer_capture_target.as_ref().map(|c| c.path.as_slice()),
        Some(&[0usize, 0usize][..])
    );

    // Insert sibling drag-ghost before keyed panel -> shifts header to [1, 0]
    show_ghost.set(true);
    runtime.update_view(build_view());
    assert_eq!(
        runtime.pointer_capture_target.as_ref().map(|c| c.path.as_slice()),
        Some(&[1usize, 0usize][..])
    );

    assert!(runtime.dispatch_pointer_move(420.0, 180.0));
    assert!(runtime.dispatch_pointer_up(420.0, 180.0));
    assert!(runtime.pointer_capture_target.is_none());

    assert_eq!(
        *log.borrow(),
        vec![
            ("header-down", 40.0),
            ("header-move", 420.0),
            ("header-up", 420.0),
        ]
    );
}

#[test]
fn test_pointer_capture_survives_sibling_shift_by_entity_id() {
    let stable_eid = strata_dom::EntityId::next();
    let show_overlay = strata_dom::Signal::new(false);
    let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));

    let build_view = {
        let show_overlay = show_overlay.clone();
        let log = Rc::clone(&log);
        move || {
            let mut children = Vec::new();
            if show_overlay.get() {
                children.push(strata_dom::NodeBlueprint::Element {
                    tag: strata_dom::Tag::Div,
                    classes: vec!["w-[80px]".into(), "h-full".into()].into(),
                    attributes: Vec::new(),
                    children: Vec::new(),
                });
            }
            let log_down = Rc::clone(&log);
            let log_move = Rc::clone(&log);
            let log_up = Rc::clone(&log);
            children.push(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["w-[80px]".into(), "h-full".into()].into(),
                attributes: vec![
                    strata_dom::Attribute::Entity(stable_eid),
                    strata_dom::Attribute::event_listener(
                        strata_dom::EventType::MouseDown,
                        move |ev: Event| {
                            ev.set_pointer_capture(ev.pointer_id);
                            log_down.borrow_mut().push("down");
                        },
                    ),
                    strata_dom::Attribute::event_listener(
                        strata_dom::EventType::MouseMove,
                        move |_ev: Event| {
                            log_move.borrow_mut().push("move");
                        },
                    ),
                    strata_dom::Attribute::event_listener(
                        strata_dom::EventType::MouseUp,
                        move |_ev: Event| {
                            log_up.borrow_mut().push("up");
                        },
                    ),
                ],
                children: Vec::new(),
            });
            strata_dom::View::new(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["w-[800px]".into(), "h-[400px]".into(), "flex".into()].into(),
                attributes: Vec::new(),
                children,
            })
        }
    };

    let mut runtime = AppRuntime::new(build_view(), 800.0, 400.0, 1.0);
    runtime.tick_frame();

    assert!(runtime.dispatch_pointer_down(20.0, 20.0));
    show_overlay.set(true);
    runtime.update_view(build_view());
    assert_eq!(
        runtime.pointer_capture_target.as_ref().map(|c| c.path.as_slice()),
        Some(&[1usize][..])
    );

    assert!(runtime.dispatch_pointer_move(300.0, 200.0));
    assert!(runtime.dispatch_pointer_up(300.0, 200.0));
    assert_eq!(*log.borrow(), vec!["down", "move", "up"]);
}

#[test]
fn test_pointer_capture_follows_element_across_portal_and_fires_lostpointercapture() {
    let in_portal = strata_dom::Signal::new(false);
    let log: Rc<RefCell<Vec<(&'static str, f64)>>> = Rc::new(RefCell::new(Vec::new()));

    let build_view = {
        let in_portal = in_portal.clone();
        let log = Rc::clone(&log);
        move || {
            let log_down = Rc::clone(&log);
            let log_move = Rc::clone(&log);
            let log_up = Rc::clone(&log);
            let log_lost = Rc::clone(&log);

            let tab_el = rsx! {
                <div
                    id="tearoff-tab"
                    class="w-[120px] h-[36px]"
                    onmousedown={move |ev: Event| {
                        ev.set_pointer_capture(ev.pointer_id);
                        log_down.borrow_mut().push(("down", ev.client_x));
                    }}
                    onmousemove={move |ev: Event| {
                        log_move.borrow_mut().push(("move", ev.client_x));
                    }}
                    onmouseup={move |ev: Event| {
                        log_up.borrow_mut().push(("up", ev.client_x));
                    }}
                    onlostpointercapture={move |ev: Event| {
                        log_lost.borrow_mut().push(("lost-capture", ev.client_x));
                    }}
                />
            };

            if in_portal.get() {
                let portal_view = strata_dom::create_portal(tab_el);
                rsx! {
                    <div class="w-[800px] h-[400px]">
                        <div class="w-full h-[36px]" />
                        {portal_view}
                    </div>
                }
            } else {
                rsx! {
                    <div class="w-[800px] h-[400px]">
                        <div class="w-full h-[36px] flex">
                            {tab_el}
                        </div>
                    </div>
                }
            }
        }
    };

    let mut runtime = AppRuntime::new(build_view(), 800.0, 400.0, 1.0);
    runtime.tick_frame();

    assert!(runtime.dispatch_pointer_down(40.0, 16.0));
    // Move #tearoff-tab out of the tab bar into a top-layer Portal mid-drag
    in_portal.set(true);
    runtime.update_view(build_view());
    assert!(
        runtime.pointer_capture_target.is_some(),
        "Capture must remain active when the element moves across a portal preserving its id"
    );

    assert!(runtime.dispatch_pointer_move(450.0, 260.0));
    assert!(runtime.dispatch_pointer_up(450.0, 260.0));
    assert!(runtime.pointer_capture_target.is_none());

    assert_eq!(
        *log.borrow(),
        vec![
            ("down", 40.0),
            ("move", 450.0),
            ("up", 450.0),
            ("lost-capture", 450.0),
        ]
    );
}

#[test]
fn test_unmount_clears_capture_without_misrouting_and_guarantees_global_pointer_up() {
    let mounted = strata_dom::Signal::new(true);
    let drag_active = strata_dom::Signal::new(false);
    let log: PointerButtonLog = Rc::new(RefCell::new(Vec::new()));

    let log_global_up = Rc::clone(&log);
    let drag_global = drag_active.clone();
    let _global_up_guard = strata_dom::use_global_pointer_up(move |ev: Event| {
        drag_global.set(false);
        log_global_up
            .borrow_mut()
            .push(("global-up", ev.client_x, ev.button));
    });

    let build_view = {
        let mounted = mounted.clone();
        let drag_active = drag_active.clone();
        let log = Rc::clone(&log);
        move || {
            let mut children = Vec::new();
            if mounted.get() {
                let log_down = Rc::clone(&log);
                let log_move = Rc::clone(&log);
                let drag_down = drag_active.clone();
                children.push(strata_dom::NodeBlueprint::Element {
                    tag: strata_dom::Tag::Div,
                    classes: vec!["w-[100px]".into(), "h-[100px]".into()].into(),
                    attributes: vec![
                        strata_dom::Attribute::Property {
                            key: "id".into(),
                            value: "source-tab".into(),
                        },
                        strata_dom::Attribute::event_listener(
                            strata_dom::EventType::MouseDown,
                            move |ev: Event| {
                                ev.set_pointer_capture(ev.pointer_id);
                                drag_down.set(true);
                                log_down
                                    .borrow_mut()
                                    .push(("source-down", ev.client_x, ev.button));
                            },
                        ),
                        strata_dom::Attribute::event_listener(
                            strata_dom::EventType::MouseMove,
                            move |ev: Event| {
                                log_move
                                    .borrow_mut()
                                    .push(("source-move", ev.client_x, ev.button));
                            },
                        ),
                    ],
                    children: Vec::new(),
                });
            }

            let log_other_move = Rc::clone(&log);
            let log_other_up = Rc::clone(&log);
            children.push(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["w-[100px]".into(), "h-[100px]".into()].into(),
                attributes: vec![
                    strata_dom::Attribute::Property {
                        key: "id".into(),
                        value: "neighbor-tab".into(),
                    },
                    strata_dom::Attribute::event_listener(
                        strata_dom::EventType::MouseMove,
                        move |ev: Event| {
                            log_other_move
                                .borrow_mut()
                                .push(("neighbor-move", ev.client_x, ev.button));
                        },
                    ),
                    strata_dom::Attribute::event_listener(
                        strata_dom::EventType::MouseUp,
                        move |ev: Event| {
                            log_other_up
                                .borrow_mut()
                                .push(("neighbor-up", ev.client_x, ev.button));
                        },
                    ),
                ],
                children: Vec::new(),
            });

            strata_dom::View::new(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["w-[800px]".into(), "h-[400px]".into(), "flex".into()].into(),
                attributes: Vec::new(),
                children,
            })
        }
    };

    let mut runtime = AppRuntime::new(build_view(), 800.0, 400.0, 1.0);
    let log_win_up = Rc::clone(&log);
    let listener_id = runtime.on_pointer_up(move |ev: Event| {
        log_win_up
            .borrow_mut()
            .push(("window-up", ev.client_x, ev.button));
    });
    runtime.tick_frame();

    // 1. Start capture on #source-tab
    runtime.step_event(&PlatformEvent::MouseDown {
        x: 50.0,
        y: 50.0,
        button: MouseButton::Left,
    });
    assert!(drag_active.get());
    assert!(runtime.pointer_capture_target.is_some());

    // 2. Unmount #source-tab mid-drag -> #neighbor-tab shifts into index [0]
    mounted.set(false);
    runtime.update_view(build_view());
    assert!(
        runtime.pointer_capture_target.is_none(),
        "Unmounting the explicitly identified captured element must immediately clear capture"
    );

    // 3. Move over empty viewport (600, 300) -> MUST NOT mis-route to #neighbor-tab at index [0]
    runtime.step_event(&PlatformEvent::MouseMove { x: 600.0, y: 300.0 });

    // 4. Release over empty viewport (600, 300) -> both window and global on_pointer_up MUST fire!
    runtime.step_event(&PlatformEvent::MouseUp {
        x: 600.0,
        y: 300.0,
        button: MouseButton::Left,
    });
    assert!(!drag_active.get());

    assert!(runtime.remove_pointer_up_listener(listener_id));
    assert_eq!(
        *log.borrow(),
        vec![
            ("source-down", 50.0, 0),
            ("window-up", 600.0, 0),
            ("global-up", 600.0, 0),
        ]
    );
}

#[test]
fn test_window_blur_and_touch_cancel_fire_pointer_cancel_and_clear_capture() {
    let log: CancelLog = Rc::new(RefCell::new(Vec::new()));
    let drag_active = strata_dom::Signal::new(false);

    let log_global_cancel = Rc::clone(&log);
    let drag_cancel = drag_active.clone();
    let _global_cancel_guard = strata_dom::use_global_pointer_cancel(move |ev: Event| {
        drag_cancel.set(false);
        log_global_cancel
            .borrow_mut()
            .push(("global-cancel".into(), ev.data));
    });

    let log_down = Rc::clone(&log);
    let log_el_cancel = Rc::clone(&log);
    let log_el_lost = Rc::clone(&log);
    let drag_down = drag_active.clone();

    let log_builder_cancel = Rc::clone(&log);
    let mut app = strata::StrataApp::builder()
        .with_window(strata::WindowConfig {
            width: 800,
            height: 400,
            ..Default::default()
        })
        .on_pointer_cancel(move |ev: Event| {
            log_builder_cancel
                .borrow_mut()
                .push(("window-cancel".into(), ev.data));
        })
        .build(move || {
            let drag_down = drag_down.clone();
            let log_down = Rc::clone(&log_down);
            let log_el_cancel = Rc::clone(&log_el_cancel);
            let log_el_lost = Rc::clone(&log_el_lost);
            rsx! {
                <div class="w-[800px] h-[400px]">
                    <div
                        id="drag-card"
                        class="w-[200px] h-[120px]"
                        onmousedown={move |ev: Event| {
                            ev.set_pointer_capture(ev.pointer_id);
                            drag_down.set(true);
                            log_down.borrow_mut().push(("down".into(), None));
                        }}
                        onpointercancel={move |ev: Event| {
                            log_el_cancel.borrow_mut().push(("el-cancel".into(), ev.data));
                        }}
                        onlostpointercapture={move |_ev: Event| {
                            log_el_lost.borrow_mut().push(("el-lost".into(), None));
                        }}
                    />
                </div>
            }
        });

    // 1. Idle focus loss must NOT emit spurious pointercancel events
    app.dispatch_event(&PlatformEvent::WindowFocused(false));
    assert!(log.borrow().is_empty());

    // 2. Start drag capture on #drag-card
    app.dispatch_event(&PlatformEvent::MouseDown {
        x: 50.0,
        y: 50.0,
        button: MouseButton::Left,
    });
    assert!(drag_active.get());
    assert!(app.runtime.is_mouse_down);
    assert!(app.runtime.pointer_capture_target.is_some());

    // 3. CursorLeft during an active captured drag must preserve is_mouse_down & capture
    app.dispatch_event(&PlatformEvent::CursorLeft);
    assert!(app.runtime.is_mouse_down);
    assert!(app.runtime.pointer_capture_target.is_some());

    // 4. WindowFocused(false) mid-drag cancels capture and notifies element + window + global hooks
    app.dispatch_event(&PlatformEvent::WindowFocused(false));
    assert!(!drag_active.get());
    assert!(!app.runtime.is_mouse_down);
    assert!(app.runtime.pointer_capture_target.is_none());

    // 5. TouchPhase::Cancelled also fires pointercancel hooks
    app.dispatch_event(&PlatformEvent::Touch {
        touch_id: 9,
        phase: strata_platform::TouchPhase::Cancelled,
        x: 60.0,
        y: 60.0,
    });

    assert_eq!(
        *log.borrow(),
        vec![
            ("down".to_string(), None),
            ("el-cancel".to_string(), Some("window_blur".to_string())),
            ("el-lost".to_string(), None),
            ("window-cancel".to_string(), Some("window_blur".to_string())),
            ("global-cancel".to_string(), Some("window_blur".to_string())),
            ("el-cancel".to_string(), Some("touch_cancelled".to_string())),
            (
                "window-cancel".to_string(),
                Some("touch_cancelled".to_string())
            ),
            (
                "global-cancel".to_string(),
                Some("touch_cancelled".to_string())
            ),
        ]
    );
}

#[test]
fn test_pointer_capture_survives_unkeyed_descendant_shift_under_identified_parent() {
    let show_overlay = strata_dom::Signal::new(false);
    let log: Rc<RefCell<Vec<(&'static str, f64)>>> = Rc::new(RefCell::new(Vec::new()));

    // Neither the floating window nor its header has an explicit `id`/`key`/`EntityId`,
    // and the conditionally inserted snap-preview overlay even contains a child `div`
    // so the stale `[0, 0]` path still resolves to a `Tag::Div`!
    let build_view = {
        let show_overlay = show_overlay.clone();
        let log = Rc::clone(&log);
        move || {
            let mut root_children = Vec::new();
            if show_overlay.get() {
                root_children.push(strata_dom::NodeBlueprint::Element {
                    tag: strata_dom::Tag::Div,
                    classes: vec!["w-[120px]".into(), "h-full".into()].into(),
                    attributes: Vec::new(),
                    children: vec![strata_dom::NodeBlueprint::Element {
                        tag: strata_dom::Tag::Div,
                        classes: vec!["w-full".into(), "h-[40px]".into()].into(),
                        attributes: Vec::new(),
                        children: Vec::new(),
                    }],
                });
            }

            let log_down = Rc::clone(&log);
            let log_move = Rc::clone(&log);
            let log_up = Rc::clone(&log);
            root_children.push(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["w-[200px]".into(), "h-[200px]".into()].into(),
                attributes: Vec::new(),
                children: vec![strata_dom::NodeBlueprint::Element {
                    tag: strata_dom::Tag::Div,
                    classes: vec!["w-full".into(), "h-[40px]".into()].into(),
                    attributes: vec![
                        strata_dom::Attribute::event_listener(
                            strata_dom::EventType::MouseDown,
                            move |ev: Event| {
                                ev.set_pointer_capture(ev.pointer_id);
                                log_down.borrow_mut().push(("down", ev.client_x));
                            },
                        ),
                        strata_dom::Attribute::event_listener(
                            strata_dom::EventType::MouseMove,
                            move |ev: Event| {
                                log_move.borrow_mut().push(("move", ev.client_x));
                            },
                        ),
                        strata_dom::Attribute::event_listener(
                            strata_dom::EventType::MouseUp,
                            move |ev: Event| {
                                log_up.borrow_mut().push(("up", ev.client_x));
                            },
                        ),
                    ],
                    children: Vec::new(),
                }],
            });

            strata_dom::View::new(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["w-[800px]".into(), "h-[400px]".into(), "flex".into()].into(),
                attributes: vec![strata_dom::Attribute::Property {
                    key: "id".into(),
                    value: "workspace-root".into(),
                }],
                children: root_children,
            })
        }
    };

    let mut runtime = AppRuntime::new(build_view(), 800.0, 400.0, 1.0);
    runtime.tick_frame();

    // 1. Capture unkeyed header at [0, 0]
    assert!(runtime.dispatch_pointer_down(40.0, 20.0));
    assert_eq!(
        runtime.pointer_capture_target.as_ref().map(|c| c.path.as_slice()),
        Some(&[0usize, 0usize][..])
    );

    // 2. Insert nested snap-preview overlay at [0] -> shifts unkeyed header to [1, 0]
    show_overlay.set(true);
    runtime.update_view(build_view());
    assert_eq!(
        runtime.pointer_capture_target.as_ref().map(|c| c.path.as_slice()),
        Some(&[1usize, 0usize][..])
    );
    assert!(runtime.dispatch_pointer_move(300.0, 150.0));

    // 3. Remove snap-preview overlay -> shifts unkeyed header back to [0, 0]
    show_overlay.set(false);
    runtime.update_view(build_view());
    assert_eq!(
        runtime.pointer_capture_target.as_ref().map(|c| c.path.as_slice()),
        Some(&[0usize, 0usize][..])
    );
    assert!(runtime.dispatch_pointer_move(450.0, 220.0));
    assert!(runtime.dispatch_pointer_up(450.0, 220.0));
    assert!(runtime.pointer_capture_target.is_none());

    assert_eq!(
        *log.borrow(),
        vec![
            ("down", 40.0),
            ("move", 300.0),
            ("move", 450.0),
            ("up", 450.0),
        ]
    );
}

#[test]
fn test_unmounting_captured_subtree_delivers_lostpointercapture_and_rebuilds_on_global_pointer_up() {
    let mounted = strata_dom::Signal::new(true);
    let lost_reasons: Rc<RefCell<Vec<Option<String>>>> = Rc::new(RefCell::new(Vec::new()));
    let non_signal_drag_active = Rc::new(std::cell::Cell::new(false));

    let drag_up = Rc::clone(&non_signal_drag_active);
    let _global_up = strata_dom::use_global_pointer_up(move |_ev: Event| {
        drag_up.set(false);
    });

    let build_view = {
        let mounted = mounted.clone();
        let lost_reasons = Rc::clone(&lost_reasons);
        let non_signal_drag_active = Rc::clone(&non_signal_drag_active);
        move || {
            if mounted.get() {
                let lost_cb = Rc::clone(&lost_reasons);
                let drag_down = Rc::clone(&non_signal_drag_active);
                rsx! {
                    <div class="w-[800px] h-[400px]">
                        <div
                            id="tearoff-handle"
                            class="w-[140px] h-[40px]"
                            onmousedown={move |ev: Event| {
                                ev.set_pointer_capture(ev.pointer_id);
                                drag_down.set(true);
                            }}
                            onlostpointercapture={move |ev: Event| {
                                lost_cb.borrow_mut().push(ev.data);
                            }}
                        />
                    </div>
                }
            } else {
                rsx! {
                    <div class="w-[800px] h-[400px]" />
                }
            }
        }
    };

    let mut runtime = AppRuntime::new(build_view(), 800.0, 400.0, 1.0);
    runtime.tick_frame();

    // 1. Press down and acquire explicit pointer capture
    runtime.step_event(&PlatformEvent::MouseDown {
        x: 40.0,
        y: 20.0,
        button: MouseButton::Left,
    });
    assert!(non_signal_drag_active.get());
    assert!(runtime.pointer_capture_target.is_some());

    // 2. Unmount the captured subtree mid-drag -> lostpointercapture MUST fire immediately
    mounted.set(false);
    runtime.update_view(build_view());
    assert!(runtime.pointer_capture_target.is_none());
    assert_eq!(
        *lost_reasons.borrow(),
        vec![Some("target_unmounted".to_string())]
    );

    // 3. Releasing mouse over empty space still fires use_global_pointer_up AND requests rebuild
    //    even though the listener mutated plain Rc<Cell<bool>> instead of a Signal.
    let _ = strata_dom::reactive::take_dirty_signals();
    let up_action = runtime.dispatch_pointer_up_action_with_button(600.0, 300.0, 0);
    assert!(!non_signal_drag_active.get());
    assert_eq!(up_action, strata::PointerUpAction::DomListenerInvoked);
}

#[test]
fn test_platform_pointer_cancel_grab_broken_cancels_active_capture() {
    let log: CancelLog = Rc::new(RefCell::new(Vec::new()));
    let log_el_cancel = Rc::clone(&log);
    let log_el_lost = Rc::clone(&log);
    let log_win_cancel = Rc::clone(&log);

    let view = rsx! {
        <div class="w-[800px] h-[400px]">
            <div
                id="drag-panel"
                class="w-[200px] h-[100px]"
                onmousedown={|ev: Event| {
                    ev.set_pointer_capture(ev.pointer_id);
                }}
                onpointercancel={move |ev: Event| {
                    log_el_cancel.borrow_mut().push(("el-cancel".into(), ev.data));
                }}
                onlostpointercapture={move |_ev: Event| {
                    log_el_lost.borrow_mut().push(("el-lost".into(), None));
                }}
            />
        </div>
    };

    let mut runtime = AppRuntime::new(view, 800.0, 400.0, 1.0);
    runtime.on_pointer_cancel(move |ev: Event| {
        log_win_cancel
            .borrow_mut()
            .push(("window-cancel".into(), ev.data));
    });
    runtime.tick_frame();

    runtime.step_event(&PlatformEvent::MouseDown {
        x: 50.0,
        y: 50.0,
        button: MouseButton::Left,
    });
    assert!(runtime.pointer_capture_target.is_some());

    // Simulate OS/WM breaking the active pointer grab (e.g. GTK grab-broken-event)
    assert!(runtime.step_event(&PlatformEvent::PointerCancel));
    assert!(runtime.pointer_capture_target.is_none());
    assert!(!runtime.is_mouse_down);

    assert_eq!(
        *log.borrow(),
        vec![
            ("el-cancel".to_string(), Some("grab_broken".to_string())),
            ("el-lost".to_string(), None),
            ("window-cancel".to_string(), Some("grab_broken".to_string())),
        ]
    );
}

#[test]
fn test_directional_resize_cursors_tailwind_and_inline_css_hit_test() {
    use strata_platform::window::CursorIcon;

    let view = rsx! {
        <div class="w-[700px] h-[100px] flex">
            <div class="w-[50px] h-full cursor-ns-resize" />
            <div class="w-[50px] h-full cursor-ew-resize" />
            <div class="w-[50px] h-full cursor-nwse-resize" />
            <div class="w-[50px] h-full cursor-nesw-resize" />
            <div class="w-[50px] h-full cursor-n-resize" />
            <div class="w-[50px] h-full cursor-s-resize" />
            <div class="w-[50px] h-full cursor-e-resize" />
            <div class="w-[50px] h-full cursor-w-resize" />
            <div class="w-[50px] h-full cursor-ne-resize" />
            <div class="w-[50px] h-full cursor-nw-resize" />
            <div class="w-[50px] h-full cursor-se-resize" />
            <div class="w-[50px] h-full cursor-sw-resize" />
            <div class="w-[50px] h-full" style="cursor: nwse-resize;" />
            <div class="w-[50px] h-full" style="cursor: nesw-resize;" />
        </div>
    };

    let mut runtime = AppRuntime::new(view, 700.0, 100.0, 1.0);
    runtime.tick_frame();

    let expected = [
        CursorIcon::ResizeN,  // cursor-ns-resize
        CursorIcon::ResizeE,  // cursor-ew-resize
        CursorIcon::ResizeNw, // cursor-nwse-resize
        CursorIcon::ResizeNe, // cursor-nesw-resize
        CursorIcon::ResizeN,  // cursor-n-resize
        CursorIcon::ResizeS,  // cursor-s-resize
        CursorIcon::ResizeE,  // cursor-e-resize
        CursorIcon::ResizeW,  // cursor-w-resize
        CursorIcon::ResizeNe, // cursor-ne-resize
        CursorIcon::ResizeNw, // cursor-nw-resize
        CursorIcon::ResizeSe, // cursor-se-resize
        CursorIcon::ResizeSw, // cursor-sw-resize
        CursorIcon::ResizeNw, // style="cursor: nwse-resize;"
        CursorIcon::ResizeNe, // style="cursor: nesw-resize;"
    ];

    for (idx, exp) in expected.iter().enumerate() {
        let x = idx as f32 * 50.0 + 25.0;
        let got = AppRuntime::hit_test_cursor(&runtime.root_blueprint, &runtime.layout_root, x, 50.0);
        assert_eq!(got, *exp, "slot #{idx} at x={x} should resolve {:?}", exp);
    }
}

#[test]
fn test_active_pointer_capture_locks_cursor_across_fast_drag_until_release_or_cancel() {
    use strata_platform::window::CursorIcon;

    strata_dom::clear_cursor_override();
    let show_overlay = strata_dom::Signal::new(false);

    let build_view = {
        let show_overlay = show_overlay.clone();
        move || {
            let mut children = Vec::new();
            if show_overlay.get() {
                children.push(strata_dom::NodeBlueprint::Element {
                    tag: strata_dom::Tag::Div,
                    classes: vec!["w-[100px]".into(), "h-full".into()].into(),
                    attributes: vec![strata_dom::Attribute::Property {
                        key: "id".into(),
                        value: "snap-overlay".into(),
                    }],
                    children: Vec::new(),
                });
            }
            children.push(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec![
                    "w-[10px]".into(),
                    "h-full".into(),
                    "cursor-ew-resize".into(),
                ]
                .into(),
                attributes: vec![
                    strata_dom::Attribute::Property {
                        key: "id".into(),
                        value: "resize-handle".into(),
                    },
                    strata_dom::Attribute::event_listener(
                        strata_dom::EventType::MouseDown,
                        move |ev: Event| {
                            ev.set_pointer_capture(ev.pointer_id);
                        },
                    ),
                ],
                children: Vec::new(),
            });
            children.push(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Button,
                classes: vec!["w-[150px]".into(), "h-full".into()].into(),
                attributes: Vec::new(),
                children: Vec::new(),
            });
            children.push(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["w-[150px]".into(), "h-full".into(), "cursor-text".into()].into(),
                attributes: Vec::new(),
                children: Vec::new(),
            });
            children.push(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["flex-1".into(), "h-full".into()].into(),
                attributes: Vec::new(),
                children: Vec::new(),
            });

            strata_dom::View::new(strata_dom::NodeBlueprint::Element {
                tag: strata_dom::Tag::Div,
                classes: vec!["w-[800px]".into(), "h-[400px]".into(), "flex".into()].into(),
                attributes: Vec::new(),
                children,
            })
        }
    };

    let mut runtime = AppRuntime::new(build_view(), 800.0, 400.0, 1.0);
    runtime.tick_frame();

    // Hover over the narrow resize handle -> ResizeE
    runtime.step_event(&PlatformEvent::MouseMove { x: 5.0, y: 100.0 });
    assert_eq!(runtime.current_cursor, CursorIcon::ResizeE);

    // Press handle to acquire pointer capture
    runtime.step_event(&PlatformEvent::MouseDown {
        x: 5.0,
        y: 100.0,
        button: MouseButton::Left,
    });
    assert_eq!(runtime.current_cursor, CursorIcon::ResizeE);

    // Fast drag over adjacent Button (x=80), Text region (x=220), and Default region (x=600)
    // MUST keep cursor locked to ResizeE!
    runtime.step_event(&PlatformEvent::MouseMove { x: 80.0, y: 100.0 });
    assert_eq!(runtime.current_cursor, CursorIcon::ResizeE);
    runtime.step_event(&PlatformEvent::MouseMove { x: 220.0, y: 100.0 });
    assert_eq!(runtime.current_cursor, CursorIcon::ResizeE);
    runtime.step_event(&PlatformEvent::MouseMove { x: 600.0, y: 100.0 });
    assert_eq!(runtime.current_cursor, CursorIcon::ResizeE);

    // CursorLeft during active capture must not flicker cursor back to Default
    runtime.step_event(&PlatformEvent::CursorLeft);
    assert_eq!(runtime.current_cursor, CursorIcon::ResizeE);

    // Insert sibling overlay shifting #resize-handle from index 0 to index 1 mid-drag -> still ResizeE
    show_overlay.set(true);
    runtime.update_view(build_view());
    runtime.step_event(&PlatformEvent::MouseMove { x: 50.0, y: 100.0 });
    assert_eq!(runtime.current_cursor, CursorIcon::ResizeE);

    // Release over Button (now at x=110..260) -> capture ends, cursor immediately switches to Pointer
    runtime.step_event(&PlatformEvent::MouseUp {
        x: 150.0,
        y: 100.0,
        button: MouseButton::Left,
    });
    assert_eq!(runtime.current_cursor, CursorIcon::Pointer);

    // Re-capture handle (now at x=100..110), drag over text pane (x=300), then cancel via focus loss
    runtime.step_event(&PlatformEvent::MouseDown {
        x: 105.0,
        y: 100.0,
        button: MouseButton::Left,
    });
    runtime.step_event(&PlatformEvent::MouseMove { x: 300.0, y: 100.0 });
    assert_eq!(runtime.current_cursor, CursorIcon::ResizeE);

    runtime.step_event(&PlatformEvent::WindowFocused(false));
    assert_eq!(runtime.current_cursor, CursorIcon::Text);
}

#[test]
fn test_strata_dom_current_cursor_override_takes_precedence_in_resolved_cursor_at() {
    use strata_platform::window::CursorIcon as PlatCursor;

    strata_dom::clear_cursor_override();
    let view = rsx! {
        <div class="w-[400px] h-[200px] flex">
            <button
                class="w-[200px] h-full"
                onmousedown={|ev: Event| {
                    ev.set_pointer_capture(ev.pointer_id);
                }}
            />
            <div class="w-[200px] h-full cursor-text" />
        </div>
    };

    let mut runtime = AppRuntime::new(view, 400.0, 200.0, 1.0);
    runtime.tick_frame();

    // Passive hit-test over button -> Pointer, over right pane -> Text
    assert_eq!(runtime.resolved_cursor_at(50.0, 50.0), PlatCursor::Pointer);
    assert_eq!(runtime.resolved_cursor_at(300.0, 50.0), PlatCursor::Text);

    // 1. Explicit set_current_cursor beats both passive hit-testing AND active pointer capture
    runtime.step_event(&PlatformEvent::MouseDown {
        x: 50.0,
        y: 50.0,
        button: MouseButton::Left,
    });
    assert_eq!(runtime.current_cursor, PlatCursor::Pointer);

    strata_dom::set_current_cursor(strata_dom::CursorIcon::NwseResize);
    runtime.step_event(&PlatformEvent::MouseMove { x: 300.0, y: 50.0 });
    assert_eq!(runtime.current_cursor, PlatCursor::ResizeNw);
    assert_eq!(runtime.resolved_cursor_at(300.0, 50.0), PlatCursor::ResizeNw);

    // Clearing override returns control to active pointer capture (button -> Pointer)
    strata_dom::clear_cursor_override();
    assert_eq!(runtime.resolved_cursor_at(300.0, 50.0), PlatCursor::Pointer);

    // Releasing capture restores passive hit-testing (x=300 -> Text)
    runtime.step_event(&PlatformEvent::MouseUp {
        x: 300.0,
        y: 50.0,
        button: MouseButton::Left,
    });
    assert_eq!(runtime.current_cursor, PlatCursor::Text);

    // 2. Reactive use_cursor hook also overrides passive hit-testing while signal is active
    let resizing = strata_dom::Signal::new(false);
    strata_dom::with_root_component_scope(|| {
        let _comp = strata_dom::ComponentScopeGuard::enter("CursorOverrideOwner");
        strata_dom::use_cursor(strata_dom::CursorIcon::NeswResize, resizing.clone());
    });
    assert_eq!(runtime.resolved_cursor_at(50.0, 50.0), PlatCursor::Pointer);

    resizing.set(true);
    assert_eq!(runtime.resolved_cursor_at(50.0, 50.0), PlatCursor::ResizeNe);
    assert_eq!(runtime.resolved_cursor_at(300.0, 50.0), PlatCursor::ResizeNe);

    resizing.set(false);
    assert_eq!(runtime.resolved_cursor_at(50.0, 50.0), PlatCursor::Pointer);

    // Clean up component hook state so sibling tests remain isolated
    strata_dom::reset_component_hooks();
    strata_dom::clear_cursor_override();
}

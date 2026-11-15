use std::cell::RefCell;
use std::rc::Rc;
use strata::AppRuntime;
use strata_dom::Event;
use strata_macro::rsx;
use strata_platform::{MouseButton, PlatformEvent};

type EventLog = Rc<RefCell<Vec<(&'static str, f64, f64)>>>;

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

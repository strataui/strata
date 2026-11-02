use strata_platform::{
    KeyCode, Modifiers, PlatformEvent, ShortcutManager, ShortcutTrigger,
};

#[test]
fn test_shortcut_trigger_parsing_and_matching() {
    let t1 = ShortcutTrigger::parse("F12").expect("F12 should parse");
    assert_eq!(t1.key, KeyCode::F12);
    assert!(!t1.modifiers.ctrl);
    assert!(!t1.modifiers.shift);

    let t2 = ShortcutTrigger::parse("Ctrl+Shift+I").expect("Ctrl+Shift+I should parse");
    assert_eq!(t2.key, KeyCode::KeyI);
    assert!(t2.modifiers.ctrl);
    assert!(t2.modifiers.shift);
    assert!(!t2.modifiers.alt);

    let t3 = ShortcutTrigger::parse("Alt+Left").expect("Alt+Left should parse");
    assert_eq!(t3.key, KeyCode::ArrowLeft);
    assert!(t3.modifiers.alt);
    assert!(!t3.modifiers.ctrl);

    // Matching
    let event_modifiers = Modifiers {
        ctrl: true,
        shift: true,
        alt: false,
        meta: false,
    };
    assert!(t2.matches(KeyCode::KeyI, event_modifiers));
    assert!(!t2.matches(KeyCode::KeyT, event_modifiers));
}

#[test]
fn test_shortcut_manager_dispatch() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let mut manager = ShortcutManager::new();
    let triggered = Arc::new(AtomicBool::new(false));

    let trig_clone = triggered.clone();
    assert!(manager.register("F12", move || {
        trig_clone.store(true, Ordering::SeqCst);
        true
    }));

    assert_eq!(manager.len(), 1);

    // Dispatch non-matching event
    let non_matching = PlatformEvent::KeyDown {
        key: KeyCode::KeyA,
        modifiers: Modifiers::default(),
    };
    assert!(!manager.dispatch(&non_matching));
    assert!(!triggered.load(Ordering::SeqCst));

    // Dispatch matching event
    let matching = PlatformEvent::KeyDown {
        key: KeyCode::F12,
        modifiers: Modifiers::default(),
    };
    assert!(manager.dispatch(&matching));
    assert!(triggered.load(Ordering::SeqCst));
}

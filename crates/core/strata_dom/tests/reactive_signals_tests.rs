//! Integration Tests: Next-Gen Reactive Signal Engine, Read/Write Split, and SpringSignal Physics.

use std::cell::Cell;
use std::rc::Rc;
use strata_dom::reactive::{create_signal, Effect};
use strata_dom::spring::{SpringConfig, SpringSignal};

#[test]
fn test_decoupled_read_write_signals() {
    let (read_count, set_count) = create_signal(10);
    assert_eq!(read_count.get(), 10);

    let effect_runs = Rc::new(Cell::new(0));
    let observed_value = Rc::new(Cell::new(0));

    let runs_clone = effect_runs.clone();
    let obs_clone = observed_value.clone();
    let r_clone = read_count.clone();

    let _effect = Effect::new(move || {
        runs_clone.set(runs_clone.get() + 1);
        obs_clone.set(r_clone.get());
        None
    });

    assert_eq!(effect_runs.get(), 1);
    assert_eq!(observed_value.get(), 10);

    // Mutate via write capability
    set_count.set(42);
    assert_eq!(read_count.get(), 42);
    assert_eq!(effect_runs.get(), 2);
    assert_eq!(observed_value.get(), 42);

    // Update via in-place closure
    set_count.update(|val| *val += 18);
    assert_eq!(read_count.get(), 60);
    assert_eq!(effect_runs.get(), 3);
    assert_eq!(observed_value.get(), 60);
}

#[test]
fn test_spring_signal_physics_and_oscillation() {
    let config = SpringConfig::stiff();
    let spring = SpringSignal::with_config(0.0, config);

    assert_eq!(spring.get(), 0.0);
    assert!(spring.is_at_rest());

    // Effect tracking spring signal mutations
    let tick_count = Rc::new(Cell::new(0));
    let latest_val = Rc::new(Cell::new(0.0f32));

    let tc = tick_count.clone();
    let lv = latest_val.clone();
    let s_clone = spring.clone();

    let _effect = Effect::new(move || {
        tc.set(tc.get() + 1);
        lv.set(s_clone.get());
        None
    });

    assert_eq!(tick_count.get(), 1);
    assert_eq!(latest_val.get(), 0.0);

    // Displace target to 150.0
    spring.set_target(150.0);
    assert!(!spring.is_at_rest());

    // Advance 60 FPS simulation loop until rest
    let mut frames = 0;
    while frames < 300 && spring.tick(0.016) {
        frames += 1;
    }

    assert!(spring.is_at_rest(), "Spring must settle at equilibrium");
    assert!((spring.get() - 150.0).abs() < 1e-2, "Spring position must be at target 150.0");
    assert!(tick_count.get() > 10, "Effect must have been reactively triggered on simulation ticks");

    // Snap to test instant teleport
    spring.snap_to(50.0);
    assert_eq!(spring.get(), 50.0);
    assert_eq!(spring.target(), 50.0);
    assert_eq!(spring.velocity(), 0.0);
    assert!(spring.is_at_rest());
}

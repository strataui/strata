//! Fine-grained reactive telemetry hooks and memory pressure callback registry.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use strata_dom::reactive::Signal;

use crate::mock::DeviceMockController;
use crate::types::*;

struct ThreadLocalSignals {
    battery: Signal<BatteryInfo>,
    thermal: Signal<ThermalState>,
    network: Signal<NetworkStatus>,
    screen: Signal<ScreenGeometry>,
    display_refresh: Signal<DisplayRefreshInfo>,
}

thread_local! {
    static SIGNALS: RefCell<Option<ThreadLocalSignals>> = const { RefCell::new(None) };
}

fn with_or_init_signals<R>(f: impl FnOnce(&ThreadLocalSignals) -> R) -> R {
    SIGNALS.with(|cell| {
        let mut borrow = cell.borrow_mut();
        if borrow.is_none() {
            let mock = DeviceMockController::new();
            *borrow = Some(ThreadLocalSignals {
                battery: Signal::new(mock.battery()),
                thermal: Signal::new(mock.thermal()),
                network: Signal::new(mock.network()),
                screen: Signal::new(mock.screen()),
                display_refresh: Signal::new(mock.display_refresh()),
            });
        }
        f(borrow.as_ref().unwrap())
    })
}

/// Synchronizes current thread's reactive signals with the mock controller state.
pub(crate) fn sync_signals_from_mock(mock: &DeviceMockController) {
    SIGNALS.with(|cell| {
        if let Ok(borrow) = cell.try_borrow() {
            if let Some(signals) = borrow.as_ref() {
                signals.battery.set(mock.battery());
                signals.thermal.set(mock.thermal());
                signals.network.set(mock.network());
                signals.screen.set(mock.screen());
                signals.display_refresh.set(mock.display_refresh());
            }
        }
    });
}

/// Reactive hook returning real-time battery status and charge level.
pub fn use_battery() -> Signal<BatteryInfo> {
    with_or_init_signals(|s| s.battery.clone())
}

/// Reactive hook returning current device thermal throttling state.
pub fn use_thermal() -> Signal<ThermalState> {
    with_or_init_signals(|s| s.thermal.clone())
}

/// Reactive hook returning network connectivity, bandwidth, and metered state.
pub fn use_network() -> Signal<NetworkStatus> {
    with_or_init_signals(|s| s.network.clone())
}

/// Reactive hook returning screen geometry, pixel dimensions, and safe area insets.
pub fn use_screen() -> Signal<ScreenGeometry> {
    with_or_init_signals(|s| s.screen.clone())
}

/// Reactive hook returning display refresh rate and VRR / ProMotion capabilities.
pub fn use_display_refresh() -> Signal<DisplayRefreshInfo> {
    with_or_init_signals(|s| s.display_refresh.clone())
}

// ---------------------------------------------------------------------------
// Memory Pressure Eviction Registry
// ---------------------------------------------------------------------------

type MemoryPressureCallback = Box<dyn Fn(MemoryPressure) + Send + Sync + 'static>;

static LISTENERS: OnceLock<Mutex<HashMap<usize, MemoryPressureCallback>>> = OnceLock::new();
static NEXT_LISTENER_ID: AtomicUsize = AtomicUsize::new(1);

fn get_listeners() -> &'static Mutex<HashMap<usize, MemoryPressureCallback>> {
    LISTENERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers an eviction callback executed when the host OS signals memory pressure.
/// Returns a unique handle ID that can be passed to `unregister_memory_pressure`.
pub fn on_memory_pressure<F>(callback: F) -> usize
where
    F: Fn(MemoryPressure) + Send + Sync + 'static,
{
    let id = NEXT_LISTENER_ID.fetch_add(1, Ordering::SeqCst);
    let mut listeners = get_listeners().lock().unwrap();
    listeners.insert(id, Box::new(callback));
    id
}

/// Unregisters a memory pressure eviction callback.
pub fn unregister_memory_pressure(id: usize) {
    let mut listeners = get_listeners().lock().unwrap();
    listeners.remove(&id);
}

/// Dispatches a memory pressure notification to all registered handlers.
pub fn notify_memory_pressure(level: MemoryPressure) {
    let listeners = get_listeners().lock().unwrap();
    for callback in listeners.values() {
        callback(level);
    }
}

/// Clears all registered memory pressure listeners (primarily for testing isolation).
pub fn clear_memory_pressure_listeners() {
    let mut listeners = get_listeners().lock().unwrap();
    listeners.clear();
}

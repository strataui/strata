//! Fine-grained reactive telemetry & sensor hooks and memory pressure callback registry.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use strata_dom::reactive::Signal;

use crate::mock::DeviceMockController;
use crate::os_sensors::{probe_os_geolocation_async, register_web_api_geolocation_provider};
use crate::types::*;

struct ThreadLocalSignals {
    battery: Signal<BatteryInfo>,
    thermal: Signal<ThermalState>,
    network: Signal<NetworkStatus>,
    screen: Signal<ScreenGeometry>,
    display_refresh: Signal<DisplayRefreshInfo>,
    geolocation: Signal<GeolocationInfo>,
    compass: Signal<CompassReading>,
    imu: Signal<ImuReading>,
    barometer: Signal<BarometerReading>,
    ambient_light: Signal<AmbientLightReading>,
}

thread_local! {
    static SIGNALS: RefCell<Option<ThreadLocalSignals>> = const { RefCell::new(None) };
}

static PENDING_GEO_FIX: OnceLock<Mutex<Option<GeolocationInfo>>> = OnceLock::new();
static GEO_PROBE_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

fn pending_geo_slot() -> &'static Mutex<Option<GeolocationInfo>> {
    PENDING_GEO_FIX.get_or_init(|| Mutex::new(None))
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
                geolocation: Signal::new(mock.geolocation()),
                compass: Signal::new(mock.compass()),
                imu: Signal::new(mock.imu()),
                barometer: Signal::new(mock.barometer()),
                ambient_light: Signal::new(mock.ambient_light()),
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
                signals.geolocation.set(mock.geolocation());
                signals.compass.set(mock.compass());
                signals.imu.set(mock.imu());
                signals.barometer.set(mock.barometer());
                signals.ambient_light.set(mock.ambient_light());
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

/// Reactive hook returning live host geolocation coordinates and fix metadata.
///
/// Never blocks UI rendering: returns any active mock / `STRATA_GEO_LAT_LON` / cached fix
/// immediately and drains pending asynchronous background fixes into the reactive signal.
pub fn use_geolocation() -> Signal<GeolocationInfo> {
    register_web_api_geolocation_provider();
    let _ = poll_geolocation_updates();
    with_or_init_signals(|s| s.geolocation.clone())
}

/// Reactive geolocation hook that optionally launches a non-blocking OS probe in the background
/// when no fix is currently available.
pub fn use_geolocation_with_options(
    auto_probe_async: bool,
    allow_ip_fallback: bool,
) -> Signal<GeolocationInfo> {
    let sig = use_geolocation();
    if auto_probe_async && !sig.untracked_get().is_available {
        request_geolocation_refresh_async(allow_ip_fallback);
    }
    sig
}

/// Spawns a non-blocking OS geolocation probe on a background worker thread.
/// Once resolved, the fix is queued and applied on the next [`poll_geolocation_updates`] /
/// [`use_geolocation`] call (unless a `MockOverride` was set in the meantime).
pub fn request_geolocation_refresh_async(allow_ip_fallback: bool) {
    if GEO_PROBE_IN_FLIGHT
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    probe_os_geolocation_async(allow_ip_fallback, |maybe_fix| {
        if let Some(fix) = maybe_fix {
            if let Ok(mut pending) = pending_geo_slot().lock() {
                *pending = Some(fix);
            }
        }
        GEO_PROBE_IN_FLIGHT.store(false, Ordering::SeqCst);
    });
}

/// Drains any resolved asynchronous geolocation fix into the global snapshot and current-thread
/// reactive [`Signal<GeolocationInfo>`] without overwriting an active [`GeolocationSource::MockOverride`].
pub fn poll_geolocation_updates() -> Option<GeolocationInfo> {
    let fix = pending_geo_slot().lock().ok()?.take()?;
    let mock = DeviceMockController::new();
    let existing = mock.geolocation();
    if existing.is_available && existing.source == GeolocationSource::MockOverride {
        return Some(existing);
    }
    mock.set_geolocation(fix.clone());
    Some(fix)
}

/// Reactive hook returning live magnetometer / compass heading telemetry.
pub fn use_compass() -> Signal<CompassReading> {
    with_or_init_signals(|s| s.compass.clone())
}

/// Reactive hook returning live 6-DoF IMU (accelerometer, gyroscope, fused quaternion) telemetry.
pub fn use_imu() -> Signal<ImuReading> {
    with_or_init_signals(|s| s.imu.clone())
}

/// Reactive hook returning live barometric pressure and ISA pressure-altitude telemetry.
pub fn use_barometer() -> Signal<BarometerReading> {
    with_or_init_signals(|s| s.barometer.clone())
}

/// Reactive hook returning live ambient light illuminance (lux) and daylight adaptation factor.
pub fn use_ambient_light() -> Signal<AmbientLightReading> {
    with_or_init_signals(|s| s.ambient_light.clone())
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

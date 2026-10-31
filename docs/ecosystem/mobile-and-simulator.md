# Mobile Simulator & Test Orchestrator

Developing mobile applications traditionally requires running heavy simulators (Xcode iOS Simulator or Android Studio Emulator) consuming gigabytes of system memory.

Strata provides a dogfooded, lightweight native device simulator (**`strata_sim`**) and headless automated test orchestrator (**`strata_mobile_test`**) built directly in pure Rust.

---

## Dogfooded Native Simulator (`strata_sim`)

Launch the simulator using the Strata CLI:

```bash
strata sim --device "iphone-15-pro"
```

### Simulator Capabilities
1. **Accurate Hardware Bezels & Cutouts**:
   - iPhone Dynamic Island and interactive camera notch cutouts.
   - Android punch-hole cameras and gesture navigation bars.
   - Real-time Safe Area Inset simulation (`env(safe-area-inset-top)`).
2. **Multi-Touch & Gesture Synthesis**:
   - Left-click drag simulates single finger touch pan and swipe gestures.
   - `Alt` + Mouse scroll simulates multi-finger pinch-to-zoom and rotational gestures.
   - Physics-based momentum scrolling matching iOS and Android scroll curves.
3. **Hardware Sensor Injection**:
   - Gyroscope & Accelerometer tilt simulation via mouse dragging.
   - Battery level and thermal state mocking via simulator command palette.
   - Network status toggles (WiFi, 5G, Offline flight mode).
4. **Orientation Toggles**: Instant seamless rotation between Portrait and Landscape modes (`Cmd+R` / `Ctrl+R`).

---

## Supported Device Profiles

| Device Profile | Resolution | Scale Factor | Safe Area Top | Safe Area Bottom |
| :--- | :--- | :--- | :--- | :--- |
| **iPhone 15 Pro** | 393 x 852 pt | 3.0x | 59 pt | 34 pt |
| **iPhone 15 Pro Max** | 430 x 932 pt | 3.0x | 59 pt | 34 pt |
| **Google Pixel 8 Pro** | 412 x 892 pt | 2.6x | 48 pt | 24 pt |
| **Samsung Galaxy S24** | 360 x 780 pt | 3.0x | 40 pt | 20 pt |
| **iPad Pro 11-inch** | 834 x 1194 pt | 2.0x | 24 pt | 20 pt |

---

## Automated Mobile Testing (`strata_mobile_test`)

`strata_mobile_test` provides a headless, programmable driver for executing end-to-end regression tests across mobile device form factors without requiring physical devices or emulators:

```rust
#[cfg(test)]
mod tests {
    use strata_mobile_test::prelude::*;

    #[test]
    fn test_mobile_navigation_flow() {
        let mut session = MobileTestSession::new(DeviceProfile::IPhone15Pro);
        session.load_app(MyApp);

        // Verify initial render within safe areas
        assert!(session.find_by_text("Cluster Telemetry").is_visible());

        // Simulate touch tap on tab button
        session.tap_element("#nav-workloads");

        // Verify state update and layout reflow
        assert_eq!(session.query_value("#active-tab-title"), "Workloads");

        // Capture deterministic golden screenshot for pixel regression verification
        session.assert_screenshot_matches("golden/mobile_workloads.png");
    }
}
```

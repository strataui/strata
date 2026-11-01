# strata_mobile_test: Automated Headless Mobile Orchestrator & Visual Regression Suite

The `strata_mobile_test` crate provides an automated test execution harness, pure-Rust Android Debug Bridge (ADB) client, multi-backend emulator orchestrator, foldable posture driver, and perceptual visual regression engine for Project Strata.

It enables cross-platform automated testing across Android and iOS targets without requiring human intervention or proprietary vendor test suites.

---

## Architectural Topology

```text
┌────────────────────────────────────────────────────────────────────────┐
│                      strata_mobile_test Runner                         │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│   ┌─────────────────────┐   ┌───────────────────┐   ┌──────────────┐   │
│   │   Pure-Rust ADB     │   │  iOS simctl Host  │   │ Fold Posture │   │
│   │   Packet Codec      │   │   Mock Viewport   │   │  Controller  │   │
│   └──────────┬──────────┘   └─────────┬─────────┘   └───────┬──────┘   │
│              │                        │                     │          │
│              ▼                        ▼                     ▼          │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │                Automated Execution Pipeline                    │   │
│   │  (Device Discovery -> Posture Transition -> Monkey Fuzzing)      │   │
│   └───────────────────────────────┬────────────────────────────────┘   │
│                                   │                                    │
│                 ┌─────────────────┴─────────────────┐                  │
│                 ▼                                   ▼                  │
│   ┌───────────────────────────┐       ┌────────────────────────────┐   │
│   │      SSIM Visual Diff     │       │    Logcat Panic Trapping   │   │
│   │  (Perceptual Framebuffer) │       │   (Rust Backtrace Parser)  │   │
│   └───────────────────────────┘       └────────────────────────────┘   │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 1. Pure-Rust ADB Client & Server Engine

To eliminate external dependencies on Google's standalone `adb` binary in continuous integration containers, `strata_mobile_test` implements the complete ADB wire protocol in native Rust:

### Packet Framing & Codec (`AdbPacketCodec`)

The ADB protocol utilizes 24-byte packet headers containing command tags, arguments, payload lengths, CRC32 checksums, and bitwise magic inversions:

```rust
pub struct AdbPacket {
    pub command: u32,       // A_SYNC, A_CNXN, A_OPEN, A_OKAY, A_CLSE, A_WRTE
    pub arg0: u32,          // Local stream ID or protocol version
    pub arg1: u32,          // Remote stream ID or max payload
    pub data_length: u32,   // Payload byte count
    pub data_crc32: u32,    // Checksum of payload bytes
    pub magic: u32,         // command ^ 0xFFFFFFFF
}
```

The codec validates packet integrity by verifying `magic == command ^ 0xFFFFFFFF` and computing payload checksums before passing messages to stream channels.

### Emulator Port Scanner (`AdbPortScanner`)

The harness auto-discovers running local Android emulators by scanning TCP loopback interfaces across the standard Android emulator port range (`5555..=5585`):

```rust
pub const STANDARD_EMULATOR_PORTS: std::ops::RangeInclusive<u16> = 5555..=5585;

pub struct AdbPortScanner;

impl AdbPortScanner {
    pub fn scan_local_emulators() -> Vec<AdbDevice> {
        // Scans even numbered console ports and odd numbered ADB communication ports
        // Establishes handshake to retrieve device serials and connection states
    }
}
```

### In-Memory Mock ADB Server (`MockAdbServer`)

For hermetic unit testing without running heavy hypervisors, `MockAdbServer` simulates emulator state machines, responding to `host:devices-l`, `shell:input`, `shell:cmd device_state`, and port forwarding commands with spec-compliant `OKAY` / `FAIL` tokens.

---

## 2. Multi-Backend Emulator Orchestration

The harness supports dual backend targets: Android QEMU/KVM virtual devices and iOS Simulator environments.

### Android Headless Lifecycle (`android.rs`)

Configures headless execution parameters for high-density CI runners:

- **GPU Acceleration**: Supports `swiftshader_indirect` (software Vulkan/GLES), `host` (native GPU pass-through), or `off`.
- **KVM Acceleration Check**: Validates `/dev/kvm` write permissions on Linux hosts to detect hardware virtualization support.
- **Boot Telemetry Verification**: Polls Android system properties via ADB shell until `sys.boot_completed` returns `1`.

```rust
pub struct EmulatorConfig {
    pub avd_name: String,
    pub no_window: bool,
    pub gpu_mode: EmulatorGpuMode,
    pub memory_mb: u32,
    pub cores: u32,
}
```

### Apple iOS Orchestration & Non-macOS Fallback (`ios.rs`)

On macOS hosts with Xcode installed, the harness interfaces with `xcrun simctl` to boot named runtimes (`iPhone 16 Pro`, `iPad Pro`). On Linux and Windows build agents, it activates `MockIosViewport`, mocking the CoreAnimation composition surface, safe-area insets, and Darwin display link timings without proprietary binaries:

```rust
pub enum AppleLicenseTier {
    NativeMacOsHost { simctl_path: PathBuf },
    HeadlessCrossPlatformMock { viewport: MockIosViewport },
}
```

---

## 3. Foldable & Dual-Screen Posture Engine

Strata natively adapts UI layouts to folding hinges, flexible OLED panels, and multi-display enclosures. The `foldable.rs` module models physical hinge angles and drives real-time posture changes on emulators:

```text
    0° Hinge Angle          90° Hinge Angle          180° Hinge Angle
     [Closed Mode]         [Flex / Tabletop]            [Flat Canvas]
   ┌───────────────┐       ┌───────────────┐       ┌───────┬───────┐
   │  Cover Screen │       │ Top Display   │       │       │       │
   │               │       ├───────────────┤       │ Pane  │ Pane  │
   │               │       │ Bottom Screen │       │   A   │   B   │
   └───────────────┘       └───────────────┘       └───────┴───────┘
```

### Posture States (`FoldPosture`)

1. **`Closed` ($0^\circ$)**: Device is folded shut; input routes exclusively to the outer cover screen.
2. **`HalfOpened` ($90^\circ$)**: Tabletop "Flex Mode" dividing the viewport across the physical hinge.
3. **`Flat` ($180^\circ$)**: Continuous uninterrupted canvas across both panels.
4. **`Flipped` ($360^\circ$)**: Folded backwards for single-hand presentation.

### Display Crease Features (`DisplayCreaseFeature`)

```rust
pub struct DisplayCreaseFeature {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub is_vertical: bool,
    pub is_separating: bool,
}
```

### Posture Command Emission (`PostureCommandEmitter`)

Emits emulator console control commands (`emu posture <posture>`) and Android framework device state switches (`cmd device_state state <id>`) to trigger runtime layout recalculations.

---

## 4. Perceptual Visual Regression Engine (SSIM)

Rather than raw pixel-byte hashing which breaks across differing GPU rasterization rounding rules and antialiasing passes, `strata_mobile_test` implements a first-principles Structural Similarity Index Measure (SSIM) algorithm (`visual.rs`).

### Mathematical Formulation

Given reference image $x$ and candidate image $y$, SSIM evaluates luminance $l(x,y)$, contrast $c(x,y)$, and structure $s(x,y)$:

$$\text{SSIM}(x,y) = \frac{(2\mu_x\mu_y + C_1)(2\sigma_{xy} + C_2)}{(\mu_x^2 + \mu_y^2 + C_1)(\sigma_x^2 + \sigma_y^2 + C_2)}$$

Where:
- $C_1 = (K_1 L)^2 = (0.01 \times 255)^2 = 6.5025$
- $C_2 = (K_2 L)^2 = (0.03 \times 255)^2 = 58.5225$
- $\mu_x, \mu_y$: Local pixel luminance averages
- $\sigma_x^2, \sigma_y^2$: Local variances
- $\sigma_{xy}$: Cross-covariance of $x$ and $y$

```rust
pub struct VisualDiffResult {
    pub ssim_score: f32,
    pub is_match: bool,
    pub mismatched_pixels: usize,
    pub damage_rects: Vec<(u32, u32, u32, u32)>,
}
```

The engine pinpoints the minimal bounding box around visual anomalies (`damage_rects`), enabling automated CI reports to highlight exact layout regressions.

---

## 5. Chaos Monkey Stress Fuzzing

The `stress.rs` module subjects applications to random input permutations to detect state desynchronizations, layout deadlocks, and race conditions.

- **Deterministic PRNG (`FuzzRng`)**: Seedable Linear Congruential Generator ensuring every fuzz sequence is 100% reproducible across machines.
- **Event Matrix (`MonkeyEvent`)**:
  - `Tap { x, y }`
  - `Drag { start_x, start_y, end_x, end_y, duration_ms }`
  - `MultiTouchPinch { center_x, center_y, scale }`
  - `HardwareKey { keycode }` (Back, Home, Volume)
  - `OrientationChange { orientation }`
  - `ThermalJolt { thermal_level }`
- **ADB Command Translation**: Converts structured events into efficient `input tap`, `input swipe`, or `input keyevent` invocations.

---

## 6. Runtime Panic Interception & Logcat Parser

The `logcat.rs` subsystem hooks the Android logcat stream and iOS diagnostic logs to capture panics:

- **`PanicTrap`**: Maintains internal rings of intercepted panic strings.
- **`LogcatParser`**: Extracts Rust thread panic markers (`thread 'main' panicked at '...'`), file paths, line numbers, and backtraces. Intercepts Android JVM exceptions (`FATAL EXCEPTION: main`) and Mach-O crash signals (`EXC_BAD_ACCESS`, `SIGSEGV`).

---

## Execution and Reporting

Running the complete automated mobile test suite:

```rust
use strata_mobile_test::{StrataMobileTestConfig, StrataMobileTestRunner};

let mut config = StrataMobileTestConfig::default();
config.test_foldable = true;
config.enable_monkey_fuzz = true;
config.fuzz_event_count = 100;

let mut runner = StrataMobileTestRunner::new(config);
let report = runner.execute();

assert!(report.passed);
println!("Tests Run: {}, Failed: {}, SSIM Score: {:.4}",
    report.tests_run, report.tests_failed, report.ssim_score);
```

### Verification Commands

```bash
# Execute mobile test harness integration tests
cargo test -p strata_mobile_test
```

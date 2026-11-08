//! Automated Mobile Testing, Pure-Rust ADB Client, Multi-Backend Emulator Orchestration, and Foldable Posture Testing for Strata.

pub mod adb;
pub mod android;
pub mod foldable;
pub mod ios;
pub mod logcat;
pub mod runner;
pub mod stress;
pub mod visual;

pub use adb::{
    AdbClient, AdbDevice, AdbPacketCodec, AdbPortScanner, EmulatorConsoleClient, MockAdbServer,
    MockEmulatorConsoleServer, STANDARD_EMULATOR_PORTS,
};
pub use android::{BootStatusChecker, EmulatorConfig, EmulatorGpuMode, HostDetector, HostPlatform};
pub use foldable::{
    DisplayCreaseFeature, FoldPosture, FoldableProfile, PostureCommandEmitter,
};
pub use ios::{AppleLicenseTier, IosDeviceModel, MockIosViewport, SimCtlDriver};
pub use logcat::{LogcatEntry, LogcatParser, LogLevel, PanicReport, PanicTrap};
pub use runner::{MobileTestReport, StrataMobileTestConfig, StrataMobileTestRunner};
pub use stress::{FuzzRng, MonkeyEvent, MonkeyFuzzer};
pub use visual::{SsimEngine, VisualDiffResult};

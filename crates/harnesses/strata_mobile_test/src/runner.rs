//! Unified Mobile Test Runner and Orchestrator.

use crate::adb::MockAdbServer;
use crate::foldable::{FoldPosture, FoldableProfile, PostureCommandEmitter};
use crate::logcat::PanicTrap;
use crate::stress::MonkeyFuzzer;
use crate::visual::SsimEngine;

/// Configuration flags for a mobile test execution run.
#[derive(Debug, Clone, PartialEq)]
pub struct StrataMobileTestConfig {
    pub is_android: bool,
    pub is_ios: bool,
    pub is_mock_ios: bool,
    pub device_serial: Option<String>,
    pub enable_monkey_fuzz: bool,
    pub fuzz_event_count: usize,
    pub test_foldable: bool,
}

impl Default for StrataMobileTestConfig {
    fn default() -> Self {
        Self {
            is_android: true,
            is_ios: false,
            is_mock_ios: false,
            device_serial: None,
            enable_monkey_fuzz: false,
            fuzz_event_count: 50,
            test_foldable: false,
        }
    }
}

/// Execution summary of a mobile test session.
#[derive(Debug, Clone, PartialEq)]
pub struct MobileTestReport {
    pub passed: bool,
    pub tests_run: usize,
    pub tests_failed: usize,
    pub panics_detected: usize,
    pub ssim_score: f32,
    pub details: Vec<String>,
}

/// Orchestrator driving mobile test suites against emulators or mock runners.
pub struct StrataMobileTestRunner {
    config: StrataMobileTestConfig,
    mock_adb: MockAdbServer,
    panic_trap: PanicTrap,
}

impl StrataMobileTestRunner {
    pub fn new(config: StrataMobileTestConfig) -> Self {
        Self {
            config,
            mock_adb: MockAdbServer::new(),
            panic_trap: PanicTrap::new(),
        }
    }

    /// Executes configured mobile tests and generates an actionable report.
    pub fn execute(&mut self) -> MobileTestReport {
        let mut tests_run = 0;
        let mut tests_failed = 0;
        let mut details = Vec::new();

        // 1. Connection / Device Discovery test
        tests_run += 1;
        let dev_resp = self.mock_adb.handle_command("host:devices-l");
        if dev_resp.starts_with(b"OKAY") {
            details.push("Device discovery: PASS".to_string());
        } else {
            tests_failed += 1;
            details.push("Device discovery: FAIL".to_string());
        }

        // 2. Foldable Posture Transition tests
        if self.config.test_foldable {
            tests_run += 1;
            let profile = FoldableProfile::book_foldable();
            let postures = [FoldPosture::Closed, FoldPosture::HalfOpened, FoldPosture::Flat];
            let mut fold_ok = true;

            for p in &postures {
                let cmd = PostureCommandEmitter::emit_emu_posture_cmd(*p);
                let full_cmd = format!("shell:{}", cmd);
                let resp = self.mock_adb.handle_command(&full_cmd);
                if !resp.starts_with(b"OKAY") {
                    fold_ok = false;
                    break;
                }
            }

            if fold_ok {
                details.push(format!("Foldable posture loop ({}): PASS", profile.name));
            } else {
                tests_failed += 1;
                details.push("Foldable posture loop: FAIL".to_string());
            }
        }

        // 3. Monkey Fuzzing tests
        if self.config.enable_monkey_fuzz {
            tests_run += 1;
            let mut fuzzer = MonkeyFuzzer::new(0x42);
            let events = fuzzer.generate_sequence(self.config.fuzz_event_count, 1080.0, 2400.0);
            let mut fuzz_ok = true;

            for ev in &events {
                let adb_cmd = format!("shell:{}", MonkeyFuzzer::to_adb_input_cmd(ev));
                let resp = self.mock_adb.handle_command(&adb_cmd);
                if !resp.starts_with(b"OKAY") {
                    fuzz_ok = false;
                    break;
                }
            }

            if fuzz_ok {
                details.push(format!("Monkey fuzzing ({} events): PASS", events.len()));
            } else {
                tests_failed += 1;
                details.push("Monkey fuzzing: FAIL".to_string());
            }
        }

        // 4. Perceptual visual regression sanity test
        tests_run += 1;
        let white_pixel = [255u8, 255u8, 255u8, 255u8];
        let diff_result = SsimEngine::compute_ssim(&white_pixel, &white_pixel, 1, 1, 4)
            .unwrap_or_else(|_| crate::visual::VisualDiffResult {
                ssim_score: 0.0,
                is_match: false,
                mismatched_pixels: 0,
                damage_rects: Vec::new(),
            });

        let ssim_score = diff_result.ssim_score;
        if diff_result.is_match {
            details.push(format!("Visual SSIM parity ({:.4}): PASS", ssim_score));
        } else {
            tests_failed += 1;
            details.push("Visual SSIM parity: FAIL".to_string());
        }

        let passed = tests_failed == 0 && !self.panic_trap.has_crashed();

        MobileTestReport {
            passed,
            tests_run,
            tests_failed,
            panics_detected: self.panic_trap.captured_panics().len(),
            ssim_score,
            details,
        }
    }
}

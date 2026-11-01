//! Autonomous Monkey Fuzzing and Stress-Testing Engine.

/// Simulated user interaction event generated during monkey fuzzing.
#[derive(Debug, Clone, PartialEq)]
pub enum MonkeyEvent {
    Touch { x: f32, y: f32 },
    Motion { from_x: f32, from_y: f32, to_x: f32, to_y: f32 },
    NavBack,
    SysKey { key_code: u32 },
    AppSwitch,
}

/// Simple, zero-dependency XorShift64 pseudo-random number generator for deterministic fuzzing.
#[derive(Debug, Clone)]
pub struct FuzzRng {
    state: u64,
}

impl FuzzRng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x5EED_CAFE_DEAD_BEEF } else { seed },
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() & 0xFFFF_FFFF) as f32 / 4294967296.0
    }
}

/// Autonomous monkey stress-testing orchestrator.
#[derive(Debug, Clone)]
pub struct MonkeyFuzzer {
    rng: FuzzRng,
}

impl MonkeyFuzzer {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: FuzzRng::new(seed),
        }
    }

    /// Generates a single pseudo-random interaction event mapped within viewport dimensions.
    pub fn generate_event(&mut self, viewport_w: f32, viewport_h: f32) -> MonkeyEvent {
        let roll = self.rng.next_f32();

        if roll < 0.60 {
            // 60% Touch/Click
            let x = self.rng.next_f32() * viewport_w;
            let y = self.rng.next_f32() * viewport_h;
            MonkeyEvent::Touch { x, y }
        } else if roll < 0.85 {
            // 25% Drag/Scroll
            let from_x = self.rng.next_f32() * viewport_w;
            let from_y = self.rng.next_f32() * viewport_h;
            let to_x = self.rng.next_f32() * viewport_w;
            let to_y = self.rng.next_f32() * viewport_h;
            MonkeyEvent::Motion { from_x, from_y, to_x, to_y }
        } else if roll < 0.95 {
            // 10% Back Navigation
            MonkeyEvent::NavBack
        } else {
            // 5% App Switch
            MonkeyEvent::AppSwitch
        }
    }

    /// Generates a batch of N consecutive fuzzing events.
    pub fn generate_sequence(&mut self, count: usize, viewport_w: f32, viewport_h: f32) -> Vec<MonkeyEvent> {
        (0..count)
            .map(|_| self.generate_event(viewport_w, viewport_h))
            .collect()
    }

    /// Converts a monkey event into an ADB input shell command.
    pub fn to_adb_input_cmd(event: &MonkeyEvent) -> String {
        match event {
            MonkeyEvent::Touch { x, y } => format!("input tap {:.0} {:.0}", x, y),
            MonkeyEvent::Motion { from_x, from_y, to_x, to_y } => {
                format!("input swipe {:.0} {:.0} {:.0} {:.0} 250", from_x, from_y, to_x, to_y)
            }
            MonkeyEvent::NavBack => "input keyevent 4".to_string(), // KEYCODE_BACK
            MonkeyEvent::SysKey { key_code } => format!("input keyevent {}", key_code),
            MonkeyEvent::AppSwitch => "input keyevent 187".to_string(), // KEYCODE_APP_SWITCH
        }
    }
}

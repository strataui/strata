//! Dynamic Island physical mass-spring-damper oscillator and interactive cutout morphing.

/// Dynamic Island morphing state variants.
#[derive(Debug, Clone, PartialEq)]
pub enum DynamicIslandState {
    /// Resting compact pill (e.g. 125pt width, 37pt height).
    Compact,
    /// Split into two independent bubbles (e.g. Active Call on left + Timer on right).
    SplitBubbles {
        leading_width: f32,
        trailing_width: f32,
    },
    /// Expanded transient alert (e.g. FaceID unlock, Silent Switch toggle, AirDrop transfer).
    ExpandedAlert {
        alert_kind: DynamicAlertKind,
    },
    /// Interactive expanded modal HUD on user tap/long-press.
    InteractiveModal {
        title: String,
        subtitle: String,
    },
}

/// Transient alert types displayed by Dynamic Island.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicAlertKind {
    FaceIdUnlock,
    SilentSwitchOn,
    SilentSwitchOff,
    AirDropReceived,
    CallIncoming,
    LowBatteryWarning,
}

/// Mass-spring-damper physical oscillator driving fluid Dynamic Island morphing animations.
///
/// Governed by: m * x'' + c * x' + k * (x - x_target) = 0
/// With sub-critical damping ratio zeta = 0.82 and natural frequency omega_n = 28.0 rad/s.
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicIslandOscillator {
    pub current_width: f32,
    pub current_height: f32,
    pub target_width: f32,
    pub target_height: f32,
    pub velocity_w: f32,
    pub velocity_h: f32,
    pub damping_ratio: f32,     // 0.82
    pub angular_frequency: f32, // 28.0 rad/s
    pub active_state: DynamicIslandState,
}

impl DynamicIslandOscillator {
    /// Constructs a resting compact Dynamic Island oscillator.
    pub fn new(resting_width: f32, resting_height: f32) -> Self {
        Self {
            current_width: resting_width,
            current_height: resting_height,
            target_width: resting_width,
            target_height: resting_height,
            velocity_w: 0.0,
            velocity_h: 0.0,
            damping_ratio: 0.82,
            angular_frequency: 28.0,
            active_state: DynamicIslandState::Compact,
        }
    }

    /// Transitions to a new morphing state and adjusts target dimensions accordingly.
    pub fn transition_to(&mut self, state: DynamicIslandState) {
        self.active_state = state;
        match &self.active_state {
            DynamicIslandState::Compact => {
                self.target_width = 125.0;
                self.target_height = 37.0;
            }
            DynamicIslandState::SplitBubbles { .. } => {
                self.target_width = 160.0;
                self.target_height = 37.0;
            }
            DynamicIslandState::ExpandedAlert { .. } => {
                self.target_width = 220.0;
                self.target_height = 46.0;
            }
            DynamicIslandState::InteractiveModal { .. } => {
                self.target_width = 360.0;
                self.target_height = 160.0;
            }
        }
    }

    /// Advances the physical oscillator by dt seconds using semi-implicit Euler integration.
    pub fn step(&mut self, dt_secs: f32) {
        let zeta = self.damping_ratio;
        let omega = self.angular_frequency;

        // Acceleration along width: a = -2*zeta*omega*v - omega^2 * (x - target)
        let accel_w = -2.0 * zeta * omega * self.velocity_w - (omega * omega) * (self.current_width - self.target_width);
        self.velocity_w += accel_w * dt_secs;
        self.current_width += self.velocity_w * dt_secs;

        // Acceleration along height
        let accel_h = -2.0 * zeta * omega * self.velocity_h - (omega * omega) * (self.current_height - self.target_height);
        self.velocity_h += accel_h * dt_secs;
        self.current_height += self.velocity_h * dt_secs;

        // Snapping clamp near equilibrium
        if (self.current_width - self.target_width).abs() < 0.05 && self.velocity_w.abs() < 0.1 {
            self.current_width = self.target_width;
            self.velocity_w = 0.0;
        }
        if (self.current_height - self.target_height).abs() < 0.05 && self.velocity_h.abs() < 0.1 {
            self.current_height = self.target_height;
            self.velocity_h = 0.0;
        }
    }

    /// Returns true if the oscillator has settled into equilibrium.
    pub fn is_settled(&self) -> bool {
        self.current_width == self.target_width
            && self.current_height == self.target_height
            && self.velocity_w == 0.0
            && self.velocity_h == 0.0
    }
}

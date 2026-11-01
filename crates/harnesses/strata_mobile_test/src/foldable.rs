//! Foldable, Flip, and Dual-Screen Form Factor Emulation & Posture Engine.

/// Physical posture state of a folding or dual-screen device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FoldPosture {
    /// 0° Hinge Angle: Folded shut with outer cover screen active.
    Closed,
    /// 90° Hinge Angle: Half-opened tabletop / tent "Flex Mode" separating content into two panes.
    HalfOpened,
    /// 180° Hinge Angle: Fully unfolded flat continuous canvas.
    Flat,
    /// 360° Hinge Angle: Fully wrapped backwards with rear display presentation.
    Flipped,
}

impl FoldPosture {
    pub fn as_adb_emu_str(&self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::HalfOpened => "half-opened",
            Self::Flat => "flat",
            Self::Flipped => "flipped",
        }
    }

    pub fn device_state_id(&self) -> u32 {
        match self {
            Self::Closed => 0,
            Self::HalfOpened => 1,
            Self::Flat => 2,
            Self::Flipped => 3,
        }
    }
}

/// Hardware display occlusion feature or hinge crease separating screen real-estate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayCreaseFeature {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub is_vertical: bool,
    pub is_separating: bool,
}

/// Form factor profile for advanced mobile devices (Book Foldables, Clamshell Flips, Dual-Screens).
#[derive(Debug, Clone, PartialEq)]
pub struct FoldableProfile {
    pub name: String,
    pub cover_resolution: (u32, u32),
    pub unfolded_resolution: (u32, u32),
    pub crease: DisplayCreaseFeature,
}

impl FoldableProfile {
    /// Google Pixel Fold / Galaxy Z Fold (Book-style horizontal fold).
    pub fn book_foldable() -> Self {
        Self {
            name: "Pixel Fold".to_string(),
            cover_resolution: (1080, 2092),
            unfolded_resolution: (2208, 1840),
            crease: DisplayCreaseFeature {
                x: 1104,
                y: 0,
                width: 0,
                height: 1840,
                is_vertical: true,
                is_separating: false,
            },
        }
    }

    /// Samsung Galaxy Z Flip (Clamshell vertical flip).
    pub fn clamshell_flip() -> Self {
        Self {
            name: "Galaxy Z Flip".to_string(),
            cover_resolution: (1056, 1066),
            unfolded_resolution: (1080, 2640),
            crease: DisplayCreaseFeature {
                x: 0,
                y: 1320,
                width: 1080,
                height: 0,
                is_vertical: false,
                is_separating: false,
            },
        }
    }

    /// Microsoft Surface Duo (Dual physical screen separated by an 84px physical hinge).
    pub fn dual_screen_duo() -> Self {
        Self {
            name: "Surface Duo".to_string(),
            cover_resolution: (1350, 1800),
            unfolded_resolution: (2784, 1800), // 1350 + 84 + 1350
            crease: DisplayCreaseFeature {
                x: 1350,
                y: 0,
                width: 84,
                height: 1800,
                is_vertical: true,
                is_separating: true, // Content cannot span physical bezel
            },
        }
    }

    /// Returns the active resolution for a given posture.
    pub fn resolution_for_posture(&self, posture: FoldPosture) -> (u32, u32) {
        match posture {
            FoldPosture::Closed | FoldPosture::Flipped => self.cover_resolution,
            FoldPosture::HalfOpened | FoldPosture::Flat => self.unfolded_resolution,
        }
    }
}

/// Generates ADB and emulator console commands to programmatically command device posture.
pub struct PostureCommandEmitter;

impl PostureCommandEmitter {
    /// Emits `adb emu posture set <posture>` command.
    pub fn emit_emu_posture_cmd(posture: FoldPosture) -> String {
        format!("emu posture set {}", posture.as_adb_emu_str())
    }

    /// Emits Android `cmd device_state state <id>` command.
    pub fn emit_device_state_cmd(posture: FoldPosture) -> String {
        format!("cmd device_state state {}", posture.device_state_id())
    }

    /// Emits Telnet console `sensor set hinge_angle <deg>` command.
    pub fn emit_telnet_hinge_angle_cmd(degrees: u32) -> String {
        format!("sensor set hinge_angle {}", degrees)
    }
}

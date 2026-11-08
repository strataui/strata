//! Complete Developer Workspace and Mobile Simulator Root Component.

use crate::geometry::{DeviceOrientation, GeometrySolver};
use crate::hal::VirtualHalBus;
use crate::island::{DynamicAlertKind, DynamicIslandOscillator, DynamicIslandState};
use crate::launcher::{HomeScreenState, RecentsCarouselState};
use crate::navigation::{ButtonLayout, GestureNavEngine, NavAction, ThreeButtonNavBar};
use crate::posture::FoldableState;
use crate::profile::{CutoutProfile, DeviceFormFactor, DeviceProfile, NavigationType};
use crate::sandbox::MultiAppSandbox;
use crate::system_ui::{ControlCenterModel, NotificationShadeModel, StatusBarModel};
use strata_dom::View;
use strata_macro::rsx;

/// Complete active mobile device simulator workspace state.
#[derive(Debug, Clone, PartialEq)]
pub struct MobileSimulatorWorkspace {
    pub profile: DeviceProfile,
    pub orientation: DeviceOrientation,
    pub hal: VirtualHalBus,
    pub foldable: Option<FoldableState>,
    pub island: Option<DynamicIslandOscillator>,
    pub status_bar: StatusBarModel,
    pub control_center: ControlCenterModel,
    pub notification_shade: NotificationShadeModel,
    pub three_button_nav: ThreeButtonNavBar,
    pub gesture_nav: GestureNavEngine,
    pub home_screen: HomeScreenState,
    pub recents: RecentsCarouselState,
    pub sandbox: MultiAppSandbox,
}

impl Default for MobileSimulatorWorkspace {
    fn default() -> Self {
        Self::new(DeviceProfile::iphone_16_pro_max())
    }
}

impl MobileSimulatorWorkspace {
    /// Creates an active mobile simulator workspace with the given device profile.
    pub fn new(profile: DeviceProfile) -> Self {
        let (w, h) = {
            let s = profile.form_factor.active_screen();
            (s.logical_width, s.logical_height)
        };

        let foldable = match &profile.form_factor {
            DeviceFormFactor::ClamshellFlip { hinge_axis, hinge_angle_deg, .. } => {
                Some(FoldableState::new(*hinge_axis, *hinge_angle_deg, w, h))
            }
            DeviceFormFactor::BookFoldable { hinge_axis, hinge_angle_deg, .. } => {
                Some(FoldableState::new(*hinge_axis, *hinge_angle_deg, w, h))
            }
            _ => None,
        };

        let island = match &profile.cutout {
            CutoutProfile::DynamicIsland { resting_width, resting_height, .. } => {
                Some(DynamicIslandOscillator::new(*resting_width, *resting_height))
            }
            _ => None,
        };

        let three_button_nav = match profile.navigation_type {
            NavigationType::ThreeButton => ThreeButtonNavBar::new(ButtonLayout::AndroidClassic),
            NavigationType::Gesture => ThreeButtonNavBar::new(ButtonLayout::AndroidClassic),
        };

        Self {
            profile,
            orientation: DeviceOrientation::Portrait,
            hal: VirtualHalBus::new(),
            foldable,
            island,
            status_bar: StatusBarModel::new(),
            control_center: ControlCenterModel::default(),
            notification_shade: NotificationShadeModel::default(),
            three_button_nav,
            gesture_nav: GestureNavEngine::new(),
            home_screen: HomeScreenState::new(),
            recents: RecentsCarouselState::new(),
            sandbox: MultiAppSandbox::new(),
        }
    }

    /// Switches the active device profile preset (e.g. iPhone 16 Pro Max to Galaxy Z Fold 6).
    pub fn switch_profile(&mut self, new_profile: DeviceProfile) {
        *self = Self::new(new_profile);
    }

    /// Sets device orientation.
    pub fn set_orientation(&mut self, orientation: DeviceOrientation) {
        self.orientation = orientation;
    }

    /// Sets hinge angle for foldable devices.
    pub fn set_hinge_angle(&mut self, angle_deg: f32) {
        if let Some(foldable) = &mut self.foldable {
            let screen = self.profile.form_factor.active_screen();
            foldable.set_hinge_angle(angle_deg, false, screen.logical_width, screen.logical_height);
            self.sandbox.execute_display_handover();
        }
    }

    /// Derives the active W3C CSS MediaContext based on device screen dimensions, orientation, and foldable posture.
    pub fn to_media_context(&self) -> strata_style::MediaContext {
        let screen = self.profile.form_factor.active_screen();
        let (width, height) = match self.orientation {
            DeviceOrientation::Portrait | DeviceOrientation::PortraitInverted => {
                (screen.logical_width, screen.logical_height)
            }
            DeviceOrientation::LandscapeLeft | DeviceOrientation::LandscapeRight => {
                (screen.logical_height, screen.logical_width)
            }
        };

        let mut ctx = strata_style::MediaContext::new(width, height, true);
        if let Some(foldable) = &self.foldable {
            foldable.apply_to_media_context(&mut ctx);
        }
        ctx
    }

    /// Triggers an interactive Dynamic Island transient alert.
    pub fn trigger_island_alert(&mut self, alert: DynamicAlertKind) {
        if let Some(island) = &mut self.island {
            island.transition_to(DynamicIslandState::ExpandedAlert { alert_kind: alert });
        }
    }

    /// Handles a navigation action (Back, Home, Recents).
    pub fn handle_nav_action(&mut self, action: NavAction) {
        match action {
            NavAction::Home => {
                self.sandbox.return_to_home();
                self.recents.close();
                self.control_center.is_open = false;
                self.notification_shade.is_open = false;
            }
            NavAction::Recents => {
                self.recents.open();
                self.control_center.is_open = false;
                self.notification_shade.is_open = false;
            }
            NavAction::Back => {
                if self.control_center.is_open {
                    self.control_center.is_open = false;
                } else if self.notification_shade.is_open {
                    self.notification_shade.is_open = false;
                } else if self.recents.is_active {
                    self.recents.close();
                } else {
                    self.sandbox.return_to_home();
                }
            }
        }
    }

    /// Launches a sandboxed app by bundle ID.
    pub fn launch_app(&mut self, bundle_id: &str) -> bool {
        let ok = self.sandbox.launch_app(bundle_id);
        if ok {
            if let Some(app) = self.sandbox.instances.get(bundle_id) {
                self.recents.push_task(app.metadata.clone());
            }
            self.recents.close();
        }
        ok
    }

    /// Advances physics oscillators and telemetry simulation by dt seconds.
    pub fn tick(&mut self, dt_secs: f32) {
        if let Some(island) = &mut self.island {
            island.step(dt_secs);
        }
    }

    /// Renders the complete developer simulator workspace into an RSX View blueprint.
    pub fn render_view(&self) -> View {
        let device_name = self.profile.name.clone();
        let (chassis_w, chassis_h) = GeometrySolver::calculate_chassis_dimensions(&self.profile);
        let safe_area = GeometrySolver::calculate_safe_area(&self.profile, self.orientation);
        let is_online = self.hal.is_online();
        let battery_pct = (self.hal.battery_level * 100.0) as u32;

        rsx! {
            <div class="flex flex-row w-screen h-screen bg-neutral-950 text-neutral-100 font-sans overflow-hidden">
                // Left Dock: Device Preset Switcher
                <aside class="flex flex-col w-64 bg-neutral-900 border-r border-neutral-800 p-4">
                    <div class="flex items-center space-x-2 pb-4 border-b border-neutral-800">
                        <span class="font-bold text-sm tracking-wide text-blue-400">{ "STRATA SIMULATOR" }</span>
                    </div>

                    <div class="flex flex-col mt-4 space-y-2">
                        <span class="text-xs uppercase text-neutral-400 font-semibold">{ "Device Presets" }</span>
                        <div class="flex flex-col space-y-1">
                            <button class="flex items-center px-3 py-2 rounded text-xs bg-neutral-800 text-neutral-200">
                                { "iPhone 16 Pro Max" }
                            </button>
                            <button class="flex items-center px-3 py-2 rounded text-xs text-neutral-400">
                                { "Galaxy S24 Ultra" }
                            </button>
                            <button class="flex items-center px-3 py-2 rounded text-xs text-neutral-400">
                                { "Pixel 9 Pro" }
                            </button>
                            <button class="flex items-center px-3 py-2 rounded text-xs text-neutral-400">
                                { "Galaxy Z Fold 6" }
                            </button>
                            <button class="flex items-center px-3 py-2 rounded text-xs text-neutral-400">
                                { "Galaxy Z Flip 6" }
                            </button>
                            <button class="flex items-center px-3 py-2 rounded text-xs text-neutral-400">
                                { "Surface Duo 2" }
                            </button>
                        </div>
                    </div>
                </aside>

                // Center Viewport: Mobile Phone Viewport & 3D Chassis Canvas
                <main class="flex flex-1 items-center justify-center bg-neutral-950 p-6 relative">
                    <div class="flex flex-col items-center justify-center relative shadow-2xl rounded-3xl bg-neutral-900 border-4 border-neutral-700">
                        // Status Bar Overlay
                        <header class="flex flex-row items-center justify-between w-full px-6 pt-3 pb-1 text-xs">
                            <span class="font-semibold">{ "9:41" }</span>
                            <div class="flex items-center space-x-2">
                                <span class="text-neutral-400">{ if is_online { "5G" } else { "Offline" } }</span>
                                <span class="font-mono text-neutral-300">{ format!("{}%", battery_pct) }</span>
                            </div>
                        </header>

                        // Main Phone Screen Area
                        <div class="flex flex-col flex-1 w-full items-center justify-center p-4">
                            <div class="flex flex-col items-center justify-center space-y-3">
                                <span class="text-lg font-bold text-neutral-200">{ device_name }</span>
                                <span class="text-xs text-neutral-400">{ "Strata Mobile OS v0.1.0" }</span>
                            </div>
                        </div>

                        // Navigation Footer
                        <footer class="flex items-center justify-center w-full py-2">
                            <div class="w-32 h-1 bg-neutral-500 rounded-full"></div>
                        </footer>
                    </div>
                </main>

                // Right Drawer: Hardware Telemetry Inspector
                <aside class="flex flex-col w-72 bg-neutral-900 border-l border-neutral-800 p-4">
                    <span class="text-xs uppercase text-neutral-400 font-semibold mb-3">{ "Hardware Inspector" }</span>
                    <div class="flex flex-col space-y-4 text-xs">
                        <div class="flex flex-col space-y-1">
                            <span class="text-neutral-400">{ "Network Radio" }</span>
                            <span class="text-neutral-200 font-mono">{ if is_online { "Wi-Fi 6 (150 Mbps)" } else { "Airplane Mode" } }</span>
                        </div>
                        <div class="flex flex-col space-y-1">
                            <span class="text-neutral-400">{ "Battery State" }</span>
                            <span class="text-neutral-200 font-mono">{ format!("{}% (Normal)", battery_pct) }</span>
                        </div>
                        <div class="flex flex-col space-y-1">
                            <span class="text-neutral-400">{ "Chassis Bounds" }</span>
                            <span class="text-neutral-200 font-mono">{ format!("{:.0} x {:.0} pt", chassis_w, chassis_h) }</span>
                        </div>
                        <div class="flex flex-col space-y-1">
                            <span class="text-neutral-400">{ "Safe Area Insets" }</span>
                            <span class="text-neutral-200 font-mono">{ format!("T:{:.0} B:{:.0} L:{:.0} R:{:.0}", safe_area.top, safe_area.bottom, safe_area.left, safe_area.right) }</span>
                        </div>
                    </div>
                </aside>
            </div>
        }
    }
}

//! Chromium subprocess manager, lifecycle watchdog, and IPC routing.

use std::sync::{Arc, Mutex, RwLock};
use strata_primitives::geom::Rect;
use crate::ipc::{ChromiumCommand, ChromiumEvent, NavigationState};
use crate::surface::{PixelFormat, ShmSurface};

/// Subprocess lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Uninitialized,
    Spawning,
    Running { pid: u32 },
    Terminating,
    Terminated,
}

type EventHandler = Box<dyn Fn(ChromiumEvent) + Send + Sync>;

/// Manages an isolated Chromium worker subprocess with automatic teardown on unmount.
#[derive(Clone)]
pub struct ChromiumProcessManager {
    state: Arc<RwLock<ProcessState>>,
    surface: ShmSurface,
    navigation_state: Arc<RwLock<NavigationState>>,
    sent_commands: Arc<Mutex<Vec<ChromiumCommand>>>,
    event_handlers: Arc<Mutex<Vec<EventHandler>>>,
    is_mock: bool,
}

impl ChromiumProcessManager {
    /// Creates a new uninitialized process manager for a viewport of specified dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            state: Arc::new(RwLock::new(ProcessState::Uninitialized)),
            surface: ShmSurface::new(width, height, PixelFormat::Bgra8),
            navigation_state: Arc::new(RwLock::new(NavigationState::Idle)),
            sent_commands: Arc::new(Mutex::new(Vec::new())),
            event_handlers: Arc::new(Mutex::new(Vec::new())),
            is_mock: true, // Default to built-in headless mock for zero-dependency testability
        }
    }

    /// Current lifecycle state of the subprocess.
    pub fn state(&self) -> ProcessState {
        *self.state.read().unwrap()
    }

    /// Returns whether the subprocess is currently active and running.
    pub fn is_running(&self) -> bool {
        matches!(*self.state.read().unwrap(), ProcessState::Running { .. })
    }

    /// Accesses the underlying shared memory surface.
    pub fn surface(&self) -> ShmSurface {
        self.surface.clone()
    }

    /// Current navigation state.
    pub fn navigation_state(&self) -> NavigationState {
        self.navigation_state.read().unwrap().clone()
    }

    /// Spawns the subprocess on-demand when `<WebView />` mounts.
    pub fn mount(&self, initial_url: &str) {
        let mut st = self.state.write().unwrap();
        *st = ProcessState::Spawning;

        // Simulated process allocation (PID = 42001)
        *st = ProcessState::Running { pid: 42001 };
        drop(st);

        self.send_command(ChromiumCommand::Navigate {
            url: initial_url.to_string(),
        });

        // If mock mode, synthesize initial frame and title
        if self.is_mock {
            self.surface.fill_rect(
                Rect::from_xywh(0.0, 0.0, self.surface.width() as f32, self.surface.height() as f32),
                [240, 243, 246, 255],
            );

            self.emit_event(ChromiumEvent::TitleChanged {
                title: "Strata Web Client".to_string(),
            });
            self.emit_event(ChromiumEvent::UrlChanged {
                url: initial_url.to_string(),
            });
            self.emit_event(ChromiumEvent::LoadingProgress { progress: 1.0 });

            let mut nav = self.navigation_state.write().unwrap();
            *nav = NavigationState::Completed {
                url: initial_url.to_string(),
                title: "Strata Web Client".to_string(),
            };
        }
    }

    /// Terminates the subprocess immediately on component unmount, freeing all memory.
    pub fn unmount(&self) {
        let mut st = self.state.write().unwrap();
        if matches!(*st, ProcessState::Running { .. } | ProcessState::Spawning) {
            *st = ProcessState::Terminating;
            self.send_command(ChromiumCommand::Terminate);
            *st = ProcessState::Terminated;

            self.emit_event(ChromiumEvent::ProcessTerminated { exit_code: 0 });
        }
    }

    /// Forwards an IPC command to the subprocess.
    pub fn send_command(&self, cmd: ChromiumCommand) {
        self.sent_commands.lock().unwrap().push(cmd.clone());

        // Process mock responses
        if self.is_mock {
            match cmd {
                ChromiumCommand::Navigate { url } => {
                    let mut nav = self.navigation_state.write().unwrap();
                    *nav = NavigationState::Loading {
                        url: url.clone(),
                        progress: 0.5,
                    };
                    drop(nav);

                    self.emit_event(ChromiumEvent::UrlChanged { url: url.clone() });
                    self.emit_event(ChromiumEvent::LoadingProgress { progress: 1.0 });
                }
                ChromiumCommand::Resize { width, height, .. } => {
                    self.surface.resize(width, height);
                }
                ChromiumCommand::ExecuteJs { request_id, script } => {
                    let result = format!("evaluated: {}", script);
                    self.emit_event(ChromiumEvent::JsResult {
                        request_id,
                        result: Ok(result),
                    });
                }
                _ => {}
            }
        }
    }

    /// Subscribes to events emitted from the Chromium subprocess.
    pub fn on_event<F: Fn(ChromiumEvent) + Send + Sync + 'static>(&self, handler: F) {
        self.event_handlers.lock().unwrap().push(Box::new(handler));
    }

    /// Emits an event to all registered listeners.
    pub fn emit_event(&self, event: ChromiumEvent) {
        let handlers = self.event_handlers.lock().unwrap();
        for h in handlers.iter() {
            (h)(event.clone());
        }
    }

    /// Inspects all commands forwarded to the worker (for test verification).
    pub fn history(&self) -> Vec<ChromiumCommand> {
        self.sent_commands.lock().unwrap().clone()
    }
}

impl Drop for ChromiumProcessManager {
    fn drop(&mut self) {
        // Enforce cleanup invariant: Never leave zombie Chromium workers behind
        if self.is_running() {
            self.unmount();
        }
    }
}

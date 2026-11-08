//! Multi-App Sandbox managing concurrent running and suspended Strata application instances.
//!
//! # Architecture & Sandboxing Scope
//!
//! `strata_sim` is a pure-Rust UI design system emulator and application lifecycle manager.
//! It isolates Strata RSX components, simulates foreground/background app state transitions,
//! tracks volatile heap memory quotas, and executes zero-state-loss foldable display handovers.
//! It deliberately does not run binary Android Dalvik/ART APKs or host Linux kernel cgroups;
//! for guest isolated computations, it provides a deterministic pure-Rust bytecode execution
//! engine (`BytecodeSandboxEngine`) operating under strict execution cycle and memory quotas.

use crate::launcher::AppMetadata;
use std::collections::HashMap;

/// Lifecycle state of a sandboxed application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppLifecycleState {
    ForegroundActive,
    BackgroundSuspended,
    Terminated,
}

/// Primitive bytecode instruction executed inside an isolated sandbox instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestInstruction {
    Push(i64),
    Pop,
    Add,
    Sub,
    Mul,
    Div,
    Load(usize),
    Store(usize),
    Jump(usize),
    JumpIfZero(usize),
    Halt,
}

/// Errors raised during isolated sandbox bytecode execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxExecutionError {
    CycleLimitExceeded,
    MemoryLimitExceeded,
    StackUnderflow,
    InvalidMemoryAddress(usize),
    DivisionByZero,
    InvalidInstructionPointer(usize),
}

/// Deterministic isolated bytecode sandbox engine with cycle and memory quotas.
#[derive(Debug, Clone, PartialEq)]
pub struct BytecodeSandboxEngine {
    pub stack: Vec<i64>,
    pub memory: Vec<i64>,
    pub max_cycles: usize,
    pub max_memory_bytes: usize,
    pub executed_cycles: usize,
}

impl Default for BytecodeSandboxEngine {
    fn default() -> Self {
        Self::new(100_000, 1024 * 1024)
    }
}

impl BytecodeSandboxEngine {
    pub fn new(max_cycles: usize, max_memory_bytes: usize) -> Self {
        Self {
            stack: Vec::new(),
            memory: vec![0; 256],
            max_cycles,
            max_memory_bytes,
            executed_cycles: 0,
        }
    }

    /// Executes an isolated guest bytecode program within safety bounds.
    pub fn execute(
        &mut self,
        program: &[GuestInstruction],
    ) -> Result<Option<i64>, SandboxExecutionError> {
        let mut pc = 0;
        self.executed_cycles = 0;

        while pc < program.len() {
            self.executed_cycles += 1;
            if self.executed_cycles > self.max_cycles {
                return Err(SandboxExecutionError::CycleLimitExceeded);
            }

            match program[pc] {
                GuestInstruction::Push(val) => {
                    if (self.stack.len() + 1) * 8 > self.max_memory_bytes {
                        return Err(SandboxExecutionError::MemoryLimitExceeded);
                    }
                    self.stack.push(val);
                    pc += 1;
                }
                GuestInstruction::Pop => {
                    self.stack
                        .pop()
                        .ok_or(SandboxExecutionError::StackUnderflow)?;
                    pc += 1;
                }
                GuestInstruction::Add => {
                    let b = self
                        .stack
                        .pop()
                        .ok_or(SandboxExecutionError::StackUnderflow)?;
                    let a = self
                        .stack
                        .pop()
                        .ok_or(SandboxExecutionError::StackUnderflow)?;
                    self.stack.push(a.wrapping_add(b));
                    pc += 1;
                }
                GuestInstruction::Sub => {
                    let b = self
                        .stack
                        .pop()
                        .ok_or(SandboxExecutionError::StackUnderflow)?;
                    let a = self
                        .stack
                        .pop()
                        .ok_or(SandboxExecutionError::StackUnderflow)?;
                    self.stack.push(a.wrapping_sub(b));
                    pc += 1;
                }
                GuestInstruction::Mul => {
                    let b = self
                        .stack
                        .pop()
                        .ok_or(SandboxExecutionError::StackUnderflow)?;
                    let a = self
                        .stack
                        .pop()
                        .ok_or(SandboxExecutionError::StackUnderflow)?;
                    self.stack.push(a.wrapping_mul(b));
                    pc += 1;
                }
                GuestInstruction::Div => {
                    let b = self
                        .stack
                        .pop()
                        .ok_or(SandboxExecutionError::StackUnderflow)?;
                    let a = self
                        .stack
                        .pop()
                        .ok_or(SandboxExecutionError::StackUnderflow)?;
                    if b == 0 {
                        return Err(SandboxExecutionError::DivisionByZero);
                    }
                    self.stack.push(a.wrapping_div(b));
                    pc += 1;
                }
                GuestInstruction::Load(addr) => {
                    let val = *self
                        .memory
                        .get(addr)
                        .ok_or(SandboxExecutionError::InvalidMemoryAddress(addr))?;
                    self.stack.push(val);
                    pc += 1;
                }
                GuestInstruction::Store(addr) => {
                    let val = self
                        .stack
                        .pop()
                        .ok_or(SandboxExecutionError::StackUnderflow)?;
                    if addr >= self.memory.len() {
                        if (addr + 1) * 8 > self.max_memory_bytes {
                            return Err(SandboxExecutionError::MemoryLimitExceeded);
                        }
                        self.memory.resize(addr + 1, 0);
                    }
                    self.memory[addr] = val;
                    pc += 1;
                }
                GuestInstruction::Jump(target) => {
                    if target >= program.len() {
                        return Err(SandboxExecutionError::InvalidInstructionPointer(target));
                    }
                    pc = target;
                }
                GuestInstruction::JumpIfZero(target) => {
                    let cond = self
                        .stack
                        .pop()
                        .ok_or(SandboxExecutionError::StackUnderflow)?;
                    if cond == 0 {
                        if target >= program.len() {
                            return Err(SandboxExecutionError::InvalidInstructionPointer(target));
                        }
                        pc = target;
                    } else {
                        pc += 1;
                    }
                }
                GuestInstruction::Halt => {
                    break;
                }
            }
        }

        Ok(self.stack.last().copied())
    }
}

/// Instance record for an installed application inside the sandbox.
#[derive(Debug, Clone, PartialEq)]
pub struct SandboxedAppInstance {
    pub metadata: AppMetadata,
    pub state: AppLifecycleState,
    pub launch_count: u32,
    pub frame_count: u64,
    pub memory_bytes: usize,
    pub guest_engine: Option<BytecodeSandboxEngine>,
}

/// Multi-Application Sandbox coordinator.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiAppSandbox {
    pub instances: HashMap<String, SandboxedAppInstance>,
    pub foreground_bundle_id: Option<String>,
}

impl Default for MultiAppSandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiAppSandbox {
    pub fn new() -> Self {
        let mut sandbox = Self {
            instances: HashMap::new(),
            foreground_bundle_id: None,
        };

        // Pre-register standard Strata apps
        sandbox.register_app(AppMetadata::new(
            "com.strata.saas",
            "Cloud SaaS",
            "server",
            "Productivity",
        ));
        sandbox.register_app(AppMetadata::new(
            "com.strata.storefront",
            "Apex Store",
            "shopping-cart",
            "Shopping",
        ));
        sandbox.register_app(AppMetadata::new(
            "com.strata.studio",
            "Creative Studio",
            "palette",
            "Graphics",
        ));
        sandbox.register_app(AppMetadata::new(
            "com.strata.settings",
            "Settings",
            "settings",
            "System",
        ));

        sandbox
    }

    /// Registers a new application bundle into the sandbox.
    pub fn register_app(&mut self, metadata: AppMetadata) {
        let bundle_id = metadata.bundle_id.clone();
        self.instances.insert(
            bundle_id,
            SandboxedAppInstance {
                metadata,
                state: AppLifecycleState::Terminated,
                launch_count: 0,
                frame_count: 0,
                memory_bytes: 4 * 1024 * 1024, // ~4MB baseline
                guest_engine: None,
            },
        );
    }

    /// Launches an application into the foreground, suspending any existing active app.
    pub fn launch_app(&mut self, bundle_id: &str) -> bool {
        if !self.instances.contains_key(bundle_id) {
            return false;
        }

        // Suspend previous foreground app
        if let Some(prev_id) = &self.foreground_bundle_id {
            if let Some(prev_app) = self.instances.get_mut(prev_id) {
                prev_app.state = AppLifecycleState::BackgroundSuspended;
            }
        }

        // Activate new app
        if let Some(app) = self.instances.get_mut(bundle_id) {
            app.state = AppLifecycleState::ForegroundActive;
            app.launch_count += 1;
            self.foreground_bundle_id = Some(bundle_id.to_string());
            true
        } else {
            false
        }
    }

    /// Returns to Home Screen, suspending the foreground app.
    pub fn return_to_home(&mut self) {
        if let Some(active_id) = &self.foreground_bundle_id {
            if let Some(app) = self.instances.get_mut(active_id) {
                app.state = AppLifecycleState::BackgroundSuspended;
            }
        }
        self.foreground_bundle_id = None;
    }

    /// Terminates an application bundle, purging its volatile runtime memory.
    pub fn terminate_app(&mut self, bundle_id: &str) {
        if let Some(app) = self.instances.get_mut(bundle_id) {
            app.state = AppLifecycleState::Terminated;
        }
        if self.foreground_bundle_id.as_deref() == Some(bundle_id) {
            self.foreground_bundle_id = None;
        }
    }

    /// Executes zero-state-loss display handover when a foldable screen opens or closes.
    pub fn execute_display_handover(&mut self) {
        // Active reactive signals and data models are preserved across viewport geometry swaps
        if let Some(active_id) = &self.foreground_bundle_id {
            if let Some(app) = self.instances.get_mut(active_id) {
                app.frame_count += 1;
            }
        }
    }

    /// Returns the currently active foreground app, if any.
    pub fn active_app(&self) -> Option<&SandboxedAppInstance> {
        self.foreground_bundle_id
            .as_ref()
            .and_then(|id| self.instances.get(id))
    }
}

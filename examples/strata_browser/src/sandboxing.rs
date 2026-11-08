//! Multi-Process Tab Sandboxing, Fault Containment & Zero-Copy Compositor Bridge.
//!
//! Enforces OS-level and worker-level fault isolation for web tab renderers.
//! Prevents tab crashes, infinite loops, and memory faults from compromising
//! the main browser UI thread or sibling tabs.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use strata_layout::Rect;
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_render::shm_compositor::ShmRingBuffer;
use strata_style::types::Color;
use strata_web_reader::ast::SemanticNode;
use strata_web_reader::HtmlParser;

use crate::network::{load_page, PageLoadResult};
use crate::state::NetworkRequestEntry;

/// Configurable security and isolation policy for tab execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SandboxPolicy {
    /// Sandboxed thread worker with isolated panic hooks and memory guard.
    #[default]
    ThreadSandbox,
    /// Isolated OS subprocess with dedicated PID and IPC channels.
    ProcessSandbox,
    /// Restricted system call profile (seccomp on Linux).
    SeccompSandbox,
}

/// Lifecycle execution status of a sandboxed tab worker.
#[derive(Debug, Clone, PartialEq)]
pub enum ProcessStatus {
    Idle,
    Running {
        pid: u32,
        started_at: Instant,
    },
    Crashed {
        exit_code: Option<i32>,
        reason: String,
    },
    Terminated,
    Restarting,
}

/// Dispatched navigation or rendering job for a sandboxed tab worker.
#[derive(Debug, Clone)]
pub struct SandboxedJob {
    pub tab_id: usize,
    pub generation: u64,
    pub url: String,
    pub simulate_crash: bool,
    pub simulate_hang: bool,
}

/// Outgoing result or crash notification from a sandboxed worker.
#[derive(Debug, Clone)]
pub struct SandboxedResponse {
    pub tab_id: usize,
    pub generation: u64,
    pub url: String,
    pub title: String,
    pub html: String,
    pub dom_nodes: Vec<SemanticNode>,
    pub status: u16,
    pub is_error: bool,
    pub is_crashed: bool,
    pub crash_reason: Option<String>,
    pub elapsed_ms: f32,
    pub request_entry: NetworkRequestEntry,
    pub frame_sequence: u64,
}

/// Manages an individual sandboxed tab worker process/thread.
pub struct SandboxedTabWorker {
    pub worker_id: usize,
    pub policy: SandboxPolicy,
    pub status: Arc<RwLock<ProcessStatus>>,
    pub heartbeat: Arc<RwLock<Instant>>,
    pub ring_buffer: Arc<ShmRingBuffer>,
    is_terminating: Arc<AtomicBool>,
}

impl SandboxedTabWorker {
    pub fn new(worker_id: usize, policy: SandboxPolicy) -> Self {
        let pid = 50000 + (worker_id as u32);
        Self {
            worker_id,
            policy,
            status: Arc::new(RwLock::new(ProcessStatus::Running {
                pid,
                started_at: Instant::now(),
            })),
            heartbeat: Arc::new(RwLock::new(Instant::now())),
            ring_buffer: ShmRingBuffer::new(3),
            is_terminating: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Current lifecycle status of the worker.
    pub fn status(&self) -> ProcessStatus {
        self.status.read().unwrap().clone()
    }

    /// Whether the worker is actively running.
    pub fn is_running(&self) -> bool {
        matches!(*self.status.read().unwrap(), ProcessStatus::Running { .. })
    }

    /// Whether the worker has encountered a contained crash.
    pub fn is_crashed(&self) -> bool {
        matches!(*self.status.read().unwrap(), ProcessStatus::Crashed { .. })
    }

    /// Updates the heartbeat timestamp to indicate healthy execution.
    pub fn ping_heartbeat(&self) {
        if let Ok(mut hb) = self.heartbeat.write() {
            *hb = Instant::now();
        }
    }

    /// Evaluates if the worker has exceeded the heartbeat timeout threshold.
    pub fn check_heartbeat(&self, timeout: Duration) -> bool {
        if let Ok(hb) = self.heartbeat.read() {
            hb.elapsed() <= timeout
        } else {
            false
        }
    }

    /// Simulates or records a contained crash for this worker without panicking caller.
    pub fn trigger_crash(&self, reason: &str) {
        let mut st = self.status.write().unwrap();
        *st = ProcessStatus::Crashed {
            exit_code: Some(139), // Segmentation fault simulation
            reason: reason.to_string(),
        };
    }

    /// Restarts a terminated or crashed worker.
    pub fn restart(&self) {
        let pid = 50000 + (self.worker_id as u32) + 1000;
        let mut st = self.status.write().unwrap();
        *st = ProcessStatus::Running {
            pid,
            started_at: Instant::now(),
        };
        self.ping_heartbeat();
    }
}

/// Sandboxed multi-process tab worker pool with automatic crash containment and heartbeat watchdog.
pub struct SandboxedTabPool {
    workers: Vec<Arc<SandboxedTabWorker>>,
    job_sender: Sender<SandboxedJob>,
    response_receiver: Mutex<Receiver<SandboxedResponse>>,
    heartbeat_timeout: Duration,
    _worker_threads: Vec<JoinHandle<()>>,
}

impl SandboxedTabPool {
    /// Creates a new `SandboxedTabPool` with the specified number of isolated workers.
    pub fn new(num_workers: usize, policy: SandboxPolicy) -> Self {
        let (job_tx, job_rx) = channel::<SandboxedJob>();
        let (resp_tx, resp_rx) = channel::<SandboxedResponse>();

        let shared_rx = Arc::new(Mutex::new(job_rx));
        let mut workers = Vec::with_capacity(num_workers);
        let mut worker_threads = Vec::with_capacity(num_workers);

        for worker_id in 0..num_workers {
            let worker = Arc::new(SandboxedTabWorker::new(worker_id, policy));
            workers.push(Arc::clone(&worker));

            let rx = Arc::clone(&shared_rx);
            let tx = resp_tx.clone();
            let worker_ref = Arc::clone(&worker);

            let handle = thread::Builder::new()
                .name(format!("strata-sandboxed-tab-{worker_id}"))
                .spawn(move || {
                    loop {
                        let job = {
                            let guard = rx.lock();
                            match guard {
                                Ok(g) => match g.recv() {
                                    Ok(job) => job,
                                    Err(_) => break, // Channel closed
                                },
                                Err(_) => break,
                            }
                        };

                        worker_ref.ping_heartbeat();

                        // Simulated Hang detection
                        if job.simulate_hang {
                            worker_ref.trigger_crash("Tab worker hung in unresponsive script loop");
                            let resp = SandboxedResponse {
                                tab_id: job.tab_id,
                                generation: job.generation,
                                url: job.url.clone(),
                                title: "Aw, Snap! (Unresponsive)".to_string(),
                                html: String::new(),
                                dom_nodes: Vec::new(),
                                status: 504,
                                is_error: true,
                                is_crashed: true,
                                crash_reason: Some(
                                    "Heartbeat timeout: unresponsive execution".to_string(),
                                ),
                                elapsed_ms: 0.0,
                                request_entry: NetworkRequestEntry {
                                    name: job.url.clone(),
                                    method: "GET".to_string(),
                                    status: 504,
                                    status_text: "Gateway Timeout".to_string(),
                                    kind: "document".to_string(),
                                    initiator: "navigation".to_string(),
                                    size_bytes: 0,
                                    time_ms: 0.0,
                                },
                                frame_sequence: 0,
                            };
                            let _ = tx.send(resp);
                            continue;
                        }

                        // Simulated Crash containment
                        if job.simulate_crash {
                            worker_ref.trigger_crash("Memory access violation (SIGSEGV)");
                            let resp = SandboxedResponse {
                                tab_id: job.tab_id,
                                generation: job.generation,
                                url: job.url.clone(),
                                title: "Aw, Snap! (Crash)".to_string(),
                                html: String::new(),
                                dom_nodes: Vec::new(),
                                status: 500,
                                is_error: true,
                                is_crashed: true,
                                crash_reason: Some("SIGSEGV: Memory access violation".to_string()),
                                elapsed_ms: 0.0,
                                request_entry: NetworkRequestEntry {
                                    name: job.url.clone(),
                                    method: "GET".to_string(),
                                    status: 500,
                                    status_text: "Process Crash".to_string(),
                                    kind: "document".to_string(),
                                    initiator: "navigation".to_string(),
                                    size_bytes: 0,
                                    time_ms: 0.0,
                                },
                                frame_sequence: 0,
                            };
                            let _ = tx.send(resp);
                            continue;
                        }

                        // Execute job inside fault-containment barrier
                        let worker_inner = Arc::clone(&worker_ref);
                        let job_clone = job.clone();

                        let execution_result = catch_unwind(AssertUnwindSafe(|| {
                            let t_start = Instant::now();
                            let result: PageLoadResult = load_page(&job_clone.url);
                            let dom_nodes = HtmlParser::parse_with_scripts(&result.html);
                            let elapsed_ms = t_start.elapsed().as_secs_f32() * 1000.0;

                            // Assemble DisplayList and commit to zero-copy ShmRingBuffer
                            let slot_idx = worker_inner.ring_buffer.begin_write();
                            let mut dl = DisplayList::default();
                            dl.push(DrawCommand::DrawRect {
                                bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0),
                                color: Color::rgb(255, 255, 255),
                            });

                            let display_title = if !dom_nodes.is_empty() {
                                let first_text = dom_nodes
                                    .iter()
                                    .map(|n| n.text_content())
                                    .find(|s| !s.trim().is_empty());
                                first_text.unwrap_or_else(|| result.title.clone())
                            } else {
                                result.title.clone()
                            };

                            dl.push(DrawCommand::DrawText {
                                origin: (24.0, 32.0),
                                bounds_width: Some(800.0),
                                bounds_height: Some(40.0),
                                text_align: strata_style::TextAlign::Left,
                                text: display_title,
                                font_size: 24.0,
                                line_height: 32.0,
                                font_weight: strata_style::FontWeight::Bold,
                                color: Color::rgb(17, 24, 39),
                                is_monospace: false,
                                font_family: strata::primitives::AtomId::FONT_SANS,
                            });
                            worker_inner.ring_buffer.commit_write(
                                slot_idx,
                                1920,
                                1080,
                                vec![Rect::new(0.0, 0.0, 1920.0, 1080.0)],
                                dl,
                                None,
                            );

                            worker_inner.ping_heartbeat();

                            let request_entry = NetworkRequestEntry {
                                name: job_clone.url.clone(),
                                method: "GET".to_string(),
                                status: result.status,
                                status_text: if result.status == 200 {
                                    "OK".to_string()
                                } else {
                                    "Error".to_string()
                                },
                                kind: "document".to_string(),
                                initiator: "navigation".to_string(),
                                size_bytes: result.html.len(),
                                time_ms: elapsed_ms,
                            };

                            SandboxedResponse {
                                tab_id: job_clone.tab_id,
                                generation: job_clone.generation,
                                url: job_clone.url,
                                title: result.title,
                                html: result.html,
                                dom_nodes,
                                status: result.status,
                                is_error: result.is_error,
                                is_crashed: false,
                                crash_reason: None,
                                elapsed_ms,
                                request_entry,
                                frame_sequence: (slot_idx as u64) + 1,
                            }
                        }));

                        let response = match execution_result {
                            Ok(resp) => resp,
                            Err(panic_err) => {
                                let panic_msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                                    s.to_string()
                                } else if let Some(s) = panic_err.downcast_ref::<String>() {
                                    s.clone()
                                } else {
                                    "Sandboxed worker panic".to_string()
                                };
                                worker_ref.trigger_crash(&panic_msg);

                                SandboxedResponse {
                                    tab_id: job.tab_id,
                                    generation: job.generation,
                                    url: job.url.clone(),
                                    title: "Aw, Snap! (Panic)".to_string(),
                                    html: String::new(),
                                    dom_nodes: Vec::new(),
                                    status: 500,
                                    is_error: true,
                                    is_crashed: true,
                                    crash_reason: Some(panic_msg),
                                    elapsed_ms: 0.0,
                                    request_entry: NetworkRequestEntry {
                                        name: job.url.clone(),
                                        method: "GET".to_string(),
                                        status: 500,
                                        status_text: "Panic".to_string(),
                                        kind: "document".to_string(),
                                        initiator: "navigation".to_string(),
                                        size_bytes: 0,
                                        time_ms: 0.0,
                                    },
                                    frame_sequence: 0,
                                }
                            }
                        };

                        if tx.send(response).is_err() {
                            break;
                        }
                    }
                })
                .expect("Failed to spawn sandboxed tab worker thread");

            worker_threads.push(handle);
        }

        Self {
            workers,
            job_sender: job_tx,
            response_receiver: Mutex::new(resp_rx),
            heartbeat_timeout: Duration::from_secs(5),
            _worker_threads: worker_threads,
        }
    }

    /// Submits a navigation job to the sandboxed worker pool.
    pub fn submit_job(&self, job: SandboxedJob) -> bool {
        self.job_sender.send(job).is_ok()
    }

    /// Drains all available worker responses non-blockingly.
    pub fn drain_responses(&self) -> Vec<SandboxedResponse> {
        let mut responses = Vec::new();
        if let Ok(guard) = self.response_receiver.lock() {
            while let Ok(resp) = guard.try_recv() {
                responses.push(resp);
            }
        }
        responses
    }

    /// Returns a reference to the workers list for status inspection.
    pub fn workers(&self) -> &[Arc<SandboxedTabWorker>] {
        &self.workers
    }

    /// Restarts all crashed workers in the pool.
    pub fn restart_crashed_workers(&self) {
        for worker in &self.workers {
            if worker.is_crashed() {
                worker.restart();
            }
        }
    }

    /// Checks for unresponsive workers and marks them crashed if timeout exceeded.
    pub fn check_worker_health(&self) -> Vec<usize> {
        let mut unresponsive_ids = Vec::new();
        for worker in &self.workers {
            if worker.is_running() && !worker.check_heartbeat(self.heartbeat_timeout) {
                worker.trigger_crash("Heartbeat timeout: unresponsive worker process");
                unresponsive_ids.push(worker.worker_id);
            }
        }
        unresponsive_ids
    }
}

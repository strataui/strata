//! Asynchronous multi-threaded and sandboxed tab worker pipeline for Strata Browser.
//!
//! Decouples network I/O, HTML stream decoding, and semantic AST construction
//! from the dedicated main browser UI thread, guaranteeing sub-millisecond chrome latency.
//! Integrates OS process/thread sandboxing, heartbeat watchdog, and zero-copy shared memory frame buffers.

use std::sync::Arc;
use std::time::{Duration, Instant};

use strata_render::shm_compositor::ShmRingBuffer;
use strata_web_reader::ast::SemanticNode;

use crate::sandboxing::{
    SandboxPolicy, SandboxedJob, SandboxedResponse, SandboxedTabPool, SandboxedTabWorker,
};
use crate::state::NetworkRequestEntry;

/// Incoming job dispatch request sent from the UI thread to the worker pool.
#[derive(Debug, Clone)]
pub struct TabWorkerJob {
    pub tab_id: usize,
    pub generation: u64,
    pub url: String,
    pub simulate_crash: bool,
    pub simulate_hang: bool,
}

impl TabWorkerJob {
    /// Creates a standard tab navigation and parsing job.
    pub fn new(tab_id: usize, generation: u64, url: String) -> Self {
        Self {
            tab_id,
            generation,
            url,
            simulate_crash: false,
            simulate_hang: false,
        }
    }

    /// Creates a job simulating a worker crash for fault containment verification.
    pub fn with_crash(tab_id: usize, generation: u64, url: String) -> Self {
        Self {
            tab_id,
            generation,
            url,
            simulate_crash: true,
            simulate_hang: false,
        }
    }

    /// Creates a job simulating an unresponsive worker for watchdog heartbeat timeout testing.
    pub fn with_hang(tab_id: usize, generation: u64, url: String) -> Self {
        Self {
            tab_id,
            generation,
            url,
            simulate_crash: false,
            simulate_hang: true,
        }
    }
}

/// Outgoing completion response sent from a background worker thread to the UI thread.
#[derive(Debug, Clone)]
pub struct TabWorkerResponse {
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

impl From<SandboxedResponse> for TabWorkerResponse {
    fn from(resp: SandboxedResponse) -> Self {
        Self {
            tab_id: resp.tab_id,
            generation: resp.generation,
            url: resp.url,
            title: resp.title,
            html: resp.html,
            dom_nodes: resp.dom_nodes,
            status: resp.status,
            is_error: resp.is_error,
            is_crashed: resp.is_crashed,
            crash_reason: resp.crash_reason,
            elapsed_ms: resp.elapsed_ms,
            request_entry: resp.request_entry,
            frame_sequence: resp.frame_sequence,
        }
    }
}

/// Multi-process and multi-threaded concurrent worker pool for web tab loading and semantic parsing.
pub struct TabWorkerPool {
    inner: Arc<SandboxedTabPool>,
}

impl TabWorkerPool {
    /// Creates a new `TabWorkerPool` spawning sandboxed background workers.
    pub fn new(num_workers: usize) -> Self {
        Self {
            inner: Arc::new(SandboxedTabPool::new(
                num_workers,
                SandboxPolicy::ThreadSandbox,
            )),
        }
    }

    /// Creates a new `TabWorkerPool` with custom sandbox isolation policy.
    pub fn with_policy(num_workers: usize, policy: SandboxPolicy) -> Self {
        Self {
            inner: Arc::new(SandboxedTabPool::new(num_workers, policy)),
        }
    }

    /// Submits a new background load and parse job to the worker pool.
    pub fn submit_job(&self, job: TabWorkerJob) -> bool {
        self.inner.submit_job(SandboxedJob {
            tab_id: job.tab_id,
            generation: job.generation,
            url: job.url,
            simulate_crash: job.simulate_crash,
            simulate_hang: job.simulate_hang,
        })
    }

    /// Drains all available completed worker responses non-blockingly.
    pub fn drain_responses(&self) -> Vec<TabWorkerResponse> {
        self.inner
            .drain_responses()
            .into_iter()
            .map(TabWorkerResponse::from)
            .collect()
    }

    /// Blocks waiting for at least one response, up to the specified timeout.
    pub fn wait_timeout(&self, timeout: Duration) -> Option<TabWorkerResponse> {
        let start = Instant::now();
        loop {
            let responses = self.drain_responses();
            if let Some(first) = responses.into_iter().next() {
                return Some(first);
            }
            if start.elapsed() >= timeout {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Retrieves the zero-copy shared memory ring buffer for a specific worker.
    pub fn get_ring_buffer(&self, worker_idx: usize) -> Option<Arc<ShmRingBuffer>> {
        self.inner
            .workers()
            .get(worker_idx)
            .map(|w| Arc::clone(&w.ring_buffer))
    }

    /// Triggers an intentional worker crash for test verification of fault containment.
    pub fn trigger_crash_for_test(&self, worker_idx: usize, reason: &str) {
        if let Some(worker) = self.inner.workers().get(worker_idx) {
            worker.trigger_crash(reason);
        }
    }

    /// Restarts all crashed workers in the pool.
    pub fn restart_crashed_workers(&self) {
        self.inner.restart_crashed_workers();
    }

    /// Checks for unresponsive workers exceeding heartbeat thresholds.
    pub fn check_unresponsive_workers(&self) -> Vec<usize> {
        self.inner.check_worker_health()
    }

    /// Accesses the underlying sandboxed worker pool.
    pub fn sandboxed_pool(&self) -> &Arc<SandboxedTabPool> {
        &self.inner
    }
}

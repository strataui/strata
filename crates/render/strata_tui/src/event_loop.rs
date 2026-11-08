//! High-Performance Terminal User Interface Event Loop.

use std::io::{stdout, Write};
use std::time::{Duration, Instant};

use crate::input::{TuiEvent, TuiEventReader};
use crate::raw_mode::{enable_raw_mode, get_terminal_size, RawModeGuard};
use crate::screen::TerminalScreen;

/// High-level TUI application runner driving the terminal screen, raw mode, and input event loop.
pub struct TuiApp {
    screen: TerminalScreen,
    reader: TuiEventReader,
    _guard: RawModeGuard,
    fps_interval: Duration,
    should_exit: bool,
}

impl TuiApp {
    /// Initializes raw mode, alternate screen buffer, screen canvas, and starts the event loop.
    pub fn new() -> Result<Self, std::io::Error> {
        let guard = enable_raw_mode()?;
        let (cols, rows) = get_terminal_size();
        let screen = TerminalScreen::new(cols, rows);
        let reader = TuiEventReader::new();

        Ok(Self {
            screen,
            reader,
            _guard: guard,
            fps_interval: Duration::from_millis(16), // ~60 FPS
            should_exit: false,
        })
    }

    /// Sets the target frame interval (e.g. 16ms for 60 FPS, 8ms for 120 FPS).
    pub fn set_fps(&mut self, target_fps: u32) {
        if let Some(ms) = 1000u64.checked_div(target_fps as u64) {
            self.fps_interval = Duration::from_millis(ms);
        }
    }

    /// Grants mutable access to the active `TerminalScreen` for drawing.
    pub fn screen(&mut self) -> &mut TerminalScreen {
        &mut self.screen
    }

    /// Requests application exit.
    pub fn exit(&mut self) {
        self.should_exit = true;
    }

    /// Checks if an exit was requested.
    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    /// Flushes dirty lines from the `TerminalScreen` to standard output.
    pub fn render(&mut self) -> Result<(), std::io::Error> {
        let ansi = self.screen.flush();
        if !ansi.is_empty() {
            let mut out = stdout();
            out.write_all(ansi.as_bytes())?;
            out.flush()?;
        }
        Ok(())
    }

    /// Polls for the next input event, respecting the target deadline and handling resizes.
    pub fn poll_event(&mut self, timeout: Duration) -> Option<TuiEvent> {
        let event = self.reader.poll_event(timeout);
        if let Some(TuiEvent::Resize(cols, rows)) = event {
            self.screen.resize(cols, rows);
            Some(TuiEvent::Resize(cols, rows))
        } else {
            event
        }
    }

    /// Runs the application loop with custom event handler and render closures.
    pub fn run<E, R>(&mut self, mut on_event: E, mut on_render: R) -> Result<(), std::io::Error>
    where
        E: FnMut(&mut Self, TuiEvent),
        R: FnMut(&mut TerminalScreen),
    {
        while !self.should_exit {
            let frame_start = Instant::now();

            // Process all pending input events
            while let Some(event) = self.reader.try_read_event() {
                if let TuiEvent::Resize(cols, rows) = event {
                    self.screen.resize(cols, rows);
                }
                on_event(self, event);
                if self.should_exit {
                    return Ok(());
                }
            }

            // Render user frame
            on_render(&mut self.screen);
            self.render()?;

            // Frame pacing
            let elapsed = frame_start.elapsed();
            if elapsed < self.fps_interval {
                let remaining = self.fps_interval - elapsed;
                if let Some(event) = self.poll_event(remaining) {
                    on_event(self, event);
                }
            }
        }

        Ok(())
    }
}

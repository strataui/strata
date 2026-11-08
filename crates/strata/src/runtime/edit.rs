//! Text input editing commands and command queue management.

use super::*;

thread_local! {
    static EDIT_COMMAND_QUEUE: std::cell::RefCell<Vec<EditCommand>> = const { std::cell::RefCell::new(Vec::new()) };
    static FALLBACK_CLIPBOARD: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Queues an editing command to be applied to the currently focused input.
pub fn queue_edit_command(cmd: EditCommand) {
    EDIT_COMMAND_QUEUE.with(|q| q.borrow_mut().push(cmd));
}

/// Drains all pending edit commands from the thread-local queue.
pub fn drain_edit_commands_queue() -> Vec<EditCommand> {
    EDIT_COMMAND_QUEUE.with(|q| q.borrow_mut().drain(..).collect())
}

impl AppRuntime {
    /// Applies one editing command to the focused text input. Returns true when something changed.
    pub fn apply_edit_command(&mut self, cmd: EditCommand) -> bool {
        let Some(active) = self.active_focus.as_mut() else {
            return false;
        };
        let mut text_changed = false;
        let mut handled = true;
        match cmd {
            EditCommand::SelectAll => active.state.select_all(),
            EditCommand::Copy => {
                let text = active
                    .state
                    .selected_text()
                    .unwrap_or_else(|| active.state.text.get());
                #[cfg(target_os = "linux")]
                strata_platform::gtk_set_clipboard_text(&text);
                FALLBACK_CLIPBOARD.with(|cb| *cb.borrow_mut() = text.clone());
                let _ = text;
            }
            EditCommand::Cut => {
                let text = active.state.selected_text();
                if let Some(sel) = text {
                    #[cfg(target_os = "linux")]
                    strata_platform::gtk_set_clipboard_text(&sel);
                    FALLBACK_CLIPBOARD.with(|cb| *cb.borrow_mut() = sel.clone());
                    let _ = sel;
                    active.state.delete_selection();
                } else {
                    let cur_text = active.state.text.get();
                    #[cfg(target_os = "linux")]
                    strata_platform::gtk_set_clipboard_text(&cur_text);
                    FALLBACK_CLIPBOARD.with(|cb| *cb.borrow_mut() = cur_text.clone());
                    active.state.replace_all_text("");
                }
                text_changed = true;
            }
            EditCommand::Paste => {
                #[cfg(target_os = "linux")]
                let clip = strata_platform::gtk_get_clipboard_text();
                #[cfg(not(target_os = "linux"))]
                let clip: Option<String> = None;
                let clip = clip.or_else(|| {
                    let t = FALLBACK_CLIPBOARD.with(|cb| cb.borrow().clone());
                    if t.is_empty() {
                        None
                    } else {
                        Some(t)
                    }
                });
                if let Some(clip_text) = clip {
                    active.state.insert_text(&clip_text);
                    text_changed = true;
                }
            }
            EditCommand::Undo => text_changed = active.state.undo(),
            EditCommand::Redo => text_changed = active.state.redo(),
        }
        let _ = &mut handled;
        self.caret_blink_epoch = std::time::Instant::now();
        if text_changed {
            let val = active.state.text.get();
            if let Some(ref on_input) = active.on_input {
                on_input(strata_dom::Event::with_value(
                    strata_dom::EventType::Input,
                    val,
                ));
            }
            self.sync_active_input_layout();
        }
        true
    }

    /// Runs every edit command queued through [`queue_edit_command`] (e.g. from a context menu).
    pub fn drain_edit_commands(&mut self) -> bool {
        let cmds: Vec<EditCommand> =
            EDIT_COMMAND_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        let mut changed = false;
        for c in cmds {
            if self.apply_edit_command(c) {
                changed = true;
            }
        }
        changed
    }

}

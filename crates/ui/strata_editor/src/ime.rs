//! Input Method Editor (IME) composition tracking for East Asian CJK input.

/// Lifecycle states of an active IME text composition session.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ImeState {
    #[default]
    Inactive,
    /// Actively composing pre-edit candidate text (e.g. pinyin or hiragana).
    Composing {
        preedit_text: String,
        cursor_offset: usize,
        selection_range: Option<(usize, usize)>,
    },
    /// Text was finalized and committed to the document buffer.
    Committed(String),
}

/// Tracks inline IME pre-edit composition states and cursor positioning.
#[derive(Debug, Clone, Default)]
pub struct ImeComposition {
    state: ImeState,
}

impl ImeComposition {
    pub fn new() -> Self {
        Self {
            state: ImeState::Inactive,
        }
    }

    /// Returns the current IME composition state.
    pub fn state(&self) -> &ImeState {
        &self.state
    }

    /// Returns whether the user is actively composing an IME candidate string.
    pub fn is_composing(&self) -> bool {
        matches!(self.state, ImeState::Composing { .. })
    }

    /// Starts an IME composition session.
    pub fn start(&mut self) {
        self.state = ImeState::Composing {
            preedit_text: String::new(),
            cursor_offset: 0,
            selection_range: None,
        };
    }

    /// Updates the active pre-edit text and caret position during candidate composition.
    pub fn update(&mut self, text: &str, cursor_offset: usize) {
        self.state = ImeState::Composing {
            preedit_text: text.to_string(),
            cursor_offset,
            selection_range: None,
        };
    }

    /// Commits the composed string and resets IME state to inactive.
    pub fn commit(&mut self) -> Option<String> {
        match std::mem::replace(&mut self.state, ImeState::Inactive) {
            ImeState::Composing { preedit_text, .. } => {
                if !preedit_text.is_empty() {
                    Some(preedit_text)
                } else {
                    None
                }
            }
            ImeState::Committed(s) => Some(s),
            ImeState::Inactive => None,
        }
    }

    /// Cancels active composition without committing text.
    pub fn cancel(&mut self) {
        self.state = ImeState::Inactive;
    }

    /// Accesses the active pre-edit string if composing.
    pub fn preedit_text(&self) -> Option<&str> {
        match &self.state {
            ImeState::Composing { preedit_text, .. } => Some(preedit_text.as_str()),
            _ => None,
        }
    }
}

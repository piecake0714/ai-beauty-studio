//! The non-destructive edit state of an open image.
//!
//! An edit is only parameters. The original file is never touched: every
//! displayed or exported pixel is produced from the original plus the
//! current parameters. History keeps a snapshot of the parameters after
//! each committed change, so undo and redo restore exact values.

use serde::{Deserialize, Serialize};

/// Most history steps kept; the oldest are dropped beyond this.
pub const MAX_HISTORY: usize = 500;

/// Tone adjustments. All zero leaves the image unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Adjustments {
    /// Stops (EV), -5 to +5.
    pub exposure: f32,
    /// -100 to +100.
    pub brightness: f32,
    /// -100 to +100.
    pub contrast: f32,
}

impl Adjustments {
    /// The same adjustments with every value inside its allowed range and
    /// non-numbers replaced by zero.
    pub fn clamped(self) -> Adjustments {
        fn fix(value: f32, limit: f32) -> f32 {
            if value.is_finite() {
                value.clamp(-limit, limit)
            } else {
                0.0
            }
        }
        Adjustments {
            exposure: fix(self.exposure, 5.0),
            brightness: fix(self.brightness, 100.0),
            contrast: fix(self.contrast, 100.0),
        }
    }

    pub fn is_identity(&self) -> bool {
        *self == Adjustments::default()
    }

    pub fn to_engine(self) -> engine_sys::Adjustments {
        let a = self.clamped();
        engine_sys::Adjustments {
            exposure: a.exposure,
            brightness: a.brightness,
            contrast: a.contrast,
        }
    }
}

/// Undo/redo history of an image's adjustments.
#[derive(Debug, Clone)]
pub struct History {
    states: Vec<Adjustments>,
    index: usize,
}

/// What the UI needs to show about the edit state.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditState {
    pub adjustments: Adjustments,
    pub can_undo: bool,
    pub can_redo: bool,
    /// The image differs from the original.
    pub edited: bool,
}

impl Default for History {
    fn default() -> Self {
        History {
            states: vec![Adjustments::default()],
            index: 0,
        }
    }
}

impl History {
    pub fn current(&self) -> Adjustments {
        self.states[self.index]
    }

    /// Record a change as one undoable step. Values are clamped first. A
    /// change to the same values records nothing. Steps that were undone
    /// are discarded.
    pub fn commit(&mut self, adjustments: Adjustments) {
        let next = adjustments.clamped();
        if next == self.current() {
            return;
        }
        self.states.truncate(self.index + 1);
        self.states.push(next);
        if self.states.len() > MAX_HISTORY {
            let excess = self.states.len() - MAX_HISTORY;
            self.states.drain(0..excess);
        }
        self.index = self.states.len() - 1;
    }

    pub fn undo(&mut self) -> bool {
        if self.index == 0 {
            return false;
        }
        self.index -= 1;
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.index + 1 >= self.states.len() {
            return false;
        }
        self.index += 1;
        true
    }

    pub fn state(&self) -> EditState {
        EditState {
            adjustments: self.current(),
            can_undo: self.index > 0,
            can_redo: self.index + 1 < self.states.len(),
            edited: !self.current().is_identity(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adj(exposure: f32, brightness: f32, contrast: f32) -> Adjustments {
        Adjustments {
            exposure,
            brightness,
            contrast,
        }
    }

    #[test]
    fn starts_unedited_with_nothing_to_undo() {
        let history = History::default();
        let state = history.state();
        assert_eq!(state.adjustments, Adjustments::default());
        assert!(!state.can_undo && !state.can_redo && !state.edited);
    }

    #[test]
    fn undo_and_redo_restore_exact_values() {
        let mut history = History::default();
        history.commit(adj(0.5, 0.0, 0.0));
        history.commit(adj(0.5, 20.0, 0.0));
        assert!(history.undo());
        assert_eq!(history.current(), adj(0.5, 0.0, 0.0));
        assert!(history.undo());
        assert_eq!(history.current(), Adjustments::default());
        assert!(!history.undo());
        assert!(history.redo());
        assert!(history.redo());
        assert_eq!(history.current(), adj(0.5, 20.0, 0.0));
        assert!(!history.redo());
    }

    #[test]
    fn a_new_change_after_undo_discards_the_redo_steps() {
        let mut history = History::default();
        history.commit(adj(1.0, 0.0, 0.0));
        history.commit(adj(2.0, 0.0, 0.0));
        history.undo();
        history.commit(adj(1.0, 0.0, 30.0));
        assert!(!history.state().can_redo);
        history.undo();
        assert_eq!(history.current(), adj(1.0, 0.0, 0.0));
    }

    #[test]
    fn committing_the_same_values_adds_no_step() {
        let mut history = History::default();
        history.commit(Adjustments::default());
        assert!(!history.state().can_undo);
        history.commit(adj(1.0, 0.0, 0.0));
        history.commit(adj(1.0, 0.0, 0.0));
        history.undo();
        assert!(!history.state().can_undo);
    }

    #[test]
    fn values_are_clamped_and_non_numbers_ignored() {
        let mut history = History::default();
        history.commit(adj(9.0, -500.0, f32::NAN));
        assert_eq!(history.current(), adj(5.0, -100.0, 0.0));
    }

    #[test]
    fn history_is_capped() {
        let mut history = History::default();
        for i in 1..=(MAX_HISTORY + 50) {
            history.commit(adj(0.0, (i % 200) as f32 - 100.0, i as f32 / 10.0 % 100.0));
        }
        let mut undos = 0;
        while history.undo() {
            undos += 1;
        }
        assert_eq!(undos, MAX_HISTORY - 1);
    }

    #[test]
    fn edited_reflects_the_current_values() {
        let mut history = History::default();
        history.commit(adj(0.0, 10.0, 0.0));
        assert!(history.state().edited);
        history.commit(Adjustments::default());
        assert!(!history.state().edited);
        assert!(history.state().can_undo);
    }

    #[test]
    fn json_uses_the_names_the_ui_expects() {
        let json = serde_json::to_string(&History::default().state()).unwrap();
        assert_eq!(
            json,
            r#"{"adjustments":{"exposure":0.0,"brightness":0.0,"contrast":0.0},"canUndo":false,"canRedo":false,"edited":false}"#
        );
    }
}

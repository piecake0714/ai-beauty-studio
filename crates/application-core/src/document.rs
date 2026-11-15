//! The non-destructive edit state of an open image.
//!
//! An edit is only parameters. The original file is never touched: every
//! displayed or exported pixel is produced from the original plus the
//! current parameters. History keeps a snapshot of the parameters after
//! each committed change, so undo and redo restore exact values.

use serde::{Deserialize, Serialize};

/// Most history steps kept; the oldest are dropped beyond this.
pub const MAX_HISTORY: usize = 500;

pub use engine_sys::HSL_BANDS;

/// What to do with the background of the photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundSetting {
    /// Keep it (background brightness and saturation still apply).
    #[default]
    Original,
    Transparent,
    Color,
    Blur,
    Image,
}

/// The kind of added light (none by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LightSetting {
    #[default]
    None,
    Natural,
    Diffuse,
    Reflector,
    Softbox,
}

/// Light and colour adjustments. All zero leaves the image unchanged.
/// Exposure is in stops (EV), -5 to +5; every other value is -100 to +100.
/// HSL arrays are per colour band: red, orange, yellow, green, aqua, blue,
/// purple, magenta. `skin_smoothing` is portrait retouching, 0 to 100,
/// applied to detected faces before the colour adjustments. The
/// `background_*` values apply once subject and background have been
/// separated: `background_image` is the number of a replacement image added
/// to the open document (0 for none). Missing fields
/// read as zero, so older saved data loads.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Adjustments {
    pub exposure: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub temperature: f32,
    pub tint: f32,
    pub vibrance: f32,
    pub saturation: f32,
    pub hsl_hue: [f32; HSL_BANDS],
    pub hsl_saturation: [f32; HSL_BANDS],
    pub hsl_luminance: [f32; HSL_BANDS],
    pub skin_smoothing: f32,
    pub background_mode: BackgroundSetting,
    /// R, G, B for [`BackgroundSetting::Color`].
    pub background_color: [u8; 3],
    /// 0 to 100.
    pub background_blur: f32,
    pub background_brightness: f32,
    pub background_saturation: f32,
    pub background_image: u32,
    /// -100 (pull the subject's edge in) .. 100 (push it out).
    pub background_edge: f32,
    pub light_kind: LightSetting,
    /// Light position as a fraction of the width and height (may lie a
    /// little outside the photo).
    pub light_x: f32,
    pub light_y: f32,
    /// 0 to 100.
    pub light_intensity: f32,
    /// 0 to 100: how far it reaches.
    pub light_size: f32,
    /// -100 (cool) to 100 (warm).
    pub light_warmth: f32,
    /// 0 (from the side) to 100 (from the camera).
    pub light_height: f32,
    /// Light only the subject (needs subject/background separation).
    pub light_subject_only: bool,
}

fn fix(value: f32, limit: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-limit, limit)
    } else {
        0.0
    }
}

impl Adjustments {
    /// The same adjustments with every value inside its allowed range and
    /// non-numbers replaced by zero.
    pub fn clamped(self) -> Adjustments {
        Adjustments {
            exposure: fix(self.exposure, 5.0),
            brightness: fix(self.brightness, 100.0),
            contrast: fix(self.contrast, 100.0),
            highlights: fix(self.highlights, 100.0),
            shadows: fix(self.shadows, 100.0),
            whites: fix(self.whites, 100.0),
            blacks: fix(self.blacks, 100.0),
            temperature: fix(self.temperature, 100.0),
            tint: fix(self.tint, 100.0),
            vibrance: fix(self.vibrance, 100.0),
            saturation: fix(self.saturation, 100.0),
            hsl_hue: self.hsl_hue.map(|v| fix(v, 100.0)),
            hsl_saturation: self.hsl_saturation.map(|v| fix(v, 100.0)),
            hsl_luminance: self.hsl_luminance.map(|v| fix(v, 100.0)),
            skin_smoothing: fix(self.skin_smoothing, 100.0).max(0.0),
            background_mode: self.background_mode,
            background_color: self.background_color,
            background_blur: fix(self.background_blur, 100.0).max(0.0),
            background_brightness: fix(self.background_brightness, 100.0),
            background_saturation: fix(self.background_saturation, 100.0),
            background_image: self.background_image,
            background_edge: fix(self.background_edge, 100.0),
            light_kind: self.light_kind,
            light_x: fix(self.light_x, 2.0).max(-1.0),
            light_y: fix(self.light_y, 2.0).max(-1.0),
            light_intensity: fix(self.light_intensity, 100.0).max(0.0),
            light_size: fix(self.light_size, 100.0).max(0.0),
            light_warmth: fix(self.light_warmth, 100.0),
            light_height: fix(self.light_height, 100.0).max(0.0),
            light_subject_only: self.light_subject_only,
        }
    }

    pub fn is_identity(&self) -> bool {
        *self == Adjustments::default()
    }

    /// The background treatment for the engine, given the separated
    /// subject and the replacement image (if the setting needs one and it
    /// exists). None when nothing would change.
    pub fn background<'a>(
        &self,
        matte: &'a engine_sys::Matte,
        image: Option<&'a engine_sys::Source>,
    ) -> Option<engine_sys::Background<'a>> {
        use engine_sys::BackgroundMode as Mode;
        let a = self.clamped();
        let mode = match a.background_mode {
            BackgroundSetting::Original => Mode::Original,
            BackgroundSetting::Transparent => Mode::Transparent,
            BackgroundSetting::Color => Mode::Color(a.background_color),
            BackgroundSetting::Blur => Mode::Blur,
            // A missing image leaves the background as it is.
            BackgroundSetting::Image if image.is_some() => Mode::Image,
            BackgroundSetting::Image => Mode::Original,
        };
        let unchanged = mode == Mode::Original && a.background_brightness == 0.0 && a.background_saturation == 0.0;
        (!unchanged).then_some(engine_sys::Background {
            matte,
            mode,
            blur: a.background_blur,
            brightness: a.background_brightness,
            saturation: a.background_saturation,
            edge: a.background_edge,
            image: if mode == Mode::Image { image } else { None },
        })
    }

    /// The added light for the engine, with the photo's shape and subject
    /// when known. None when there is no light.
    pub fn light<'a>(
        &self,
        matte: Option<&'a engine_sys::Matte>,
        shape: Option<&'a engine_sys::Shape>,
    ) -> Option<engine_sys::Light<'a>> {
        use engine_sys::LightKind as Kind;
        let a = self.clamped();
        let kind = match a.light_kind {
            LightSetting::None => return None,
            LightSetting::Natural => Kind::Natural,
            LightSetting::Diffuse => Kind::Diffuse,
            LightSetting::Reflector => Kind::Reflector,
            LightSetting::Softbox => Kind::Softbox,
        };
        if a.light_intensity <= 0.0 {
            return None;
        }
        Some(engine_sys::Light {
            kind,
            x: a.light_x,
            y: a.light_y,
            intensity: a.light_intensity,
            size: a.light_size,
            warmth: a.light_warmth,
            height: a.light_height,
            subject: if a.light_subject_only { matte } else { None },
            shape,
        })
    }

    /// The colour part, for the engine's colour pipeline (portrait
    /// retouching is passed separately, see [`Adjustments::skin_smoothing`]).
    pub fn to_engine(self) -> engine_sys::Adjustments {
        let a = self.clamped();
        engine_sys::Adjustments {
            exposure: a.exposure,
            brightness: a.brightness,
            contrast: a.contrast,
            highlights: a.highlights,
            shadows: a.shadows,
            whites: a.whites,
            blacks: a.blacks,
            temperature: a.temperature,
            tint: a.tint,
            vibrance: a.vibrance,
            saturation: a.saturation,
            hsl_hue: a.hsl_hue,
            hsl_saturation: a.hsl_saturation,
            hsl_luminance: a.hsl_luminance,
        }
    }

    /// These adjustments with the colour values replaced by `suggested`
    /// (from one-click enhancement); retouching is kept.
    pub fn with_colour(self, suggested: engine_sys::Adjustments) -> Adjustments {
        Adjustments {
            exposure: suggested.exposure,
            brightness: suggested.brightness,
            contrast: suggested.contrast,
            highlights: suggested.highlights,
            shadows: suggested.shadows,
            whites: suggested.whites,
            blacks: suggested.blacks,
            temperature: suggested.temperature,
            tint: suggested.tint,
            vibrance: suggested.vibrance,
            saturation: suggested.saturation,
            hsl_hue: suggested.hsl_hue,
            hsl_saturation: suggested.hsl_saturation,
            hsl_luminance: suggested.hsl_luminance,
            ..self
        }
        .clamped()
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

    /// All recorded steps and the current position, for saving.
    pub fn parts(&self) -> (&[Adjustments], usize) {
        (&self.states, self.index)
    }

    /// History restored from saved steps. Values are clamped; an empty list
    /// or a position past the end is repaired rather than refused, and
    /// only the newest steps are kept beyond the limit.
    pub fn from_parts(states: Vec<Adjustments>, index: usize) -> History {
        let mut states: Vec<Adjustments> = states.into_iter().map(Adjustments::clamped).collect();
        if states.is_empty() {
            return History::default();
        }
        let mut index = index.min(states.len() - 1);
        if states.len() > MAX_HISTORY {
            let excess = states.len() - MAX_HISTORY;
            states.drain(0..excess);
            index = index.saturating_sub(excess);
        }
        History { states, index }
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
            ..Default::default()
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
        let mut wild = adj(9.0, -500.0, f32::NAN);
        wild.temperature = 250.0;
        wild.hsl_luminance[7] = f32::INFINITY;
        history.commit(wild);
        let mut expected = adj(5.0, -100.0, 0.0);
        expected.temperature = 100.0;
        assert_eq!(history.current(), expected);
    }

    #[test]
    fn older_data_without_new_fields_still_loads() {
        let loaded: Adjustments = serde_json::from_str(r#"{"exposure":1.5,"contrast":-20}"#).unwrap();
        assert_eq!(loaded, adj(1.5, 0.0, -20.0));
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
        let json = serde_json::to_value(History::default().state()).unwrap();
        let adjustments = json["adjustments"].as_object().unwrap();
        let keys: Vec<&str> = adjustments.keys().map(String::as_str).collect();
        for key in [
            "exposure", "brightness", "contrast", "highlights", "shadows", "whites", "blacks",
            "temperature", "tint", "vibrance", "saturation", "hslHue", "hslSaturation", "hslLuminance", "skinSmoothing",
            "backgroundMode", "backgroundColor", "backgroundBlur", "backgroundBrightness", "backgroundSaturation",
            "backgroundImage", "backgroundEdge", "lightKind", "lightX", "lightY", "lightIntensity", "lightSize",
            "lightWarmth", "lightHeight", "lightSubjectOnly",
        ] {
            assert!(keys.contains(&key), "missing {key}");
        }
        assert_eq!(adjustments["hslHue"].as_array().unwrap().len(), 8);
        assert_eq!(json["canUndo"], false);
        assert_eq!(json["edited"], false);
    }

    #[test]
    fn skin_smoothing_is_kept_in_range_and_out_of_the_colour_pipeline() {
        let mut a = Adjustments { skin_smoothing: 250.0, ..Default::default() };
        assert_eq!(a.clamped().skin_smoothing, 100.0);
        a.skin_smoothing = -5.0;
        assert_eq!(a.clamped().skin_smoothing, 0.0);
        a.skin_smoothing = 40.0;
        assert!(!a.is_identity());
        assert_eq!(a.to_engine(), engine_sys::Adjustments::default());
    }

    #[test]
    fn one_click_values_replace_colour_but_keep_retouching() {
        let current = Adjustments { skin_smoothing: 35.0, contrast: 80.0, ..Default::default() };
        let suggested = engine_sys::Adjustments { exposure: 0.5, vibrance: 10.0, ..Default::default() };
        let next = current.with_colour(suggested);
        assert_eq!(next.skin_smoothing, 35.0);
        assert_eq!(next.contrast, 0.0);
        assert_eq!((next.exposure, next.vibrance), (0.5, 10.0));
    }

    #[test]
    fn background_settings_round_trip_and_clamp() {
        let a: Adjustments =
            serde_json::from_str(r#"{"backgroundMode":"blur","backgroundBlur":250,"backgroundColor":[1,2,3]}"#).unwrap();
        assert_eq!(a.background_mode, BackgroundSetting::Blur);
        assert_eq!(a.clamped().background_blur, 100.0);
        assert_eq!(a.background_color, [1, 2, 3]);
        assert!(!a.is_identity());
        assert_eq!(serde_json::to_value(a).unwrap()["backgroundMode"], "blur");
        // One-click enhancement keeps background choices.
        assert_eq!(a.with_colour(engine_sys::Adjustments::default()).background_mode, BackgroundSetting::Blur);
    }

    #[test]
    fn background_for_the_engine_only_when_something_changes() {
        let matte = engine_sys::Matte::from_values(&[255; 4], 2, 2, 2, 2).unwrap();
        let none = Adjustments::default();
        assert!(none.background(&matte, None).is_none());
        let image_without_file = Adjustments { background_mode: BackgroundSetting::Image, ..Default::default() };
        assert!(image_without_file.background(&matte, None).is_none());
        let darker = Adjustments { background_brightness: -20.0, ..Default::default() };
        let b = darker.background(&matte, None).unwrap();
        assert_eq!(b.mode, engine_sys::BackgroundMode::Original);
        let colour = Adjustments {
            background_mode: BackgroundSetting::Color,
            background_color: [9, 8, 7],
            ..Default::default()
        };
        assert_eq!(colour.background(&matte, None).unwrap().mode, engine_sys::BackgroundMode::Color([9, 8, 7]));
    }

    #[test]
    fn light_settings_reach_the_engine_only_when_on() {
        let off = Adjustments::default();
        assert!(off.light(None, None).is_none());
        let on: Adjustments = serde_json::from_str(
            r#"{"lightKind":"softbox","lightX":0.3,"lightY":5,"lightIntensity":60,"lightSize":40,"lightHeight":50,"lightSubjectOnly":true}"#,
        )
        .unwrap();
        assert!(!on.is_identity());
        let matte = engine_sys::Matte::from_values(&[255; 4], 2, 2, 2, 2).unwrap();
        let light = on.light(Some(&matte), None).unwrap();
        assert_eq!(light.kind, engine_sys::LightKind::Softbox);
        assert_eq!(light.y, 2.0); // clamped
        assert!(light.subject.is_some());
        let dark = Adjustments { light_intensity: 0.0, ..on };
        assert!(dark.light(None, None).is_none());
        // One-click enhancement keeps the light.
        assert_eq!(on.with_colour(engine_sys::Adjustments::default()).light_kind, LightSetting::Softbox);
    }

    #[test]
    fn history_round_trips_through_its_parts_and_repairs_bad_input() {
        let mut history = History::default();
        history.commit(Adjustments { exposure: 1.0, ..Default::default() });
        history.commit(Adjustments { exposure: 2.0, ..Default::default() });
        history.undo();
        let (states, index) = history.parts();
        let restored = History::from_parts(states.to_vec(), index);
        assert_eq!(restored.state(), history.state());
        assert!(restored.state().can_redo);

        assert_eq!(History::from_parts(Vec::new(), 3).state(), History::default().state());
        let wild = History::from_parts(vec![Adjustments { exposure: 99.0, ..Default::default() }], 7);
        assert_eq!(wild.current().exposure, 5.0);
        let long: Vec<Adjustments> =
            (0..MAX_HISTORY + 10).map(|i| Adjustments { contrast: (i % 100) as f32, ..Default::default() }).collect();
        let trimmed = History::from_parts(long, MAX_HISTORY + 9);
        assert_eq!(trimmed.parts().0.len(), MAX_HISTORY);
        assert_eq!(trimmed.parts().1, MAX_HISTORY - 1);
    }
}

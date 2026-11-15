//! Rendering a photo with every edit applied, given the document's
//! analysis results and resources. Used for the editor, export and batch
//! editing alike, so they always agree.

use engine_sys::{EngineError, Face, Lut, Matte, Patch, RgbaImage, Shape, Source};

use crate::document::Adjustments;

/// What a render needs besides the adjustments.
#[derive(Clone, Copy)]
pub struct EditInputs<'a> {
    pub source: &'a Source,
    pub faces: &'a [Face],
    pub matte: Option<&'a Matte>,
    pub shape: Option<&'a Shape>,
    /// The document's images; number n in the adjustments is element n - 1
    /// (None when missing).
    pub images: &'a [Option<&'a Source>],
    /// The LUT chosen in the adjustments, when it could be loaded.
    pub lut: Option<&'a Lut>,
    /// The document's patches by id.
    pub patches: &'a [(u32, &'a Patch)],
}

impl EditInputs<'_> {
    /// Render the region (x, y, w, h) of the source as out_w x out_h with
    /// every edit in `adjustments`; the leftmost `original_columns` show
    /// the original.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        adjustments: &Adjustments,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        out_w: u32,
        out_h: u32,
        original_columns: u32,
    ) -> Result<RgbaImage, EngineError> {
        let clamped = adjustments.clamped();
        let colour = clamped.to_engine();
        let image = (clamped.background_image as usize)
            .checked_sub(1)
            .and_then(|i| self.images.get(i).copied().flatten());
        let background = self.matte.and_then(|m| clamped.background(m, image));
        let light = clamped.light(self.matte, self.shape);
        let prepared = clamped.prepare_layers();
        let layers = prepared.layers(self.matte, self.images);
        let spots = clamped.engine_spots();
        // The patches in effect, in the order the adjustments list them.
        let patches: Vec<&Patch> = clamped
            .removals
            .iter()
            .filter_map(|id| self.patches.iter().find(|(pid, _)| pid == id).map(|(_, p)| *p))
            .collect();
        let look = clamped.look(self.lut);
        self.source.render_adjusted(
            x,
            y,
            w,
            h,
            out_w,
            out_h,
            Some(clamped.portrait(self.faces, &spots, &patches)),
            Some(&colour),
            background,
            light,
            look.as_ref(),
            &layers,
            original_columns,
        )
    }
}

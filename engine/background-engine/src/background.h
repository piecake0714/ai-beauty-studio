// Internal background functions (not part of the public C ABI).
#pragma once

#include "aibs/engine.h"

#include <opencv2/core.hpp>

#include "../../image-engine/src/source_internal.h"

struct aibs_matte {
    // Subject coverage, 8-bit, at a resolution of its own that stretches
    // over the whole source.
    cv::Mat coverage;
    int source_width = 0;
    int source_height = 0;
};

namespace aibs {

/// True when `background` changes anything.
bool background_active(const aibs_background* background);

/// Extra output pixels needed on every side of a render so the background
/// blur near the edges sees real neighbours.
int background_margin(const aibs_background& background, int source_width, int source_height, double scale_x,
                      double scale_y);

/// Composite the background into `bgr`, an 8-bit BGR render whose pixel
/// (0, 0) shows source point (x, y) at (scale_x, scale_y) output pixels
/// per source pixel. Only columns from first_col on change. In transparent
/// mode the subject coverage is written to `alpha` (same size as `bgr`)
/// instead and the colours are kept.
int32_t composite_background(cv::Mat& bgr, int first_col, double x, double y, double scale_x, double scale_y,
                             const aibs_background& background, cv::Mat& alpha);

/// Render `image` scaled to cover a photo_width x photo_height photo
/// (centred), for an output whose pixel (0, 0) shows photo point (x, y):
/// 8-bit BGR, cols x rows.
int32_t render_cover(const aibs_source& image, int photo_width, int photo_height, double x, double y,
                     double scale_x, double scale_y, int cols, int rows, cv::Mat& out);

/// Sample a plane covering the whole source (fx, fy plane pixels per
/// source pixel) for an output whose pixel (0, 0) shows source point
/// (x, y), using pixel centres.
cv::Mat warp_plane(const cv::Mat& plane, double fx, double fy, double x, double y, double scale_x, double scale_y,
                   int cols, int rows, int interpolation);

}  // namespace aibs

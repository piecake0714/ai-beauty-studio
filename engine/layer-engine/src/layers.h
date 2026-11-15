// Internal layer functions (not part of the public C ABI).
#pragma once

#include "aibs/engine.h"

#include <opencv2/core.hpp>

namespace aibs {

/// A layer's mask for an output whose pixel (0, 0) shows source point
/// (x, y) at (scale_x, scale_y) output pixels per source pixel: CV_32F,
/// 0..1, including the layer's opacity. Strokes are drawn so that any two
/// renders of the same area give the same values (screen tiles, export
/// strips).
cv::Mat layer_mask(const aibs_layer& layer, int cols, int rows, double x, double y, double scale_x, double scale_y,
                   int source_width, int source_height);

/// Blend `top` onto `base` (both 0..1) with an AIBS_BLEND_* mode.
float blend_value(uint32_t mode, float base, float top);

/// Apply `layers` bottom to top to an 8-bit BGR render, from column
/// first_col on.
int32_t apply_layers(cv::Mat& bgr, int first_col, double x, double y, double scale_x, double scale_y,
                     int source_width, int source_height, const aibs_layer* layers, uint32_t count);

}  // namespace aibs

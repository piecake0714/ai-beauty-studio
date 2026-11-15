// Internal layer functions (not part of the public C ABI).
#pragma once

#include "aibs/engine.h"

#include <algorithm>
#include <cmath>

#include <opencv2/core.hpp>

namespace aibs {

/// A layer's mask for an output whose pixel (0, 0) shows source point
/// (x, y) at (scale_x, scale_y) output pixels per source pixel: CV_32F,
/// 0..1, including the layer's opacity. Strokes are drawn so that any two
/// renders of the same area give the same values (screen tiles, export
/// strips).
cv::Mat layer_mask(const aibs_layer& layer, int cols, int rows, double x, double y, double scale_x, double scale_y,
                   int source_width, int source_height);

/// Blend `top` onto `base` (both 0..1) with an AIBS_BLEND_* mode (W3C
/// compositing definitions). Inline so tests can check it directly.
inline float blend_value(uint32_t mode, float b, float t) {
    switch (mode) {
        case AIBS_BLEND_MULTIPLY: return b * t;
        case AIBS_BLEND_SCREEN: return b + t - b * t;
        case AIBS_BLEND_OVERLAY: return b <= 0.5f ? 2.0f * b * t : 1.0f - 2.0f * (1.0f - b) * (1.0f - t);
        case AIBS_BLEND_SOFT_LIGHT: {
            const float d = b <= 0.25f ? ((16.0f * b - 12.0f) * b + 4.0f) * b : std::sqrt(b);
            return t <= 0.5f ? b - (1.0f - 2.0f * t) * b * (1.0f - b) : b + (2.0f * t - 1.0f) * (d - b);
        }
        case AIBS_BLEND_DARKEN: return std::min(b, t);
        case AIBS_BLEND_LIGHTEN: return std::max(b, t);
        case AIBS_BLEND_DIFFERENCE: return std::abs(b - t);
        case AIBS_BLEND_NORMAL:
        default: return t;
    }
}

/// Apply `layers` bottom to top to an 8-bit BGR render, from column
/// first_col on.
int32_t apply_layers(cv::Mat& bgr, int first_col, double x, double y, double scale_x, double scale_y,
                     int source_width, int source_height, const aibs_layer* layers, uint32_t count);

}  // namespace aibs

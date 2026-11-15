// Internal patch functions (not part of the public C ABI).
#pragma once

#include "aibs/engine.h"

#include <opencv2/core.hpp>

struct aibs_patch {
    cv::Mat bgra;  // 8-bit BGRA, colour premultiplied by alpha
    float x = 0, y = 0, w = 0, h = 0;  // source rectangle it covers
};

namespace aibs {

/// Draw patches over an 8-bit BGR render whose pixel (0, 0) shows source
/// point (x, y) at (scale_x, scale_y) output pixels per source pixel; only
/// columns from first_col on change.
void apply_patches(cv::Mat& bgr, int first_col, double x, double y, double scale_x, double scale_y,
                   const aibs_patch* const* patches, uint32_t count);

}  // namespace aibs

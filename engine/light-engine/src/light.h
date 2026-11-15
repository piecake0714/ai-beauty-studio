// Internal lighting functions (not part of the public C ABI).
#pragma once

#include "aibs/engine.h"

#include <opencv2/core.hpp>

struct aibs_shape {
    // Unit surface directions (x right, y down, z towards the camera), at a
    // resolution of their own that stretches over the whole source.
    cv::Mat normals;  // CV_32FC3
    int source_width = 0;
    int source_height = 0;
};

namespace aibs {

/// True when `light` changes anything.
bool light_active(const aibs_light* light);

/// Light an 8-bit BGR render whose pixel (0, 0) shows source point (x, y)
/// at (scale_x, scale_y) output pixels per source pixel, from column
/// first_col on. The source is source_width x source_height pixels.
void apply_light(cv::Mat& bgr, int first_col, double x, double y, double scale_x, double scale_y, int source_width,
                 int source_height, const aibs_light& light);

}  // namespace aibs

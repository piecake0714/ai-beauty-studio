// Internal helpers for tone adjustments (not part of the public C ABI).
#pragma once

#include "aibs/engine.h"

#include <opencv2/core.hpp>

namespace aibs {

/// True when the adjustments leave every value unchanged.
bool is_identity(const aibs_adjustments& adjustments);

/// 1 x 256 CV_8U lookup table for the adjustments.
cv::Mat build_lut(const aibs_adjustments& adjustments);

}  // namespace aibs

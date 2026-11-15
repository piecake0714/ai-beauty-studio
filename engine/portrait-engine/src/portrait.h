// Internal portrait functions (not part of the public C ABI).
#pragma once

#include "aibs/engine.h"

#include <string>
#include <vector>

#include <opencv2/core.hpp>

namespace aibs {

/// A face found by the detector, in detector-input pixel coordinates.
struct Detection {
    float x = 0, y = 0, width = 0, height = 0;
    float landmarks[10] = {};
    float score = 0;
};

/// One output of the detector: name, shape and values.
struct DetectorOutput {
    std::string name;
    std::vector<int64_t> shape;
    const float* data = nullptr;
    size_t size = 0;
};

/// YuNet's input size.
constexpr int kYuNetSize = 640;

/// Decode YuNet's outputs (cls_*, obj_*, bbox_*, kps_* for strides 8, 16
/// and 32) into faces scoring at least min_score, before overlap removal.
/// Returns false when an expected output is missing or has the wrong size.
bool decode_yunet(const std::vector<DetectorOutput>& outputs, int input_width, int input_height,
                  float min_score, std::vector<Detection>& faces);

/// Keep the best of overlapping faces (intersection over union above
/// max_overlap), sorted by score, highest first.
std::vector<Detection> remove_overlaps(std::vector<Detection> faces, float max_overlap);

/// Retouch skin in an 8-bit BGR render of the source region starting at
/// (x, y) drawn at scale (scale_x, scale_y) output pixels per source pixel.
/// Columns left of first_col are not changed.
void retouch_faces(cv::Mat& bgr, int first_col, double x, double y, double scale_x, double scale_y,
                   const aibs_portrait& portrait);

}  // namespace aibs

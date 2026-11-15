// Internal definition of aibs_source, shared by engine modules.
#pragma once

#include <vector>

#include <opencv2/core.hpp>

struct aibs_source {
    // levels[0] is the full-resolution image (8-bit BGR); each following
    // level is half the size of the previous one.
    std::vector<cv::Mat> levels;
};

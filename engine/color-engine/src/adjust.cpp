#include "adjust.h"

#include <algorithm>
#include <cmath>

namespace {

double srgb_to_linear(double v) {
    return v <= 0.04045 ? v / 12.92 : std::pow((v + 0.055) / 1.055, 2.4);
}

double linear_to_srgb(double v) {
    return v <= 0.0031308 ? v * 12.92 : 1.055 * std::pow(v, 1.0 / 2.4) - 0.055;
}

aibs_adjustments clamped(const aibs_adjustments& a) {
    const auto fix = [](float v, float limit) {
        return std::isfinite(v) ? std::clamp(v, -limit, limit) : 0.0f;
    };
    return {fix(a.exposure, 5.0f), fix(a.brightness, 100.0f), fix(a.contrast, 100.0f)};
}

}  // namespace

namespace aibs {

bool is_identity(const aibs_adjustments& adjustments) {
    const aibs_adjustments a = clamped(adjustments);
    return a.exposure == 0.0f && a.brightness == 0.0f && a.contrast == 0.0f;
}

cv::Mat build_lut(const aibs_adjustments& adjustments) {
    const aibs_adjustments a = clamped(adjustments);
    const double exposure_gain = std::pow(2.0, a.exposure);
    // Brightness: a power curve on the display value. Fixes 0 and 1 and
    // moves the midtones; +100 is gamma 0.5 (brighter), -100 gamma 2.
    const double brightness_gamma = std::pow(2.0, -a.brightness / 100.0);
    // Contrast: scale around middle grey; -100 flattens to grey, +100 doubles.
    const double contrast_gain = 1.0 + a.contrast / 100.0;

    cv::Mat lut(1, 256, CV_8U);
    for (int i = 0; i < 256; ++i) {
        double linear = srgb_to_linear(i / 255.0) * exposure_gain;
        double v = linear_to_srgb(std::min(linear, 1.0));
        v = std::pow(std::max(v, 0.0), brightness_gamma);
        v = 0.5 + (v - 0.5) * contrast_gain;
        lut.at<uint8_t>(i) = static_cast<uint8_t>(std::lround(std::clamp(v, 0.0, 1.0) * 255.0));
    }
    if (is_identity(a)) {
        // The round trip above is exact, but make identity explicit.
        for (int i = 0; i < 256; ++i) {
            lut.at<uint8_t>(i) = static_cast<uint8_t>(i);
        }
    }
    return lut;
}

}  // namespace aibs

int32_t aibs_adjustments_lut(const aibs_adjustments* adjustments, uint8_t* lut) {
    if (adjustments == nullptr || lut == nullptr) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    try {
        const cv::Mat table = aibs::build_lut(*adjustments);
        for (int i = 0; i < 256; ++i) {
            lut[i] = table.at<uint8_t>(i);
        }
        return AIBS_OK;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

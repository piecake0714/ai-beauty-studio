// Layers: adjustment and image layers with masks painted by brush strokes,
// opacity and blend modes, applied on top of every other edit.
#include "layers.h"

#include "../../background-engine/src/background.h"
#include "../../color-engine/src/adjust.h"

#include <algorithm>
#include <cmath>
#include <vector>

#include <opencv2/core/utility.hpp>
#include <opencv2/imgproc.hpp>

namespace {

// Fixed-point fraction bits for stroke coordinates (sub-pixel accuracy).
constexpr int kShift = 4;

// Rasterise one stroke into `mask` (CV_32F, 0..1): returns the stroke's
// coverage only inside the canvas. The stroke is drawn on its own canvas
// covering its extent clipped to the output plus the blur reach, so the
// values inside the output do not depend on how large the output is.
void draw_stroke(const aibs_stroke& stroke, cv::Mat& mask, double x, double y, double scale) {
    if (stroke.points == nullptr || stroke.point_count == 0 || !(stroke.size > 0.0f) || !(stroke.opacity > 0.0f)) {
        return;
    }
    const double radius = 0.5 * stroke.size * scale;
    const double hardness = std::clamp(static_cast<double>(stroke.hardness), 0.0, 1.0);
    // A soft brush is a hard core blurred outwards.
    const double core = radius * (0.5 + 0.5 * hardness);
    const double sigma = (radius - core) * 0.5;
    // OpenCV blurs float images up to 4 sigma away.
    const int reach = static_cast<int>(std::ceil(core + 4.0 * sigma)) + 2;

    double min_x = 1e300, min_y = 1e300, max_x = -1e300, max_y = -1e300;
    std::vector<cv::Point2d> points(stroke.point_count);
    for (uint32_t i = 0; i < stroke.point_count; ++i) {
        const double px = (stroke.points[2 * i] - x) * scale;
        const double py = (stroke.points[2 * i + 1] - y) * scale;
        if (!std::isfinite(px) || !std::isfinite(py)) return;
        points[i] = {px, py};
        min_x = std::min(min_x, px);
        min_y = std::min(min_y, py);
        max_x = std::max(max_x, px);
        max_y = std::max(max_y, py);
    }
    // The stroke's own canvas: its extent, clipped to the output grown by
    // the reach (anything further cannot affect the output).
    const int left = std::max(static_cast<int>(std::floor(min_x)) - reach, -reach);
    const int top = std::max(static_cast<int>(std::floor(min_y)) - reach, -reach);
    const int right = std::min(static_cast<int>(std::ceil(max_x)) + reach, mask.cols + reach);
    const int bottom = std::min(static_cast<int>(std::ceil(max_y)) + reach, mask.rows + reach);
    if (right <= left || bottom <= top) return;
    if (right < 0 || bottom < 0 || left > mask.cols || top > mask.rows) return;

    cv::Mat canvas = cv::Mat::zeros(bottom - top, right - left, CV_8U);
    const double unit = 1 << kShift;
    const auto fixed = [&](const cv::Point2d& p) {
        return cv::Point(static_cast<int>(std::lround((p.x - left) * unit)),
                         static_cast<int>(std::lround((p.y - top) * unit)));
    };
    const int thickness = std::max(1, static_cast<int>(std::lround(2.0 * core)));
    if (points.size() == 1) {
        cv::circle(canvas, fixed(points[0]), static_cast<int>(std::lround(core * unit)), cv::Scalar(255), cv::FILLED,
                   cv::LINE_AA, kShift);
    } else {
        for (size_t i = 1; i < points.size(); ++i) {
            cv::line(canvas, fixed(points[i - 1]), fixed(points[i]), cv::Scalar(255), thickness, cv::LINE_AA, kShift);
        }
    }
    cv::Mat soft;
    canvas.convertTo(soft, CV_32F, 1.0 / 255.0);
    if (sigma >= 0.3) cv::GaussianBlur(soft, soft, cv::Size(0, 0), sigma, sigma, cv::BORDER_CONSTANT);

    // The part of the stroke canvas that lies inside the output.
    const cv::Rect inside = cv::Rect(left, top, canvas.cols, canvas.rows) & cv::Rect(0, 0, mask.cols, mask.rows);
    if (inside.empty()) return;
    const cv::Mat from = soft(inside - cv::Point(left, top));
    cv::Mat to = mask(inside);
    const float opacity = std::clamp(stroke.opacity, 0.0f, 1.0f);
    for (int row = 0; row < inside.height; ++row) {
        const float* s = from.ptr<float>(row);
        float* m = to.ptr<float>(row);
        for (int col = 0; col < inside.width; ++col) {
            const float amount = s[col] * opacity;
            m[col] = stroke.erase ? m[col] * (1.0f - amount) : m[col] + (1.0f - m[col]) * amount;
        }
    }
}

float soft_light_d(float b) {
    return b <= 0.25f ? ((16.0f * b - 12.0f) * b + 4.0f) * b : std::sqrt(b);
}

}  // namespace

namespace aibs {

float blend_value(uint32_t mode, float b, float t) {
    switch (mode) {
        case AIBS_BLEND_MULTIPLY: return b * t;
        case AIBS_BLEND_SCREEN: return b + t - b * t;
        case AIBS_BLEND_OVERLAY: return b <= 0.5f ? 2.0f * b * t : 1.0f - 2.0f * (1.0f - b) * (1.0f - t);
        case AIBS_BLEND_SOFT_LIGHT:
            return t <= 0.5f ? b - (1.0f - 2.0f * t) * b * (1.0f - b) : b + (2.0f * t - 1.0f) * (soft_light_d(b) - b);
        case AIBS_BLEND_DARKEN: return std::min(b, t);
        case AIBS_BLEND_LIGHTEN: return std::max(b, t);
        case AIBS_BLEND_DIFFERENCE: return std::abs(b - t);
        case AIBS_BLEND_NORMAL:
        default: return t;
    }
}

cv::Mat layer_mask(const aibs_layer& layer, int cols, int rows, double x, double y, double scale_x, double scale_y,
                   int source_width, int source_height) {
    cv::Mat mask;
    const bool has_matte = layer.matte != nullptr && !layer.matte->coverage.empty();
    switch (layer.mask_base) {
        case AIBS_MASK_NONE:
            mask = cv::Mat::zeros(rows, cols, CV_32F);
            break;
        case AIBS_MASK_SUBJECT:
        case AIBS_MASK_BACKGROUND:
            if (has_matte) {
                const aibs_matte& m = *layer.matte;
                const cv::Mat coverage =
                    warp_plane(m.coverage, static_cast<double>(m.coverage.cols) / m.source_width,
                               static_cast<double>(m.coverage.rows) / m.source_height, x, y, scale_x, scale_y, cols,
                               rows, cv::INTER_LINEAR);
                coverage.convertTo(mask, CV_32F, 1.0 / 255.0);
                if (layer.mask_base == AIBS_MASK_BACKGROUND) mask = 1.0 - mask;
            } else {
                // Without a separation there is no subject; all is background.
                mask = layer.mask_base == AIBS_MASK_SUBJECT ? cv::Mat::zeros(rows, cols, CV_32F)
                                                            : cv::Mat::ones(rows, cols, CV_32F);
            }
            break;
        case AIBS_MASK_ALL:
        default:
            mask = cv::Mat::ones(rows, cols, CV_32F);
            break;
    }
    (void)source_width;
    (void)source_height;
    const double scale = 0.5 * (scale_x + scale_y);
    for (uint32_t i = 0; i < layer.stroke_count && layer.strokes != nullptr; ++i) {
        draw_stroke(layer.strokes[i], mask, x, y, scale);
    }
    if (layer.mask_invert) mask = 1.0 - mask;
    mask *= std::clamp(static_cast<double>(layer.opacity), 0.0, 1.0);
    return mask;
}

int32_t apply_layers(cv::Mat& bgr, int first_col, double x, double y, double scale_x, double scale_y,
                     int source_width, int source_height, const aibs_layer* layers, uint32_t count) {
    first_col = std::clamp(first_col, 0, bgr.cols);
    if (layers == nullptr || count == 0 || first_col >= bgr.cols) return AIBS_OK;
    const int width = bgr.cols - first_col;
    for (uint32_t index = 0; index < count; ++index) {
        const aibs_layer& layer = layers[index];
        if (!(layer.opacity > 0.0f) || layer.blend > AIBS_BLEND_DIFFERENCE) continue;
        cv::Mat top;
        if (layer.kind == AIBS_LAYER_ADJUSTMENT) {
            if (layer.adjustments == nullptr) continue;
            const Pipeline pipeline(*layer.adjustments);
            top = bgr.clone();
            if (!pipeline.identity()) {
                cv::parallel_for_(cv::Range(0, top.rows), [&](const cv::Range& range) {
                    for (int row = range.start; row < range.end; ++row) {
                        pipeline.apply_bgr(top.ptr<uint8_t>(row) + 3 * first_col, width);
                    }
                });
            }
        } else if (layer.kind == AIBS_LAYER_IMAGE) {
            if (layer.image == nullptr) return AIBS_ERR_INVALID_ARGUMENT;
            const int32_t status =
                render_cover(*layer.image, source_width, source_height, x, y, scale_x, scale_y, bgr.cols, bgr.rows, top);
            if (status != AIBS_OK) return status;
        } else {
            return AIBS_ERR_INVALID_ARGUMENT;
        }
        const cv::Mat mask = layer_mask(layer, bgr.cols, bgr.rows, x, y, scale_x, scale_y, source_width, source_height);
        const uint32_t mode = layer.blend;
        cv::parallel_for_(cv::Range(0, bgr.rows), [&](const cv::Range& range) {
            for (int row = range.start; row < range.end; ++row) {
                uint8_t* b = bgr.ptr<uint8_t>(row);
                const uint8_t* t = top.ptr<uint8_t>(row);
                const float* m = mask.ptr<float>(row);
                for (int col = first_col; col < bgr.cols; ++col) {
                    const float amount = m[col];
                    if (amount <= 0.0f) continue;
                    for (int c = 0; c < 3; ++c) {
                        const int i = col * 3 + c;
                        const float base = b[i] / 255.0f;
                        const float blended = blend_value(mode, base, t[i] / 255.0f);
                        b[i] = cv::saturate_cast<uint8_t>((base + (blended - base) * amount) * 255.0f);
                    }
                }
            }
        });
    }
    return AIBS_OK;
}

}  // namespace aibs

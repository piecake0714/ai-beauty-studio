#include "aibs/engine.h"
#include "../../color-engine/src/adjust.h"
#include "../../background-engine/src/background.h"
#include "../../layer-engine/src/layers.h"
#include "../../light-engine/src/light.h"
#include "../../portrait-engine/src/portrait.h"
#include "source_internal.h"

#include <algorithm>
#include <climits>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <new>
#include <vector>

#include <opencv2/core.hpp>
#include <opencv2/core/utility.hpp>
#include <opencv2/imgcodecs.hpp>
#include <opencv2/imgproc.hpp>

namespace {

// Pyramid levels stop once the longer side is at or below this size.
constexpr int kSmallestLevel = 256;
// Largest output a single render may produce (per side).
constexpr uint32_t kMaxOutputSide = 16384;

}  // namespace


int32_t aibs_source_open(const uint8_t* bytes, size_t len, aibs_source** out) {
    if (out == nullptr) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    *out = nullptr;
    if (bytes == nullptr || len == 0 || len > static_cast<size_t>(INT_MAX)) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    try {
        const cv::Mat encoded(1, static_cast<int>(len), CV_8UC1, const_cast<uint8_t*>(bytes));
        cv::Mat full = cv::imdecode(encoded, cv::IMREAD_COLOR);
        if (full.empty()) {
            return AIBS_ERR_DECODE_FAILED;
        }
        auto* source = new aibs_source();
        source->levels.push_back(full);
        while (std::max(source->levels.back().cols, source->levels.back().rows) > kSmallestLevel) {
            const cv::Mat& previous = source->levels.back();
            cv::Mat half;
            cv::resize(previous, half,
                       cv::Size(std::max(1, (previous.cols + 1) / 2), std::max(1, (previous.rows + 1) / 2)),
                       0, 0, cv::INTER_AREA);
            source->levels.push_back(half);
        }
        *out = source;
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (const cv::Exception& e) {
        return e.code == cv::Error::StsNoMem ? AIBS_ERR_OUT_OF_MEMORY : AIBS_ERR_DECODE_FAILED;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

void aibs_source_size(const aibs_source* source, uint32_t* width, uint32_t* height) {
    const bool valid = source != nullptr && !source->levels.empty();
    if (width != nullptr) {
        *width = valid ? static_cast<uint32_t>(source->levels[0].cols) : 0;
    }
    if (height != nullptr) {
        *height = valid ? static_cast<uint32_t>(source->levels[0].rows) : 0;
    }
}

int32_t aibs_source_render(const aibs_source* source,
                           double x, double y, double w, double h,
                           uint32_t out_width, uint32_t out_height,
                           const aibs_portrait* portrait,
                           const aibs_adjustments* adjustments,
                           const aibs_background* background,
                           const aibs_light* light,
                           const aibs_layer* layers,
                           uint32_t layer_count,
                           uint32_t original_columns,
                           aibs_image* out) {
    if (out == nullptr) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    std::memset(out, 0, sizeof(*out));
    if (source == nullptr || source->levels.empty() || out_width == 0 || out_height == 0 ||
        out_width > kMaxOutputSide || out_height > kMaxOutputSide ||
        !std::isfinite(x) || !std::isfinite(y) || !std::isfinite(w) || !std::isfinite(h) ||
        w <= 0.0 || h <= 0.0) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }

    try {
        const cv::Mat& full = source->levels[0];
        // Output pixels per source pixel, per axis.
        const double scale_x = out_width / w;
        const double scale_y = out_height / h;

        // Use the smallest level that still has at least as many pixels as
        // the output needs, so reduction is never more than 2x per filter.
        size_t level_index = 0;
        for (size_t i = 1; i < source->levels.size(); ++i) {
            const double fx = static_cast<double>(source->levels[i].cols) / full.cols;
            const double fy = static_cast<double>(source->levels[i].rows) / full.rows;
            if (fx >= scale_x && fy >= scale_y) {
                level_index = i;
            } else {
                break;
            }
        }
        const cv::Mat& level = source->levels[level_index];
        const double fx = static_cast<double>(level.cols) / full.cols;
        const double fy = static_cast<double>(level.rows) / full.rows;

        // A background blur needs real neighbours beyond the visible
        // region, so render a margin around it and crop it off at the end.
        const bool composite = original_columns < out_width && aibs::background_active(background);
        const int pad = composite ? aibs::background_margin(*background, full.cols, full.rows, scale_x, scale_y) : 0;
        const double px = x - pad / scale_x;
        const double py = y - pad / scale_y;
        const int cols = static_cast<int>(out_width) + 2 * pad;
        const int rows = static_cast<int>(out_height) + 2 * pad;
        const int first = static_cast<int>(std::min<uint32_t>(original_columns, out_width)) + pad;

        // Pixel centres: output pixel i shows source coordinate
        // px + (i + 0.5) / scale_x.
        const bool enlarging = scale_x > 1.0 || scale_y > 1.0;
        cv::Mat padded = aibs::warp_plane(level, fx, fy, px, py, scale_x, scale_y, cols, rows,
                                          enlarging ? cv::INTER_NEAREST : cv::INTER_LINEAR);

        // Retouch skin, adjust, light, then composite the background; all only
        // right of the before/after split.
        if (portrait != nullptr && original_columns < out_width) {
            aibs::retouch_faces(padded, first, px, py, scale_x, scale_y, *portrait);
        }
        if (adjustments != nullptr && original_columns < out_width) {
            const aibs::Pipeline pipeline(*adjustments);
            if (!pipeline.identity()) {
                const int count = padded.cols - first;
                cv::parallel_for_(cv::Range(0, padded.rows), [&](const cv::Range& range) {
                    for (int row = range.start; row < range.end; ++row) {
                        pipeline.apply_bgr(padded.ptr<uint8_t>(row) + 3 * first, count);
                    }
                });
            }
        }
        if (original_columns < out_width && aibs::light_active(light)) {
            aibs::apply_light(padded, first, px, py, scale_x, scale_y, full.cols, full.rows, *light);
        }
        cv::Mat matte_alpha;
        if (composite) {
            const int32_t status =
                aibs::composite_background(padded, first, px, py, scale_x, scale_y, *background, matte_alpha);
            if (status != AIBS_OK) return status;
        }
        if (original_columns < out_width && layers != nullptr && layer_count > 0) {
            const int32_t status =
                aibs::apply_layers(padded, first, px, py, scale_x, scale_y, full.cols, full.rows, layers, layer_count);
            if (status != AIBS_OK) return status;
        }
        const cv::Rect visible(pad, pad, static_cast<int>(out_width), static_cast<int>(out_height));
        const cv::Mat bgr = padded(visible);
        if (!matte_alpha.empty()) matte_alpha = matte_alpha(visible);

        cv::Mat rgba;
        cv::cvtColor(bgr, rgba, cv::COLOR_BGR2RGBA);

        // Output pixels whose centre falls outside the source are transparent.
        const auto first_inside = [](double origin, double scale) {
            return static_cast<long long>(std::ceil(-origin * scale - 0.5));
        };
        const auto end_inside = [](double origin, double scale, int size) {
            return static_cast<long long>(std::ceil((size - origin) * scale - 0.5));
        };
        const long long col0 = std::clamp<long long>(first_inside(x, scale_x), 0, out_width);
        const long long col1 = std::clamp<long long>(end_inside(x, scale_x, full.cols), col0, out_width);
        const long long row0 = std::clamp<long long>(first_inside(y, scale_y), 0, out_height);
        const long long row1 = std::clamp<long long>(end_inside(y, scale_y, full.rows), row0, out_height);
        for (int row = 0; row < rgba.rows; ++row) {
            uint8_t* p = rgba.ptr<uint8_t>(row);
            const bool row_inside = row >= row0 && row < row1;
            for (int col = 0; col < rgba.cols; ++col) {
                const bool inside = row_inside && col >= col0 && col < col1;
                p[col * 4 + 3] = !inside ? 0 : matte_alpha.empty() ? 255 : matte_alpha.at<uint8_t>(row, col);
            }
        }

        const size_t total = static_cast<size_t>(out_width) * out_height * 4;
        uint8_t* buffer = static_cast<uint8_t*>(std::malloc(total));
        if (buffer == nullptr) {
            return AIBS_ERR_OUT_OF_MEMORY;
        }
        const size_t row_bytes = static_cast<size_t>(out_width) * 4;
        for (int row = 0; row < rgba.rows; ++row) {
            std::memcpy(buffer + row_bytes * static_cast<size_t>(row), rgba.ptr<uint8_t>(row), row_bytes);
        }
        out->data = buffer;
        out->width = out_width;
        out->height = out_height;
        out->source_width = static_cast<uint32_t>(full.cols);
        out->source_height = static_cast<uint32_t>(full.rows);
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (const cv::Exception& e) {
        return e.code == cv::Error::StsNoMem ? AIBS_ERR_OUT_OF_MEMORY : AIBS_ERR_INTERNAL;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

void aibs_source_free(aibs_source* source) {
    delete source;
}

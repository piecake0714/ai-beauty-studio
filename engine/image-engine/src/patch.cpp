// Patches: replacement pixels for parts of the photo (object removal),
// drawn over the render before any other edit.
#include "patch_internal.h"

#include <algorithm>
#include <cmath>
#include <new>

#include <opencv2/imgproc.hpp>

int32_t aibs_patch_create(const uint8_t* rgba, uint32_t width, uint32_t height, float x, float y, float w, float h,
                          aibs_patch** out) {
    if (out == nullptr) return AIBS_ERR_INVALID_ARGUMENT;
    *out = nullptr;
    if (rgba == nullptr || width == 0 || height == 0 || width > 16384 || height > 16384 || !std::isfinite(x) ||
        !std::isfinite(y) || !std::isfinite(w) || !std::isfinite(h) || !(w > 0) || !(h > 0)) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    try {
        auto* patch = new aibs_patch();
        const cv::Mat wrapped(static_cast<int>(height), static_cast<int>(width), CV_8UC4, const_cast<uint8_t*>(rgba));
        cv::cvtColor(wrapped, patch->bgra, cv::COLOR_RGBA2BGRA);
        // Premultiplied, so smooth sampling at its edges never darkens.
        for (int row = 0; row < patch->bgra.rows; ++row) {
            cv::Vec4b* p = patch->bgra.ptr<cv::Vec4b>(row);
            for (int col = 0; col < patch->bgra.cols; ++col) {
                for (int c = 0; c < 3; ++c) p[col][c] = static_cast<uint8_t>((p[col][c] * p[col][3] + 127) / 255);
            }
        }
        patch->x = x;
        patch->y = y;
        patch->w = w;
        patch->h = h;
        *out = patch;
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

void aibs_patch_free(aibs_patch* patch) {
    delete patch;
}

namespace aibs {

void apply_patches(cv::Mat& bgr, int first_col, double x, double y, double scale_x, double scale_y,
                   const aibs_patch* const* patches, uint32_t count) {
    if (patches == nullptr || first_col >= bgr.cols) return;
    first_col = std::max(first_col, 0);
    count = std::min<uint32_t>(count, AIBS_MAX_PATCHES);
    for (uint32_t i = 0; i < count; ++i) {
        const aibs_patch* patch = patches[i];
        if (patch == nullptr || patch->bgra.empty()) continue;
        // The patch's rectangle in output pixels, limited to the buffer.
        const double left = (patch->x - x) * scale_x, top = (patch->y - y) * scale_y;
        const double right = left + patch->w * scale_x, bottom = top + patch->h * scale_y;
        cv::Rect roi(static_cast<int>(std::floor(left)), static_cast<int>(std::floor(top)), 0, 0);
        roi.width = static_cast<int>(std::ceil(right)) - roi.x;
        roi.height = static_cast<int>(std::ceil(bottom)) - roi.y;
        roi &= cv::Rect(first_col, 0, bgr.cols - first_col, bgr.rows);
        if (roi.width <= 0 || roi.height <= 0) continue;
        // Output pixel (i, j) of the ROI shows source point
        // x + (roi.x + i + 0.5) / scale_x, i.e. patch pixel u with
        // u + 0.5 = (that - patch.x) * cols / w.
        const double fx = patch->bgra.cols / patch->w, fy = patch->bgra.rows / patch->h;
        const double ax = fx / scale_x, ay = fy / scale_y;
        const double bx = (x + (roi.x + 0.5) / scale_x - patch->x) * fx - 0.5;
        const double by = (y + (roi.y + 0.5) / scale_y - patch->y) * fy - 0.5;
        const cv::Matx23d inverse(ax, 0.0, bx, 0.0, ay, by);
        cv::Mat sampled;
        cv::warpAffine(patch->bgra, sampled, inverse, roi.size(), cv::INTER_LINEAR | cv::WARP_INVERSE_MAP,
                       cv::BORDER_CONSTANT, cv::Scalar(0, 0, 0, 0));
        for (int row = 0; row < roi.height; ++row) {
            cv::Vec3b* o = bgr.ptr<cv::Vec3b>(roi.y + row) + roi.x;
            const cv::Vec4b* p = sampled.ptr<cv::Vec4b>(row);
            for (int col = 0; col < roi.width; ++col) {
                const int a = p[col][3];
                if (a == 0) continue;
                for (int c = 0; c < 3; ++c) {
                    o[col][c] = cv::saturate_cast<uint8_t>((o[col][c] * (255 - a) + 127) / 255 + p[col][c]);
                }
            }
        }
    }
}

}  // namespace aibs

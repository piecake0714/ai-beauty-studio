// Encoding for export and project files, and access to analysis results
// so they can be saved and restored.
#include "../../background-engine/src/background.h"
#include "../../light-engine/src/light.h"

#include "aibs/engine.h"

#include <cmath>
#include <cstdlib>
#include <cstring>
#include <new>
#include <vector>

#include <opencv2/imgcodecs.hpp>
#include <opencv2/imgproc.hpp>

int32_t aibs_encode(const uint8_t* pixels, uint32_t width, uint32_t height, uint32_t channels, uint32_t format,
                    uint32_t quality, aibs_buffer* out) {
    if (out == nullptr) return AIBS_ERR_INVALID_ARGUMENT;
    out->data = nullptr;
    out->size = 0;
    const bool jpeg = format == AIBS_FORMAT_JPEG;
    if (pixels == nullptr || width == 0 || height == 0 || width > 65535 || height > 65535 ||
        (channels != 1 && channels != 3 && channels != 4) || (format != AIBS_FORMAT_JPEG && format != AIBS_FORMAT_PNG) ||
        (jpeg && (channels == 4 || quality < 1 || quality > 100))) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    try {
        const int type = channels == 1 ? CV_8UC1 : channels == 3 ? CV_8UC3 : CV_8UC4;
        const cv::Mat in(static_cast<int>(height), static_cast<int>(width), type, const_cast<uint8_t*>(pixels));
        cv::Mat bgr;
        if (channels == 3) {
            cv::cvtColor(in, bgr, cv::COLOR_RGB2BGR);
        } else if (channels == 4) {
            cv::cvtColor(in, bgr, cv::COLOR_RGBA2BGRA);
        } else {
            bgr = in;
        }
        std::vector<int> params;
        if (jpeg) {
            params = {cv::IMWRITE_JPEG_QUALITY, static_cast<int>(quality), cv::IMWRITE_JPEG_OPTIMIZE, 1};
            if (quality >= 90) {
                // Full colour resolution at high quality, so colour edges
                // (a subject against a new background) stay sharp.
                params.push_back(cv::IMWRITE_JPEG_SAMPLING_FACTOR);
                params.push_back(cv::IMWRITE_JPEG_SAMPLING_FACTOR_444);
            }
        } else {
            params = {cv::IMWRITE_PNG_COMPRESSION, 6};
        }
        std::vector<uint8_t> bytes;
        if (!cv::imencode(jpeg ? ".jpg" : ".png", bgr, bytes, params)) return AIBS_ERR_INTERNAL;
        out->data = static_cast<uint8_t*>(std::malloc(bytes.size()));
        if (out->data == nullptr) return AIBS_ERR_OUT_OF_MEMORY;
        std::memcpy(out->data, bytes.data(), bytes.size());
        out->size = bytes.size();
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

void aibs_buffer_free(aibs_buffer* buffer) {
    if (buffer == nullptr) return;
    std::free(buffer->data);
    buffer->data = nullptr;
    buffer->size = 0;
}

int32_t aibs_matte_values(const aibs_matte* matte, uint32_t* width, uint32_t* height, uint32_t* source_width,
                          uint32_t* source_height, const uint8_t** values) {
    if (matte == nullptr || width == nullptr || height == nullptr || source_width == nullptr ||
        source_height == nullptr || values == nullptr || matte->coverage.empty() || !matte->coverage.isContinuous()) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    *width = static_cast<uint32_t>(matte->coverage.cols);
    *height = static_cast<uint32_t>(matte->coverage.rows);
    *source_width = static_cast<uint32_t>(matte->source_width);
    *source_height = static_cast<uint32_t>(matte->source_height);
    *values = matte->coverage.ptr<uint8_t>(0);
    return AIBS_OK;
}

int32_t aibs_shape_normals(const aibs_shape* shape, uint32_t* width, uint32_t* height, uint32_t* source_width,
                           uint32_t* source_height, const float** normals) {
    if (shape == nullptr || width == nullptr || height == nullptr || source_width == nullptr ||
        source_height == nullptr || normals == nullptr || shape->normals.empty() || !shape->normals.isContinuous()) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    *width = static_cast<uint32_t>(shape->normals.cols);
    *height = static_cast<uint32_t>(shape->normals.rows);
    *source_width = static_cast<uint32_t>(shape->source_width);
    *source_height = static_cast<uint32_t>(shape->source_height);
    *normals = shape->normals.ptr<float>(0);
    return AIBS_OK;
}

int32_t aibs_shape_from_normals(const float* normals, uint32_t width, uint32_t height, uint32_t source_width,
                                uint32_t source_height, aibs_shape** out) {
    if (out == nullptr) return AIBS_ERR_INVALID_ARGUMENT;
    *out = nullptr;
    if (normals == nullptr || width == 0 || height == 0 || width > 16384 || height > 16384 || source_width == 0 ||
        source_height == 0 || source_width > 1u << 20 || source_height > 1u << 20) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    try {
        auto* shape = new aibs_shape();
        shape->normals.create(static_cast<int>(height), static_cast<int>(width), CV_32FC3);
        const size_t count = static_cast<size_t>(width) * height;
        auto* n = shape->normals.ptr<float>(0);
        for (size_t i = 0; i < count; ++i) {
            double x = normals[i * 3], y = normals[i * 3 + 1], z = normals[i * 3 + 2];
            if (!std::isfinite(x) || !std::isfinite(y) || !std::isfinite(z) || z <= 0.0) {
                x = 0.0;
                y = 0.0;
                z = 1.0;
            }
            const double len = std::sqrt(x * x + y * y + z * z);
            n[i * 3] = static_cast<float>(x / len);
            n[i * 3 + 1] = static_cast<float>(y / len);
            n[i * 3 + 2] = static_cast<float>(z / len);
        }
        shape->source_width = static_cast<int>(source_width);
        shape->source_height = static_cast<int>(source_height);
        *out = shape;
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

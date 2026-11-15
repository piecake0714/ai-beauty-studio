#include "aibs/engine.h"
#include "decode_internal.h"

#include <algorithm>
#include <climits>
#include <cstdlib>
#include <cstring>
#include <new>

#include <opencv2/core.hpp>
#include <opencv2/imgcodecs.hpp>
#include <opencv2/imgproc.hpp>

const char* aibs_engine_version(void) {
    return "0.1.0";
}

int32_t aibs_decode_rgba8(const uint8_t* bytes, size_t len,
                          uint32_t max_dimension, aibs_image* out) {
    if (out == nullptr) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    std::memset(out, 0, sizeof(*out));
    if (bytes == nullptr || len == 0 || len > static_cast<size_t>(INT_MAX)) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }

    try {
        // 8-bit BGR, upright (EXIF orientation applied; RAW developed).
        cv::Mat bgr = aibs::decode_bgr(bytes, len, false);
        if (bgr.empty()) {
            return AIBS_ERR_DECODE_FAILED;
        }

        const uint32_t source_width = static_cast<uint32_t>(bgr.cols);
        const uint32_t source_height = static_cast<uint32_t>(bgr.rows);

        const uint32_t longer = std::max(source_width, source_height);
        if (max_dimension != 0 && longer > max_dimension) {
            const double scale = static_cast<double>(max_dimension) / longer;
            const int w = std::max(1, static_cast<int>(source_width * scale + 0.5));
            const int h = std::max(1, static_cast<int>(source_height * scale + 0.5));
            cv::Mat resized;
            cv::resize(bgr, resized, cv::Size(w, h), 0, 0, cv::INTER_AREA);
            bgr = resized;
        }

        cv::Mat rgba;
        cv::cvtColor(bgr, rgba, cv::COLOR_BGR2RGBA);

        const size_t row_bytes = static_cast<size_t>(rgba.cols) * 4;
        const size_t total = row_bytes * static_cast<size_t>(rgba.rows);
        uint8_t* buffer = static_cast<uint8_t*>(std::malloc(total));
        if (buffer == nullptr) {
            return AIBS_ERR_OUT_OF_MEMORY;
        }
        for (int y = 0; y < rgba.rows; ++y) {
            std::memcpy(buffer + row_bytes * static_cast<size_t>(y),
                        rgba.ptr<uint8_t>(y), row_bytes);
        }

        out->data = buffer;
        out->width = static_cast<uint32_t>(rgba.cols);
        out->height = static_cast<uint32_t>(rgba.rows);
        out->source_width = source_width;
        out->source_height = source_height;
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (const cv::Exception& e) {
        // OpenCV reports allocation failures as cv::Exception too.
        return e.code == cv::Error::StsNoMem ? AIBS_ERR_OUT_OF_MEMORY
                                             : AIBS_ERR_DECODE_FAILED;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

void aibs_image_free(aibs_image* image) {
    if (image == nullptr) {
        return;
    }
    std::free(image->data);
    std::memset(image, 0, sizeof(*image));
}

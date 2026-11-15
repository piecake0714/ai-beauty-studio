// Decoding, including camera RAW files.
//
// JPEG, PNG and WebP go straight to OpenCV. Anything else is first offered
// to LibRaw (camera RAW formats, many of which are TIFF containers that
// OpenCV would misread or only show the reduced preview of); when LibRaw does
// not recognise it, OpenCV tries (plain TIFF). RAW files are developed with
// the camera's white balance into 8-bit sRGB, turned upright.
#include "decode_internal.h"

#include <algorithm>
#include <climits>
#include <cstring>
#include <memory>
#include <new>

#include <opencv2/imgcodecs.hpp>
#include <opencv2/imgproc.hpp>

#ifdef _MSC_VER
#pragma warning(push, 0)
#endif
#include <libraw/libraw.h>
#ifdef _MSC_VER
#pragma warning(pop)
#endif

namespace aibs {

namespace {

cv::Mat decode_raw(const uint8_t* bytes, size_t len, bool reduced) {
    // LibRaw is large; keep it off the stack.
    auto raw = std::make_unique<LibRaw>();
    if (raw->open_buffer(bytes, len) != LIBRAW_SUCCESS) return {};
    // Only camera files: a plain TIFF (no camera maker, not DNG) is left
    // to OpenCV, which keeps its colours exactly.
    if (raw->imgdata.idata.make[0] == '\0' && raw->imgdata.idata.dng_version == 0) return {};
    if (raw->unpack() != LIBRAW_SUCCESS) return {};
    libraw_output_params_t& params = raw->imgdata.params;
    params.use_camera_wb = 1;
    params.output_color = 1;  // sRGB
    params.output_bps = 8;
    params.half_size = reduced ? 1 : 0;
    params.user_qual = 3;  // AHD demosaicing
    if (raw->dcraw_process() != LIBRAW_SUCCESS) return {};
    int error = LIBRAW_SUCCESS;
    libraw_processed_image_t* image = raw->dcraw_make_mem_image(&error);
    if (image == nullptr || error != LIBRAW_SUCCESS) {
        LibRaw::dcraw_clear_mem(image);
        return {};
    }
    cv::Mat bgr;
    if (image->type == LIBRAW_IMAGE_BITMAP && image->bits == 8 && (image->colors == 3 || image->colors == 1)) {
        const cv::Mat wrapped(image->height, image->width, image->colors == 3 ? CV_8UC3 : CV_8UC1, image->data,
                              static_cast<size_t>(image->width) * image->colors);
        cv::cvtColor(wrapped, bgr, image->colors == 3 ? cv::COLOR_RGB2BGR : cv::COLOR_GRAY2BGR);
    }
    LibRaw::dcraw_clear_mem(image);
    return bgr;
}

}  // namespace

bool plain_image(const uint8_t* bytes, size_t len) {
    if (bytes == nullptr) return false;
    const auto starts = [&](const char* magic, size_t n, size_t at = 0) {
        return len >= at + n && std::memcmp(bytes + at, magic, n) == 0;
    };
    return starts("\xFF\xD8\xFF", 3) || starts("\x89PNG\r\n\x1a\n", 8) || (starts("RIFF", 4) && starts("WEBP", 4, 8));
}

cv::Mat decode_bgr(const uint8_t* bytes, size_t len, bool reduced) {
    if (bytes == nullptr || len == 0 || len > static_cast<size_t>(INT_MAX)) return {};
    if (!plain_image(bytes, len)) {
        cv::Mat raw = decode_raw(bytes, len, reduced);
        if (!raw.empty()) return raw;
    }
    // Wraps the caller's buffer without copying; imdecode only reads it.
    const cv::Mat encoded(1, static_cast<int>(len), CV_8UC1, const_cast<uint8_t*>(bytes));
    return cv::imdecode(encoded, cv::IMREAD_COLOR);
}

}  // namespace aibs

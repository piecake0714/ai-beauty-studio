// Internal decoding shared by aibs_decode_rgba8 and aibs_source_open.
#pragma once

#include <cstddef>
#include <cstdint>

#include <opencv2/core.hpp>

namespace aibs {

/// Decode an encoded image to 8-bit BGR: JPEG, PNG, TIFF and WebP through
/// OpenCV, camera RAW files through LibRaw. `small` asks RAW decoding for
/// half size (faster, for thumbnails). Empty when the data is not an image
/// this can read. Throws std::bad_alloc when memory runs out.
cv::Mat decode_bgr(const uint8_t* bytes, size_t len, bool small);

/// True when the bytes start like a JPEG, PNG or WebP file (never RAW).
bool plain_image(const uint8_t* bytes, size_t len);

}  // namespace aibs

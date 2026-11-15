// Tests for encoding (JPEG/PNG round trips through the decoder) and for
// reading analysis results back so they can be saved and restored.
#include "aibs/engine.h"
#include "check.h"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>

#include <opencv2/core.hpp>
#include <opencv2/imgcodecs.hpp>

namespace {

struct Buffer {
    aibs_buffer b{};
    ~Buffer() { aibs_buffer_free(&b); }
};

struct Image {
    aibs_image img{};
    ~Image() { aibs_image_free(&img); }
};

std::vector<uint8_t> pattern(uint32_t w, uint32_t h, uint32_t channels) {
    std::vector<uint8_t> p(static_cast<size_t>(w) * h * channels);
    for (uint32_t y = 0; y < h; ++y)
        for (uint32_t x = 0; x < w; ++x)
            for (uint32_t c = 0; c < channels; ++c)
                p[(static_cast<size_t>(y) * w + x) * channels + c] =
                    static_cast<uint8_t>(c == 3 ? (x < w / 2 ? 255 : 0) : (x * 4 + y * 2 + c * 60) % 256);
    return p;
}

void test_png_round_trip_keeps_pixels_and_alpha() {
    const uint32_t w = 40, h = 30;
    const std::vector<uint8_t> rgba = pattern(w, h, 4);
    Buffer png;
    CHECK(aibs_encode(rgba.data(), w, h, 4, AIBS_FORMAT_PNG, 0, &png.b) == AIBS_OK);
    CHECK(png.b.size > 8 && png.b.data[1] == 'P' && png.b.data[2] == 'N' && png.b.data[3] == 'G');
    const cv::Mat bgra = cv::imdecode(cv::Mat(1, static_cast<int>(png.b.size), CV_8U, png.b.data), cv::IMREAD_UNCHANGED);
    CHECK(bgra.cols == static_cast<int>(w) && bgra.rows == static_cast<int>(h) && bgra.channels() == 4);
    if (bgra.channels() == 4) {
        bool same = true;  // lossless, alpha kept
        for (uint32_t i = 0; i < w * h && same; ++i) {
            const cv::Vec4b p = bgra.at<cv::Vec4b>(static_cast<int>(i / w), static_cast<int>(i % w));
            same = p[2] == rgba[i * 4] && p[1] == rgba[i * 4 + 1] && p[0] == rgba[i * 4 + 2] && p[3] == rgba[i * 4 + 3];
        }
        CHECK(same);
    }
    // Grey (masks) and RGB as well.
    const std::vector<uint8_t> grey = pattern(w, h, 1);
    Buffer grey_png;
    CHECK(aibs_encode(grey.data(), w, h, 1, AIBS_FORMAT_PNG, 0, &grey_png.b) == AIBS_OK);
    // The app's decoder reads it back as grey RGB.
    Image grey_back;
    CHECK(aibs_decode_rgba8(grey_png.b.data, grey_png.b.size, 0, &grey_back.img) == AIBS_OK);
    if (grey_back.img.data) CHECK(grey_back.img.data[4 * 7] == grey[7] && grey_back.img.data[4 * 7 + 2] == grey[7]);
}

void test_jpeg_quality_and_limits() {
    // A smooth photo-like gradient with a sharp colour edge in the middle.
    const uint32_t w = 64, h = 48;
    std::vector<uint8_t> rgb(static_cast<size_t>(w) * h * 3);
    for (uint32_t y = 0; y < h; ++y)
        for (uint32_t x = 0; x < w; ++x) {
            uint8_t* p = &rgb[(static_cast<size_t>(y) * w + x) * 3];
            p[0] = static_cast<uint8_t>(x < w / 2 ? 200 : 40 + x);
            p[1] = static_cast<uint8_t>(60 + y * 2);
            p[2] = static_cast<uint8_t>(x < w / 2 ? 50 : 180);
        }
    Buffer high, low;
    CHECK(aibs_encode(rgb.data(), w, h, 3, AIBS_FORMAT_JPEG, 95, &high.b) == AIBS_OK);
    CHECK(aibs_encode(rgb.data(), w, h, 3, AIBS_FORMAT_JPEG, 20, &low.b) == AIBS_OK);
    CHECK(high.b.size > low.b.size);
    CHECK(high.b.data && high.b.data[0] == 0xFF && high.b.data[1] == 0xD8);
    Image back;
    CHECK(aibs_decode_rgba8(high.b.data, high.b.size, 0, &back.img) == AIBS_OK);
    if (back.img.data) {
        double error = 0;
        for (size_t i = 0; i < static_cast<size_t>(w) * h; ++i)
            for (int c = 0; c < 3; ++c) error += std::abs(back.img.data[i * 4 + c] - rgb[i * 3 + c]);
        error /= static_cast<double>(w) * h * 3;
        std::printf("info: JPEG quality 95 mean error %.2f, %zu bytes (quality 20: %zu bytes)\n", error, high.b.size,
                    low.b.size);
        CHECK(error < 4.0);
    }
    Buffer refused;
    const std::vector<uint8_t> rgba(static_cast<size_t>(w) * h * 4, 128);
    CHECK(aibs_encode(rgba.data(), w, h, 4, AIBS_FORMAT_JPEG, 90, &refused.b) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(aibs_encode(rgb.data(), w, h, 3, AIBS_FORMAT_JPEG, 0, &refused.b) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(aibs_encode(rgb.data(), w, h, 3, 9, 90, &refused.b) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(aibs_encode(rgb.data(), 0, h, 3, AIBS_FORMAT_PNG, 0, &refused.b) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(refused.b.data == nullptr && refused.b.size == 0);
    aibs_buffer_free(nullptr);
}

void test_analysis_results_round_trip() {
    // Matte.
    std::vector<uint8_t> values(12 * 8);
    for (size_t i = 0; i < values.size(); ++i) values[i] = static_cast<uint8_t>(i * 2);
    aibs_matte* matte = nullptr;
    CHECK(aibs_matte_create(values.data(), 12, 8, 120, 80, &matte) == AIBS_OK);
    uint32_t w = 0, h = 0, sw = 0, sh = 0;
    const uint8_t* read = nullptr;
    CHECK(aibs_matte_values(matte, &w, &h, &sw, &sh, &read) == AIBS_OK);
    CHECK(w == 12 && h == 8 && sw == 120 && sh == 80);
    CHECK(read && std::memcmp(read, values.data(), values.size()) == 0);
    CHECK(aibs_matte_values(nullptr, &w, &h, &sw, &sh, &read) == AIBS_ERR_INVALID_ARGUMENT);
    aibs_matte_free(matte);

    // Shape: depth -> normals -> saved -> restored gives the same normals.
    std::vector<float> depth(32 * 24);
    for (int y = 0; y < 24; ++y)
        for (int x = 0; x < 32; ++x) depth[static_cast<size_t>(y) * 32 + x] = static_cast<float>(x) / 32.0f;
    aibs_shape* shape = nullptr;
    CHECK(aibs_shape_create(depth.data(), 32, 24, 64, 48, &shape) == AIBS_OK);
    const float* normals = nullptr;
    CHECK(aibs_shape_normals(shape, &w, &h, &sw, &sh, &normals) == AIBS_OK);
    CHECK(sw == 64 && sh == 48 && w > 0 && h > 0);
    if (normals) {
        const std::vector<float> saved(normals, normals + static_cast<size_t>(w) * h * 3);
        CHECK(saved[0] < 0.0f && saved[2] > 0.0f);  // depth rises to the right: faces left
        aibs_shape* restored = nullptr;
        CHECK(aibs_shape_from_normals(saved.data(), w, h, sw, sh, &restored) == AIBS_OK);
        const float* again = nullptr;
        uint32_t w2 = 0, h2 = 0, sw2 = 0, sh2 = 0;
        CHECK(aibs_shape_normals(restored, &w2, &h2, &sw2, &sh2, &again) == AIBS_OK);
        if (again) {
            double worst = 0;
            for (size_t i = 0; i < saved.size(); ++i) worst = std::max(worst, std::abs(double(saved[i]) - again[i]));
            CHECK(worst < 1e-5);
        }
        aibs_shape_free(restored);
        // Damaged values fall back to facing the camera instead of failing.
        const float bad[3] = {NAN, 0.0f, -1.0f};
        aibs_shape* fallback = nullptr;
        CHECK(aibs_shape_from_normals(bad, 1, 1, 1, 1, &fallback) == AIBS_OK);
        CHECK(aibs_shape_normals(fallback, &w2, &h2, &sw2, &sh2, &again) == AIBS_OK);
        if (again) CHECK(again[0] == 0.0f && again[2] == 1.0f);
        aibs_shape_free(fallback);
    }
    aibs_shape_free(shape);
}

}  // namespace

void run_export_tests() {
    test_png_round_trip_keeps_pixels_and_alpha();
    test_jpeg_quality_and_limits();
    test_analysis_results_round_trip();
}

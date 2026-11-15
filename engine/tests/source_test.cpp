// Tests for full-resolution sources and region rendering.
#include "aibs/engine.h"
#include "check.h"

#include <chrono>
#include <cstdlib>

#include <opencv2/core.hpp>
#include <opencv2/imgcodecs.hpp>

namespace {

struct Source {
    aibs_source* ptr = nullptr;
    ~Source() { aibs_source_free(ptr); }
};

struct Image {
    aibs_image img{};
    ~Image() { aibs_image_free(&img); }
    const uint8_t* px(uint32_t col, uint32_t row) const {
        return img.data + (static_cast<size_t>(row) * img.width + col) * 4;
    }
};

bool open_fixture(Source& source) {
    const std::vector<uint8_t> bytes = read_fixture("gradient_64x48.png");
    return aibs_source_open(bytes.data(), bytes.size(), &source.ptr) == AIBS_OK;
}

// Fixture pixel (x, y): r = x * 4, g = y * 5, b = 128.
bool is_fixture_pixel(const uint8_t* p, int x, int y) {
    return p[0] == x * 4 && p[1] == y * 5 && p[2] == 128 && p[3] == 255;
}

void test_open_reports_full_size() {
    Source source;
    CHECK(open_fixture(source));
    uint32_t w = 0, h = 0;
    aibs_source_size(source.ptr, &w, &h);
    CHECK(w == 64 && h == 48);
}

void test_render_at_100_percent_is_exact() {
    Source source;
    CHECK(open_fixture(source));
    Image out;
    CHECK(aibs_source_render(source.ptr, 0, 0, 64, 48, 64, 48, nullptr, 0, &out.img) == AIBS_OK);
    CHECK(out.img.width == 64 && out.img.height == 48);
    CHECK(out.img.source_width == 64 && out.img.source_height == 48);
    bool all_exact = out.img.data != nullptr;
    for (int y = 0; all_exact && y < 48; ++y) {
        for (int x = 0; x < 64; ++x) {
            if (!is_fixture_pixel(out.px(x, y), x, y)) {
                all_exact = false;
                break;
            }
        }
    }
    CHECK(all_exact);
}

void test_render_at_200_percent_repeats_pixels() {
    Source source;
    CHECK(open_fixture(source));
    Image out;
    // 8 x 6 source pixels starting at (10, 5), shown at 2x.
    CHECK(aibs_source_render(source.ptr, 10, 5, 8, 6, 16, 12, nullptr, 0, &out.img) == AIBS_OK);
    bool all_exact = out.img.data != nullptr;
    for (int row = 0; all_exact && row < 12; ++row) {
        for (int col = 0; col < 16; ++col) {
            if (!is_fixture_pixel(out.px(col, row), 10 + col / 2, 5 + row / 2)) {
                all_exact = false;
                break;
            }
        }
    }
    CHECK(all_exact);
}

void test_area_outside_the_image_is_transparent() {
    Source source;
    CHECK(open_fixture(source));
    Image out;
    // Shifted 4 pixels left: the first 4 output columns are outside.
    CHECK(aibs_source_render(source.ptr, -4, 0, 64, 48, 64, 48, nullptr, 0, &out.img) == AIBS_OK);
    if (out.img.data != nullptr) {
        CHECK(out.px(3, 10)[3] == 0);
        CHECK(is_fixture_pixel(out.px(4, 10), 0, 10));
        CHECK(is_fixture_pixel(out.px(63, 47), 59, 47));
    }

    Image away;
    CHECK(aibs_source_render(source.ptr, 1000, 1000, 10, 10, 10, 10, nullptr, 0, &away.img) == AIBS_OK);
    bool all_clear = away.img.data != nullptr;
    for (uint32_t i = 0; all_clear && i < 100; ++i) {
        all_clear = away.img.data[i * 4 + 3] == 0;
    }
    CHECK(all_clear);
}

void test_render_reduced() {
    Source source;
    CHECK(open_fixture(source));
    Image out;
    CHECK(aibs_source_render(source.ptr, 0, 0, 64, 48, 16, 12, nullptr, 0, &out.img) == AIBS_OK);
    CHECK(out.img.width == 16 && out.img.height == 12);
    if (out.img.data != nullptr) {
        // Output pixel 0 samples source x = 1.5 (r = 6), pixel 3 x = 13.5 (r = 54).
        CHECK(std::abs(out.px(0, 0)[0] - 6) <= 1);
        CHECK(std::abs(out.px(3, 0)[0] - 54) <= 1);
        CHECK(out.px(15, 11)[3] == 255);
    }
}

void test_invalid_render_arguments() {
    Source source;
    CHECK(open_fixture(source));
    Image out;
    CHECK(aibs_source_render(source.ptr, 0, 0, 0, 48, 10, 10, nullptr, 0, &out.img) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(aibs_source_render(source.ptr, 0, 0, 64, 48, 0, 10, nullptr, 0, &out.img) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(aibs_source_render(source.ptr, 0, 0, 64, 48, 20000, 10, nullptr, 0, &out.img) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(aibs_source_render(nullptr, 0, 0, 64, 48, 10, 10, nullptr, 0, &out.img) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(out.img.data == nullptr);

    const std::vector<uint8_t> bad = read_fixture("corrupted.jpg");
    aibs_source* none = nullptr;
    CHECK(aibs_source_open(bad.data(), bad.size(), &none) == AIBS_ERR_DECODE_FAILED);
    CHECK(none == nullptr);
    aibs_source_free(nullptr);
}

double ms_since(std::chrono::steady_clock::time_point start) {
    return std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count();
}

// A 48-megapixel image: checks the pyramid gives correct colours at every
// zoom and records timings in the CI log.
void test_large_image() {
    cv::Mat big(6000, 8000, CV_8UC3, cv::Scalar(30, 120, 200));  // BGR
    std::vector<uint8_t> jpeg;
    CHECK(cv::imencode(".jpg", big, jpeg, {cv::IMWRITE_JPEG_QUALITY, 95}));

    auto start = std::chrono::steady_clock::now();
    Source source;
    CHECK(aibs_source_open(jpeg.data(), jpeg.size(), &source.ptr) == AIBS_OK);
    const double open_ms = ms_since(start);
    uint32_t w = 0, h = 0;
    aibs_source_size(source.ptr, &w, &h);
    CHECK(w == 8000 && h == 6000);

    struct Case {
        const char* name;
        double x, y, rw, rh;
        uint32_t ow, oh;
    };
    const Case cases[] = {
        {"fit 1600x1200", 0, 0, 8000, 6000, 1600, 1200},
        {"100% 1920x1080", 3000, 2000, 1920, 1080, 1920, 1080},
        {"800% 1920x1080", 4000, 3000, 240, 135, 1920, 1080},
        {"6% 480x360", 0, 0, 8000, 6000, 480, 360},
    };
    for (const Case& c : cases) {
        start = std::chrono::steady_clock::now();
        Image out;
        CHECK(aibs_source_render(source.ptr, c.x, c.y, c.rw, c.rh, c.ow, c.oh, nullptr, 0, &out.img) == AIBS_OK);
        const double render_ms = ms_since(start);
        if (out.img.data != nullptr) {
            const uint8_t* p = out.px(c.ow / 2, c.oh / 2);
            CHECK(std::abs(p[0] - 200) <= 3 && std::abs(p[1] - 120) <= 3 && std::abs(p[2] - 30) <= 3);
            CHECK(p[3] == 255);
        }
        std::printf("timing: render %s took %.1f ms\n", c.name, render_ms);
    }
    aibs_adjustments every{};
    every.exposure = 0.5f;
    every.brightness = 10;
    every.contrast = 20;
    every.highlights = -40;
    every.shadows = 30;
    every.whites = 10;
    every.blacks = -10;
    every.temperature = 20;
    every.tint = 5;
    every.vibrance = 30;
    every.saturation = 10;
    for (int i = 0; i < AIBS_HSL_BANDS; ++i) {
        every.hsl_hue[i] = 10;
        every.hsl_saturation[i] = -10;
        every.hsl_luminance[i] = 5;
    }
    start = std::chrono::steady_clock::now();
    Image adjusted;
    CHECK(aibs_source_render(source.ptr, 3000, 2000, 1920, 1080, 1920, 1080, &every, 0, &adjusted.img) == AIBS_OK);
    std::printf("timing: render 100%% 1920x1080 with every adjustment took %.1f ms\n", ms_since(start));
    std::printf("timing: open 48 MP JPEG took %.1f ms\n", open_ms);
}

void test_render_adjusts_right_of_the_split() {
    Source source;
    CHECK(open_fixture(source));
    Image original;
    CHECK(aibs_source_render(source.ptr, 0, 0, 64, 48, 64, 48, nullptr, 0, &original.img) == AIBS_OK);

    const aibs_adjustments none{};
    Image unchanged;
    CHECK(aibs_source_render(source.ptr, 0, 0, 64, 48, 64, 48, &none, 0, &unchanged.img) == AIBS_OK);
    bool same = original.img.data != nullptr && unchanged.img.data != nullptr;
    for (size_t i = 0; same && i < 64u * 48u * 4u; ++i) same = original.img.data[i] == unchanged.img.data[i];
    CHECK(same);

    aibs_adjustments look{};
    look.exposure = 0.7f;
    look.contrast = 20;
    look.shadows = 40;
    look.temperature = 30;
    look.saturation = 25;
    look.hsl_hue[5] = 40;
    Image split;
    CHECK(aibs_source_render(source.ptr, 0, 0, 64, 48, 64, 48, &look, 30, &split.img) == AIBS_OK);
    bool correct = original.img.data != nullptr && split.img.data != nullptr;
    for (int row = 0; correct && row < 48; ++row) {
        for (int col = 0; col < 64; ++col) {
            const size_t p = (static_cast<size_t>(row) * 64 + col) * 4;
            uint8_t expected[3] = {original.img.data[p], original.img.data[p + 1], original.img.data[p + 2]};
            if (col >= 30) CHECK(aibs_adjust_rgb8(&look, expected, expected, 1) == AIBS_OK);
            for (int c = 0; c < 3; ++c) correct = correct && split.img.data[p + c] == expected[c];
            correct = correct && split.img.data[p + 3] == 255;
        }
    }
    CHECK(correct);

    Image all_original;
    CHECK(aibs_source_render(source.ptr, 0, 0, 64, 48, 64, 48, &look, 1000, &all_original.img) == AIBS_OK);
    same = original.img.data != nullptr && all_original.img.data != nullptr;
    for (size_t i = 0; same && i < 64u * 48u * 4u; ++i) same = original.img.data[i] == all_original.img.data[i];
    CHECK(same);
}

}  // namespace

void run_source_tests() {
    test_open_reports_full_size();
    test_render_at_100_percent_is_exact();
    test_render_at_200_percent_repeats_pixels();
    test_area_outside_the_image_is_transparent();
    test_render_reduced();
    test_invalid_render_arguments();
    test_render_adjusts_right_of_the_split();
    test_large_image();
}

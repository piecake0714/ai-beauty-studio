// Tests for tone adjustments and their use in rendering.
#include "aibs/engine.h"
#include "check.h"

#include <cmath>
#include <limits>

namespace {

double srgb_to_linear(double v) {
    return v <= 0.04045 ? v / 12.92 : std::pow((v + 0.055) / 1.055, 2.4);
}

bool lut_of(aibs_adjustments a, uint8_t* lut) {
    return aibs_adjustments_lut(&a, lut) == AIBS_OK;
}

void test_zero_adjustments_change_nothing() {
    uint8_t lut[256];
    CHECK(lut_of({0, 0, 0}, lut));
    bool identity = true;
    for (int i = 0; i < 256; ++i) identity = identity && lut[i] == i;
    CHECK(identity);

    // Non-finite input is treated as zero.
    const float nan = std::numeric_limits<float>::quiet_NaN();
    CHECK(lut_of({nan, nan, nan}, lut));
    identity = true;
    for (int i = 0; i < 256; ++i) identity = identity && lut[i] == i;
    CHECK(identity);
}

void test_exposure_plus_one_doubles_linear_light() {
    uint8_t lut[256];
    CHECK(lut_of({1, 0, 0}, lut));
    for (int i = 60; i <= 186; i += 7) {
        const double ratio = srgb_to_linear(lut[i] / 255.0) / srgb_to_linear(i / 255.0);
        CHECK(std::abs(ratio - 2.0) < 0.05);
    }
    CHECK(lut[0] == 0 && lut[255] == 255);
}

void test_curves_are_monotonic_and_limited() {
    const aibs_adjustments settings[] = {
        {2, 0, 0}, {-2, 0, 0}, {0, 60, 0}, {0, -60, 0}, {0, 0, 80}, {0, 0, -50}, {1, -30, 40},
    };
    for (const aibs_adjustments& a : settings) {
        uint8_t lut[256];
        CHECK(lut_of(a, lut));
        bool monotonic = true;
        for (int i = 1; i < 256; ++i) monotonic = monotonic && lut[i] >= lut[i - 1];
        CHECK(monotonic);
    }

    uint8_t lut[256];
    CHECK(lut_of({0, 100, 0}, lut));
    CHECK(lut[0] == 0 && lut[255] == 255 && lut[128] > 128);  // brightness keeps black and white
    CHECK(lut_of({0, 0, -100}, lut));
    CHECK(lut[0] == 128 && lut[255] == 128);  // no contrast: flat middle grey

    uint8_t clamped[256];
    CHECK(lut_of({50, 0, 0}, lut));
    CHECK(lut_of({5, 0, 0}, clamped));
    bool same = true;
    for (int i = 0; i < 256; ++i) same = same && lut[i] == clamped[i];
    CHECK(same);  // out-of-range exposure is clamped to +5

    CHECK(aibs_adjustments_lut(nullptr, lut) == AIBS_ERR_INVALID_ARGUMENT);
}

struct Source {
    aibs_source* ptr = nullptr;
    ~Source() { aibs_source_free(ptr); }
};

struct Image {
    aibs_image img{};
    ~Image() { aibs_image_free(&img); }
};

void test_render_applies_adjustments_right_of_the_split() {
    const std::vector<uint8_t> bytes = read_fixture("gradient_64x48.png");
    Source source;
    CHECK(aibs_source_open(bytes.data(), bytes.size(), &source.ptr) == AIBS_OK);

    Image original;
    CHECK(aibs_source_render(source.ptr, 0, 0, 64, 48, 64, 48, nullptr, 0, &original.img) == AIBS_OK);

    const aibs_adjustments none{0, 0, 0};
    Image unchanged;
    CHECK(aibs_source_render(source.ptr, 0, 0, 64, 48, 64, 48, &none, 0, &unchanged.img) == AIBS_OK);
    bool same = original.img.data != nullptr && unchanged.img.data != nullptr;
    for (size_t i = 0; same && i < 64u * 48u * 4u; ++i) same = original.img.data[i] == unchanged.img.data[i];
    CHECK(same);

    const aibs_adjustments brighter{1, 10, 20};
    uint8_t lut[256];
    CHECK(aibs_adjustments_lut(&brighter, lut) == AIBS_OK);
    Image split;
    CHECK(aibs_source_render(source.ptr, 0, 0, 64, 48, 64, 48, &brighter, 30, &split.img) == AIBS_OK);
    bool correct = original.img.data != nullptr && split.img.data != nullptr;
    for (int row = 0; correct && row < 48; ++row) {
        for (int col = 0; col < 64; ++col) {
            const size_t p = (static_cast<size_t>(row) * 64 + col) * 4;
            for (int c = 0; c < 3; ++c) {
                const uint8_t expected = col < 30 ? original.img.data[p + c] : lut[original.img.data[p + c]];
                if (split.img.data[p + c] != expected) correct = false;
            }
            if (split.img.data[p + 3] != 255) correct = false;
        }
    }
    CHECK(correct);

    Image all_original;
    CHECK(aibs_source_render(source.ptr, 0, 0, 64, 48, 64, 48, &brighter, 1000, &all_original.img) == AIBS_OK);
    same = original.img.data != nullptr && all_original.img.data != nullptr;
    for (size_t i = 0; same && i < 64u * 48u * 4u; ++i) same = original.img.data[i] == all_original.img.data[i];
    CHECK(same);
}

}  // namespace

void run_adjust_tests() {
    test_zero_adjustments_change_nothing();
    test_exposure_plus_one_doubles_linear_light();
    test_curves_are_monotonic_and_limited();
    test_render_applies_adjustments_right_of_the_split();
}

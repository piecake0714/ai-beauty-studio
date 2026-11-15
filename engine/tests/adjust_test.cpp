// Tests for the light and colour adjustments (pixel pipeline only).
#include "aibs/engine.h"
#include "check.h"

#include <algorithm>
#include <cmath>
#include <cstdlib>
#include <initializer_list>
#include <limits>
#include <vector>

namespace {

struct Rgb {
    int r, g, b;
};

Rgb adjust(const aibs_adjustments& a, Rgb in) {
    const uint8_t px[3] = {static_cast<uint8_t>(in.r), static_cast<uint8_t>(in.g), static_cast<uint8_t>(in.b)};
    uint8_t out[3] = {0, 0, 0};
    CHECK(aibs_adjust_rgb8(&a, px, out, 1) == AIBS_OK);
    return {out[0], out[1], out[2]};
}

int grey(const aibs_adjustments& a, int v) {
    return adjust(a, {v, v, v}).g;
}

double srgb_to_linear(double v) {
    return v <= 0.04045 ? v / 12.92 : std::pow((v + 0.055) / 1.055, 2.4);
}

int chroma(Rgb c) {
    return std::max({c.r, c.g, c.b}) - std::min({c.r, c.g, c.b});
}

aibs_adjustments none() {
    return aibs_adjustments{};
}

void test_all_zero_changes_nothing() {
    std::vector<uint8_t> in(256 * 256 * 3);
    for (size_t i = 0; i < in.size(); ++i) in[i] = static_cast<uint8_t>((i * 7919) % 256);
    std::vector<uint8_t> out(in.size());
    const aibs_adjustments a = none();
    CHECK(aibs_adjust_rgb8(&a, in.data(), out.data(), in.size() / 3) == AIBS_OK);
    CHECK(out == in);

    aibs_adjustments nan = none();
    nan.exposure = nan.saturation = nan.hsl_hue[0] = std::numeric_limits<float>::quiet_NaN();
    CHECK(aibs_adjust_rgb8(&nan, in.data(), out.data(), in.size() / 3) == AIBS_OK);
    CHECK(out == in);

    CHECK(aibs_adjust_rgb8(nullptr, in.data(), out.data(), 1) == AIBS_ERR_INVALID_ARGUMENT);
}

void test_exposure_plus_one_doubles_linear_light() {
    aibs_adjustments a = none();
    a.exposure = 1;
    for (int v = 60; v <= 186; v += 7) {
        const double ratio = srgb_to_linear(grey(a, v) / 255.0) / srgb_to_linear(v / 255.0);
        CHECK(std::abs(ratio - 2.0) < 0.05);
    }
    CHECK(grey(a, 0) == 0 && grey(a, 255) == 255);
}

void test_grey_ramps_stay_in_order() {
    const auto make = [](float aibs_adjustments::*field, float value) {
        aibs_adjustments a = none();
        a.*field = value;
        return a;
    };
    const aibs_adjustments cases[] = {
        make(&aibs_adjustments::exposure, 2),       make(&aibs_adjustments::exposure, -2),
        make(&aibs_adjustments::brightness, 60),    make(&aibs_adjustments::contrast, 80),
        make(&aibs_adjustments::contrast, -50),     make(&aibs_adjustments::highlights, -100),
        make(&aibs_adjustments::highlights, 100),   make(&aibs_adjustments::shadows, 100),
        make(&aibs_adjustments::shadows, -100),     make(&aibs_adjustments::whites, 100),
        make(&aibs_adjustments::whites, -100),      make(&aibs_adjustments::blacks, 100),
        make(&aibs_adjustments::blacks, -100),      make(&aibs_adjustments::temperature, 80),
        make(&aibs_adjustments::tint, -80),         make(&aibs_adjustments::saturation, 100),
    };
    for (const aibs_adjustments& a : cases) {
        int previous[3] = {0, 0, 0};
        bool ordered = true;
        for (int v = 0; v < 256; ++v) {
            const Rgb out = adjust(a, {v, v, v});
            ordered = ordered && out.r >= previous[0] && out.g >= previous[1] && out.b >= previous[2];
            previous[0] = out.r;
            previous[1] = out.g;
            previous[2] = out.b;
        }
        CHECK(ordered);
    }
}

void test_brightness_and_contrast() {
    aibs_adjustments a = none();
    a.brightness = 100;
    CHECK(grey(a, 0) == 0 && grey(a, 255) == 255 && grey(a, 128) > 150);
    a = none();
    a.contrast = -100;
    CHECK(grey(a, 0) == 128 && grey(a, 255) == 128);
    a.contrast = 50;
    CHECK(grey(a, 64) < 64 && grey(a, 192) > 192);
}

void test_tone_controls_act_on_their_range() {
    aibs_adjustments a = none();
    a.highlights = -100;
    CHECK(std::abs(grey(a, 220) - 220) > 2 * std::abs(grey(a, 40) - 40));
    CHECK(grey(a, 220) < 220);

    a = none();
    a.shadows = 100;
    CHECK(grey(a, 40) - 40 > 2 * std::abs(grey(a, 220) - 220));

    a = none();
    a.whites = 100;
    CHECK(grey(a, 230) - 230 > 2 * std::abs(grey(a, 30) - 30));

    a = none();
    a.blacks = -100;
    CHECK(20 - grey(a, 20) > 2 * std::abs(grey(a, 200) - 200));

    // Hue is kept: a coloured pixel changes brightness, not colour order.
    a = none();
    a.shadows = 100;
    const Rgb out = adjust(a, {80, 40, 20});
    CHECK(out.r > out.g && out.g > out.b && out.r > 80);
}

void test_white_balance() {
    aibs_adjustments a = none();
    a.temperature = 50;
    Rgb out = adjust(a, {128, 128, 128});
    CHECK(out.r > out.b + 10);
    a.temperature = -50;
    out = adjust(a, {128, 128, 128});
    CHECK(out.b > out.r + 10);
    a = none();
    a.tint = 50;
    out = adjust(a, {128, 128, 128});
    CHECK(out.g + 5 < out.r && out.g + 5 < out.b);
}

void test_saturation_and_vibrance() {
    aibs_adjustments a = none();
    a.saturation = -100;
    const Rgb grey_out = adjust(a, {200, 80, 30});
    CHECK(grey_out.r == grey_out.g && grey_out.g == grey_out.b);

    a.saturation = 50;
    CHECK(chroma(adjust(a, {150, 110, 90})) > chroma({150, 110, 90}));

    // Vibrance raises weak colours relatively more than strong ones.
    a = none();
    a.vibrance = 100;
    const Rgb weak{140, 120, 110}, strong{220, 40, 30};
    const double weak_gain = static_cast<double>(chroma(adjust(a, weak))) / chroma(weak);
    const double strong_gain = static_cast<double>(chroma(adjust(a, strong))) / chroma(strong);
    CHECK(weak_gain > strong_gain + 0.2);
}

void test_hsl_bands_are_selective() {
    aibs_adjustments a = none();
    a.hsl_saturation[0] = -100;  // red
    const Rgb red = adjust(a, {220, 30, 30});
    CHECK(chroma(red) < 60);
    const Rgb blue = adjust(a, {30, 30, 220});
    CHECK(std::abs(blue.r - 30) <= 1 && std::abs(blue.g - 30) <= 1 && std::abs(blue.b - 220) <= 1);
    const Rgb neutral = adjust(a, {128, 128, 128});
    CHECK(neutral.r == neutral.g && neutral.g == neutral.b);

    a = none();
    a.hsl_hue[5] = 100;  // blue shifts 30 degrees towards purple
    const Rgb shifted = adjust(a, {30, 30, 220});
    CHECK(shifted.r > 60 && shifted.b > 180);
    const Rgb green = adjust(a, {30, 200, 30});
    CHECK(std::abs(green.r - 30) <= 1 && std::abs(green.g - 200) <= 1 && std::abs(green.b - 30) <= 1);

    a = none();
    a.hsl_luminance[3] = -100;  // green darker
    const Rgb darker = adjust(a, {40, 180, 40});
    CHECK(darker.g < 150);
}

}  // namespace

// ---- look: curves, grading, LUT -----------------------------------------

namespace look_tests {

std::vector<uint8_t> grey_ramp() {
    std::vector<uint8_t> px;
    for (int v = 0; v < 256; ++v) {
        px.push_back(static_cast<uint8_t>(v));
        px.push_back(static_cast<uint8_t>(v));
        px.push_back(static_cast<uint8_t>(v));
    }
    return px;
}

std::vector<uint8_t> run_look(const aibs_look& look, const std::vector<uint8_t>& in) {
    std::vector<uint8_t> out(in.size());
    CHECK(aibs_look_rgb8(&look, in.data(), out.data(), in.size() / 3) == AIBS_OK);
    return out;
}

void set_curve(aibs_look& look, int which, std::initializer_list<float> xy) {
    int i = 0;
    for (const float v : xy) look.curves[which][i++] = v;
    look.curve_points[which] = static_cast<uint32_t>(xy.size() / 2);
}

void test_empty_look_changes_nothing() {
    const auto ramp = grey_ramp();
    aibs_look look{};
    CHECK(run_look(look, ramp) == ramp);
    // A straight curve and a LUT at amount 0 change nothing either.
    set_curve(look, AIBS_CURVE_MASTER, {0, 0, 1, 1});
    CHECK(run_look(look, ramp) == ramp);
    CHECK(aibs_look_rgb8(nullptr, ramp.data(), nullptr, 1) == AIBS_ERR_INVALID_ARGUMENT);
}

void test_curves() {
    const auto ramp = grey_ramp();
    // An S curve: darker shadows, brighter highlights, ends and middle kept,
    // and never decreasing.
    aibs_look look{};
    set_curve(look, AIBS_CURVE_MASTER, {0, 0, 0.25f, 0.15f, 0.5f, 0.5f, 0.75f, 0.85f, 1, 1});
    const auto out = run_look(look, ramp);
    CHECK(out[3 * 64] < 64 - 15 && out[3 * 191] > 191 + 15);
    CHECK(std::abs(out[3 * 128] - 128) <= 2 && out[0] == 0 && out[3 * 255] == 255);
    bool rising = true;
    for (int v = 1; v < 256; ++v) rising = rising && out[3 * v] >= out[3 * (v - 1)];
    CHECK(rising);
    // A red-only curve lifts red and leaves green and blue.
    aibs_look red{};
    set_curve(red, AIBS_CURVE_RED, {0, 0.2f, 1, 1});
    const auto lifted = run_look(red, ramp);
    CHECK(lifted[3 * 100] > 100 + 20 && lifted[3 * 100 + 1] == 100 && lifted[3 * 100 + 2] == 100);
    // Points out of order or not numbers are skipped; one point is straight.
    aibs_look bad{};
    set_curve(bad, AIBS_CURVE_MASTER, {0.5f, 0.9f, 0.4f, 0.1f, NAN, 0.3f});
    CHECK(run_look(bad, ramp) == ramp);
}

void test_grading() {
    const auto ramp = grey_ramp();
    // Blue shadows, orange highlights: dark greys turn blue, light greys
    // orange; luminance stays close.
    aibs_look look{};
    look.grade_hue[AIBS_GRADE_SHADOWS] = 220;
    look.grade_saturation[AIBS_GRADE_SHADOWS] = 100;
    look.grade_hue[AIBS_GRADE_HIGHLIGHTS] = 30;
    look.grade_saturation[AIBS_GRADE_HIGHLIGHTS] = 100;
    const auto out = run_look(look, ramp);
    const uint8_t* dark = &out[3 * 40];
    const uint8_t* light = &out[3 * 215];
    CHECK(dark[2] > dark[0] + 10);
    CHECK(light[0] > light[2] + 10);
    const double dark_luma = 0.2126 * dark[0] + 0.7152 * dark[1] + 0.0722 * dark[2];
    CHECK(std::abs(dark_luma - 40) < 6);
    // Midtone brightness raises the middle most.
    aibs_look mid{};
    mid.grade_luminance[AIBS_GRADE_MIDTONES] = 100;
    const auto brighter = run_look(mid, ramp);
    CHECK(brighter[3 * 128] - 128 > brighter[3 * 20] - 20);
    CHECK(brighter[3 * 128] > 128 + 15);
}

void test_lut() {
    // A 2x2x2 LUT that swaps red and blue.
    const float swap[] = {0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 1, 1, 1, 0, 1, 1, 1};
    aibs_lut* lut = nullptr;
    CHECK(aibs_lut_create(swap, 1, &lut) == AIBS_ERR_INVALID_ARGUMENT && lut == nullptr);
    CHECK(aibs_lut_create(nullptr, 2, &lut) == AIBS_ERR_INVALID_ARGUMENT);
    aibs_lut* made = nullptr;
    CHECK(aibs_lut_create(swap, 2, &made) == AIBS_OK);
    aibs_look look{};
    look.lut = made;
    look.lut_amount = 100;
    const std::vector<uint8_t> px = {200, 100, 20};
    const auto out = run_look(look, px);
    CHECK(std::abs(out[0] - 20) <= 1 && std::abs(out[1] - 100) <= 1 && std::abs(out[2] - 200) <= 1);
    look.lut_amount = 50;
    const auto half = run_look(look, px);
    CHECK(std::abs(half[0] - 110) <= 1 && std::abs(half[2] - 110) <= 1);
    aibs_lut_free(made);
    aibs_lut_free(lut);
    aibs_lut_free(nullptr);
}

}  // namespace look_tests

void run_adjust_tests() {
    test_all_zero_changes_nothing();
    test_exposure_plus_one_doubles_linear_light();
    test_grey_ramps_stay_in_order();
    test_brightness_and_contrast();
    test_tone_controls_act_on_their_range();
    test_white_balance();
    test_saturation_and_vibrance();
    test_hsl_bands_are_selective();
    look_tests::test_empty_look_changes_nothing();
    look_tests::test_curves();
    look_tests::test_grading();
    look_tests::test_lut();
}

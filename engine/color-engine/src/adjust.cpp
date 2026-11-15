#include "adjust.h"

#include <algorithm>
#include <cmath>

namespace {

// Linear values above this are clipped (5 stops of exposure on white).
constexpr float kMaxLinear = 32.0f;
constexpr int kToneSize = 4096;
constexpr int kEncodeSize = 8192;
constexpr int kFinishSize = 16384;

// Centres of the HSL colour bands, in degrees: red, orange, yellow, green,
// aqua, blue, purple, magenta.
constexpr float kBandCentres[AIBS_HSL_BANDS] = {0, 30, 60, 120, 180, 240, 280, 320};
// Largest hue shift, in degrees, at a band hue of +/-100.
constexpr float kMaxHueShift = 30.0f;

double srgb_to_linear(double v) {
    return v <= 0.04045 ? v / 12.92 : std::pow((v + 0.055) / 1.055, 2.4);
}

// sRGB encoding, extended past 1 for values brighter than white.
double linear_to_srgb(double v) {
    return v <= 0.0031308 ? v * 12.92 : 1.055 * std::pow(v, 1.0 / 2.4) - 0.055;
}

double srgb_decode_extended(double v) {
    return srgb_to_linear(std::max(v, 0.0));
}

float fix(float value, float limit) {
    return std::isfinite(value) ? std::clamp(value, -limit, limit) : 0.0f;
}

aibs_adjustments clamped(const aibs_adjustments& in) {
    aibs_adjustments a = in;
    a.exposure = fix(a.exposure, 5.0f);
    for (float* v : {&a.brightness, &a.contrast, &a.highlights, &a.shadows, &a.whites, &a.blacks,
                     &a.temperature, &a.tint, &a.vibrance, &a.saturation}) {
        *v = fix(*v, 100.0f);
    }
    for (int i = 0; i < AIBS_HSL_BANDS; ++i) {
        a.hsl_hue[i] = fix(a.hsl_hue[i], 100.0f);
        a.hsl_saturation[i] = fix(a.hsl_saturation[i], 100.0f);
        a.hsl_luminance[i] = fix(a.hsl_luminance[i], 100.0f);
    }
    return a;
}

// Linear interpolation in a table sampled at t = 0, 1/(n-1), ..., 1.
float lookup(const std::vector<float>& table, float t) {
    const float x = std::clamp(t, 0.0f, 1.0f) * static_cast<float>(table.size() - 1);
    const size_t i = std::min(static_cast<size_t>(x), table.size() - 2);
    const float f = x - static_cast<float>(i);
    return table[i] + (table[i + 1] - table[i]) * f;
}

// The tone curve on a display-encoded luminance p (may exceed 1).
double tone_curve(double p, const aibs_adjustments& a) {
    const double q = std::clamp(p, 0.0, 1.0);
    const double shadows = 6.75 * q * (1 - q) * (1 - q);   // peaks at 1/3
    const double highlights = 6.75 * q * q * (1 - q);      // peaks at 2/3
    const double blacks = (1 - q) * (1 - q) * (1 - q);     // strongest at black
    const double whites = p <= 1.0 ? q * q * q : 1.0 + 3.0 * (p - 1.0);  // strongest at and past white
    return p + 0.25 * a.shadows / 100.0 * shadows + 0.25 * a.highlights / 100.0 * highlights +
           0.15 * a.blacks / 100.0 * blacks + 0.15 * a.whites / 100.0 * whites;
}

}  // namespace

namespace aibs {

Pipeline::Pipeline(const aibs_adjustments& adjustments) : a_(clamped(adjustments)) {
    const aibs_adjustments& a = a_;
    has_tone_ = a.highlights != 0 || a.shadows != 0 || a.whites != 0 || a.blacks != 0;
    for (int i = 0; i < AIBS_HSL_BANDS; ++i) {
        has_hue_ = has_hue_ || a.hsl_hue[i] != 0;
        has_bands_ = has_bands_ || a.hsl_hue[i] != 0 || a.hsl_saturation[i] != 0 || a.hsl_luminance[i] != 0;
    }
    has_colour_ = has_bands_ || a.saturation != 0 || a.vibrance != 0;
    identity_ = !has_tone_ && !has_colour_ && a.exposure == 0 && a.brightness == 0 && a.contrast == 0 &&
                a.temperature == 0 && a.tint == 0;
    if (identity_) {
        return;
    }

    for (int i = 0; i < 256; ++i) {
        to_linear_[i] = static_cast<float>(srgb_to_linear(i / 255.0));
    }

    // White balance: warm raises red and lowers blue; magenta lowers green.
    // Normalised so neutral grey keeps its luminance. Exposure multiplies all.
    const double t = a.temperature / 100.0;
    const double u = a.tint / 100.0;
    const double r0 = std::pow(2.0, 0.35 * t), g0 = std::pow(2.0, -0.3 * u), b0 = std::pow(2.0, -0.35 * t);
    const double norm = 0.2126 * r0 + 0.7152 * g0 + 0.0722 * b0;
    const double exposure = std::pow(2.0, a.exposure);
    gain_r_ = static_cast<float>(r0 / norm * exposure);
    gain_g_ = static_cast<float>(g0 / norm * exposure);
    gain_b_ = static_cast<float>(b0 / norm * exposure);

    encode_.resize(kEncodeSize);
    for (int j = 0; j < kEncodeSize; ++j) {
        const double s = static_cast<double>(j) / (kEncodeSize - 1);
        encode_[j] = static_cast<float>(linear_to_srgb(s * s * kMaxLinear));
    }

    if (has_tone_) {
        // Ratio new/old luminance per luminance; forced non-decreasing in
        // the output so the curve never turns tones around.
        tone_.resize(kToneSize);
        double highest = 0.0;
        for (int j = 0; j < kToneSize; ++j) {
            const double s = std::max(static_cast<double>(j), 0.5) / (kToneSize - 1);
            const double y = s * s * kMaxLinear;
            const double p = linear_to_srgb(y);
            highest = std::max(highest, std::max(tone_curve(p, a), 0.0));
            tone_[j] = static_cast<float>(srgb_decode_extended(highest) / y);
        }
    }

    global_saturation_ = 1.0f + a.saturation / 100.0f;
    vibrance_ = a.vibrance / 100.0f;

    const double brightness_gamma = std::pow(2.0, -a.brightness / 100.0);
    const double contrast_gain = 1.0 + a.contrast / 100.0;
    finish_.resize(kFinishSize);
    for (int j = 0; j < kFinishSize; ++j) {
        double v = static_cast<double>(j) / (kFinishSize - 1);
        v = std::pow(v, brightness_gamma);
        v = 0.5 + (v - 0.5) * contrast_gain;
        finish_[j] = static_cast<uint8_t>(std::lround(std::clamp(v, 0.0, 1.0) * 255.0));
    }
}

float Pipeline::tone_ratio(float luminance) const {
    return lookup(tone_, std::sqrt(std::max(luminance, 0.0f) / kMaxLinear));
}

float Pipeline::encode(float linear) const {
    return lookup(encode_, std::sqrt(std::max(linear, 0.0f) / kMaxLinear));
}

uint8_t Pipeline::finish(float encoded) const {
    const float t = std::clamp(encoded, 0.0f, 1.0f) * static_cast<float>(kFinishSize - 1);
    return finish_[static_cast<size_t>(t + 0.5f)];
}

void Pipeline::colour(float& r, float& g, float& b) const {
    const float mx = std::max({r, g, b});
    const float mn = std::min({r, g, b});
    const float chroma = mx - mn;
    // Band changes fade out towards grey so neutral tones stay neutral.
    const float colourful = std::clamp(chroma * 4.0f, 0.0f, 1.0f);

    float band_hue = 0, band_saturation = 0, band_luminance = 0;
    if (has_bands_ && chroma > 0) {
        float hue;
        if (mx == r) {
            hue = 60.0f * ((g - b) / chroma);
            if (hue < 0.0f) hue += 360.0f;
        } else if (mx == g) {
            hue = 60.0f * ((b - r) / chroma + 2.0f);
        } else {
            hue = 60.0f * ((r - g) / chroma + 4.0f);
        }
        // Blend the two bands the hue lies between.
        int lower = AIBS_HSL_BANDS - 1;
        for (int i = 0; i < AIBS_HSL_BANDS; ++i) {
            if (hue >= kBandCentres[i]) lower = i;
        }
        const int upper = (lower + 1) % AIBS_HSL_BANDS;
        const float start = kBandCentres[lower];
        const float end = upper == 0 ? 360.0f : kBandCentres[upper];
        const float w = (hue - start) / (end - start);
        const auto mix = [&](const float* values) {
            return (values[lower] * (1.0f - w) + values[upper] * w) / 100.0f;
        };
        band_hue = mix(a_.hsl_hue) * kMaxHueShift * colourful;
        band_saturation = mix(a_.hsl_saturation) * colourful;
        band_luminance = mix(a_.hsl_luminance) * colourful;

        if (has_hue_ && band_hue != 0) {
            // Rotate the hue, keeping HSV saturation and value.
            float degrees = hue + band_hue;
            if (degrees < 0.0f) degrees += 360.0f;
            if (degrees >= 360.0f) degrees -= 360.0f;
            const float h = degrees / 60.0f;
            const int sector = std::min(static_cast<int>(h), 5);
            const float f = h - static_cast<float>(sector);
            const float x = chroma * ((sector & 1) ? 1.0f - f : f);
            float r1 = 0, g1 = 0, b1 = 0;
            switch (sector) {
                case 0: r1 = chroma; g1 = x; break;
                case 1: r1 = x; g1 = chroma; break;
                case 2: g1 = chroma; b1 = x; break;
                case 3: g1 = x; b1 = chroma; break;
                case 4: r1 = x; b1 = chroma; break;
                default: r1 = chroma; b1 = x; break;
            }
            r = r1 + mn;
            g = g1 + mn;
            b = b1 + mn;
        }
    }

    // Saturation scales colour around the pixel's luma; vibrance does so
    // more for weakly coloured pixels.
    const float luma = 0.2126f * r + 0.7152f * g + 0.0722f * b;
    const float pixel_saturation = mx > 0 ? chroma / mx : 0.0f;
    const float factor = std::max(0.0f, global_saturation_ * (1.0f + vibrance_ * (1.0f - pixel_saturation)) *
                                             (1.0f + band_saturation));
    r = luma + (r - luma) * factor;
    g = luma + (g - luma) * factor;
    b = luma + (b - luma) * factor;

    if (band_luminance != 0) {
        const float m = 1.0f + 0.5f * band_luminance;
        r *= m;
        g *= m;
        b *= m;
    }
}

void Pipeline::apply(uint8_t r8, uint8_t g8, uint8_t b8, uint8_t* out_r, uint8_t* out_g, uint8_t* out_b) const {
    if (identity_) {
        *out_r = r8;
        *out_g = g8;
        *out_b = b8;
        return;
    }
    float r = to_linear_[r8] * gain_r_;
    float g = to_linear_[g8] * gain_g_;
    float b = to_linear_[b8] * gain_b_;
    if (has_tone_) {
        const float k = tone_ratio(0.2126f * r + 0.7152f * g + 0.0722f * b);
        r *= k;
        g *= k;
        b *= k;
    }
    float pr = encode(r), pg = encode(g), pb = encode(b);
    if (has_colour_) {
        colour(pr, pg, pb);
    }
    *out_r = finish(pr);
    *out_g = finish(pg);
    *out_b = finish(pb);
}

void Pipeline::apply_bgr(uint8_t* bgr, int count) const {
    if (identity_) {
        return;
    }
    for (int i = 0; i < count; ++i) {
        uint8_t* p = bgr + 3 * i;
        apply(p[2], p[1], p[0], &p[2], &p[1], &p[0]);
    }
}

}  // namespace aibs

int32_t aibs_adjust_rgb8(const aibs_adjustments* adjustments, const uint8_t* in, uint8_t* out,
                         size_t pixel_count) {
    if (adjustments == nullptr || (pixel_count > 0 && (in == nullptr || out == nullptr))) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    try {
        const aibs::Pipeline pipeline(*adjustments);
        for (size_t i = 0; i < pixel_count; ++i) {
            const uint8_t r = in[3 * i], g = in[3 * i + 1], b = in[3 * i + 2];
            pipeline.apply(r, g, b, &out[3 * i], &out[3 * i + 1], &out[3 * i + 2]);
        }
        return AIBS_OK;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

// The look: colour grading, RGB curves and a 3D LUT, applied to display
// values after the adjustments.
#include "look.h"

#include <algorithm>
#include <cmath>
#include <new>

namespace aibs {

namespace {

float finite_or(float v, float fallback) {
    return std::isfinite(v) ? v : fallback;
}

// Fully saturated colour of `hue` degrees, 0..1.
std::array<double, 3> hue_colour(double hue) {
    hue = std::fmod(std::fmod(hue, 360.0) + 360.0, 360.0);
    const double h = hue / 60.0;
    const double x = 1.0 - std::abs(std::fmod(h, 2.0) - 1.0);
    switch (static_cast<int>(h)) {
        case 0: return {1, x, 0};
        case 1: return {x, 1, 0};
        case 2: return {0, 1, x};
        case 3: return {0, x, 1};
        case 4: return {x, 0, 1};
        default: return {1, 0, x};
    }
}

uint8_t to8(double v) {
    return static_cast<uint8_t>(std::lround(std::clamp(v, 0.0, 255.0)));
}

}  // namespace

std::array<uint8_t, 256> curve_table(const float* points, uint32_t count) {
    std::array<uint8_t, 256> table{};
    for (int i = 0; i < 256; ++i) table[i] = static_cast<uint8_t>(i);
    if (points == nullptr) return table;
    std::vector<double> xs, ys;
    for (uint32_t i = 0; i < std::min<uint32_t>(count, AIBS_CURVE_POINTS); ++i) {
        const float x = points[2 * i], y = points[2 * i + 1];
        if (!std::isfinite(x) || !std::isfinite(y)) continue;
        const double cx = std::clamp(static_cast<double>(x), 0.0, 1.0);
        if (!xs.empty() && cx <= xs.back()) continue;
        xs.push_back(cx);
        ys.push_back(std::clamp(static_cast<double>(y), 0.0, 1.0));
    }
    const size_t n = xs.size();
    if (n < 2) return table;

    // Fritsch-Carlson monotone cubic tangents.
    std::vector<double> delta(n - 1), m(n);
    for (size_t i = 0; i + 1 < n; ++i) delta[i] = (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i]);
    m[0] = delta[0];
    m[n - 1] = delta[n - 2];
    for (size_t i = 1; i + 1 < n; ++i) {
        m[i] = delta[i - 1] * delta[i] <= 0 ? 0.0 : (delta[i - 1] + delta[i]) / 2.0;
    }
    for (size_t i = 0; i + 1 < n; ++i) {
        if (delta[i] == 0) {
            m[i] = 0;
            m[i + 1] = 0;
            continue;
        }
        const double a = m[i] / delta[i], b = m[i + 1] / delta[i];
        const double h = a * a + b * b;
        if (h > 9.0) {
            const double t = 3.0 / std::sqrt(h);
            m[i] = t * a * delta[i];
            m[i + 1] = t * b * delta[i];
        }
    }
    for (int v = 0; v < 256; ++v) {
        const double x = v / 255.0;
        double y;
        if (x <= xs[0]) {
            y = ys[0];
        } else if (x >= xs[n - 1]) {
            y = ys[n - 1];
        } else {
            size_t k = 0;
            while (k + 2 < n && x > xs[k + 1]) ++k;
            const double h = xs[k + 1] - xs[k];
            const double t = (x - xs[k]) / h;
            const double t2 = t * t, t3 = t2 * t;
            y = (2 * t3 - 3 * t2 + 1) * ys[k] + (t3 - 2 * t2 + t) * h * m[k] + (-2 * t3 + 3 * t2) * ys[k + 1] +
                (t3 - t2) * h * m[k + 1];
        }
        table[v] = to8(y * 255.0);
    }
    return table;
}

Look::Look(const aibs_look& look) {
    // Grading.
    const double balance = std::clamp(static_cast<double>(finite_or(look.grade_balance, 0)), -100.0, 100.0) / 100.0;
    std::array<std::array<double, 3>, 3> tint{};
    std::array<double, 3> lift{};
    for (int k = 0; k < 3; ++k) {
        const double sat = std::clamp(static_cast<double>(finite_or(look.grade_saturation[k], 0)), 0.0, 100.0) / 100.0;
        const double lum = std::clamp(static_cast<double>(finite_or(look.grade_luminance[k], 0)), -100.0, 100.0) / 100.0;
        const auto c = hue_colour(finite_or(look.grade_hue[k], 0));
        const double luma = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        for (int ch = 0; ch < 3; ++ch) tint[k][ch] = (c[ch] - luma) * sat * 0.25 * 255.0;
        lift[k] = lum * 0.2 * 255.0;
        has_grade_ = has_grade_ || sat > 0 || lum != 0;
    }
    if (has_grade_) {
        grade_.resize(256);
        for (int v = 0; v < 256; ++v) {
            const double l = std::clamp(v / 255.0 + 0.3 * balance, 0.0, 1.0);
            const double w[3] = {(1 - l) * (1 - l), 2 * l * (1 - l), l * l};
            for (int ch = 0; ch < 3; ++ch) {
                double d = 0;
                for (int k = 0; k < 3; ++k) d += w[k] * (tint[k][ch] + lift[k]);
                grade_[v][ch] = static_cast<float>(d);
            }
        }
    }

    // Curves: master then the channel's own.
    const auto master = curve_table(look.curves[AIBS_CURVE_MASTER], look.curve_points[AIBS_CURVE_MASTER]);
    for (int ch = 0; ch < 3; ++ch) {
        const auto own = curve_table(look.curves[AIBS_CURVE_RED + ch], look.curve_points[AIBS_CURVE_RED + ch]);
        for (int v = 0; v < 256; ++v) {
            curve_[ch][v] = own[master[v]];
            has_curves_ = has_curves_ || curve_[ch][v] != v;
        }
    }

    const double amount = std::clamp(static_cast<double>(finite_or(look.lut_amount, 0)), 0.0, 100.0) / 100.0;
    if (look.lut != nullptr && look.lut->size >= 2 && amount > 0) {
        lut_ = look.lut;
        lut_amount_ = static_cast<float>(amount);
    }
    identity_ = !has_grade_ && !has_curves_ && lut_ == nullptr;
}

void Look::apply(uint8_t r, uint8_t g, uint8_t b, uint8_t* out_r, uint8_t* out_g, uint8_t* out_b) const {
    if (identity_) {
        *out_r = r;
        *out_g = g;
        *out_b = b;
        return;
    }
    if (has_grade_) {
        const int luma = std::clamp(static_cast<int>(std::lround(0.2126 * r + 0.7152 * g + 0.0722 * b)), 0, 255);
        const auto& d = grade_[luma];
        r = to8(r + d[0]);
        g = to8(g + d[1]);
        b = to8(b + d[2]);
    }
    if (has_curves_) {
        r = curve_[0][r];
        g = curve_[1][g];
        b = curve_[2][b];
    }
    if (lut_ != nullptr) {
        const int n = lut_->size;
        const float scale = static_cast<float>(n - 1) / 255.0f;
        const float fr = r * scale, fg = g * scale, fb = b * scale;
        const int r0 = std::min(static_cast<int>(fr), n - 2), g0 = std::min(static_cast<int>(fg), n - 2),
                  b0 = std::min(static_cast<int>(fb), n - 2);
        const float tr = fr - r0, tg = fg - g0, tb = fb - b0;
        const float* data = lut_->rgb.data();
        const auto at = [&](int ri, int gi, int bi, int c) {
            return data[3 * ((static_cast<size_t>(bi) * n + gi) * n + ri) + c];
        };
        float result[3];
        for (int c = 0; c < 3; ++c) {
            const float c00 = at(r0, g0, b0, c) * (1 - tr) + at(r0 + 1, g0, b0, c) * tr;
            const float c10 = at(r0, g0 + 1, b0, c) * (1 - tr) + at(r0 + 1, g0 + 1, b0, c) * tr;
            const float c01 = at(r0, g0, b0 + 1, c) * (1 - tr) + at(r0 + 1, g0, b0 + 1, c) * tr;
            const float c11 = at(r0, g0 + 1, b0 + 1, c) * (1 - tr) + at(r0 + 1, g0 + 1, b0 + 1, c) * tr;
            const float c0 = c00 * (1 - tg) + c10 * tg;
            const float c1 = c01 * (1 - tg) + c11 * tg;
            result[c] = (c0 * (1 - tb) + c1 * tb) * 255.0f;
        }
        const uint8_t in[3] = {r, g, b};
        r = to8(in[0] + lut_amount_ * (result[0] - in[0]));
        g = to8(in[1] + lut_amount_ * (result[1] - in[1]));
        b = to8(in[2] + lut_amount_ * (result[2] - in[2]));
    }
    *out_r = r;
    *out_g = g;
    *out_b = b;
}

void Look::apply_bgr(uint8_t* bgr, int count) const {
    if (identity_) return;
    for (int i = 0; i < count; ++i) {
        uint8_t* p = bgr + 3 * i;
        apply(p[2], p[1], p[0], &p[2], &p[1], &p[0]);
    }
}

}  // namespace aibs

int32_t aibs_lut_create(const float* rgb, uint32_t size, aibs_lut** out) {
    if (out == nullptr) return AIBS_ERR_INVALID_ARGUMENT;
    *out = nullptr;
    if (rgb == nullptr || size < 2 || size > 65) return AIBS_ERR_INVALID_ARGUMENT;
    try {
        const size_t count = static_cast<size_t>(size) * size * size * 3;
        auto* lut = new aibs_lut();
        lut->size = static_cast<int>(size);
        lut->rgb.resize(count);
        for (size_t i = 0; i < count; ++i) {
            const float v = rgb[i];
            lut->rgb[i] = std::isfinite(v) ? std::clamp(v, 0.0f, 1.0f) : 0.0f;
        }
        *out = lut;
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

void aibs_lut_free(aibs_lut* lut) {
    delete lut;
}

int32_t aibs_look_rgb8(const aibs_look* look, const uint8_t* in, uint8_t* out, size_t pixel_count) {
    if (look == nullptr || (pixel_count > 0 && (in == nullptr || out == nullptr))) return AIBS_ERR_INVALID_ARGUMENT;
    try {
        const aibs::Look prepared(*look);
        for (size_t i = 0; i < pixel_count; ++i) {
            const uint8_t r = in[3 * i], g = in[3 * i + 1], b = in[3 * i + 2];
            prepared.apply(r, g, b, &out[3 * i], &out[3 * i + 1], &out[3 * i + 2]);
        }
        return AIBS_OK;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

// One-click enhancement: suggested adjustment values from image statistics.
//
// The result is ordinary slider values, so the user sees exactly what was
// changed and can edit or undo it. Nothing here moves pixels.
#include "../../image-engine/src/source_internal.h"

#include "aibs/engine.h"

#include <algorithm>
#include <cmath>
#include <cstring>
#include <new>
#include <vector>

namespace {

double srgb_to_linear(double v) {
    return v <= 0.04045 ? v / 12.92 : std::pow((v + 0.055) / 1.055, 2.4);
}

struct Image {
    aibs_image img{};
    ~Image() { aibs_image_free(&img); }
};

// Value below which `fraction` of the histogram lies (0..1 scale).
double percentile(const std::vector<double>& histogram, double total, double fraction) {
    double seen = 0;
    for (size_t i = 0; i < histogram.size(); ++i) {
        seen += histogram[i];
        if (seen >= fraction * total) return (static_cast<double>(i) + 0.5) / histogram.size();
    }
    return 1.0;
}

}  // namespace

int32_t aibs_auto_adjust(const aibs_source* source, const aibs_face* faces, uint32_t face_count,
                         aibs_adjustments* out) {
    if (out == nullptr || source == nullptr || source->levels.empty() || (faces == nullptr && face_count > 0)) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    std::memset(out, 0, sizeof(*out));
    try {
        const cv::Mat& full = source->levels[0];
        const double scale = std::min(1.0, 512.0 / std::max(full.cols, full.rows));
        const auto w = static_cast<uint32_t>(std::max(1.0, std::round(full.cols * scale)));
        const auto h = static_cast<uint32_t>(std::max(1.0, std::round(full.rows * scale)));
        Image small;
        const int32_t status =
            aibs_source_render(source, 0, 0, full.cols, full.rows, w, h, nullptr, nullptr, nullptr, nullptr, 0, &small.img);
        if (status != AIBS_OK) return status;

        // Brightness histogram (display luma) and neutral-colour averages.
        std::vector<double> histogram(256, 0.0);
        double neutral_r = 0, neutral_g = 0, neutral_b = 0, neutral_n = 0;
        const size_t pixels = static_cast<size_t>(w) * h;
        for (size_t i = 0; i < pixels; ++i) {
            const uint8_t* p = small.img.data + i * 4;
            const double r = p[0] / 255.0, g = p[1] / 255.0, b = p[2] / 255.0;
            const double luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            histogram[std::min<size_t>(255, static_cast<size_t>(luma * 256))] += 1;
            const double chroma = std::max({r, g, b}) - std::min({r, g, b});
            if (luma > 0.15 && luma < 0.9 && chroma < 0.25) {
                neutral_r += srgb_to_linear(r);
                neutral_g += srgb_to_linear(g);
                neutral_b += srgb_to_linear(b);
                neutral_n += 1;
            }
        }
        const auto total = static_cast<double>(pixels);
        const double p005 = percentile(histogram, total, 0.005), p05 = percentile(histogram, total, 0.05);
        const double p10 = percentile(histogram, total, 0.10), p50 = percentile(histogram, total, 0.50);
        const double p95 = percentile(histogram, total, 0.95), p98 = percentile(histogram, total, 0.98);
        const double p995 = percentile(histogram, total, 0.995);

        // Exposure: bring the subject to a comfortable brightness. With
        // faces, the subject is the skin; otherwise the median tone.
        double current = p50, target = 0.46;
        if (face_count > 0) {
            double sum = 0, n = 0;
            for (uint32_t f = 0; f < face_count; ++f) {
                const aibs_face& face = faces[f];
                const int x0 = std::max(0, static_cast<int>((face.x + 0.25 * face.width) * scale));
                const int x1 = std::min(static_cast<int>(w), static_cast<int>((face.x + 0.75 * face.width) * scale));
                const int y0 = std::max(0, static_cast<int>((face.y + 0.3 * face.height) * scale));
                const int y1 = std::min(static_cast<int>(h), static_cast<int>((face.y + 0.8 * face.height) * scale));
                for (int yy = y0; yy < y1; ++yy) {
                    for (int xx = x0; xx < x1; ++xx) {
                        const uint8_t* p = small.img.data + (static_cast<size_t>(yy) * w + xx) * 4;
                        sum += (0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]) / 255.0;
                        n += 1;
                    }
                }
            }
            if (n > 0) {
                current = sum / n;
                target = 0.62;
            }
        }
        double exposure = std::log2(srgb_to_linear(target) / std::max(srgb_to_linear(current), 1e-4)) * 0.85;
        exposure = std::clamp(exposure, -2.0, 2.0);
        if (std::abs(exposure) < 0.1) exposure = 0;
        out->exposure = static_cast<float>(std::round(exposure * 20) / 20);  // slider steps of 0.05

        // Tone: recover bright and dark areas, extend a short tonal range.
        out->highlights = p98 > 0.92 ? static_cast<float>(-std::min(60.0, (p98 - 0.92) * 600 + 15)) : 0.0f;
        out->shadows = p10 < 0.15 ? static_cast<float>(std::min(50.0, (0.15 - p10) * 300 + 10)) : 0.0f;
        out->whites = p995 < 0.85 ? static_cast<float>(std::min(40.0, (0.85 - p995) * 150)) : 0.0f;
        out->blacks = p005 > 0.06 ? static_cast<float>(-std::min(30.0, (p005 - 0.06) * 200)) : 0.0f;
        const double spread = p95 - p05;
        out->contrast = spread < 0.65 ? static_cast<float>(std::min(25.0, (0.65 - spread) * 60)) : 0.0f;

        // White balance: make near-neutral colours neutral (grey world on
        // low-saturation midtones), halfway, only when there are enough.
        if (neutral_n > 0.02 * total) {
            const double blue_over_red = neutral_b / std::max(neutral_r, 1e-6);
            const double green_excess = neutral_g / std::max(std::sqrt(neutral_r * neutral_b), 1e-6);
            // Temperature +1 (slider 100) changes blue/red by 2^-0.7; tint
            // +1 lowers green by 2^-0.3 (see the colour pipeline).
            out->temperature = static_cast<float>(std::clamp(std::log2(blue_over_red) / 0.7 * 100 * 0.6, -40.0, 40.0));
            out->tint = static_cast<float>(std::clamp(std::log2(green_excess) / 0.3 * 100 * 0.6, -30.0, 30.0));
            if (std::abs(out->temperature) < 3) out->temperature = 0;
            if (std::abs(out->tint) < 3) out->tint = 0;
        }

        // A little more colour; gentler with people, to spare skin tones.
        out->vibrance = face_count > 0 ? 10.0f : 15.0f;

        for (float* v : {&out->highlights, &out->shadows, &out->whites, &out->blacks, &out->contrast,
                         &out->temperature, &out->tint}) {
            *v = std::round(*v);
        }
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

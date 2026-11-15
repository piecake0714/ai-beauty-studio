// Lighting: a light added to the photo, shaded by the photo's 3D shape.
//
// The shape comes from AI depth estimation, turned into surface directions
// once per image. Each render then computes, per pixel, the direction to
// the light and how much the surface faces it, and changes brightness in
// linear light: surfaces facing the light get brighter, surfaces facing
// away darker. Cast shadows are not modelled.
#include "light.h"

#include "../../ai-runtime/src/session_internal.h"
#include "../../background-engine/src/background.h"
#include "../../image-engine/src/source_internal.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <new>
#include <vector>

#include <opencv2/core/utility.hpp>
#include <opencv2/imgproc.hpp>

namespace {

// Depth Anything V2: longer input side (a multiple of its 14-pixel patch).
constexpr int kDepthSide = 518;
constexpr int kPatch = 14;
// Surface directions are kept at about this many pixels on the longer side.
constexpr int kShapeSide = 1024;
// How deep the photo's relief is taken to be, relative to its longer side.
constexpr double kRelief = 0.35;
// Slopes (tangent of the tilt) from which depth counts as a jump between
// objects rather than a surface: fully a jump from kSteepTo.
constexpr double kSteepFrom = 1.5;
constexpr double kSteepTo = 3.0;
// Most a light may darken a surface facing away from it, in stops.
constexpr double kMaxDarkening = 1.0;

struct Image {
    aibs_image img{};
    ~Image() { aibs_image_free(&img); }
};

struct Kind {
    double reach;  // relative reach for a given size
    double wrap;   // how far light wraps around a surface (softness)
    bool fill;     // only brightens dark areas
    double warm;   // built-in warmth
};

Kind kind_of(uint32_t type) {
    switch (type) {
        case AIBS_LIGHT_NATURAL: return {2.0, 0.05, false, 0.25};
        case AIBS_LIGHT_DIFFUSE: return {1.6, 0.8, false, 0.0};
        case AIBS_LIGHT_REFLECTOR: return {1.0, 0.5, true, 0.05};
        case AIBS_LIGHT_SOFTBOX: return {0.8, 0.35, false, 0.0};
        default: return {1.0, 0.0, false, 0.0};
    }
}

cv::Mat box(const cv::Mat& m, int radius) {
    cv::Mat out;
    cv::boxFilter(m, out, -1, cv::Size(2 * radius + 1, 2 * radius + 1), cv::Point(-1, -1), true,
                  cv::BORDER_REFLECT);
    return out;
}

cv::Mat guided(const cv::Mat& g, const cv::Mat& p, int radius, double eps) {
    const cv::Mat mean_g = box(g, radius), mean_p = box(p, radius);
    const cv::Mat cov = box(g.mul(p), radius) - mean_g.mul(mean_p);
    const cv::Mat var = box(g.mul(g), radius) - mean_g.mul(mean_g);
    cv::Mat a;
    cv::divide(cov, var + eps, a);
    const cv::Mat b = mean_p - a.mul(mean_g);
    return box(a, radius).mul(g) + box(b, radius);
}

// Build a shape from relative inverse depth (larger is closer) stretched
// over a source_width x source_height image. `guide` (8-bit BGR, any size,
// may be empty) aligns depth edges with the picture.
aibs_shape* make_shape(const cv::Mat& depth_in, int source_width, int source_height, const cv::Mat& guide) {
    // Robust normalisation to 0..1.
    std::vector<float> values(depth_in.begin<float>(), depth_in.end<float>());
    std::sort(values.begin(), values.end());
    const float lo = values[values.size() / 100];
    const float hi = values[values.size() - 1 - values.size() / 100];
    cv::Mat depth;
    depth_in.convertTo(depth, CV_32F, 1.0 / std::max(hi - lo, 1e-6f), -lo / std::max(hi - lo, 1e-6f));
    depth = cv::min(cv::max(depth, 0.0), 1.0);

    const double fit = static_cast<double>(kShapeSide) / std::max(source_width, source_height);
    const int w = std::max(2, static_cast<int>(std::lround(source_width * std::min(1.0, fit))));
    const int h = std::max(2, static_cast<int>(std::lround(source_height * std::min(1.0, fit))));
    cv::Mat d;
    cv::resize(depth, d, cv::Size(w, h), 0, 0, cv::INTER_CUBIC);
    const int longer = std::max(w, h);
    if (!guide.empty()) {
        cv::Mat grey;
        cv::cvtColor(guide, grey, cv::COLOR_BGR2GRAY);
        cv::resize(grey, grey, d.size(), 0, 0, cv::INTER_AREA);
        grey.convertTo(grey, CV_32F, 1.0 / 255.0);
        d = guided(grey, d, std::max(2, longer / 256), 1e-3);
    }
    cv::GaussianBlur(d, d, cv::Size(0, 0), std::max(1.0, 0.004 * longer));

    cv::Mat gx, gy;
    cv::Sobel(d, gx, CV_32F, 1, 0, 3, 1.0 / 8.0, 0, cv::BORDER_REPLICATE);
    cv::Sobel(d, gy, CV_32F, 0, 1, 3, 1.0 / 8.0, 0, cv::BORDER_REPLICATE);
    const double relief = kRelief * longer;

    // Depth jumps (a subject's outline against the background) are not
    // surfaces: their estimated position is blurred and a few pixels off,
    // and shading them draws a bright/dark rim beside the real edge (a
    // ghost outline). Treat steep slopes as jumps and keep them flat.
    cv::Mat slope;
    cv::magnitude(gx, gy, slope);
    slope *= relief;
    cv::Mat jump(h, w, CV_32F);
    for (int row = 0; row < h; ++row) {
        const float* sl = slope.ptr<float>(row);
        float* j = jump.ptr<float>(row);
        for (int col = 0; col < w; ++col) {
            const double t = std::clamp((sl[col] - kSteepFrom) / (kSteepTo - kSteepFrom), 0.0, 1.0);
            j[col] = static_cast<float>(t * t * (3.0 - 2.0 * t));
        }
    }
    const int guard = std::max(2, longer / 256);
    // Widen the jump to cover the blurred tails on both sides, then soften.
    cv::dilate(jump, jump, cv::getStructuringElement(cv::MORPH_ELLIPSE, cv::Size(4 * guard + 1, 4 * guard + 1)));
    jump = box(jump, guard);

    auto* shape = new aibs_shape();
    shape->normals.create(h, w, CV_32FC3);
    for (int row = 0; row < h; ++row) {
        const float* px = gx.ptr<float>(row);
        const float* py = gy.ptr<float>(row);
        const float* j = jump.ptr<float>(row);
        cv::Vec3f* n = shape->normals.ptr<cv::Vec3f>(row);
        for (int col = 0; col < w; ++col) {
            const double keep = 1.0 - j[col];
            const double nx = -px[col] * relief * keep, ny = -py[col] * relief * keep;
            const double len = std::sqrt(nx * nx + ny * ny + 1.0);
            n[col] = cv::Vec3f(static_cast<float>(nx / len), static_cast<float>(ny / len), static_cast<float>(1.0 / len));
        }
    }
    shape->source_width = source_width;
    shape->source_height = source_height;
    return shape;
}

}  // namespace

namespace aibs {

bool light_active(const aibs_light* light) {
    return light != nullptr && light->type >= AIBS_LIGHT_NATURAL && light->type <= AIBS_LIGHT_SOFTBOX &&
           light->intensity > 0.0f && std::isfinite(light->x) && std::isfinite(light->y);
}

void apply_light(cv::Mat& bgr, int first_col, double x, double y, double scale_x, double scale_y, int source_width,
                 int source_height, const aibs_light& light) {
    first_col = std::clamp(first_col, 0, bgr.cols);
    if (first_col >= bgr.cols) return;
    const Kind kind = kind_of(light.type);
    const double longer = std::max(source_width, source_height);
    const double lx = std::clamp(static_cast<double>(light.x), -1.0, 2.0) * source_width / longer;
    const double ly = std::clamp(static_cast<double>(light.y), -1.0, 2.0) * source_height / longer;
    const double lz = 0.02 + 0.98 * std::clamp(static_cast<double>(light.height), 0.0, 100.0) / 100.0;
    const double reach = (0.15 + 0.85 * std::clamp(static_cast<double>(light.size), 0.0, 100.0) / 100.0) * kind.reach;
    const double strength = std::clamp(static_cast<double>(light.intensity), 0.0, 100.0) / 100.0 * 1.5;
    const double warmth = std::clamp(std::clamp(static_cast<double>(light.warmth), -100.0, 100.0) / 100.0 + kind.warm, -1.0, 1.0);
    // Colour of the added light (B, G, R gains).
    const double tint[3] = {1.0 - 0.25 * warmth, 1.0 + 0.02 * warmth, 1.0 + 0.2 * warmth};

    cv::Mat normals;
    if (light.shape != nullptr && !light.shape->normals.empty()) {
        const aibs_shape& s = *light.shape;
        normals = warp_plane(s.normals, static_cast<double>(s.normals.cols) / s.source_width,
                             static_cast<double>(s.normals.rows) / s.source_height, x, y, scale_x, scale_y, bgr.cols,
                             bgr.rows, cv::INTER_LINEAR);
    }
    cv::Mat weight;
    if (light.subject != nullptr && !light.subject->coverage.empty()) {
        const aibs_matte& m = *light.subject;
        weight = warp_plane(m.coverage, static_cast<double>(m.coverage.cols) / m.source_width,
                            static_cast<double>(m.coverage.rows) / m.source_height, x, y, scale_x, scale_y, bgr.cols,
                            bgr.rows, cv::INTER_LINEAR);
    }

    std::array<float, 256> to_linear{};
    for (int i = 0; i < 256; ++i) {
        const double v = i / 255.0;
        to_linear[i] = static_cast<float>(v <= 0.04045 ? v / 12.92 : std::pow((v + 0.055) / 1.055, 2.4));
    }
    // Display encoding, indexed by sqrt of the linear value (0..1).
    constexpr int kSteps = 4096;
    std::vector<uint8_t> encode(kSteps + 1);
    for (int i = 0; i <= kSteps; ++i) {
        const double v = std::pow(static_cast<double>(i) / kSteps, 2.0);
        const double e = v <= 0.0031308 ? v * 12.92 : 1.055 * std::pow(v, 1.0 / 2.4) - 0.055;
        encode[i] = cv::saturate_cast<uint8_t>(e * 255.0);
    }
    const auto to_display = [&](double v) {
        if (v <= 0.0) return encode[0];
        if (v >= 1.0) return encode[kSteps];
        return encode[static_cast<size_t>(std::sqrt(v) * kSteps + 0.5)];
    };
    const double wrap = kind.wrap;
    const auto lambert = [wrap](double d) { return std::clamp((d + wrap) / (1.0 + wrap), 0.0, 1.0); };

    cv::parallel_for_(cv::Range(0, bgr.rows), [&](const cv::Range& rows) {
        for (int row = rows.start; row < rows.end; ++row) {
            uint8_t* p = bgr.ptr<uint8_t>(row);
            const cv::Vec3f* n = normals.empty() ? nullptr : normals.ptr<cv::Vec3f>(row);
            const uint8_t* wgt = weight.empty() ? nullptr : weight.ptr<uint8_t>(row);
            const double py = (y + (row + 0.5) / scale_y) / longer;
            for (int col = first_col; col < bgr.cols; ++col) {
                const double share = wgt ? wgt[col] / 255.0 : 1.0;
                if (share <= 0.0) continue;
                const double px = (x + (col + 0.5) / scale_x) / longer;
                const double dx = lx - px, dy = ly - py;
                const double t = std::hypot(dx, dy) / reach;
                const double falloff = light.type == AIBS_LIGHT_NATURAL ? 1.0 / (1.0 + t * t) : std::exp(-t * t);
                const double amount = strength * falloff * share;
                if (amount < 1e-3) continue;
                const double len = std::sqrt(dx * dx + dy * dy + lz * lz);
                const double ux = dx / len, uy = dy / len, uz = lz / len;
                const double facing = n ? n[col][0] * ux + n[col][1] * uy + n[col][2] * uz : uz;
                const double lit = lambert(facing);
                const double side = lit - lambert(uz);  // vs. a surface facing the camera

                uint8_t* q = p + col * 3;
                double c[3] = {to_linear[q[0]], to_linear[q[1]], to_linear[q[2]]};
                if (kind.fill) {
                    const double lum = 0.0722 * c[0] + 0.7152 * c[1] + 0.2126 * c[2];
                    const double add = amount * 0.25 * lit * std::pow(1.0 - std::min(lum, 1.0), 2.0);
                    for (int k = 0; k < 3; ++k) c[k] += add * tint[k];
                } else {
                    const double stops = light.type == AIBS_LIGHT_DIFFUSE ? amount * (0.7 * lit + 0.4 * side)
                                                                          : amount * (0.5 * lit + 1.5 * side);
                    const double gain = std::exp2(std::max(stops, -kMaxDarkening));
                    for (int k = 0; k < 3; ++k) c[k] *= gain >= 1.0 ? 1.0 + (gain - 1.0) * tint[k] : gain;
                }
                for (int k = 0; k < 3; ++k) q[k] = to_display(c[k]);
            }
        }
    });
}

}  // namespace aibs

int32_t aibs_shape_create(const float* depth, uint32_t width, uint32_t height, uint32_t source_width,
                          uint32_t source_height, aibs_shape** out) {
    if (out == nullptr) return AIBS_ERR_INVALID_ARGUMENT;
    *out = nullptr;
    if (depth == nullptr || width < 2 || height < 2 || width > 16384 || height > 16384 || source_width == 0 ||
        source_height == 0 || source_width > 1u << 20 || source_height > 1u << 20) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    try {
        const cv::Mat d(static_cast<int>(height), static_cast<int>(width), CV_32F, const_cast<float*>(depth));
        *out = make_shape(d, static_cast<int>(source_width), static_cast<int>(source_height), cv::Mat());
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

int32_t aibs_estimate_shape(const aibs_source* source, aibs_ai_session* model, aibs_shape** out) {
    if (out == nullptr) return AIBS_ERR_INVALID_ARGUMENT;
    *out = nullptr;
    if (source == nullptr || source->levels.empty() || model == nullptr) return AIBS_ERR_INVALID_ARGUMENT;
    try {
        const cv::Mat& full = source->levels[0];
        const auto side = [](double v) { return std::max(kPatch, static_cast<int>(std::ceil(v / kPatch)) * kPatch); };
        const double longer = std::max(full.cols, full.rows);
        const int w = side(kDepthSide * full.cols / longer);
        const int h = side(kDepthSide * full.rows / longer);
        Image input_image;
        int32_t status = aibs_source_render(source, 0, 0, full.cols, full.rows, static_cast<uint32_t>(w),
                                            static_cast<uint32_t>(h), nullptr, nullptr, nullptr, nullptr, 0,
                                            &input_image.img);
        if (status != AIBS_OK) return status;
        const size_t plane = static_cast<size_t>(w) * h;
        std::vector<float> input(3 * plane);
        const float mean[3] = {0.485f, 0.456f, 0.406f};
        const float stddev[3] = {0.229f, 0.224f, 0.225f};
        for (size_t i = 0; i < plane; ++i) {
            const uint8_t* p = input_image.img.data + i * 4;
            for (int c = 0; c < 3; ++c) input[c * plane + i] = (p[c] / 255.0f - mean[c]) / stddev[c];
        }
        const int64_t shape[4] = {1, 3, h, w};
        std::vector<aibs::NamedTensor> outputs;
        status = aibs::run_all_outputs(model, input.data(), shape, 4, outputs);
        if (status != AIBS_OK) return status;
        const aibs::NamedTensor* depth = nullptr;
        for (const aibs::NamedTensor& t : outputs) {
            if (t.data.size() == plane) {
                depth = &t;
                break;
            }
        }
        if (depth == nullptr) return AIBS_ERR_MODEL_INVALID;
        const cv::Mat d(h, w, CV_32F, const_cast<float*>(depth->data.data()));
        size_t level = 0;
        while (level + 1 < source->levels.size() &&
               std::max(source->levels[level].cols, source->levels[level].rows) > kShapeSide) {
            ++level;
        }
        *out = make_shape(d, full.cols, full.rows, source->levels[level]);
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (const cv::Exception& e) {
        return e.code == cv::Error::StsNoMem ? AIBS_ERR_OUT_OF_MEMORY : AIBS_ERR_INTERNAL;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

void aibs_shape_free(aibs_shape* shape) {
    delete shape;
}

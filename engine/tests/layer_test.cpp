// Tests for layers: adjustment and image layers, masks (base, strokes,
// erase, invert, subject), opacity, blend modes, the before/after split,
// and that brush strokes look the same at any zoom and in any tile.
#include "aibs/engine.h"
#include "check.h"

#include <cmath>
#include <cstdio>
#include <cstring>
#include <tuple>
#include <vector>
#include <algorithm>

#include <opencv2/core.hpp>
#include <opencv2/imgcodecs.hpp>

#include "../layer-engine/src/layers.h"

namespace {

struct Source {
    aibs_source* ptr = nullptr;
    ~Source() { aibs_source_free(ptr); }
};

struct Matte {
    aibs_matte* ptr = nullptr;
    ~Matte() { aibs_matte_free(ptr); }
};

struct Image {
    aibs_image img{};
    ~Image() { aibs_image_free(&img); }
    const uint8_t* px(int col, int row) const {
        return img.data + (static_cast<size_t>(row) * img.width + static_cast<size_t>(col)) * 4;
    }
    int green(int col, int row) const { return px(col, row)[1]; }
};

bool open_mat(const cv::Mat& bgr, Source& source) {
    std::vector<uint8_t> png;
    if (!cv::imencode(".png", bgr, png)) return false;
    return aibs_source_open(png.data(), png.size(), &source.ptr) == AIBS_OK;
}

constexpr int kW = 160, kH = 100;

// A horizontal grey ramp.
cv::Mat ramp() {
    cv::Mat m(kH, kW, CV_8UC3);
    for (int y = 0; y < kH; ++y)
        for (int x = 0; x < kW; ++x) {
            const auto v = static_cast<uint8_t>(30 + x);
            m.at<cv::Vec3b>(y, x) = cv::Vec3b(v, v, v);
        }
    return m;
}

bool render(const Source& source, const aibs_adjustments* adjustments, const aibs_layer* layers, uint32_t count,
            Image& out, uint32_t split = 0) {
    return aibs_source_render(source.ptr, 0, 0, kW, kH, kW, kH, nullptr, adjustments, nullptr, nullptr, layers, count,
                              split, &out.img) == AIBS_OK;
}

aibs_layer adjustment_layer(const aibs_adjustments* adjustments) {
    aibs_layer layer{};
    layer.kind = AIBS_LAYER_ADJUSTMENT;
    layer.blend = AIBS_BLEND_NORMAL;
    layer.opacity = 1.0f;
    layer.mask_base = AIBS_MASK_ALL;
    layer.adjustments = adjustments;
    return layer;
}

bool same(const Image& a, const Image& b) {
    return a.img.data && b.img.data && std::memcmp(a.img.data, b.img.data, static_cast<size_t>(kW) * kH * 4) == 0;
}

void test_adjustment_layers() {
    Source source;
    CHECK(open_mat(ramp(), source));
    aibs_adjustments brighter{};
    brighter.exposure = 1.0f;
    Image original, global, layered;
    CHECK(render(source, nullptr, nullptr, 0, original));
    CHECK(render(source, &brighter, nullptr, 0, global));
    aibs_layer layer = adjustment_layer(&brighter);
    CHECK(render(source, nullptr, &layer, 1, layered));
    CHECK(same(global, layered));  // a full layer equals the same global edit

    layer.opacity = 0.5f;
    Image half;
    CHECK(render(source, nullptr, &layer, 1, half));
    if (half.img.data && original.img.data && global.img.data) {
        const int o = original.green(80, 50), g = global.green(80, 50), h = half.green(80, 50);
        CHECK(std::abs(h - (o + g) / 2) <= 1);
    }
    layer.opacity = 1.0f;
    layer.mask_base = AIBS_MASK_NONE;
    Image hidden;
    CHECK(render(source, nullptr, &layer, 1, hidden));
    CHECK(same(original, hidden));
    layer.mask_invert = 1;
    Image inverted;
    CHECK(render(source, nullptr, &layer, 1, inverted));
    CHECK(same(global, inverted));

    // Before/after split: the left part stays original.
    layer.mask_invert = 0;
    layer.mask_base = AIBS_MASK_ALL;
    Image split;
    CHECK(render(source, nullptr, &layer, 1, split, 80));
    if (split.img.data) {
        CHECK(std::memcmp(split.px(40, 50), original.px(40, 50), 4) == 0);
        CHECK(std::memcmp(split.px(120, 50), global.px(120, 50), 4) == 0);
    }
}

void test_strokes() {
    Source source;
    CHECK(open_mat(ramp(), source));
    aibs_adjustments brighter{};
    brighter.exposure = 1.0f;
    Image original, global;
    CHECK(render(source, nullptr, nullptr, 0, original));
    CHECK(render(source, &brighter, nullptr, 0, global));

    // Paint a hard line across the middle, then erase its right half.
    const float line[] = {10.0f, 50.0f, 150.0f, 50.0f};
    const float right[] = {90.0f, 50.0f, 150.0f, 50.0f};
    aibs_stroke strokes[2] = {{line, 2, 20.0f, 1.0f, 1.0f, 0}, {right, 2, 30.0f, 1.0f, 1.0f, 1}};
    aibs_layer layer = adjustment_layer(&brighter);
    layer.mask_base = AIBS_MASK_NONE;
    layer.strokes = strokes;
    layer.stroke_count = 1;
    Image painted;
    CHECK(render(source, nullptr, &layer, 1, painted));
    if (painted.img.data) {
        CHECK(std::memcmp(painted.px(60, 50), global.px(60, 50), 4) == 0);    // on the line
        CHECK(std::memcmp(painted.px(60, 10), original.px(60, 10), 4) == 0);  // far from it
        CHECK(painted.green(60, 59) > original.green(60, 59));                // edge, inside
        CHECK(std::memcmp(painted.px(60, 63), original.px(60, 63), 4) == 0);  // edge, outside
    }
    layer.stroke_count = 2;
    Image erased;
    CHECK(render(source, nullptr, &layer, 1, erased));
    if (erased.img.data) {
        CHECK(std::memcmp(erased.px(40, 50), global.px(40, 50), 4) == 0);
        CHECK(std::memcmp(erased.px(120, 50), original.px(120, 50), 4) == 0);
    }
    // A soft brush fades out gradually.
    strokes[0].hardness = 0.0f;
    layer.stroke_count = 1;
    Image soft;
    CHECK(render(source, nullptr, &layer, 1, soft));
    if (soft.img.data) {
        const int centre = soft.green(60, 50) - original.green(60, 50);
        const int edge = soft.green(60, 57) - original.green(60, 57);
        std::printf("info: soft brush gain at centre %d, near the edge %d\n", centre, edge);
        CHECK(centre > edge && edge > 0);
    }

    // The same stroke at 4x zoom on a crop matches the whole render.
    strokes[0].hardness = 0.6f;
    Image whole, zoomed;
    CHECK(render(source, nullptr, &layer, 1, whole));
    CHECK(aibs_source_render(source.ptr, 50, 40, 20, 20, 80, 80, nullptr, nullptr, nullptr, nullptr, &layer, 1, 0,
                             &zoomed.img) == AIBS_OK);
    if (whole.img.data && zoomed.img.data) {
        // Away from the stroke's edge the values agree; on the edge a pixel
        // of the whole render averages over its area while the zoomed view
        // resolves the edge, so they differ a little more there.
        int worst = 0;
        double total = 0;
        for (int y = 0; y < 20; ++y)
            for (int x = 0; x < 20; ++x) {
                // Output pixels 4x+1 and 4x+2 straddle the centre of source pixel x.
                const int z = (zoomed.green(x * 4 + 1, y * 4 + 1) + zoomed.green(x * 4 + 2, y * 4 + 1) +
                               zoomed.green(x * 4 + 1, y * 4 + 2) + zoomed.green(x * 4 + 2, y * 4 + 2) + 2) / 4;
                const int d = std::abs(z - whole.green(50 + x, 40 + y));
                worst = std::max(worst, d);
                total += d;
            }
        std::printf("info: stroke at 4x zoom vs whole render, largest difference %d, mean %.2f\n", worst,
                    total / 400.0);
        CHECK(worst <= 16);
        CHECK(total / 400.0 <= 2.5);  // the sampled area is mostly stroke edges
    }
    // Tiles of the same view meet exactly.
    Image top, bottom;
    CHECK(aibs_source_render(source.ptr, 0, 0, kW, 50, kW, 50, nullptr, nullptr, nullptr, nullptr, &layer, 1, 0,
                             &top.img) == AIBS_OK);
    CHECK(aibs_source_render(source.ptr, 0, 50, kW, 50, kW, 50, nullptr, nullptr, nullptr, nullptr, &layer, 1, 0,
                             &bottom.img) == AIBS_OK);
    if (top.img.data && bottom.img.data && whole.img.data) {
        bool exact = std::memcmp(top.img.data, whole.img.data, static_cast<size_t>(kW) * 50 * 4) == 0 &&
                     std::memcmp(bottom.img.data, whole.img.data + static_cast<size_t>(kW) * 50 * 4,
                                 static_cast<size_t>(kW) * 50 * 4) == 0;
        CHECK(exact);
    }
}

void test_blend_modes_and_image_layers() {
    for (const auto& [mode, b, t, expected] : std::vector<std::tuple<uint32_t, float, float, float>>{
             {AIBS_BLEND_NORMAL, 0.2f, 0.7f, 0.7f},
             {AIBS_BLEND_MULTIPLY, 0.5f, 0.5f, 0.25f},
             {AIBS_BLEND_SCREEN, 0.5f, 0.5f, 0.75f},
             {AIBS_BLEND_OVERLAY, 0.25f, 0.5f, 0.25f},
             {AIBS_BLEND_OVERLAY, 0.75f, 0.5f, 0.75f},
             {AIBS_BLEND_SOFT_LIGHT, 0.3f, 0.5f, 0.3f},
             {AIBS_BLEND_DARKEN, 0.4f, 0.6f, 0.4f},
             {AIBS_BLEND_LIGHTEN, 0.4f, 0.6f, 0.6f},
             {AIBS_BLEND_DIFFERENCE, 0.9f, 0.3f, 0.6f},
         }) {
        CHECK(std::abs(aibs::blend_value(mode, b, t) - expected) < 1e-5f);
    }

    Source source, grey;
    CHECK(open_mat(ramp(), source));
    CHECK(open_mat(cv::Mat(10, 10, CV_8UC3, cv::Scalar(128, 128, 128)), grey));
    Image original;
    CHECK(render(source, nullptr, nullptr, 0, original));
    aibs_layer layer{};
    layer.kind = AIBS_LAYER_IMAGE;
    layer.opacity = 1.0f;
    layer.mask_base = AIBS_MASK_ALL;
    layer.image = grey.ptr;
    Image normal, multiply, screen;
    layer.blend = AIBS_BLEND_NORMAL;
    CHECK(render(source, nullptr, &layer, 1, normal));
    layer.blend = AIBS_BLEND_MULTIPLY;
    CHECK(render(source, nullptr, &layer, 1, multiply));
    layer.blend = AIBS_BLEND_SCREEN;
    CHECK(render(source, nullptr, &layer, 1, screen));
    if (normal.img.data && multiply.img.data && screen.img.data && original.img.data) {
        CHECK(normal.green(20, 20) == 128 && normal.green(150, 80) == 128);
        CHECK(multiply.green(100, 50) < original.green(100, 50));
        CHECK(screen.green(100, 50) > original.green(100, 50));
    }
    layer.image = nullptr;
    Image refused;
    CHECK(!render(source, nullptr, &layer, 1, refused));

    // Subject and background bases follow the matte.
    std::vector<uint8_t> values(static_cast<size_t>(kW) * kH);
    for (int y = 0; y < kH; ++y)
        for (int x = 0; x < kW; ++x) values[static_cast<size_t>(y) * kW + x] = x < kW / 2 ? 255 : 0;
    Matte matte;
    CHECK(aibs_matte_create(values.data(), kW, kH, kW, kH, &matte.ptr) == AIBS_OK);
    aibs_adjustments darker{};
    darker.exposure = -1.0f;
    aibs_layer subject = adjustment_layer(&darker);
    subject.mask_base = AIBS_MASK_SUBJECT;
    subject.matte = matte.ptr;
    Image on_subject;
    CHECK(render(source, nullptr, &subject, 1, on_subject));
    if (on_subject.img.data) {
        CHECK(on_subject.green(20, 50) < original.green(20, 50));
        CHECK(std::memcmp(on_subject.px(140, 50), original.px(140, 50), 4) == 0);
    }
    subject.mask_base = AIBS_MASK_BACKGROUND;
    Image on_background;
    CHECK(render(source, nullptr, &subject, 1, on_background));
    if (on_background.img.data) {
        CHECK(std::memcmp(on_background.px(20, 50), original.px(20, 50), 4) == 0);
        CHECK(on_background.green(140, 50) < original.green(140, 50));
    }
    // Layers stack: a darkening layer above a brightening one.
    aibs_adjustments brighter{};
    brighter.exposure = 1.0f;
    aibs_layer both[2] = {adjustment_layer(&brighter), adjustment_layer(&darker)};
    Image stacked;
    CHECK(render(source, nullptr, both, 2, stacked));
    if (stacked.img.data) CHECK(std::abs(stacked.green(80, 50) - original.green(80, 50)) <= 3);
}

}  // namespace

void run_layer_tests() {
    test_adjustment_layers();
    test_strokes();
    test_blend_modes_and_image_layers();
}

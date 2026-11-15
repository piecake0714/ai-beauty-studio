// Tests for lighting: flat and shaded (with a hand-made dome shape), light
// types, warmth, subject-only, the before/after split, and 3D shape
// estimation with the real depth model on a drawn ball (no photographs).
#include "aibs/engine.h"
#include "check.h"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>

#include <opencv2/core.hpp>
#include <opencv2/imgcodecs.hpp>
#include <opencv2/imgproc.hpp>

namespace {

struct Source {
    aibs_source* ptr = nullptr;
    ~Source() { aibs_source_free(ptr); }
};

struct Shape {
    aibs_shape* ptr = nullptr;
    ~Shape() { aibs_shape_free(ptr); }
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

bool render(const Source& source, int w, int h, const aibs_light* light, Image& out, uint32_t split = 0) {
    return aibs_source_render(source.ptr, 0, 0, w, h, static_cast<uint32_t>(w), static_cast<uint32_t>(h), nullptr,
                              nullptr, nullptr, light, nullptr, 0, split, &out.img) == AIBS_OK;
}

aibs_light make_light(uint32_t type, float x, float y) {
    aibs_light light{};
    light.type = type;
    light.x = x;
    light.y = y;
    light.intensity = 80;
    light.size = 40;
    light.height = 40;
    return light;
}

void test_flat_light() {
    const int w = 200, h = 100;
    Source source;
    CHECK(open_mat(cv::Mat(h, w, CV_8UC3, cv::Scalar(110, 110, 110)), source));
    Image original;
    CHECK(render(source, w, h, nullptr, original));
    if (!original.img.data) return;

    aibs_light light = make_light(AIBS_LIGHT_SOFTBOX, 0.1f, 0.5f);
    light.intensity = 0;
    Image off;
    CHECK(render(source, w, h, &light, off));
    CHECK(off.img.data && std::memcmp(off.img.data, original.img.data, w * h * 4) == 0);
    light.type = AIBS_LIGHT_NONE;
    light.intensity = 80;
    CHECK(render(source, w, h, &light, off));
    CHECK(off.img.data && std::memcmp(off.img.data, original.img.data, w * h * 4) == 0);

    light.type = AIBS_LIGHT_SOFTBOX;
    Image lit;
    CHECK(render(source, w, h, &light, lit));
    if (lit.img.data) {
        std::printf("info: softbox near %d, far %d (was %d)\n", lit.green(20, 50), lit.green(195, 50),
                    original.green(20, 50));
        CHECK(lit.green(20, 50) > original.green(20, 50) + 15);
        CHECK(std::abs(lit.green(195, 50) - original.green(195, 50)) <= 1);
    }
    // Warm light adds more red than blue.
    light.warmth = 100;
    Image warm;
    CHECK(render(source, w, h, &light, warm));
    if (warm.img.data) CHECK(warm.px(20, 50)[0] > warm.px(20, 50)[2] + 5);
    light.warmth = 0;
    // Before/after split: the left part stays original.
    Image split;
    CHECK(render(source, w, h, &light, split, 50));
    if (split.img.data) {
        CHECK(std::memcmp(split.px(20, 50), original.px(20, 50), 4) == 0);
        CHECK(split.green(60, 50) > original.green(60, 50));
    }
    // The same light at 2x zoom on a crop matches.
    Image zoomed;
    CHECK(aibs_source_render(source.ptr, 10, 40, 20, 20, 40, 40, nullptr, nullptr, nullptr, &light, nullptr, 0, 0, &zoomed.img) ==
          AIBS_OK);
    if (zoomed.img.data && lit.img.data) CHECK(std::abs(zoomed.green(21, 21) - lit.green(20, 50)) <= 2);
}

void test_reflector_fills_shadows() {
    // Left half dark, right half bright; a reflector at the centre.
    const int w = 200, h = 100;
    cv::Mat m(h, w, CV_8UC3, cv::Scalar(40, 40, 40));
    m(cv::Rect(100, 0, 100, 100)).setTo(cv::Scalar(200, 200, 200));
    Source source;
    CHECK(open_mat(m, source));
    Image original, lit;
    CHECK(render(source, w, h, nullptr, original));
    aibs_light light = make_light(AIBS_LIGHT_REFLECTOR, 0.5f, 0.5f);
    CHECK(render(source, w, h, &light, lit));
    if (original.img.data && lit.img.data) {
        const double dark_gain = lit.green(90, 50) / static_cast<double>(original.green(90, 50));
        const double bright_gain = lit.green(110, 50) / static_cast<double>(original.green(110, 50));
        std::printf("info: reflector dark x%.2f, bright x%.2f\n", dark_gain, bright_gain);
        CHECK(dark_gain > bright_gain + 0.1);
        CHECK(bright_gain >= 1.0);
    }
}

// A dome (hemisphere) shape on a flat grey image: lit from the left, its
// left side gets brighter and its right side darker.
void test_shading_follows_shape() {
    const int w = 200, h = 200;
    Source source;
    CHECK(open_mat(cv::Mat(h, w, CV_8UC3, cv::Scalar(120, 120, 120)), source));
    std::vector<float> depth(static_cast<size_t>(w) * h);
    for (int y = 0; y < h; ++y) {
        for (int x = 0; x < w; ++x) {
            const double r = std::hypot(x + 0.5 - 100.0, y + 0.5 - 100.0) / 70.0;
            depth[static_cast<size_t>(y) * w + x] = static_cast<float>(r < 1.0 ? std::sqrt(1.0 - r * r) : 0.0);
        }
    }
    Shape shape;
    CHECK(aibs_shape_create(depth.data(), w, h, w, h, &shape.ptr) == AIBS_OK);
    aibs_light light = make_light(AIBS_LIGHT_SOFTBOX, -0.2f, 0.5f);
    light.size = 100;
    light.height = 20;
    Image flat, shaded, original;
    CHECK(render(source, w, h, nullptr, original));
    CHECK(render(source, w, h, &light, flat));
    light.shape = shape.ptr;
    CHECK(render(source, w, h, &light, shaded));
    if (flat.img.data && shaded.img.data && original.img.data) {
        const int left = shaded.green(50, 100), right = shaded.green(150, 100);
        std::printf("info: dome lit from the left: left side %d, right side %d (flat light %d / %d, original %d)\n",
                    left, right, flat.green(50, 100), flat.green(150, 100), original.green(150, 100));
        CHECK(left > right + 20);
        CHECK(right < original.green(150, 100));     // the side facing away gets darker
        CHECK(left > flat.green(50, 100));           // the side facing the light more than flat
    }
    aibs_shape* bad = nullptr;
    CHECK(aibs_shape_create(depth.data(), 1, 1, w, h, &bad) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(bad == nullptr);
}

// A flat-topped disc standing out from the background (a depth jump, as at
// a person's outline): a strong, wide light must not draw a bright or dark
// rim at the jump.
void test_no_rim_at_depth_jumps() {
    const int w = 240, h = 200;
    Source source;
    CHECK(open_mat(cv::Mat(h, w, CV_8UC3, cv::Scalar(120, 120, 120)), source));
    std::vector<float> depth(static_cast<size_t>(w) * h);
    for (int y = 0; y < h; ++y)
        for (int x = 0; x < w; ++x)
            depth[static_cast<size_t>(y) * w + x] = std::hypot(x + 0.5 - 120.0, y + 0.5 - 100.0) < 60.0 ? 1.0f : 0.0f;
    Shape shape;
    CHECK(aibs_shape_create(depth.data(), w, h, w, h, &shape.ptr) == AIBS_OK);
    aibs_light light = make_light(AIBS_LIGHT_SOFTBOX, -0.3f, 0.5f);
    light.intensity = 100;
    light.size = 100;
    light.height = 20;
    light.shape = shape.ptr;
    Image lit;
    CHECK(render(source, w, h, &light, lit));
    if (!lit.img.data) return;
    // Along the middle row, brightness should change smoothly across both
    // edges of the disc (x = 60 and x = 180): no pixel far outside the
    // range of its neighbours 12 pixels away.
    int worst = 0;
    for (const int edge : {60, 180}) {
        for (int x = edge - 6; x <= edge + 6; ++x) {
            const int v = lit.green(x, 100);
            const int a = lit.green(x - 12, 100), b = lit.green(x + 12, 100);
            worst = std::max(worst, std::max(v - std::max(a, b), std::min(a, b) - v));
        }
    }
    std::printf("info: depth jump, largest rim beyond the neighbours: %d\n", worst);
    CHECK(worst <= 3);
}

void test_subject_only() {
    const int w = 120, h = 80;
    Source source;
    CHECK(open_mat(cv::Mat(h, w, CV_8UC3, cv::Scalar(100, 100, 100)), source));
    std::vector<uint8_t> values(static_cast<size_t>(w) * h);
    for (int y = 0; y < h; ++y)
        for (int x = 0; x < w; ++x) values[static_cast<size_t>(y) * w + x] = x < 60 ? 255 : 0;
    Matte matte;
    CHECK(aibs_matte_create(values.data(), w, h, w, h, &matte.ptr) == AIBS_OK);
    aibs_light light = make_light(AIBS_LIGHT_SOFTBOX, 0.5f, 0.5f);
    light.subject = matte.ptr;
    Image original, lit;
    CHECK(render(source, w, h, nullptr, original));
    CHECK(render(source, w, h, &light, lit));
    if (original.img.data && lit.img.data) {
        CHECK(lit.green(55, 40) > original.green(55, 40));
        CHECK(std::memcmp(lit.px(65, 40), original.px(65, 40), 4) == 0);
    }
}

void test_shape_estimation() {
    const std::string path = std::string(AIBS_MODEL_DIR) + "/depth-anything-v2-small.onnx";
    std::ifstream file(path, std::ios::binary);
    if (!file) {
        std::printf("FAILED: depth model missing at %s (run scripts/fetch_models.py)\n", path.c_str());
        ++g_failures;
        return;
    }
    const std::vector<uint8_t> model((std::istreambuf_iterator<char>(file)), std::istreambuf_iterator<char>());
    aibs_ai_session* session = nullptr;
    int32_t used = -1;
    CHECK(aibs_ai_open(model.data(), model.size(), AIBS_PROVIDER_CPU, &session, &used, nullptr, 0) == AIBS_OK);
    if (session == nullptr) return;

    // A drawn ball, shaded from the top so it reads as round, on a floor.
    const int w = 640, h = 480;
    cv::Mat scene(h, w, CV_8UC3, cv::Scalar(170, 175, 180));
    for (int y = 0; y < h; ++y) {
        for (int x = 0; x < w; ++x) {
            const double dx = (x - 320.0) / 150.0, dy = (y - 240.0) / 150.0;
            const double r2 = dx * dx + dy * dy;
            if (r2 >= 1.0) continue;
            const double nz = std::sqrt(1.0 - r2);
            const double shade = std::clamp(0.15 + 0.85 * (-0.4 * dx - 0.5 * dy + 0.77 * nz), 0.05, 1.0);
            scene.at<cv::Vec3b>(y, x) = cv::Vec3b(static_cast<uint8_t>(60 * shade), static_cast<uint8_t>(140 * shade),
                                                  static_cast<uint8_t>(230 * shade));
        }
    }
    Source source;
    CHECK(open_mat(scene, source));
    Shape shape;
    CHECK(aibs_estimate_shape(source.ptr, session, &shape.ptr) == AIBS_OK);
    if (shape.ptr) {
        // Light from the right at the ball's height: its right edge should
        // gain more than its left edge.
        aibs_light light = make_light(AIBS_LIGHT_SOFTBOX, 1.2f, 0.5f);
        light.size = 100;
        light.height = 10;
        light.shape = shape.ptr;
        Image original, lit;
        CHECK(render(source, w, h, nullptr, original));
        CHECK(render(source, w, h, &light, lit));
        if (original.img.data && lit.img.data) {
            const double right = lit.green(440, 240) / std::max(1.0, double(original.green(440, 240)));
            const double left = lit.green(200, 240) / std::max(1.0, double(original.green(200, 240)));
            std::printf("info: drawn ball lit from the right with estimated shape: right edge x%.2f, left edge x%.2f\n",
                        right, left);
            CHECK(right > left + 0.1);
        }
    }
    aibs_shape* none = nullptr;
    CHECK(aibs_estimate_shape(source.ptr, nullptr, &none) == AIBS_ERR_INVALID_ARGUMENT);
    aibs_ai_close(session);
}

}  // namespace

void run_light_tests() {
    test_flat_light();
    test_reflector_fills_shadows();
    test_shading_follows_shape();
    test_no_rim_at_depth_jumps();
    test_subject_only();
    test_shape_estimation();
}

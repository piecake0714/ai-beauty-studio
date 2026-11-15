// Tests for background compositing (with hand-made mattes, pixel exact)
// and subject separation with the real model (on a drawn image; no
// photographs of people are used).
#include "aibs/engine.h"
#include "check.h"

#include <chrono>
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
};

bool open_mat(const cv::Mat& bgr, Source& source) {
    std::vector<uint8_t> png;
    if (!cv::imencode(".png", bgr, png)) return false;
    return aibs_source_open(png.data(), png.size(), &source.ptr) == AIBS_OK;
}

constexpr int kW = 120, kH = 80;

// Left half: a red subject. Right half: a textured blue-grey background.
cv::Mat test_scene() {
    cv::Mat m(kH, kW, CV_8UC3);
    for (int y = 0; y < kH; ++y) {
        for (int x = 0; x < kW; ++x) {
            if (x < kW / 2) {
                m.at<cv::Vec3b>(y, x) = cv::Vec3b(40, 40, 200);
            } else {
                const int v = ((x / 3 + y / 3) % 2) ? 40 : -40;
                m.at<cv::Vec3b>(y, x) = cv::Vec3b(static_cast<uint8_t>(150 + v), static_cast<uint8_t>(120 + v),
                                                  static_cast<uint8_t>(100 + v));
            }
        }
    }
    return m;
}

// Matte for the scene: left half subject, at half resolution.
bool half_matte(Matte& matte) {
    std::vector<uint8_t> values(static_cast<size_t>(kW / 2) * (kH / 2));
    for (int y = 0; y < kH / 2; ++y)
        for (int x = 0; x < kW / 2; ++x) values[static_cast<size_t>(y) * (kW / 2) + x] = x < kW / 4 ? 255 : 0;
    return aibs_matte_create(values.data(), kW / 2, kH / 2, kW, kH, &matte.ptr) == AIBS_OK;
}

bool render(const Source& source, const aibs_background* background, Image& out, uint32_t split = 0) {
    return aibs_source_render(source.ptr, 0, 0, kW, kH, kW, kH, nullptr, nullptr, background, split, &out.img) ==
           AIBS_OK;
}

bool same_px(const uint8_t* a, const uint8_t* b) {
    return std::memcmp(a, b, 4) == 0;
}

void test_matte_creation() {
    Matte matte;
    CHECK(half_matte(matte));
    CHECK(std::abs(aibs_matte_coverage(matte.ptr) - 0.5f) < 1e-3f);
    aibs_matte* bad = nullptr;
    const uint8_t one = 255;
    CHECK(aibs_matte_create(nullptr, 1, 1, 1, 1, &bad) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(aibs_matte_create(&one, 0, 1, 1, 1, &bad) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(aibs_matte_create(&one, 1, 1, 0, 1, &bad) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(bad == nullptr);
    CHECK(aibs_matte_coverage(nullptr) == 0.0f);
    aibs_matte_free(nullptr);
}

void test_compositing() {
    Source source;
    Matte matte;
    CHECK(open_mat(test_scene(), source));
    CHECK(half_matte(matte));
    Image original;
    CHECK(render(source, nullptr, original));
    if (!original.img.data) return;

    // Columns well inside the subject and the background (the matte edge,
    // at x = 60, is interpolated).
    const int subject_x[] = {5, 30, 55}, background_x[] = {65, 90, 115};

    aibs_background bg{};
    bg.matte = matte.ptr;

    // "Original" with nothing changed is exactly the original.
    {
        Image out;
        CHECK(render(source, &bg, out));
        CHECK(out.img.data && std::memcmp(out.img.data, original.img.data, kW * kH * 4) == 0);
    }
    // Solid colour: subject untouched, background exactly the colour.
    {
        bg.mode = AIBS_BG_COLOR;
        bg.color[0] = 10;
        bg.color[1] = 200;
        bg.color[2] = 30;
        Image out;
        CHECK(render(source, &bg, out));
        if (out.img.data) {
            for (int y : {0, 40, 79}) {
                for (int x : subject_x) CHECK(same_px(out.px(x, y), original.px(x, y)));
                for (int x : background_x) {
                    const uint8_t* p = out.px(x, y);
                    CHECK(p[0] == 10 && p[1] == 200 && p[2] == 30 && p[3] == 255);
                }
            }
        }
        // The same at 3x zoom on part of the image.
        Image zoomed;
        CHECK(aibs_source_render(source.ptr, 50, 20, 20, 10, 60, 30, nullptr, nullptr, &bg, 0, &zoomed.img) == AIBS_OK);
        if (zoomed.img.data) {
            CHECK(same_px(zoomed.px(3, 15), original.px(51, 25)));
            const uint8_t* p = zoomed.px(57, 15);
            CHECK(p[0] == 10 && p[1] == 200 && p[2] == 30);
        }
        // Brightness does not alter a chosen colour.
        bg.brightness = -80;
        Image dark;
        CHECK(render(source, &bg, dark));
        if (dark.img.data) CHECK(dark.px(90, 40)[1] == 200);
        bg.brightness = 0;
    }
    // Transparent: colours kept, alpha follows the matte.
    {
        bg.mode = AIBS_BG_TRANSPARENT;
        Image out;
        CHECK(render(source, &bg, out));
        if (out.img.data) {
            for (int x : subject_x) CHECK(out.px(x, 40)[3] == 255);
            for (int x : background_x) {
                CHECK(out.px(x, 40)[3] == 0);
                CHECK(std::memcmp(out.px(x, 40), original.px(x, 40), 3) == 0);
            }
        }
        // Left of the before/after split stays opaque and original.
        Image split;
        CHECK(render(source, &bg, split, 100));
        if (split.img.data) {
            CHECK(same_px(split.px(90, 40), original.px(90, 40)));
            CHECK(split.px(110, 40)[3] == 0);
        }
    }
    // Blur: subject untouched, background smoother, no red bleeding in.
    {
        bg.mode = AIBS_BG_BLUR;
        bg.blur = 100;
        Image out;
        CHECK(render(source, &bg, out));
        if (out.img.data) {
            for (int x : subject_x) CHECK(same_px(out.px(x, 40), original.px(x, 40)));
            double before = 0, after = 0;
            for (int x = 70; x < 110; ++x) {
                before += std::abs(original.px(x, 40)[1] - original.px(x + 1, 40)[1]);
                after += std::abs(out.px(x, 40)[1] - out.px(x + 1, 40)[1]);
            }
            std::printf("info: background blur, neighbour differences %.0f -> %.0f\n", before, after);
            CHECK(after < 0.25 * before);
            // Next to the subject the background must not turn red.
            const uint8_t* near = out.px(64, 40);
            CHECK(near[0] < near[2] + 40);
        }
    }
    // Background brightness and saturation leave the subject alone.
    {
        bg.mode = AIBS_BG_ORIGINAL;
        bg.brightness = -100;
        bg.saturation = -100;
        Image out;
        CHECK(render(source, &bg, out));
        if (out.img.data) {
            for (int x : subject_x) CHECK(same_px(out.px(x, 40), original.px(x, 40)));
            const uint8_t* p = out.px(90, 40);
            const uint8_t* o = original.px(90, 40);
            CHECK(std::abs(p[0] - p[1]) <= 1 && std::abs(p[1] - p[2]) <= 1);  // grey
            CHECK(p[1] < o[1]);                   // darker
        }
        bg.brightness = 0;
        bg.saturation = 0;
    }
    // Another image as background, scaled to cover.
    {
        Source yellow;
        CHECK(open_mat(cv::Mat(30, 30, CV_8UC3, cv::Scalar(0, 220, 240)), yellow));
        bg.mode = AIBS_BG_IMAGE;
        bg.image = yellow.ptr;
        Image out;
        CHECK(render(source, &bg, out));
        if (out.img.data) {
            const uint8_t* p = out.px(100, 10);
            CHECK(p[0] == 240 && p[1] == 220 && p[2] == 0);
            CHECK(same_px(out.px(20, 10), original.px(20, 10)));
        }
        bg.image = nullptr;
        Image refused;
        CHECK(render(source, &bg, refused) == false);
    }
}

double iou(const cv::Mat& a, const cv::Mat& b) {
    cv::Mat inter, uni;
    cv::bitwise_and(a, b, inter);
    cv::bitwise_or(a, b, uni);
    const double u = cv::countNonZero(uni);
    return u > 0 ? cv::countNonZero(inter) / u : 0.0;
}

void test_segmentation() {
    const std::string path = std::string(AIBS_MODEL_DIR) + "/BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx";
    std::ifstream file(path, std::ios::binary);
    if (!file) {
        std::printf("FAILED: background model missing at %s (run scripts/fetch_models.py)\n", path.c_str());
        ++g_failures;
        return;
    }
    const std::vector<uint8_t> model((std::istreambuf_iterator<char>(file)), std::istreambuf_iterator<char>());
    aibs_ai_session* session = nullptr;
    int32_t used = -1;
    CHECK(aibs_ai_open(model.data(), model.size(), AIBS_PROVIDER_CPU, &session, &used, nullptr, 0) == AIBS_OK);
    if (session == nullptr) return;

    // A drawn object: a shaded orange ball on a plain light background.
    const int w = 800, h = 600;
    cv::Mat scene(h, w, CV_8UC3, cv::Scalar(235, 235, 230));
    cv::Mat truth = cv::Mat::zeros(h, w, CV_8U);
    cv::circle(truth, cv::Point(400, 300), 170, cv::Scalar(255), cv::FILLED);
    for (int y = 0; y < h; ++y) {
        for (int x = 0; x < w; ++x) {
            if (!truth.at<uint8_t>(y, x)) continue;
            const double shade = 1.0 - 0.5 * std::hypot(x - 340.0, y - 240.0) / 340.0;
            scene.at<cv::Vec3b>(y, x) = cv::Vec3b(static_cast<uint8_t>(30 * shade), static_cast<uint8_t>(120 * shade),
                                                  static_cast<uint8_t>(240 * shade));
        }
    }
    Source source;
    CHECK(open_mat(scene, source));
    Matte matte;
    const auto started = std::chrono::steady_clock::now();
    CHECK(aibs_segment(source.ptr, session, &matte.ptr) == AIBS_OK);
    const double ms =
        std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - started).count();
    if (matte.ptr) {
        aibs_background bg{};
        bg.matte = matte.ptr;
        bg.mode = AIBS_BG_TRANSPARENT;
        Image out;
        CHECK(aibs_source_render(source.ptr, 0, 0, w, h, w, h, nullptr, nullptr, &bg, 0, &out.img) == AIBS_OK);
        if (out.img.data) {
            cv::Mat found(h, w, CV_8U);
            for (int y = 0; y < h; ++y)
                for (int x = 0; x < w; ++x) found.at<uint8_t>(y, x) = out.px(x, y)[3] > 127 ? 255 : 0;
            const double score = iou(found, truth);
            std::printf("info: segmentation on CPU %.0f ms, coverage %.3f (drawn ball %.3f), IoU %.3f\n", ms,
                        aibs_matte_coverage(matte.ptr), cv::countNonZero(truth) / double(w * h), score);
            CHECK(score > 0.8);
        }
    }
    aibs_matte* none = nullptr;
    CHECK(aibs_segment(source.ptr, nullptr, &none) == AIBS_ERR_INVALID_ARGUMENT);
    aibs_ai_close(session);
}

}  // namespace

void run_background_tests() {
    test_matte_creation();
    test_compositing();
    test_segmentation();
}

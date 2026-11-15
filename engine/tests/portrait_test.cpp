// Tests for face-detection decoding, skin measurement, skin smoothing and
// one-click enhancement. No photographs of real people are used: decoding
// is checked with hand-made model outputs and retouching with a drawn,
// face-like test image.
#include "aibs/engine.h"
#include "check.h"
#include "portrait.h"

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

// ---- YuNet decoding ---------------------------------------------------

struct FakeOutputs {
    std::vector<std::vector<float>> storage;
    std::vector<aibs::DetectorOutput> outputs;

    FakeOutputs() {
        for (const int stride : {8, 16, 32}) {
            const size_t cells = static_cast<size_t>(640 / stride) * static_cast<size_t>(640 / stride);
            for (const auto& [name, per_cell] : {std::pair<const char*, size_t>{"cls", 1}, {"obj", 1}, {"bbox", 4},
                                                 {"kps", 10}}) {
                storage.emplace_back(cells * per_cell, 0.0f);
                outputs.push_back({std::string(name) + "_" + std::to_string(stride), {1, static_cast<int64_t>(cells),
                                   static_cast<int64_t>(per_cell)}, nullptr, cells * per_cell});
            }
        }
        for (size_t i = 0; i < outputs.size(); ++i) outputs[i].data = storage[i].data();
    }

    float* get(const std::string& name) {
        for (size_t i = 0; i < outputs.size(); ++i) {
            if (outputs[i].name == name) return storage[i].data();
        }
        return nullptr;
    }

    void face(int stride, int row, int col, float cls, float obj, float dx, float dy, float w_cells, float h_cells) {
        const size_t i = static_cast<size_t>(row) * static_cast<size_t>(640 / stride) + static_cast<size_t>(col);
        const std::string s = "_" + std::to_string(stride);
        get("cls" + s)[i] = cls;
        get("obj" + s)[i] = obj;
        float* bbox = get("bbox" + s) + i * 4;
        bbox[0] = dx;
        bbox[1] = dy;
        bbox[2] = std::log(w_cells);
        bbox[3] = std::log(h_cells);
        float* kps = get("kps" + s) + i * 10;
        for (int k = 0; k < 10; ++k) kps[k] = 0.25f * static_cast<float>(k % 2 == 0 ? k / 2 : k / 2 + 1);
    }
};

void test_yunet_decoding() {
    FakeOutputs fake;
    // One confident face at stride 16, cell (row 10, col 12).
    fake.face(16, 10, 12, 0.9f, 0.9f, 0.5f, 0.5f, 4.0f, 5.0f);
    // A weaker detection of the same face at stride 8.
    fake.face(8, 21, 25, 0.8f, 0.7f, 0.0f, 0.0f, 8.0f, 10.0f);
    // A weak, separate detection below the threshold.
    fake.face(32, 2, 2, 0.3f, 0.3f, 0.0f, 0.0f, 1.0f, 1.0f);

    std::vector<aibs::Detection> faces;
    CHECK(aibs::decode_yunet(fake.outputs, 640, 640, 0.6f, faces));
    CHECK(faces.size() == 2);
    faces = aibs::remove_overlaps(faces, 0.3f);
    CHECK(faces.size() == 1);
    if (!faces.empty()) {
        const aibs::Detection& f = faces[0];
        CHECK(std::abs(f.score - 0.9f) < 1e-5f);
        // Centre (12.5, 10.5) cells of 16 px; size 4 x 5 cells.
        CHECK(std::abs(f.x - (200 - 32)) < 1e-3f && std::abs(f.y - (168 - 40)) < 1e-3f);
        CHECK(std::abs(f.width - 64) < 1e-3f && std::abs(f.height - 80) < 1e-3f);
        // First landmark: offset (0, 0.25) cells from the cell corner.
        CHECK(std::abs(f.landmarks[0] - 192) < 1e-3f && std::abs(f.landmarks[1] - 164) < 1e-3f);
    }

    // A missing output means a different model: refuse.
    FakeOutputs broken;
    broken.outputs.pop_back();
    CHECK(!aibs::decode_yunet(broken.outputs, 640, 640, 0.6f, faces));
}

// ---- A drawn, face-like test image ---------------------------------------

// Deterministic pseudo-random value in [-1, 1].
double noise(int x, int y, int seed) {
    uint32_t h = static_cast<uint32_t>(x) * 374761393u + static_cast<uint32_t>(y) * 668265263u +
                 static_cast<uint32_t>(seed) * 2246822519u;
    h = (h ^ (h >> 13)) * 1274126177u;
    return static_cast<double>(h ^ (h >> 16)) / 2147483647.5 - 1.0;
}

struct TestFace {
    cv::Mat bgr;
    aibs_face face{};
};

// 400 x 420: grey background, a skin-coloured oval with blotches (what
// smoothing should even out) and fine grain (texture it should keep),
// dark eyes and a darker mouth.
TestFace make_test_face() {
    TestFace t;
    t.bgr = cv::Mat(420, 400, CV_8UC3, cv::Scalar(70, 70, 70));
    cv::Mat oval = cv::Mat::zeros(t.bgr.size(), CV_8U);
    cv::ellipse(oval, cv::Point(200, 210), cv::Size(110, 140), 0, 0, 360, cv::Scalar(255), cv::FILLED);
    for (int y = 0; y < t.bgr.rows; ++y) {
        for (int x = 0; x < t.bgr.cols; ++x) {
            if (!oval.at<uint8_t>(y, x)) continue;
            // Blotches about 8 px across, plus 1-px grain.
            const double blotch = 14 * noise(x / 8, y / 8, 1);
            const double grain = 5 * noise(x, y, 2);
            const double v = blotch + grain;
            t.bgr.at<cv::Vec3b>(y, x) = cv::Vec3b(cv::saturate_cast<uint8_t>(140 + v), cv::saturate_cast<uint8_t>(165 + v),
                                                  cv::saturate_cast<uint8_t>(205 + v));
        }
    }
    cv::circle(t.bgr, cv::Point(160, 180), 12, cv::Scalar(40, 30, 30), cv::FILLED);
    cv::circle(t.bgr, cv::Point(240, 180), 12, cv::Scalar(40, 30, 30), cv::FILLED);
    cv::ellipse(t.bgr, cv::Point(200, 285), cv::Size(30, 9), 0, 0, 360, cv::Scalar(90, 80, 160), cv::FILLED);

    aibs_face& f = t.face;
    f.x = 100;
    f.y = 85;
    f.width = 200;
    f.height = 250;
    const float landmarks[10] = {160, 180, 240, 180, 200, 230, 172, 285, 228, 285};
    std::memcpy(f.landmarks, landmarks, sizeof(landmarks));
    f.score = 0.95f;
    return t;
}

double stddev_of(const Image& image, int x0, int y0, int x1, int y1, int channel) {
    double sum = 0, sum_sq = 0, n = 0;
    for (int y = y0; y < y1; ++y) {
        for (int x = x0; x < x1; ++x) {
            const double v = image.px(x, y)[channel];
            sum += v;
            sum_sq += v * v;
            n += 1;
        }
    }
    const double mean = sum / n;
    return std::sqrt(std::max(0.0, sum_sq / n - mean * mean));
}

// Energy of 1-px detail: mean absolute difference from the 3x3 average.
double grain_of(const Image& image, int x0, int y0, int x1, int y1) {
    double total = 0, n = 0;
    for (int y = y0; y < y1; ++y) {
        for (int x = x0; x < x1; ++x) {
            double avg = 0;
            for (int dy = -1; dy <= 1; ++dy)
                for (int dx = -1; dx <= 1; ++dx) avg += image.px(x + dx, y + dy)[1];
            total += std::abs(image.px(x, y)[1] - avg / 9);
            n += 1;
        }
    }
    return total / n;
}

void test_skin_measurement() {
    TestFace t = make_test_face();
    Source source;
    CHECK(open_mat(t.bgr, source));
    CHECK(aibs_measure_skin(source.ptr, &t.face) == AIBS_OK);
    // Expected skin colour of BGR (140, 165, 205) in YCbCr.
    cv::Mat one(1, 1, CV_8UC3, cv::Scalar(140, 165, 205)), ycrcb;
    cv::cvtColor(one, ycrcb, cv::COLOR_BGR2YCrCb);
    const cv::Vec3b e = ycrcb.at<cv::Vec3b>(0, 0);
    CHECK(std::abs(t.face.skin[0] - e[2]) < 3 && std::abs(t.face.skin[1] - e[1]) < 3);
    CHECK(t.face.skin[2] >= 9 && t.face.skin[3] >= 9);

    aibs_face bad{};
    CHECK(aibs_measure_skin(source.ptr, &bad) == AIBS_ERR_INVALID_ARGUMENT);
}

void test_skin_smoothing() {
    TestFace t = make_test_face();
    Source source;
    CHECK(open_mat(t.bgr, source));
    CHECK(aibs_measure_skin(source.ptr, &t.face) == AIBS_OK);

    Image original, off, smoothed, split;
    const int w = t.bgr.cols, h = t.bgr.rows;
    CHECK(aibs_source_render(source.ptr, 0, 0, w, h, w, h, nullptr, nullptr, nullptr, nullptr, nullptr, 0, 0, &original.img) == AIBS_OK);
    aibs_portrait portrait{&t.face, 1, 0.0f};
    CHECK(aibs_source_render(source.ptr, 0, 0, w, h, w, h, &portrait, nullptr, nullptr, nullptr, nullptr, 0, 0, &off.img) == AIBS_OK);
    portrait.smoothing = 100.0f;
    CHECK(aibs_source_render(source.ptr, 0, 0, w, h, w, h, &portrait, nullptr, nullptr, nullptr, nullptr, 0, 0, &smoothed.img) == AIBS_OK);
    if (!original.img.data || !off.img.data || !smoothed.img.data) return;

    const size_t bytes = static_cast<size_t>(w) * h * 4;
    CHECK(std::memcmp(original.img.data, off.img.data, bytes) == 0);  // strength 0 changes nothing

    // Outside the face oval (and its soft edge) nothing changes at all.
    bool outside_same = true;
    for (int y = 0; y < h && outside_same; ++y) {
        for (int x = 0; x < w; ++x) {
            const double ex = (x + 0.5 - 200) / (200 * 0.55), ey = (y + 0.5 - 210) / (250 * 0.62);
            if (ex * ex + ey * ey < 1.3 * 1.3) continue;
            if (std::memcmp(original.px(x, y), smoothed.px(x, y), 4) != 0) {
                outside_same = false;
                break;
            }
        }
    }
    CHECK(outside_same);

    // On the cheek: blotches are evened out, fine grain mostly kept.
    const int cx0 = 120, cx1 = 150, cy0 = 215, cy1 = 255;
    const double blotch_before = stddev_of(original, cx0, cy0, cx1, cy1, 1);
    const double blotch_after = stddev_of(smoothed, cx0, cy0, cx1, cy1, 1);
    const double grain_before = grain_of(original, cx0, cy0, cx1, cy1);
    const double grain_after = grain_of(smoothed, cx0, cy0, cx1, cy1);
    std::printf("info: cheek variation %.1f -> %.1f, fine grain %.2f -> %.2f\n", blotch_before, blotch_after,
                grain_before, grain_after);
    CHECK(blotch_after < 0.7 * blotch_before);
    CHECK(grain_after > 0.4 * grain_before);

    // The eyes are left alone.
    for (const cv::Point eye : {cv::Point(160, 180), cv::Point(240, 180)}) {
        CHECK(std::memcmp(original.px(eye.x, eye.y), smoothed.px(eye.x, eye.y), 4) == 0);
    }

    // Before/after split: left of the split stays original.
    CHECK(aibs_source_render(source.ptr, 0, 0, w, h, w, h, &portrait, nullptr, nullptr, nullptr, nullptr, 0, 200, &split.img) == AIBS_OK);
    if (split.img.data) {
        CHECK(std::memcmp(split.px(130, 230), original.px(130, 230), 4) == 0);
        CHECK(std::memcmp(split.px(260, 230), smoothed.px(260, 230), 4) == 0);
    }

    // The same face drawn at half size gets the same kind of smoothing:
    // the strength follows the face's size on screen.
    Image half;
    CHECK(aibs_source_render(source.ptr, 0, 0, w, h, w / 2, h / 2, &portrait, nullptr, nullptr, nullptr, nullptr, 0, 0, &half.img) == AIBS_OK);
}

void test_detection_without_faces() {
    const std::string path = std::string(AIBS_MODEL_DIR) + "/face_detection_yunet_2023mar.onnx";
    std::ifstream file(path, std::ios::binary);
    if (!file) {
        std::printf("FAILED: face model missing at %s (run scripts/fetch_models.py)\n", path.c_str());
        ++g_failures;
        return;
    }
    const std::vector<uint8_t> model((std::istreambuf_iterator<char>(file)), std::istreambuf_iterator<char>());
    aibs_ai_session* detector = nullptr;
    int32_t used = -1;
    CHECK(aibs_ai_open(model.data(), model.size(), AIBS_PROVIDER_CPU, &detector, &used, nullptr, 0) == AIBS_OK);

    TestFace drawn = make_test_face();  // a drawing, not a face the model knows
    cv::Mat empty(480, 640, CV_8UC3, cv::Scalar(120, 140, 160));
    for (const cv::Mat* image : {&empty}) {
        Source source;
        CHECK(open_mat(*image, source));
        aibs_face faces[8];
        uint32_t count = 99;
        CHECK(aibs_detect_faces(source.ptr, detector, 0.9f, faces, 8, &count) == AIBS_OK);
        CHECK(count == 0);
    }
    Source source;
    CHECK(open_mat(drawn.bgr, source));
    aibs_face faces[8];
    uint32_t count = 0;
    CHECK(aibs_detect_faces(source.ptr, detector, 0.9f, faces, 8, &count) == AIBS_OK);
    std::printf("info: drawn test face, %u face(s) detected\n", count);
    CHECK(aibs_detect_faces(source.ptr, nullptr, 0.9f, faces, 8, &count) == AIBS_ERR_INVALID_ARGUMENT);
    aibs_ai_close(detector);
}

// ---- One-click enhancement ----------------------------------------------

aibs_adjustments auto_for(const cv::Mat& bgr) {
    Source source;
    aibs_adjustments a{};
    CHECK(open_mat(bgr, source));
    CHECK(aibs_auto_adjust(source.ptr, nullptr, 0, &a) == AIBS_OK);
    return a;
}

cv::Mat ramp(int lo, int hi, cv::Scalar tint) {
    cv::Mat m(200, 256, CV_8UC3);
    for (int y = 0; y < m.rows; ++y) {
        for (int x = 0; x < m.cols; ++x) {
            const double v = lo + (hi - lo) * (x / 255.0);
            m.at<cv::Vec3b>(y, x) = cv::Vec3b(cv::saturate_cast<uint8_t>(v * tint[0]),
                                              cv::saturate_cast<uint8_t>(v * tint[1]),
                                              cv::saturate_cast<uint8_t>(v * tint[2]));
        }
    }
    return m;
}

void test_auto_adjust() {
    const aibs_adjustments dark = auto_for(ramp(5, 90, cv::Scalar(1, 1, 1)));
    const aibs_adjustments bright = auto_for(ramp(170, 252, cv::Scalar(1, 1, 1)));
    const aibs_adjustments normal = auto_for(ramp(10, 245, cv::Scalar(1, 1, 1)));
    const aibs_adjustments bluish = auto_for(ramp(60, 200, cv::Scalar(1.25, 1.0, 0.82)));
    std::printf("info: auto exposure dark %+.2f, bright %+.2f, normal %+.2f; bluish temperature %+.0f\n",
                dark.exposure, bright.exposure, normal.exposure, bluish.temperature);
    CHECK(dark.exposure > 0.3f);
    CHECK(bright.exposure < -0.2f);
    CHECK(std::abs(normal.exposure) <= 0.3f);
    CHECK(bluish.temperature > 5.0f);
    CHECK(std::abs(normal.temperature) < 3.0f);
    CHECK(dark.shadows > 0.0f);
    CHECK(bright.highlights < 0.0f);
    // Exposure lands on the slider's 0.05 steps.
    CHECK(std::abs(dark.exposure * 20 - std::round(dark.exposure * 20)) < 1e-4f);
    for (int i = 0; i < AIBS_HSL_BANDS; ++i) {
        CHECK(dark.hsl_hue[i] == 0 && dark.hsl_saturation[i] == 0 && dark.hsl_luminance[i] == 0);
    }

    // With a face, exposure aims at the skin rather than the whole frame.
    TestFace t = make_test_face();
    t.bgr *= 0.5;  // underexposed portrait
    Source source;
    CHECK(open_mat(t.bgr, source));
    CHECK(aibs_measure_skin(source.ptr, &t.face) == AIBS_OK);
    aibs_adjustments with_face{};
    CHECK(aibs_auto_adjust(source.ptr, &t.face, 1, &with_face) == AIBS_OK);
    CHECK(with_face.exposure > 0.3f);
    CHECK(aibs_auto_adjust(nullptr, nullptr, 0, &with_face) == AIBS_ERR_INVALID_ARGUMENT);
}


// ---- 1.3 portrait tools -----------------------------------------------

bool render_full(const Source& source, const cv::Mat& bgr, const aibs_portrait* portrait, Image& out) {
    return aibs_source_render(source.ptr, 0, 0, bgr.cols, bgr.rows, bgr.cols, bgr.rows, portrait, nullptr, nullptr,
                              nullptr, nullptr, 0, 0, &out.img) == AIBS_OK &&
           out.img.data != nullptr;
}

// Mean of channel `c` of the YCrCb version of the render over a box.
double mean_ycc(const Image& image, int x0, int y0, int x1, int y1, int c) {
    double sum = 0, n = 0;
    for (int y = y0; y < y1; ++y) {
        for (int x = x0; x < x1; ++x) {
            const uint8_t* p = image.px(x, y);
            cv::Mat one(1, 1, CV_8UC3, cv::Scalar(p[2], p[1], p[0])), ycc;
            cv::cvtColor(one, ycc, cv::COLOR_BGR2YCrCb);
            sum += ycc.at<cv::Vec3b>(0, 0)[c];
            n += 1;
        }
    }
    return sum / n;
}

bool same_outside(const Image& a, const Image& b, const std::vector<cv::Rect>& allowed) {
    for (int y = 0; y < static_cast<int>(a.img.height); ++y) {
        for (int x = 0; x < static_cast<int>(a.img.width); ++x) {
            bool inside = false;
            for (const cv::Rect& r : allowed) inside = inside || r.contains(cv::Point(x, y));
            if (!inside && std::memcmp(a.px(x, y), b.px(x, y), 4) != 0) return false;
        }
    }
    return true;
}

void test_face_tools() {
    TestFace t = make_test_face();
    // Darker, purplish shadows under both eyes (eye at y 180, face 250 high:
    // the area is centred about 27 px lower).
    for (const int ex : {160, 240}) {
        cv::ellipse(t.bgr, cv::Point(ex, 207), cv::Size(22, 9), 0, 0, 360, cv::Scalar(130, 120, 150), cv::FILLED);
    }
    // Yellowish teeth in the middle of the (red) mouth.
    cv::rectangle(t.bgr, cv::Rect(184, 281, 32, 6), cv::Scalar(150, 205, 210), cv::FILLED);
    // A blemish on the cheek.
    cv::circle(t.bgr, cv::Point(135, 240), 3, cv::Scalar(100, 115, 160), cv::FILLED);
    Source source;
    CHECK(open_mat(t.bgr, source));
    CHECK(aibs_measure_skin(source.ptr, &t.face) == AIBS_OK);

    Image original;
    CHECK(render_full(source, t.bgr, nullptr, original));
    if (!original.img.data) return;
    const std::vector<cv::Rect> face_area = {cv::Rect(80, 60, 240, 300)};

    // All off: exactly the original.
    {
        aibs_portrait off{&t.face, 1, 0.0f};
        Image out;
        CHECK(render_full(source, t.bgr, &off, out));
        CHECK(out.img.data && std::memcmp(out.img.data, original.img.data, t.bgr.total() * 4) == 0);
    }
    // Dark circles: the shadow lifts; nothing outside the face changes.
    {
        aibs_portrait p{&t.face, 1, 0.0f};
        p.dark_circles = 100.0f;
        Image out;
        CHECK(render_full(source, t.bgr, &p, out));
        if (out.img.data) {
            const double before = mean_ycc(original, 150, 204, 170, 210, 0);
            const double after = mean_ycc(out, 150, 204, 170, 210, 0);
            std::printf("info: under-eye brightness %.1f -> %.1f\n", before, after);
            CHECK(after > before + 8);
            CHECK(same_outside(original, out, face_area));
            // The forehead is not part of it.
            CHECK(std::memcmp(original.px(200, 120), out.px(200, 120), 4) == 0);
        }
    }
    // Eyes: more contrast around the eye, the cheek unchanged.
    {
        aibs_portrait p{&t.face, 1, 0.0f};
        p.eyes = 100.0f;
        Image out;
        CHECK(render_full(source, t.bgr, &p, out));
        if (out.img.data) {
            const double before = stddev_of(original, 140, 168, 180, 192, 1);
            const double after = stddev_of(out, 140, 168, 180, 192, 1);
            std::printf("info: eye contrast %.1f -> %.1f\n", before, after);
            CHECK(after > before * 1.05);
            CHECK(std::memcmp(original.px(130, 250), out.px(130, 250), 4) == 0);
            CHECK(same_outside(original, out, face_area));
        }
    }
    // Teeth: less yellow (Cb up) and brighter; the lips barely change.
    {
        aibs_portrait p{&t.face, 1, 0.0f};
        p.teeth = 100.0f;
        Image out;
        CHECK(render_full(source, t.bgr, &p, out));
        if (out.img.data) {
            const double cb_before = mean_ycc(original, 190, 282, 210, 286, 2);
            const double cb_after = mean_ycc(out, 190, 282, 210, 286, 2);
            const double y_before = mean_ycc(original, 190, 282, 210, 286, 0);
            const double y_after = mean_ycc(out, 190, 282, 210, 286, 0);
            std::printf("info: teeth Cb %.1f -> %.1f, Y %.1f -> %.1f\n", cb_before, cb_after, y_before, y_after);
            CHECK(cb_after > cb_before + 5);
            CHECK(y_after > y_before + 3);
            for (const cv::Point lip : {cv::Point(200, 292), cv::Point(176, 285)}) {
                const uint8_t* a = original.px(lip.x, lip.y);
                const uint8_t* b = out.px(lip.x, lip.y);
                for (int c = 0; c < 3; ++c) CHECK(std::abs(a[c] - b[c]) <= 3);
            }
            CHECK(same_outside(original, out, face_area));
        }
    }
    // Blemishes: the dark dot fades into the skin around it.
    {
        aibs_portrait p{&t.face, 1, 0.0f};
        p.blemishes = 100.0f;
        Image out;
        CHECK(render_full(source, t.bgr, &p, out));
        if (out.img.data) {
            const double skin = mean_ycc(original, 145, 230, 155, 250, 0);
            const double dot_before = mean_ycc(original, 134, 239, 137, 242, 0);
            const double dot_after = mean_ycc(out, 134, 239, 137, 242, 0);
            std::printf("info: blemish %.1f -> %.1f (skin %.1f)\n", dot_before, dot_after, skin);
            CHECK(skin - dot_after < 0.5 * (skin - dot_before));
            CHECK(same_outside(original, out, face_area));
            // The eyes are not blemishes.
            CHECK(std::memcmp(original.px(160, 180), out.px(160, 180), 4) == 0);
        }
    }
}

void test_healing_spots() {
    // Grey with grain, and a dark spot at (100, 100).
    cv::Mat bgr(200, 220, CV_8UC3);
    for (int y = 0; y < bgr.rows; ++y)
        for (int x = 0; x < bgr.cols; ++x) {
            const uint8_t v = cv::saturate_cast<uint8_t>(128 + 10 * noise(x, y, 5));
            bgr.at<cv::Vec3b>(y, x) = cv::Vec3b(v, v, static_cast<uint8_t>(v + 10));
        }
    cv::circle(bgr, cv::Point(100, 100), 6, cv::Scalar(40, 40, 40), cv::FILLED);
    Source source;
    CHECK(open_mat(bgr, source));
    const aibs_spot spot{100.5f, 100.5f, 9.0f};
    aibs_portrait p{nullptr, 0, 0.0f};
    p.spots = &spot;
    p.spot_count = 1;
    Image original, healed;
    CHECK(render_full(source, bgr, nullptr, original));
    CHECK(render_full(source, bgr, &p, healed));
    if (!original.img.data || !healed.img.data) return;
    const uint8_t* centre = healed.px(100, 100);
    std::printf("info: healed spot centre %d (was %d)\n", centre[1], original.px(100, 100)[1]);
    CHECK(std::abs(centre[1] - 128) < 25);
    CHECK(std::abs(centre[2] - centre[1] - 10) < 8);  // the colour of the area around
    // Only inside the spot's radius.
    CHECK(same_outside(original, healed, {cv::Rect(91, 91, 19, 19)}));
    // A spot at the photo's corner takes texture from inside the photo and
    // changes nothing beyond its radius.
    const aibs_spot corner{2.0f, 2.0f, 9.0f};
    p.spots = &corner;
    Image cornered;
    CHECK(render_full(source, bgr, &p, cornered));
    CHECK(cornered.img.data && same_outside(original, cornered, {cv::Rect(0, 0, 12, 12)}));

    // A part rendered on its own matches the same part of the whole render
    // (as when exporting in strips or viewing part of the photo).
    p.spots = &spot;
    Image part;
    CHECK(aibs_source_render(source.ptr, 95, 70, 60, 60, 60, 60, &p, nullptr, nullptr, nullptr, nullptr, 0, 0, &part.img) ==
          AIBS_OK);
    if (part.img.data) {
        bool same = true;
        for (int y = 0; y < 60; ++y)
            for (int x = 0; x < 60; ++x) same = same && std::memcmp(part.px(x, y), healed.px(95 + x, 70 + y), 4) == 0;
        CHECK(same);
    }
}

}  // namespace

void run_portrait_tests() {
    test_yunet_decoding();
    test_skin_measurement();
    test_skin_smoothing();
    test_detection_without_faces();
    test_auto_adjust();
    test_face_tools();
    test_healing_spots();
}

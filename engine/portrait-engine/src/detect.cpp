// Face detection with YuNet and per-face skin colour measurement.
#include "../../ai-runtime/src/session_internal.h"
#include "../../image-engine/src/source_internal.h"
#include "portrait.h"

#include <algorithm>
#include <cmath>
#include <cstring>
#include <new>

#include <opencv2/imgproc.hpp>

namespace {

// Faces overlapping more than this are the same face.
constexpr float kMaxOverlap = 0.3f;

struct Image {
    aibs_image img{};
    ~Image() { aibs_image_free(&img); }
};

// Render the whole source at `scale` into an RGBA image.
int32_t render_scaled(const aibs_source* source, double scale, Image& out) {
    const cv::Mat& full = source->levels[0];
    const auto w = static_cast<uint32_t>(std::max(1.0, std::round(full.cols * scale)));
    const auto h = static_cast<uint32_t>(std::max(1.0, std::round(full.rows * scale)));
    return aibs_source_render(source, 0, 0, full.cols, full.rows, w, h, nullptr, nullptr, nullptr, nullptr, nullptr, 0, 0, &out.img);
}

}  // namespace

int32_t aibs_measure_skin(const aibs_source* source, aibs_face* face) {
    if (source == nullptr || source->levels.empty() || face == nullptr || !(face->width > 0) ||
        !(face->height > 0)) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    try {
        // Work at about 160 pixels across the face.
        const double scale = std::min(1.0, 160.0 / face->width);
        const double margin = 0.1 * face->width;
        const double rx = face->x - margin, ry = face->y - margin;
        const double rw = face->width + 2 * margin, rh = face->height + 2 * margin;
        const auto ow = static_cast<uint32_t>(std::max(1.0, std::round(rw * scale)));
        const auto oh = static_cast<uint32_t>(std::max(1.0, std::round(rh * scale)));
        Image region;
        const int32_t status =
            aibs_source_render(source, rx, ry, rw, rh, ow, oh, nullptr, nullptr, nullptr, nullptr, nullptr, 0, 0, &region.img);
        if (status != AIBS_OK) return status;

        cv::Mat rgba(static_cast<int>(oh), static_cast<int>(ow), CV_8UC4, region.img.data);
        cv::Mat bgr, ycrcb;
        cv::cvtColor(rgba, bgr, cv::COLOR_RGBA2BGR);
        cv::cvtColor(bgr, ycrcb, cv::COLOR_BGR2YCrCb);

        const auto to_px = [&](float sx, float sy) {
            return cv::Point2d((sx - rx) * scale, (sy - ry) * scale);
        };
        const float* lm = face->landmarks;
        const cv::Point2d eye_r = to_px(lm[0], lm[1]), eye_l = to_px(lm[2], lm[3]), nose = to_px(lm[4], lm[5]);
        const cv::Point2d mouth_r = to_px(lm[6], lm[7]), mouth_l = to_px(lm[8], lm[9]);
        const double face_px = face->width * scale;
        // Cheeks: between each eye and mouth corner, pushed outwards; and
        // the nose bridge. These are almost always bare skin.
        const cv::Point2d centre = (eye_r + eye_l + mouth_r + mouth_l) * 0.25;
        const auto outwards = [&](cv::Point2d p) {
            const cv::Point2d d = p - centre;
            const double n = std::max(1e-6, std::hypot(d.x, d.y));
            return p + d * (0.12 * face_px / n);
        };
        const cv::Point2d samples[] = {outwards((eye_r + mouth_r) * 0.5), outwards((eye_l + mouth_l) * 0.5),
                                       (eye_r + eye_l) * 0.25 + nose * 0.5};
        const double radius = std::max(2.0, 0.08 * face_px);

        std::vector<cv::Vec2d> values;
        for (const cv::Point2d& c : samples) {
            const int x0 = std::max(0, static_cast<int>(c.x - radius));
            const int x1 = std::min(ycrcb.cols - 1, static_cast<int>(c.x + radius));
            const int y0 = std::max(0, static_cast<int>(c.y - radius));
            const int y1 = std::min(ycrcb.rows - 1, static_cast<int>(c.y + radius));
            for (int yy = y0; yy <= y1; ++yy) {
                for (int xx = x0; xx <= x1; ++xx) {
                    if (std::hypot(xx + 0.5 - c.x, yy + 0.5 - c.y) > radius) continue;
                    const cv::Vec3b p = ycrcb.at<cv::Vec3b>(yy, xx);
                    values.emplace_back(p[2], p[1]);  // Cb, Cr
                }
            }
        }
        if (values.size() < 10) {
            return AIBS_ERR_INVALID_ARGUMENT;
        }

        // Mean and covariance, then once more without far-off samples
        // (stray hair, glasses, shadows).
        double mb = 0, mr = 0, vb = 0, vr = 0, cbr = 0;
        for (int pass = 0; pass < 2; ++pass) {
            double sb = 0, sr = 0, n = 0;
            for (const cv::Vec2d& v : values) {
                if (pass == 1) {
                    const double db = v[0] - mb, dr = v[1] - mr;
                    if (db * db / vb + dr * dr / vr > 2.5 * 2.5 * 2) continue;
                }
                sb += v[0];
                sr += v[1];
                n += 1;
            }
            if (n < 5) break;
            const double nb = sb / n, nr = sr / n;
            double qb = 0, qr = 0, qbr = 0;
            for (const cv::Vec2d& v : values) {
                if (pass == 1) {
                    const double db = v[0] - mb, dr = v[1] - mr;
                    if (db * db / vb + dr * dr / vr > 2.5 * 2.5 * 2) continue;
                }
                qb += (v[0] - nb) * (v[0] - nb);
                qr += (v[1] - nr) * (v[1] - nr);
                qbr += (v[0] - nb) * (v[1] - nr);
            }
            mb = nb;
            mr = nr;
            // Never narrower than a few levels, so flat skin still matches.
            vb = std::max(qb / n, 9.0);
            vr = std::max(qr / n, 9.0);
            cbr = qbr / n;
        }
        face->skin[0] = static_cast<float>(mb);
        face->skin[1] = static_cast<float>(mr);
        face->skin[2] = static_cast<float>(vb);
        face->skin[3] = static_cast<float>(vr);
        face->skin[4] = static_cast<float>(std::clamp(cbr, -0.95 * std::sqrt(vb * vr), 0.95 * std::sqrt(vb * vr)));
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

int32_t aibs_detect_faces(const aibs_source* source, aibs_ai_session* detector, float min_score, aibs_face* out,
                          uint32_t capacity, uint32_t* count) {
    if (count == nullptr) return AIBS_ERR_INVALID_ARGUMENT;
    *count = 0;
    if (source == nullptr || source->levels.empty() || detector == nullptr || (out == nullptr && capacity > 0)) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    try {
        const cv::Mat& full = source->levels[0];
        const double scale = static_cast<double>(aibs::kYuNetSize) / std::max(full.cols, full.rows);
        Image fitted;
        int32_t status = render_scaled(source, scale, fitted);
        if (status != AIBS_OK) return status;

        // Input: 1 x 3 x 640 x 640, planes B, G, R, values 0..255, image in
        // the top-left corner and the rest black.
        const size_t plane = static_cast<size_t>(aibs::kYuNetSize) * aibs::kYuNetSize;
        std::vector<float> input(3 * plane, 0.0f);
        for (uint32_t row = 0; row < fitted.img.height; ++row) {
            for (uint32_t col = 0; col < fitted.img.width; ++col) {
                const uint8_t* p = fitted.img.data + (static_cast<size_t>(row) * fitted.img.width + col) * 4;
                const size_t i = static_cast<size_t>(row) * aibs::kYuNetSize + col;
                input[i] = p[2];
                input[plane + i] = p[1];
                input[2 * plane + i] = p[0];
            }
        }
        const int64_t shape[4] = {1, 3, aibs::kYuNetSize, aibs::kYuNetSize};
        std::vector<aibs::NamedTensor> tensors;
        status = aibs::run_all_outputs(detector, input.data(), shape, 4, tensors);
        if (status != AIBS_OK) return status;

        std::vector<aibs::DetectorOutput> outputs;
        for (const aibs::NamedTensor& t : tensors) {
            outputs.push_back({t.name, t.shape, t.data.data(), t.data.size()});
        }
        std::vector<aibs::Detection> faces;
        if (!aibs::decode_yunet(outputs, aibs::kYuNetSize, aibs::kYuNetSize, min_score, faces)) {
            return AIBS_ERR_MODEL_INVALID;
        }
        faces = aibs::remove_overlaps(std::move(faces), kMaxOverlap);

        uint32_t written = 0;
        for (const aibs::Detection& d : faces) {
            if (written >= capacity) break;
            aibs_face face{};
            const auto back = [&](float v) { return static_cast<float>(v / scale); };
            face.x = back(d.x);
            face.y = back(d.y);
            face.width = back(d.width);
            face.height = back(d.height);
            for (int k = 0; k < 10; ++k) face.landmarks[k] = back(d.landmarks[k]);
            face.score = d.score;
            if (aibs_measure_skin(source, &face) != AIBS_OK) continue;  // too small or off-image
            out[written++] = face;
        }
        *count = written;
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

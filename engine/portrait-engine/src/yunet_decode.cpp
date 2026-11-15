// YuNet output decoding. Pure arithmetic, no inference: also compiled into
// the engine tests, which check it with hand-made outputs.
#include "portrait.h"

#include <algorithm>
#include <cmath>

namespace aibs {

namespace {

const DetectorOutput* find(const std::vector<DetectorOutput>& outputs, const std::string& name) {
    for (const DetectorOutput& output : outputs) {
        if (output.name == name) return &output;
    }
    return nullptr;
}

float iou(const Detection& a, const Detection& b) {
    const float x0 = std::max(a.x, b.x), y0 = std::max(a.y, b.y);
    const float x1 = std::min(a.x + a.width, b.x + b.width), y1 = std::min(a.y + a.height, b.y + b.height);
    const float inter = std::max(0.0f, x1 - x0) * std::max(0.0f, y1 - y0);
    const float uni = a.width * a.height + b.width * b.height - inter;
    return uni > 0 ? inter / uni : 0.0f;
}

}  // namespace

bool decode_yunet(const std::vector<DetectorOutput>& outputs, int input_width, int input_height, float min_score,
                  std::vector<Detection>& faces) {
    faces.clear();
    for (const int stride : {8, 16, 32}) {
        const std::string suffix = "_" + std::to_string(stride);
        const DetectorOutput* cls = find(outputs, "cls" + suffix);
        const DetectorOutput* obj = find(outputs, "obj" + suffix);
        const DetectorOutput* bbox = find(outputs, "bbox" + suffix);
        const DetectorOutput* kps = find(outputs, "kps" + suffix);
        const int cols = input_width / stride;
        const int rows = input_height / stride;
        const size_t cells = static_cast<size_t>(cols) * static_cast<size_t>(rows);
        if (cls == nullptr || obj == nullptr || bbox == nullptr || kps == nullptr || cls->size != cells ||
            obj->size != cells || bbox->size != cells * 4 || kps->size != cells * 10) {
            return false;
        }
        for (int row = 0; row < rows; ++row) {
            for (int col = 0; col < cols; ++col) {
                const size_t i = static_cast<size_t>(row) * static_cast<size_t>(cols) + static_cast<size_t>(col);
                const float c = std::clamp(cls->data[i], 0.0f, 1.0f);
                const float o = std::clamp(obj->data[i], 0.0f, 1.0f);
                const float score = std::sqrt(c * o);
                if (score < min_score) continue;
                const float s = static_cast<float>(stride);
                const float cx = (static_cast<float>(col) + bbox->data[i * 4 + 0]) * s;
                const float cy = (static_cast<float>(row) + bbox->data[i * 4 + 1]) * s;
                const float w = std::exp(bbox->data[i * 4 + 2]) * s;
                const float h = std::exp(bbox->data[i * 4 + 3]) * s;
                Detection face;
                face.x = cx - w / 2;
                face.y = cy - h / 2;
                face.width = w;
                face.height = h;
                face.score = score;
                for (int k = 0; k < 5; ++k) {
                    face.landmarks[2 * k] = (kps->data[i * 10 + 2 * k] + static_cast<float>(col)) * s;
                    face.landmarks[2 * k + 1] = (kps->data[i * 10 + 2 * k + 1] + static_cast<float>(row)) * s;
                }
                faces.push_back(face);
            }
        }
    }
    return true;
}

std::vector<Detection> remove_overlaps(std::vector<Detection> faces, float max_overlap) {
    std::sort(faces.begin(), faces.end(), [](const Detection& a, const Detection& b) { return a.score > b.score; });
    std::vector<Detection> kept;
    for (const Detection& face : faces) {
        const bool overlaps = std::any_of(kept.begin(), kept.end(),
                                          [&](const Detection& k) { return iou(face, k) > max_overlap; });
        if (!overlaps) kept.push_back(face);
    }
    return kept;
}

}  // namespace aibs

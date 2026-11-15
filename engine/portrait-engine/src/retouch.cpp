// Natural skin smoothing, applied while rendering.
//
// For each face: an edge-preserving smooth (guided filter) removes blotches
// and uneven tone, then the finest detail (pores, fine texture) is added
// back so skin does not look plastic. The result is blended in through a
// soft mask: inside the face oval, close to this face's own skin colour,
// and away from the eyes and mouth. Only colours change; nothing moves, so
// face shape and features stay exactly where they are.
//
// Everything is measured relative to the face's size on screen, so the
// look is the same at every zoom level and in a full-size export.
#include "portrait.h"

#include <algorithm>
#include <cmath>

#include <opencv2/imgproc.hpp>

namespace aibs {

namespace {

cv::Mat box(const cv::Mat& m, int radius) {
    cv::Mat out;
    cv::boxFilter(m, out, -1, cv::Size(2 * radius + 1, 2 * radius + 1), cv::Point(-1, -1), true,
                  cv::BORDER_REFLECT);
    return out;
}

// Self-guided filter on each channel of a float image.
cv::Mat guided(const cv::Mat& image, int radius, double eps) {
    const cv::Mat mean = box(image, radius);
    const cv::Mat mean_sq = box(image.mul(image), radius);
    const cv::Mat variance = mean_sq - mean.mul(mean);
    cv::Mat a;
    cv::divide(variance, variance + cv::Scalar::all(eps), a);
    const cv::Mat b = mean - a.mul(mean);
    return box(a, radius).mul(image) + box(b, radius);
}

double clamp01(double v) {
    return std::clamp(v, 0.0, 1.0);
}

// 1 well outside a feature, 0 inside it, soft in between. `d` is the
// distance from the feature centre in units of its radius.
double away_from(double d) {
    return clamp01((d - 0.8) / 0.4);
}

}  // namespace

void retouch_faces(cv::Mat& bgr, int first_col, double x, double y, double scale_x, double scale_y,
                   const aibs_portrait& portrait) {
    const double strength = std::clamp(static_cast<double>(portrait.smoothing), 0.0, 100.0) / 100.0;
    if (!(strength > 0.0) || portrait.faces == nullptr || portrait.face_count == 0 || first_col >= bgr.cols) {
        return;
    }
    first_col = std::max(first_col, 0);
    const auto to_out_x = [&](double sx) { return (sx - x) * scale_x; };
    const auto to_out_y = [&](double sy) { return (sy - y) * scale_y; };

    for (uint32_t f = 0; f < portrait.face_count; ++f) {
        const aibs_face& face = portrait.faces[f];
        if (!(face.width > 0) || !(face.height > 0) || !(face.skin[2] > 0) || !(face.skin[3] > 0)) continue;
        const double face_px = face.width * scale_x;
        if (face_px < 16) continue;  // too small on screen to retouch visibly

        // Face oval in output pixels: covers forehead to chin.
        const double cx = to_out_x(face.x + face.width * 0.5);
        const double cy = to_out_y(face.y + face.height * 0.5);
        const double ax = face.width * 0.55 * scale_x;
        const double ay = face.height * 0.62 * scale_y;

        const int radius = std::max(2, static_cast<int>(std::lround(face_px * 0.03)));
        const int fine_radius = std::max(1, static_cast<int>(std::lround(face_px * 0.006)));
        const int mask_radius = std::max(1, static_cast<int>(std::lround(face_px * 0.015)));
        const int pad = radius + 2 * mask_radius;

        const cv::Rect area(0, 0, bgr.cols, bgr.rows);
        cv::Rect roi(static_cast<int>(std::floor(cx - ax)) - pad, static_cast<int>(std::floor(cy - ay)) - pad,
                     static_cast<int>(std::ceil(2 * ax)) + 2 * pad, static_cast<int>(std::ceil(2 * ay)) + 2 * pad);
        roi &= area;
        roi &= cv::Rect(first_col, 0, bgr.cols - first_col, bgr.rows);
        if (roi.width <= 0 || roi.height <= 0) continue;

        cv::Mat image;
        bgr(roi).convertTo(image, CV_32FC3, 1.0 / 255.0);

        // Skin mask.
        cv::Mat ycrcb;
        cv::cvtColor(bgr(roi), ycrcb, cv::COLOR_BGR2YCrCb);
        const double mb = face.skin[0], mr = face.skin[1], vb = face.skin[2], vr = face.skin[3], cbr = face.skin[4];
        const double det = std::max(vb * vr - cbr * cbr, 1e-6);
        const double ib = vr / det, ir = vb / det, ibr = -cbr / det;  // inverse covariance

        const float* lm = face.landmarks;
        const double eye_radius = 0.17 * face.width * scale_x;
        const double eyes[2][2] = {{to_out_x(lm[0]), to_out_y(lm[1])}, {to_out_x(lm[2]), to_out_y(lm[3])}};
        const double mx = to_out_x((lm[6] + lm[8]) * 0.5), my = to_out_y((lm[7] + lm[9]) * 0.5);
        const double mouth_half = std::hypot(lm[6] - lm[8], lm[7] - lm[9]) * 0.5 * scale_x;
        const double mouth_ax = std::max(1.0, mouth_half * 1.25 + 0.04 * face_px);
        const double mouth_ay = std::max(1.0, 0.12 * face.height * scale_y);

        cv::Mat mask(roi.size(), CV_32F);
        for (int row = 0; row < roi.height; ++row) {
            float* m = mask.ptr<float>(row);
            const cv::Vec3b* p = ycrcb.ptr<cv::Vec3b>(row);
            const double py = roi.y + row + 0.5;
            for (int col = 0; col < roi.width; ++col) {
                const double px = roi.x + col + 0.5;
                const double ex = (px - cx) / ax, ey = (py - cy) / ay;
                const double oval = clamp01((1.0 - std::sqrt(ex * ex + ey * ey)) / 0.2);
                if (oval <= 0) {
                    m[col] = 0;
                    continue;
                }
                const double db = p[col][2] - mb, dr = p[col][1] - mr;
                const double distance = std::sqrt(std::max(0.0, db * db * ib + dr * dr * ir + 2 * db * dr * ibr));
                const double skin = clamp01(1.0 - (distance - 2.5) / 2.0);
                double features = 1.0;
                for (const auto& eye : eyes) {
                    features *= away_from(std::hypot(px - eye[0], py - eye[1]) / eye_radius);
                }
                const double mdx = (px - mx) / mouth_ax, mdy = (py - my) / mouth_ay;
                features *= away_from(std::sqrt(mdx * mdx + mdy * mdy));
                m[col] = static_cast<float>(oval * skin * features);
            }
        }
        mask = box(box(mask, mask_radius), mask_radius);

        // Smooth, then add the finest texture back.
        const cv::Mat smooth = guided(image, radius, 0.08 * 0.08);
        const cv::Mat fine = image - box(image, fine_radius);
        const cv::Mat retouched = smooth + fine * 0.6;

        cv::Mat weight;
        cv::Mat channels[3] = {mask * strength, mask * strength, mask * strength};
        cv::merge(channels, 3, weight);
        const cv::Mat result = image + weight.mul(retouched - image);

        cv::Mat result8;
        result.convertTo(result8, CV_8UC3, 255.0);
        // Write back only where the mask is non-zero, so everything else
        // stays bit-for-bit the original.
        cv::Mat touched = mask > 0;
        result8.copyTo(bgr(roi), touched);
    }
}

}  // namespace aibs

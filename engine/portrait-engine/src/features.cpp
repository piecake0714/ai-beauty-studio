// Portrait tools beyond skin smoothing, applied while rendering:
//
//  * healing spots: a round area is replaced by texture from a nearby area
//    of similar tone, shifted to the colour around the spot;
//  * blemishes: small spots darker or redder than the skin around them
//    fade into it (only on skin, inside the face);
//  * dark circles: the area under each eye is lifted towards the tone of
//    the cheek and loses its blue or purple cast;
//  * eyes: brighter and clearer (local contrast) around each eye;
//  * teeth: bright, low-colour pixels inside the mouth lose their yellow and
//    get a little brighter; lips (red) are left alone.
//
// Only colours change; nothing moves. All sizes follow the face's (or the
// spot's) size on screen, so the look is the same at every zoom level and in
// a full-size export.
#include "portrait.h"

#include <algorithm>
#include <cmath>

#include <opencv2/imgproc.hpp>

namespace aibs {

namespace {

double clamp01(double v) {
    return std::clamp(v, 0.0, 1.0);
}

cv::Mat box(const cv::Mat& m, int radius) {
    cv::Mat out;
    cv::boxFilter(m, out, -1, cv::Size(2 * radius + 1, 2 * radius + 1), cv::Point(-1, -1), true,
                  cv::BORDER_REFLECT);
    return out;
}

double strength_of(float value) {
    if (!std::isfinite(value)) return 0.0;
    return std::clamp(static_cast<double>(value), 0.0, 100.0) / 100.0;
}

// Weight of an ellipse (centre cx, cy; radii ax, ay): 1 inside 70% of it,
// fading to 0 at its edge.
double ellipse_weight(double px, double py, double cx, double cy, double ax, double ay) {
    const double ex = (px - cx) / ax, ey = (py - cy) / ay;
    return clamp01((1.0 - std::sqrt(ex * ex + ey * ey)) / 0.3);
}

// Region of the buffer covering an ellipse, limited to columns from first_col.
cv::Rect ellipse_roi(const cv::Mat& bgr, int first_col, double cx, double cy, double ax, double ay, int pad) {
    cv::Rect roi(static_cast<int>(std::floor(cx - ax)) - pad, static_cast<int>(std::floor(cy - ay)) - pad,
                 static_cast<int>(std::ceil(2 * ax)) + 2 * pad + 1, static_cast<int>(std::ceil(2 * ay)) + 2 * pad + 1);
    roi &= cv::Rect(0, 0, bgr.cols, bgr.rows);
    roi &= cv::Rect(first_col, 0, std::max(0, bgr.cols - first_col), bgr.rows);
    return roi;
}

// Write `ycrcb` (float, 0..255) back into bgr(roi) where weight > 0, so all
// other pixels stay bit-for-bit as they were.
void write_back(cv::Mat& bgr, const cv::Rect& roi, const cv::Mat& ycrcb, const cv::Mat& weight) {
    cv::Mat y8, out;
    ycrcb.convertTo(y8, CV_8UC3);
    cv::cvtColor(y8, out, cv::COLOR_YCrCb2BGR);
    out.copyTo(bgr(roi), weight > 0);
}

struct FaceGeometry {
    double face_px;       // face width on screen
    double fw, fh;        // face width and height in output pixels
    double eyes[2][2];    // eye centres
    double mouth[2];      // mouth centre
    double mouth_half;    // half the mouth-corner distance
};

FaceGeometry geometry(const aibs_face& face, double x, double y, double sx, double sy) {
    FaceGeometry g{};
    g.face_px = face.width * sx;
    g.fw = face.width * sx;
    g.fh = face.height * sy;
    const float* lm = face.landmarks;
    for (int e = 0; e < 2; ++e) {
        g.eyes[e][0] = (lm[2 * e] - x) * sx;
        g.eyes[e][1] = (lm[2 * e + 1] - y) * sy;
    }
    g.mouth[0] = ((lm[6] + lm[8]) * 0.5 - x) * sx;
    g.mouth[1] = ((lm[7] + lm[9]) * 0.5 - y) * sy;
    g.mouth_half = std::hypot((lm[6] - lm[8]) * sx, (lm[7] - lm[9]) * sy) * 0.5;
    return g;
}

bool usable(const aibs_face& face) {
    if (!(face.width > 0) || !(face.height > 0)) return false;
    for (const float v : face.landmarks) {
        if (!std::isfinite(v)) return false;
    }
    return true;
}

void lift_dark_circles(cv::Mat& bgr, int first_col, const FaceGeometry& g, double s) {
    for (const auto& eye : g.eyes) {
        const double cx = eye[0], cy = eye[1] + 0.11 * g.fh;
        const double ax = std::max(1.0, 0.16 * g.fw), ay = std::max(1.0, 0.075 * g.fh);
        const int ref_radius = std::max(2, static_cast<int>(std::lround(0.10 * g.fw)));
        const cv::Rect roi = ellipse_roi(bgr, first_col, cx, cy, ax, ay, ref_radius);
        if (roi.width <= 0 || roi.height <= 0) continue;
        cv::Mat ycrcb;
        cv::cvtColor(bgr(roi), ycrcb, cv::COLOR_BGR2YCrCb);
        ycrcb.convertTo(ycrcb, CV_32FC3);
        // The surrounding tone: a wide average (cheek and lid included).
        const cv::Mat around = box(ycrcb, ref_radius);
        cv::Mat weight(roi.size(), CV_32F);
        for (int row = 0; row < roi.height; ++row) {
            cv::Vec3f* p = ycrcb.ptr<cv::Vec3f>(row);
            const cv::Vec3f* a = around.ptr<cv::Vec3f>(row);
            float* w = weight.ptr<float>(row);
            for (int col = 0; col < roi.width; ++col) {
                const double k = s * ellipse_weight(roi.x + col + 0.5, roi.y + row + 0.5, cx, cy, ax, ay);
                w[col] = static_cast<float>(k);
                if (k <= 0) continue;
                const double lift = std::max(0.0, static_cast<double>(a[col][0] - p[col][0]));
                p[col][0] = static_cast<float>(std::min(255.0, p[col][0] + k * (0.9 * lift + 4.0)));
                p[col][1] += static_cast<float>(k * 0.6 * (a[col][1] - p[col][1]));
                p[col][2] += static_cast<float>(k * 0.6 * (a[col][2] - p[col][2]));
            }
        }
        write_back(bgr, roi, ycrcb, weight);
    }
}

void enhance_eyes(cv::Mat& bgr, int first_col, const FaceGeometry& g, double s) {
    for (const auto& eye : g.eyes) {
        const double ax = std::max(1.0, 0.13 * g.fw), ay = std::max(1.0, 0.07 * g.fh);
        const int detail_radius = std::max(1, static_cast<int>(std::lround(0.012 * g.fw)));
        const cv::Rect roi = ellipse_roi(bgr, first_col, eye[0], eye[1], ax, ay, detail_radius);
        if (roi.width <= 0 || roi.height <= 0) continue;
        cv::Mat ycrcb;
        cv::cvtColor(bgr(roi), ycrcb, cv::COLOR_BGR2YCrCb);
        ycrcb.convertTo(ycrcb, CV_32FC3);
        const cv::Mat soft = box(ycrcb, detail_radius);
        cv::Mat weight(roi.size(), CV_32F);
        for (int row = 0; row < roi.height; ++row) {
            cv::Vec3f* p = ycrcb.ptr<cv::Vec3f>(row);
            const cv::Vec3f* b = soft.ptr<cv::Vec3f>(row);
            float* w = weight.ptr<float>(row);
            for (int col = 0; col < roi.width; ++col) {
                const double k = s * ellipse_weight(roi.x + col + 0.5, roi.y + row + 0.5, eye[0], eye[1], ax, ay);
                w[col] = static_cast<float>(k);
                if (k <= 0) continue;
                const double y = p[col][0];
                const double clarity = y + 0.8 * k * (y - b[col][0]);
                // Brighter: more on light tones (the whites) than dark (iris).
                const double bright = clarity + k * 18.0 * (clarity / 255.0);
                p[col][0] = static_cast<float>(std::clamp(bright, 0.0, 255.0));
            }
        }
        write_back(bgr, roi, ycrcb, weight);
    }
}

void whiten_teeth(cv::Mat& bgr, int first_col, const FaceGeometry& g, double s) {
    const double ax = std::max(1.0, g.mouth_half * 0.95), ay = std::max(1.0, 0.075 * g.fh);
    const cv::Rect roi = ellipse_roi(bgr, first_col, g.mouth[0], g.mouth[1], ax, ay, 1);
    if (roi.width <= 0 || roi.height <= 0) return;
    cv::Mat ycrcb;
    cv::cvtColor(bgr(roi), ycrcb, cv::COLOR_BGR2YCrCb);
    ycrcb.convertTo(ycrcb, CV_32FC3);
    // Teeth are the brighter part of the mouth.
    double sum = 0, n = 0;
    for (int row = 0; row < roi.height; ++row) {
        const cv::Vec3f* p = ycrcb.ptr<cv::Vec3f>(row);
        for (int col = 0; col < roi.width; ++col) {
            if (ellipse_weight(roi.x + col + 0.5, roi.y + row + 0.5, g.mouth[0], g.mouth[1], ax, ay) <= 0) continue;
            sum += p[col][0];
            n += 1;
        }
    }
    if (n <= 0) return;
    const double mean_y = sum / n;
    cv::Mat weight(roi.size(), CV_32F);
    for (int row = 0; row < roi.height; ++row) {
        cv::Vec3f* p = ycrcb.ptr<cv::Vec3f>(row);
        float* w = weight.ptr<float>(row);
        for (int col = 0; col < roi.width; ++col) {
            const double place = ellipse_weight(roi.x + col + 0.5, roi.y + row + 0.5, g.mouth[0], g.mouth[1], ax, ay);
            const double cr = p[col][1] - 128.0, cb = p[col][2] - 128.0;
            const double bright = clamp01((p[col][0] - mean_y + 5.0) / 25.0);
            const double not_red = clamp01(1.0 - (cr - 8.0) / 8.0);  // lips, gums and skin are red
            const double k = s * place * bright * not_red;
            w[col] = static_cast<float>(k);
            if (k <= 0) continue;
            if (cb < 0) p[col][2] = static_cast<float>(128.0 + cb * (1.0 - 0.85 * k));  // less yellow
            p[col][1] = static_cast<float>(128.0 + cr * (1.0 - 0.5 * k));
            p[col][0] = static_cast<float>(std::min(255.0, p[col][0] + k * 14.0));
        }
    }
    write_back(bgr, roi, ycrcb, weight);
}

}  // namespace

void reduce_blemishes(cv::Mat& image, const cv::Mat& ycrcb, const cv::Mat& mask, double face_px, double strength) {
    // `image` is float BGR 0..1, `ycrcb` 8-bit, `mask` the skin mask (0..1).
    if (!(strength > 0.0)) return;
    const int radius = std::max(2, static_cast<int>(std::lround(face_px * 0.025)));
    cv::Mat y, cr;
    cv::extractChannel(ycrcb, y, 0);
    cv::extractChannel(ycrcb, cr, 1);
    y.convertTo(y, CV_32F);
    cr.convertTo(cr, CV_32F);
    const cv::Mat y_around = box(box(y, radius), radius);
    const cv::Mat cr_around = box(box(cr, radius), radius);
    // Lower threshold at higher strength: more and fainter spots fade.
    const double dark_t = 14.0 - 9.0 * strength;
    const double red_t = 9.0 - 5.0 * strength;
    cv::Mat spots(mask.size(), CV_32F);
    for (int row = 0; row < mask.rows; ++row) {
        const float* yy = y.ptr<float>(row);
        const float* ya = y_around.ptr<float>(row);
        const float* cc = cr.ptr<float>(row);
        const float* ca = cr_around.ptr<float>(row);
        float* o = spots.ptr<float>(row);
        for (int col = 0; col < mask.cols; ++col) {
            const double dark = clamp01((ya[col] - yy[col] - dark_t) / dark_t);
            const double red = clamp01((cc[col] - ca[col] - red_t) / red_t);
            o[col] = static_cast<float>(std::max(dark, red));
        }
    }
    // Grow and soften the spots a little so their rims go too.
    const int soft = std::max(1, radius / 3);
    cv::Mat grown;
    cv::dilate(spots, grown, cv::getStructuringElement(cv::MORPH_ELLIPSE, cv::Size(2 * soft + 1, 2 * soft + 1)));
    grown = box(grown, soft);
    const cv::Mat weight = grown.mul(mask) * strength;
    // The skin around, with its finest texture kept.
    const int fine = std::max(1, static_cast<int>(std::lround(face_px * 0.005)));
    const cv::Mat around = box(box(image, radius), radius);
    const cv::Mat texture = image - box(image, fine);
    const cv::Mat filled = around + texture * 0.5;
    cv::Mat weight3;
    cv::merge(std::vector<cv::Mat>{weight, weight, weight}, weight3);
    image = image + weight3.mul(filled - image);
}

void retouch_features(cv::Mat& bgr, int first_col, double x, double y, double scale_x, double scale_y,
                      const aibs_portrait& portrait) {
    const double circles = strength_of(portrait.dark_circles);
    const double eyes = strength_of(portrait.eyes);
    const double teeth = strength_of(portrait.teeth);
    if ((circles <= 0 && eyes <= 0 && teeth <= 0) || portrait.faces == nullptr || first_col >= bgr.cols) return;
    first_col = std::max(first_col, 0);
    for (uint32_t f = 0; f < portrait.face_count; ++f) {
        const aibs_face& face = portrait.faces[f];
        if (!usable(face)) continue;
        const FaceGeometry g = geometry(face, x, y, scale_x, scale_y);
        if (g.face_px < 16) continue;  // too small on screen to show
        if (circles > 0) lift_dark_circles(bgr, first_col, g, circles);
        if (eyes > 0) enhance_eyes(bgr, first_col, g, eyes);
        if (teeth > 0) whiten_teeth(bgr, first_col, g, teeth);
    }
}

int spot_margin(const aibs_portrait* portrait, double scale_x, double scale_y) {
    if (portrait == nullptr || portrait->spots == nullptr) return 0;
    const uint32_t count = std::min<uint32_t>(portrait->spot_count, AIBS_MAX_SPOTS);
    double widest = 0;
    for (uint32_t i = 0; i < count; ++i) {
        const float r = portrait->spots[i].radius;
        if (std::isfinite(r) && r > 0) widest = std::max(widest, static_cast<double>(r));
    }
    if (widest <= 0) return 0;
    return std::min(kMaxSpotMargin, static_cast<int>(std::ceil(5.5 * widest * std::max(scale_x, scale_y))) + 2);
}

void heal_spots(cv::Mat& bgr, int first_col, double x, double y, double scale_x, double scale_y, int source_width,
                int source_height, const aibs_portrait& portrait) {
    if (portrait.spots == nullptr || portrait.spot_count == 0 || first_col >= bgr.cols) return;
    first_col = std::max(first_col, 0);
    const uint32_t count = std::min<uint32_t>(portrait.spot_count, AIBS_MAX_SPOTS);
    // Source area in output pixels: candidates must lie inside the photo.
    const double inside_x0 = (0 - x) * scale_x, inside_x1 = (source_width - x) * scale_x;
    const double inside_y0 = (0 - y) * scale_y, inside_y1 = (source_height - y) * scale_y;
    const double scale = std::sqrt(scale_x * scale_y);

    for (uint32_t i = 0; i < count; ++i) {
        const aibs_spot& spot = portrait.spots[i];
        if (!std::isfinite(spot.x) || !std::isfinite(spot.y) || !std::isfinite(spot.radius) || !(spot.radius > 0)) continue;
        const double cx = (spot.x - x) * scale_x, cy = (spot.y - y) * scale_y;
        const double r = std::max(1.0, spot.radius * scale);
        const double ring = 1.4 * r;
        // Skip spots that do not touch the changeable part of the buffer.
        if (cx + ring < first_col || cx - ring >= bgr.cols || cy + ring < 0 || cy - ring >= bgr.rows) continue;

        const auto ring_mean = [&](double ox, double oy, cv::Vec3d& mean) {
            // Mean of the ring between r and 1.4 r around (ox, oy); false if
            // any of it lies outside the buffer.
            const int x0 = static_cast<int>(std::floor(ox - ring)), x1 = static_cast<int>(std::ceil(ox + ring));
            const int y0 = static_cast<int>(std::floor(oy - ring)), y1 = static_cast<int>(std::ceil(oy + ring));
            if (x0 < 0 || y0 < 0 || x1 >= bgr.cols || y1 >= bgr.rows) return false;
            cv::Vec3d sum(0, 0, 0);
            double n = 0;
            for (int row = y0; row <= y1; ++row) {
                const cv::Vec3b* p = bgr.ptr<cv::Vec3b>(row);
                for (int col = x0; col <= x1; ++col) {
                    const double d = std::hypot(col + 0.5 - ox, row + 0.5 - oy);
                    if (d < r || d > ring) continue;
                    sum += cv::Vec3d(p[col][0], p[col][1], p[col][2]);
                    n += 1;
                }
            }
            if (n <= 0) return false;
            mean = sum / n;
            return true;
        };
        // Texture difference between the target's ring and a candidate's
        // ring, after removing their mean colours.
        const auto ring_cost = [&](int dx, int dy, const cv::Vec3d& target_mean, const cv::Vec3d& mean) {
            double cost = 0, n = 0;
            const int x0 = static_cast<int>(std::floor(cx - ring)), x1 = static_cast<int>(std::ceil(cx + ring));
            const int y0 = static_cast<int>(std::floor(cy - ring)), y1 = static_cast<int>(std::ceil(cy + ring));
            for (int row = y0; row <= y1; ++row) {
                for (int col = x0; col <= x1; ++col) {
                    const double d = std::hypot(col + 0.5 - cx, row + 0.5 - cy);
                    if (d < r || d > ring) continue;
                    const cv::Vec3b a = bgr.at<cv::Vec3b>(row, col);
                    const cv::Vec3b b = bgr.at<cv::Vec3b>(row + dy, col + dx);
                    for (int c = 0; c < 3; ++c) {
                        const double e = (a[c] - target_mean[c]) - (b[c] - mean[c]);
                        cost += e * e;
                    }
                    n += 1;
                }
            }
            // Prefer candidates of similar overall tone too.
            const cv::Vec3d shift = target_mean - mean;
            return (n > 0 ? cost / n : 1e18) + 0.5 * shift.dot(shift);
        };

        cv::Vec3d target_mean;
        if (!ring_mean(cx, cy, target_mean)) continue;
        double best_cost = 1e300;
        int dx = 0, dy = 0;
        cv::Vec3d best_mean;
        const double distance = 2.4 * r;
        for (int k = 0; k < 8; ++k) {
            const double angle = k * CV_PI / 4.0;
            const int kx = static_cast<int>(std::lround(distance * std::cos(angle)));
            const int ky = static_cast<int>(std::lround(distance * std::sin(angle)));
            const double ox = cx + kx, oy = cy + ky;
            if (ox - ring < inside_x0 || ox + ring > inside_x1 || oy - ring < inside_y0 || oy + ring > inside_y1) continue;
            cv::Vec3d mean;
            if (!ring_mean(ox, oy, mean)) continue;
            // The candidate's own ring must also not include the spot.
            const double cost = ring_cost(kx, ky, target_mean, mean);
            if (cost < best_cost) {
                best_cost = cost;
                dx = kx;
                dy = ky;
                best_mean = mean;
            }
        }
        if (best_cost >= 1e300) continue;

        // Copy the candidate's texture, moved to the target's colour, with a
        // soft edge.
        const cv::Vec3d shift = target_mean - best_mean;
        const int x0 = std::max(first_col, static_cast<int>(std::floor(cx - r)));
        const int x1 = std::min(bgr.cols - 1, static_cast<int>(std::ceil(cx + r)));
        const int y0 = std::max(0, static_cast<int>(std::floor(cy - r)));
        const int y1 = std::min(bgr.rows - 1, static_cast<int>(std::ceil(cy + r)));
        if (x0 > x1 || y0 > y1) continue;
        // The candidate area (inside the buffer: its ring was), copied first
        // because it may overlap the spot.
        const cv::Mat from_area = bgr(cv::Rect(x0 + dx, y0 + dy, x1 - x0 + 1, y1 - y0 + 1)).clone();
        for (int row = y0; row <= y1; ++row) {
            cv::Vec3b* p = bgr.ptr<cv::Vec3b>(row);
            const cv::Vec3b* from = from_area.ptr<cv::Vec3b>(row - y0);
            for (int col = x0; col <= x1; ++col) {
                const double d = std::hypot(col + 0.5 - cx, row + 0.5 - cy);
                const double w = clamp01((r - d) / (0.35 * r));
                if (w <= 0) continue;
                const cv::Vec3b s = from[col - x0];
                for (int c = 0; c < 3; ++c) {
                    const double healed = s[c] + shift[c];
                    p[col][c] = cv::saturate_cast<uint8_t>(p[col][c] + w * (healed - p[col][c]));
                }
            }
        }
    }
}

namespace {

// Self-guided filter on one float channel.
cv::Mat guided_one(const cv::Mat& guide, const cv::Mat& p, int radius, double eps) {
    const cv::Mat mg = box(guide, radius), mp = box(p, radius);
    const cv::Mat cov = box(guide.mul(p), radius) - mg.mul(mp);
    const cv::Mat var = box(guide.mul(guide), radius) - mg.mul(mg);
    cv::Mat a;
    cv::divide(cov, var + eps, a);
    const cv::Mat b = mp - a.mul(mg);
    return box(a, radius).mul(guide) + box(b, radius);
}

struct NoiseRadii {
    int luminance = 0;  // 0: off
    int colour = 0;
};

NoiseRadii noise_radii(const aibs_portrait& portrait, double scale) {
    NoiseRadii r;
    const double l = strength_of(portrait.noise_luminance), c = strength_of(portrait.noise_color);
    // Sizes in source pixels, shown at the render's scale; too small to
    // see when zoomed far out.
    if (l > 0) {
        const double size = (1.0 + 2.0 * l) * scale;
        if (size >= 0.5) r.luminance = std::max(1, static_cast<int>(std::lround(size)));
    }
    if (c > 0) {
        const double size = (2.0 + 6.0 * c) * scale;
        if (size >= 0.5) r.colour = std::max(1, static_cast<int>(std::lround(size)));
    }
    return r;
}

}  // namespace

int noise_margin(const aibs_portrait* portrait, double scale_x, double scale_y) {
    if (portrait == nullptr) return 0;
    const NoiseRadii r = noise_radii(*portrait, std::max(scale_x, scale_y));
    const int widest = std::max(r.luminance, r.colour);
    return widest > 0 ? 4 * widest + 2 : 0;
}

void reduce_noise(cv::Mat& bgr, int first_col, double scale_x, double scale_y, const aibs_portrait& portrait) {
    if (first_col >= bgr.cols) return;
    first_col = std::max(first_col, 0);
    const NoiseRadii r = noise_radii(portrait, std::sqrt(scale_x * scale_y));
    if (r.luminance == 0 && r.colour == 0) return;
    const double l = strength_of(portrait.noise_luminance), c = strength_of(portrait.noise_color);
    // Filter the whole buffer (its margin gives the neighbours), change
    // only the columns from first_col on.
    const cv::Rect roi(first_col, 0, bgr.cols - first_col, bgr.rows);
    cv::Mat ycrcb;
    cv::cvtColor(bgr, ycrcb, cv::COLOR_BGR2YCrCb);
    ycrcb.convertTo(ycrcb, CV_32FC3, 1.0 / 255.0);
    std::vector<cv::Mat> planes;
    cv::split(ycrcb, planes);
    const cv::Mat y = planes[0].clone();
    if (r.luminance > 0) {
        // Edge-preserving: flat areas (variance below eps) smooth, edges stay.
        const double e = 0.012 + 0.04 * l;
        const cv::Mat smooth = guided_one(y, y, r.luminance, e * e);
        planes[0] = y + (smooth - y) * l;
    }
    if (r.colour > 0) {
        // Colour follows the brightness edges, so it does not bleed.
        const double e = 0.02 + 0.06 * c;
        for (int k = 1; k <= 2; ++k) {
            const cv::Mat smooth = guided_one(y, planes[k], r.colour, e * e);
            planes[k] = planes[k] + (smooth - planes[k]) * c;
        }
    }
    cv::merge(planes, ycrcb);
    cv::Mat y8, out;
    ycrcb.convertTo(y8, CV_8UC3, 255.0);
    cv::cvtColor(y8, out, cv::COLOR_YCrCb2BGR);
    out(roi).copyTo(bgr(roi));
}

}  // namespace aibs

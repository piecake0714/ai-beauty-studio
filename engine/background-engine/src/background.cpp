// Subject/background separation and background compositing.
//
// The matte (subject coverage) is computed once per image by the AI model
// and refined against the image so edges such as hair follow the picture.
// Every render then samples the matte for the region on screen and
// composites the chosen background, so it works at any zoom level.
#include "background.h"

#include "../../ai-runtime/src/session_internal.h"
#include "../../color-engine/src/adjust.h"
#include "../../image-engine/src/source_internal.h"

#include <algorithm>
#include <cmath>
#include <cstring>
#include <new>
#include <vector>

#include <opencv2/core/utility.hpp>
#include <opencv2/imgproc.hpp>

namespace {

// BiRefNet's input size.
constexpr int kModelSize = 1024;
// The refined matte is kept at about this many pixels on its longer side.
constexpr int kMatteSide = 2048;
// Background blur at strength 100: Gaussian sigma as a fraction of the
// image's longer side.
constexpr double kMaxBlurFraction = 0.02;
// Largest blur margin, in output pixels, to bound the work at high zoom.
constexpr int kMaxMargin = 1024;

struct Image {
    aibs_image img{};
    ~Image() { aibs_image_free(&img); }
};

double blur_sigma(const aibs_background& background, int source_width, int source_height, double scale) {
    const double strength = std::clamp(static_cast<double>(background.blur), 0.0, 100.0) / 100.0;
    return strength * kMaxBlurFraction * std::max(source_width, source_height) * scale;
}

// Gaussian blur that stays fast for large sigma: blur a copy reduced by a
// whole factor k. The reduction grid is anchored to absolute output pixel
// positions (origin = position of image(0, 0)), so neighbouring renders of
// one image (screen tiles, export strips) blur identically and meet without
// seams.
cv::Mat wide_blur(const cv::Mat& image, double sigma, int origin_x, int origin_y) {
    cv::Mat out;
    if (sigma < 0.5) return image.clone();
    const int k = std::max(1, static_cast<int>(std::floor(sigma / 6.0)));
    if (k == 1) {
        cv::GaussianBlur(image, out, cv::Size(0, 0), sigma, sigma, cv::BORDER_REFLECT);
        return out;
    }
    const auto mod = [k](int v) { return ((v % k) + k) % k; };
    const int left = mod(origin_x), top = mod(origin_y);
    const int right = (k - (image.cols + left) % k) % k;
    const int bottom = (k - (image.rows + top) % k) % k;
    cv::Mat padded;
    cv::copyMakeBorder(image, padded, top, bottom, left, right, cv::BORDER_REFLECT);
    cv::Mat small;
    cv::resize(padded, small, cv::Size(padded.cols / k, padded.rows / k), 0, 0, cv::INTER_AREA);
    cv::GaussianBlur(small, small, cv::Size(0, 0), sigma / k, sigma / k, cv::BORDER_REFLECT);
    cv::resize(small, out, padded.size(), 0, 0, cv::INTER_LINEAR);
    return out(cv::Rect(left, top, image.cols, image.rows)).clone();
}

// Guided-filter radius for edge detail: wide enough that its windows reach
// across the edge, so the matte takes on the picture's fine structure there.
int detail_radius(double reach) {
    return std::max(2, static_cast<int>(std::lround(0.01 * reach)));
}

cv::Mat box(const cv::Mat& m, int radius) {
    cv::Mat out;
    cv::boxFilter(m, out, -1, cv::Size(2 * radius + 1, 2 * radius + 1), cv::Point(-1, -1), true,
                  cv::BORDER_REFLECT);
    return out;
}

// Guided filter: smooth `p` while following the edges of grey guide `g`.
cv::Mat guided(const cv::Mat& g, const cv::Mat& p, int radius, double eps) {
    const cv::Mat mean_g = box(g, radius), mean_p = box(p, radius);
    const cv::Mat cov = box(g.mul(p), radius) - mean_g.mul(mean_p);
    const cv::Mat var = box(g.mul(g), radius) - mean_g.mul(mean_g);
    cv::Mat a;
    cv::divide(cov, var + eps, a);
    const cv::Mat b = mean_p - a.mul(mean_g);
    return box(a, radius).mul(g) + box(b, radius);
}

// Blur-fusion foreground colour estimation (Forte & Pitie, "Approximate
// fast foreground colour estimation", ICIP 2021): at soft edges, recover the
// subject's own colour so the old background does not show as a fringe.
// One pass: F = blur(F a)/blur(a) + a (I - a blur(F) - (1 - a) blur(B)).
void fusion_pass(const cv::Mat& image, cv::Mat& f, cv::Mat& b, const cv::Mat& alpha, int radius) {
    cv::Mat alpha3;
    cv::merge(std::vector<cv::Mat>{alpha, alpha, alpha}, alpha3);
    const cv::Mat inverse3 = cv::Scalar::all(1.0) - alpha3;
    const cv::Mat blurred_alpha = box(alpha3, radius);
    cv::Mat blurred_f, blurred_b;
    cv::divide(box(f.mul(alpha3), radius), blurred_alpha + cv::Scalar::all(1e-5), blurred_f);
    cv::divide(box(b.mul(inverse3), radius), (cv::Scalar::all(1.0) - blurred_alpha) + cv::Scalar::all(1e-5), blurred_b);
    const cv::Mat next = blurred_f + alpha3.mul(image - alpha3.mul(blurred_f) - inverse3.mul(blurred_b));
    f = cv::min(cv::max(next, 0.0), 1.0);
    b = blurred_b;
}

// Subject colours for `bgr` given coverage `m` (both the same size),
// 8-bit BGR. `reach` is the image's longer side in these pixels.
cv::Mat estimate_foreground(const cv::Mat& bgr, const cv::Mat& m, double reach) {
    cv::Mat image, alpha;
    bgr.convertTo(image, CV_32FC3, 1.0 / 255.0);
    m.convertTo(alpha, CV_32F, 1.0 / 255.0);
    cv::Mat f = image.clone(), b = image.clone();
    fusion_pass(image, f, b, alpha, std::max(2, static_cast<int>(std::lround(0.045 * reach))));
    fusion_pass(image, f, b, alpha, std::max(1, static_cast<int>(std::lround(0.003 * reach))));
    cv::Mat out;
    f.convertTo(out, CV_8UC3, 255.0);
    return out;
}

// Push the soft edge of the matte out (edge > 0) or pull it in (edge < 0),
// keeping full subject and full background as they are.
void shift_edge(cv::Mat& m, float edge) {
    const double e = std::clamp(static_cast<double>(edge), -100.0, 100.0) / 100.0;
    if (e == 0.0) return;
    const double exponent = std::pow(2.0, -1.5 * e);
    cv::Mat table(1, 256, CV_8U);
    for (int i = 0; i < 256; ++i) {
        table.at<uint8_t>(i) = cv::saturate_cast<uint8_t>(std::pow(i / 255.0, exponent) * 255.0);
    }
    cv::LUT(m, table, m);
}

// Make the soft edge of the matte follow fine detail of the picture (hair)
// more closely: a guided filter at the scale of a few strands, guided by the
// rendered image, used only in the uncertain band around the edge.
void refine_detail(cv::Mat& m, const cv::Mat& bgr, float detail, double reach) {
    const double d = std::isfinite(detail) ? std::clamp(static_cast<double>(detail), 0.0, 100.0) / 100.0 : 0.0;
    if (d <= 0.0) return;
    cv::Mat partial;
    cv::inRange(m, cv::Scalar(1), cv::Scalar(254), partial);
    if (cv::countNonZero(partial) == 0) return;
    const int radius = detail_radius(reach);
    cv::Mat grey, guide, alpha;
    cv::cvtColor(bgr, grey, cv::COLOR_BGR2GRAY);
    grey.convertTo(guide, CV_32F, 1.0 / 255.0);
    m.convertTo(alpha, CV_32F, 1.0 / 255.0);
    const cv::Mat followed = guided(guide, alpha, radius, 1e-4);
    // The band: where the matte is soft, widened a little.
    cv::Mat band;
    partial.convertTo(band, CV_32F, 1.0 / 255.0);
    band = box(band, radius);
    cv::Mat result = alpha + (band * d).mul(followed - alpha);
    cv::min(cv::max(result, 0.0), 1.0, result);
    result.convertTo(m, CV_8U, 255.0);
}

}  // namespace

namespace aibs {

cv::Mat warp_plane(const cv::Mat& plane, double fx, double fy, double x, double y, double scale_x, double scale_y,
                   int cols, int rows, int interpolation) {
    const double ax = fx / scale_x;
    const double ay = fy / scale_y;
    const cv::Matx23d inverse(ax, 0.0, 0.5 * ax + x * fx - 0.5, 0.0, ay, 0.5 * ay + y * fy - 0.5);
    cv::Mat out;
    cv::warpAffine(plane, out, inverse, cv::Size(cols, rows), interpolation | cv::WARP_INVERSE_MAP,
                   cv::BORDER_REPLICATE);
    return out;
}

int32_t render_cover(const aibs_source& image, int photo_width, int photo_height, double x, double y,
                     double scale_x, double scale_y, int cols, int rows, cv::Mat& out) {
    if (image.levels.empty()) return AIBS_ERR_INVALID_ARGUMENT;
    // Scale the image to cover the whole photo, centred.
    const cv::Mat& other = image.levels[0];
    const double cover = std::max(static_cast<double>(photo_width) / other.cols,
                                  static_cast<double>(photo_height) / other.rows);
    const double off_x = (photo_width - other.cols * cover) / 2;
    const double off_y = (photo_height - other.rows * cover) / 2;
    Image rendered;
    const int32_t status = aibs_source_render(&image, (x - off_x) / cover, (y - off_y) / cover, cols / scale_x / cover,
                                              rows / scale_y / cover, static_cast<uint32_t>(cols),
                                              static_cast<uint32_t>(rows), nullptr, nullptr, nullptr, nullptr, nullptr,
                                              0, 0, &rendered.img);
    if (status != AIBS_OK) return status;
    const cv::Mat rgba(rows, cols, CV_8UC4, rendered.img.data);
    cv::cvtColor(rgba, out, cv::COLOR_RGBA2BGR);
    return AIBS_OK;
}

bool background_active(const aibs_background* background) {
    if (background == nullptr || background->matte == nullptr || background->matte->coverage.empty()) return false;
    if (background->mode == AIBS_BG_ORIGINAL) {
        return background->brightness != 0.0f || background->saturation != 0.0f;
    }
    return background->mode <= AIBS_BG_IMAGE;
}

int background_margin(const aibs_background& background, int source_width, int source_height, double scale_x,
                      double scale_y) {
    int margin = 0;
    if (std::isfinite(background.detail) && background.detail > 0) {
        margin = 2 * detail_radius(std::max(source_width * scale_x, source_height * scale_y)) + 2;
    }
    if (background.mode != AIBS_BG_BLUR) return margin;
    const double sigma = blur_sigma(background, source_width, source_height, std::max(scale_x, scale_y));
    return std::min(kMaxMargin, std::max(margin, static_cast<int>(std::ceil(2.5 * sigma))));
}

int32_t composite_background(cv::Mat& bgr, int first_col, double x, double y, double scale_x, double scale_y,
                             const aibs_background& background, cv::Mat& alpha) {
    const aibs_matte& matte = *background.matte;
    first_col = std::clamp(first_col, 0, bgr.cols);
    if (first_col >= bgr.cols) return AIBS_OK;
    const double fx = static_cast<double>(matte.coverage.cols) / matte.source_width;
    const double fy = static_cast<double>(matte.coverage.rows) / matte.source_height;
    cv::Mat m = warp_plane(matte.coverage, fx, fy, x, y, scale_x, scale_y, bgr.cols, bgr.rows, cv::INTER_LINEAR);
    refine_detail(m, bgr, background.detail,
                  std::max(matte.source_width * scale_x, matte.source_height * scale_y));
    shift_edge(m, background.edge);

    // At soft edges, use the subject's own colour (not mixed with the old
    // background) whenever the background is replaced or removed.
    const cv::Rect changed(first_col, 0, bgr.cols - first_col, bgr.rows);
    if (background.mode != AIBS_BG_ORIGINAL) {
        cv::Mat partial;
        cv::inRange(m(changed), cv::Scalar(1), cv::Scalar(254), partial);
        if (cv::countNonZero(partial) > 0) {
            const double reach = std::max(matte.source_width * scale_x, matte.source_height * scale_y);
            const cv::Mat foreground = estimate_foreground(bgr, m, reach);
            foreground(changed).copyTo(bgr(changed), partial);
        }
    }

    if (background.mode == AIBS_BG_TRANSPARENT) {
        alpha = cv::Mat(bgr.size(), CV_8U, cv::Scalar(255));
        m(changed).copyTo(alpha(changed));
        return AIBS_OK;
    }

    cv::Mat layer;
    switch (background.mode) {
        case AIBS_BG_ORIGINAL:
            layer = bgr.clone();
            break;
        case AIBS_BG_COLOR:
            layer = cv::Mat(bgr.size(), CV_8UC3, cv::Scalar(background.color[2], background.color[1], background.color[0]));
            break;
        case AIBS_BG_BLUR: {
            // Blur the background only, so the subject does not bleed into
            // it: blur(image x weight) / blur(weight), weight = background.
            const double sigma =
                blur_sigma(background, matte.source_width, matte.source_height, std::max(scale_x, scale_y));
            cv::Mat weight;
            m.convertTo(weight, CV_32F, -1.0 / 255.0, 1.0);
            cv::Mat image;
            bgr.convertTo(image, CV_32FC3);
            cv::Mat weight3;
            cv::merge(std::vector<cv::Mat>{weight, weight, weight}, weight3);
            const int origin_x = static_cast<int>(std::lround(x * scale_x));
            const int origin_y = static_cast<int>(std::lround(y * scale_y));
            const cv::Mat numerator = wide_blur(image.mul(weight3), sigma, origin_x, origin_y);
            const cv::Mat denominator = wide_blur(weight, sigma, origin_x, origin_y);
            const cv::Mat plain = wide_blur(image, sigma, origin_x, origin_y);
            cv::Mat result(bgr.size(), CV_8UC3);
            for (int row = 0; row < bgr.rows; ++row) {
                const cv::Vec3f* n = numerator.ptr<cv::Vec3f>(row);
                const float* d = denominator.ptr<float>(row);
                const cv::Vec3f* p = plain.ptr<cv::Vec3f>(row);
                cv::Vec3b* o = result.ptr<cv::Vec3b>(row);
                for (int col = 0; col < bgr.cols; ++col) {
                    // Where almost no background is near (deep inside the
                    // subject), fall back to the plain blur; it is hidden
                    // by the subject anyway.
                    const cv::Vec3f v = d[col] > 0.02f ? n[col] / d[col] : p[col];
                    o[col] = cv::Vec3b(cv::saturate_cast<uint8_t>(v[0]), cv::saturate_cast<uint8_t>(v[1]),
                                       cv::saturate_cast<uint8_t>(v[2]));
                }
            }
            layer = result;
            break;
        }
        case AIBS_BG_IMAGE: {
            if (background.image == nullptr || background.image->levels.empty()) return AIBS_ERR_INVALID_ARGUMENT;
            const int32_t status = render_cover(*background.image, matte.source_width, matte.source_height, x, y,
                                                scale_x, scale_y, bgr.cols, bgr.rows, layer);
            if (status != AIBS_OK) return status;
            break;
        }
        default:
            return AIBS_ERR_INVALID_ARGUMENT;
    }

    // Background brightness and saturation (not for a chosen colour, which
    // is shown exactly as picked).
    if (background.mode != AIBS_BG_COLOR && (background.brightness != 0.0f || background.saturation != 0.0f)) {
        aibs_adjustments look{};
        look.exposure = std::clamp(background.brightness, -100.0f, 100.0f) / 100.0f;
        look.saturation = std::clamp(background.saturation, -100.0f, 100.0f);
        const Pipeline pipeline(look);
        cv::parallel_for_(cv::Range(0, layer.rows), [&](const cv::Range& rows) {
            for (int row = rows.start; row < rows.end; ++row) {
                pipeline.apply_bgr(layer.ptr<uint8_t>(row) + 3 * first_col, layer.cols - first_col);
            }
        });
    }

    cv::parallel_for_(cv::Range(0, bgr.rows), [&](const cv::Range& rows) {
        for (int row = rows.start; row < rows.end; ++row) {
            uint8_t* fg = bgr.ptr<uint8_t>(row);
            const uint8_t* bg = layer.ptr<uint8_t>(row);
            const uint8_t* a = m.ptr<uint8_t>(row);
            for (int col = first_col; col < bgr.cols; ++col) {
                const int k = a[col];
                if (k == 255) continue;
                for (int c = 0; c < 3; ++c) {
                    const int i = col * 3 + c;
                    fg[i] = static_cast<uint8_t>((fg[i] * k + bg[i] * (255 - k) + 127) / 255);
                }
            }
        }
    });
    return AIBS_OK;
}

}  // namespace aibs

int32_t aibs_matte_create(const uint8_t* values, uint32_t width, uint32_t height, uint32_t source_width,
                          uint32_t source_height, aibs_matte** out) {
    if (out == nullptr) return AIBS_ERR_INVALID_ARGUMENT;
    *out = nullptr;
    if (values == nullptr || width == 0 || height == 0 || source_width == 0 || source_height == 0 ||
        width > 65536 || height > 65536 || source_width > 1u << 20 || source_height > 1u << 20) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    try {
        auto* matte = new aibs_matte();
        matte->coverage = cv::Mat(static_cast<int>(height), static_cast<int>(width), CV_8U,
                                  const_cast<uint8_t*>(values))
                              .clone();
        matte->source_width = static_cast<int>(source_width);
        matte->source_height = static_cast<int>(source_height);
        *out = matte;
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

int32_t aibs_segment(const aibs_source* source, aibs_ai_session* model, aibs_matte** out) {
    if (out == nullptr) return AIBS_ERR_INVALID_ARGUMENT;
    *out = nullptr;
    if (source == nullptr || source->levels.empty() || model == nullptr) return AIBS_ERR_INVALID_ARGUMENT;
    try {
        const cv::Mat& full = source->levels[0];
        // The model sees the whole image stretched to 1024 x 1024.
        Image input_image;
        int32_t status = aibs_source_render(source, 0, 0, full.cols, full.rows, kModelSize, kModelSize, nullptr,
                                            nullptr, nullptr, nullptr, nullptr, 0, 0, &input_image.img);
        if (status != AIBS_OK) return status;

        // 1 x 3 x 1024 x 1024, planes R, G, B, ImageNet normalisation.
        const size_t plane = static_cast<size_t>(kModelSize) * kModelSize;
        std::vector<float> input(3 * plane);
        const float mean[3] = {0.485f, 0.456f, 0.406f};
        const float stddev[3] = {0.229f, 0.224f, 0.225f};
        for (size_t i = 0; i < plane; ++i) {
            const uint8_t* p = input_image.img.data + i * 4;
            for (int c = 0; c < 3; ++c) input[c * plane + i] = (p[c] / 255.0f - mean[c]) / stddev[c];
        }
        const int64_t shape[4] = {1, 3, kModelSize, kModelSize};
        std::vector<aibs::NamedTensor> outputs;
        status = aibs::run_all_outputs(model, input.data(), shape, 4, outputs);
        if (status != AIBS_OK) return status;

        // The full-resolution prediction (the last 1024 x 1024 output).
        const aibs::NamedTensor* prediction = nullptr;
        for (const aibs::NamedTensor& t : outputs) {
            if (t.data.size() == plane) prediction = &t;
        }
        if (prediction == nullptr) return AIBS_ERR_MODEL_INVALID;
        cv::Mat probability(kModelSize, kModelSize, CV_32F);
        const auto [lo, hi] = std::minmax_element(prediction->data.begin(), prediction->data.end());
        const bool logits = *lo < 0.0f || *hi > 1.0f;
        for (size_t i = 0; i < plane; ++i) {
            const float v = prediction->data[i];
            probability.at<float>(static_cast<int>(i / kModelSize), static_cast<int>(i % kModelSize)) =
                logits ? 1.0f / (1.0f + std::exp(-v)) : v;
        }

        // Refine at up to 2048 px against the image, so edges follow it.
        size_t level_index = 0;
        while (level_index + 1 < source->levels.size() &&
               std::max(source->levels[level_index].cols, source->levels[level_index].rows) > kMatteSide) {
            ++level_index;
        }
        const cv::Mat& level = source->levels[level_index];
        cv::Mat stretched;
        cv::resize(probability, stretched, level.size(), 0, 0, cv::INTER_LINEAR);
        cv::Mat grey;
        cv::cvtColor(level, grey, cv::COLOR_BGR2GRAY);
        grey.convertTo(grey, CV_32F, 1.0 / 255.0);
        // Refine only the uncertain band around the edge, so the confident
        // inside of the subject keeps the model's answer and fine detail
        // there is not smoothed away.
        const int radius = std::max(2, std::max(level.cols, level.rows) / 512);
        const cv::Mat guided_result = guided(grey, stretched, radius, 1e-4);
        cv::Mat uncertain;
        cv::inRange(stretched, cv::Scalar(0.02), cv::Scalar(0.98), uncertain);
        cv::dilate(uncertain, uncertain,
                   cv::getStructuringElement(cv::MORPH_ELLIPSE, cv::Size(4 * radius + 1, 4 * radius + 1)));
        cv::Mat band;
        uncertain.convertTo(band, CV_32F, 1.0 / 255.0);
        band = box(band, radius);
        const cv::Mat refined = stretched + band.mul(guided_result - stretched);

        auto* matte = new aibs_matte();
        refined.convertTo(matte->coverage, CV_8U, 255.0);
        matte->source_width = full.cols;
        matte->source_height = full.rows;
        *out = matte;
        return AIBS_OK;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (const cv::Exception& e) {
        return e.code == cv::Error::StsNoMem ? AIBS_ERR_OUT_OF_MEMORY : AIBS_ERR_INTERNAL;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

float aibs_matte_coverage(const aibs_matte* matte) {
    if (matte == nullptr || matte->coverage.empty()) return 0.0f;
    return static_cast<float>(cv::mean(matte->coverage)[0] / 255.0);
}

void aibs_matte_free(aibs_matte* matte) {
    delete matte;
}

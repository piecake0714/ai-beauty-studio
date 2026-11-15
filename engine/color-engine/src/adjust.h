// Internal colour pipeline (not part of the public C ABI).
#pragma once

#include "aibs/engine.h"

#include <array>
#include <cstdint>
#include <vector>

namespace aibs {

/// A prepared set of adjustments: tables are built once, then applied to
/// many pixels from any number of threads.
class Pipeline {
public:
    explicit Pipeline(const aibs_adjustments& adjustments);

    /// True when the adjustments change nothing; apply() then copies.
    bool identity() const { return identity_; }

    /// Adjust one pixel given as 8-bit R, G, B.
    void apply(uint8_t r, uint8_t g, uint8_t b, uint8_t* out_r, uint8_t* out_g, uint8_t* out_b) const;

    /// Adjust `count` pixels stored as 8-bit B, G, R (OpenCV order), in place.
    void apply_bgr(uint8_t* bgr, int count) const;

private:
    float tone_ratio(float luminance) const;
    float encode(float linear) const;
    uint8_t finish(float encoded) const;
    void colour(float& r, float& g, float& b) const;

    aibs_adjustments a_{};
    bool identity_ = true;
    bool has_tone_ = false;
    bool has_colour_ = false;
    bool has_bands_ = false;
    bool has_hue_ = false;
    float gain_r_ = 1, gain_g_ = 1, gain_b_ = 1;
    float global_saturation_ = 1;
    float vibrance_ = 0;
    std::array<float, 256> to_linear_{};
    std::vector<float> tone_;    // luminance ratio, indexed by sqrt(Y / kMaxLinear)
    std::vector<float> encode_;  // display encoding, indexed by sqrt(v / kMaxLinear)
    std::vector<uint8_t> finish_;  // brightness + contrast as 8-bit output, indexed by display value 0..1
};

}  // namespace aibs

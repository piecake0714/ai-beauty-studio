// Internal look functions (not part of the public C ABI).
#pragma once

#include "aibs/engine.h"

#include <array>
#include <cstdint>
#include <vector>

struct aibs_lut {
    int size = 0;
    // size^3 RGB triples, red fastest.
    std::vector<float> rgb;
};

namespace aibs {

/// The 256-entry table of a curve through `points` ((x, y) pairs, 0..1):
/// monotone cubic (Fritsch-Carlson) between points, flat outside them.
/// Unusable points (not numbers, x not increasing) are dropped; with fewer
/// than 2 left the table is the identity.
std::array<uint8_t, 256> curve_table(const float* points, uint32_t count);

/// A prepared look: tables built once, then applied to many pixels from
/// any number of threads.
class Look {
public:
    explicit Look(const aibs_look& look);

    bool identity() const { return identity_; }

    void apply(uint8_t r, uint8_t g, uint8_t b, uint8_t* out_r, uint8_t* out_g, uint8_t* out_b) const;

    /// `count` pixels stored as 8-bit B, G, R, in place.
    void apply_bgr(uint8_t* bgr, int count) const;

private:
    bool identity_ = true;
    bool has_grade_ = false;
    bool has_curves_ = false;
    // Grading: RGB change (0..255 units) per display luma 0..255.
    std::vector<std::array<float, 3>> grade_;
    // Curves: per channel, master applied first.
    std::array<std::array<uint8_t, 256>, 3> curve_{};
    const aibs_lut* lut_ = nullptr;
    float lut_amount_ = 0;
};

}  // namespace aibs

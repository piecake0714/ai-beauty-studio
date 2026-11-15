// Internal access to AI sessions for engine modules (not part of the C ABI).
#pragma once

#include "aibs/engine.h"

#include <cstdint>
#include <string>
#include <vector>

namespace aibs {

struct NamedTensor {
    std::string name;
    std::vector<int64_t> shape;
    std::vector<float> data;
};

/// Run a session with one float input and return every float output.
int32_t run_all_outputs(aibs_ai_session* session, const float* input, const int64_t* shape, size_t rank,
                        std::vector<NamedTensor>& outputs);

/// Run a session with one float tensor per model input (in the model's
/// input order) and return every float output.
int32_t run_inputs(aibs_ai_session* session, const float* const* inputs, const int64_t* const* shapes,
                   const size_t* ranks, size_t input_count, std::vector<NamedTensor>& outputs);

}  // namespace aibs

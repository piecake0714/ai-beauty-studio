// Tests for hardware detection and the AI runtime, using the self-test
// model from models/files (Rec. 709 luma of an RGB image).
#include "aibs/engine.h"
#include "check.h"

#include <chrono>
#include <cmath>

#ifndef AIBS_MODEL_DIR
#error "AIBS_MODEL_DIR must be defined by the build"
#endif

namespace {

std::vector<uint8_t> read_model(const char* name) {
    const std::string path = std::string(AIBS_MODEL_DIR) + "/" + name;
    std::ifstream file(path, std::ios::binary);
    if (!file) {
        std::printf("cannot open model %s\n", path.c_str());
        std::exit(2);
    }
    return std::vector<uint8_t>(std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>());
}

struct Session {
    aibs_ai_session* ptr = nullptr;
    ~Session() { aibs_ai_close(ptr); }
};

const char* provider_name(int32_t provider) {
    return provider == AIBS_PROVIDER_DIRECTML ? "DirectML" : "CPU";
}

void test_gpu_list() {
    const uint32_t count = aibs_gpu_list(nullptr, 0);
    std::vector<aibs_gpu_info> gpus(count);
    CHECK(aibs_gpu_list(gpus.data(), count) == count);
    for (const aibs_gpu_info& gpu : gpus) {
        CHECK(gpu.name[0] != '\0');
        std::printf("info: GPU \"%s\" vendor 0x%04X software=%u dedicated=%llu MB\n", gpu.name, gpu.vendor_id,
                    gpu.is_software, static_cast<unsigned long long>(gpu.dedicated_memory / (1024 * 1024)));
    }
#if defined(_WIN32)
    CHECK(count >= 1);  // Windows always has at least the basic render driver
#endif
}

// Runs the model on a width x height image and checks every output value.
void check_luma(aibs_ai_session* session, int64_t width, int64_t height, const char* label, int32_t provider) {
    const size_t plane = static_cast<size_t>(width * height);
    std::vector<float> input(3 * plane);
    for (size_t i = 0; i < input.size(); ++i) {
        input[i] = static_cast<float>((i * 2654435761u) % 1000) / 999.0f;
    }
    const int64_t shape[4] = {1, 3, height, width};
    float* output = nullptr;
    int64_t out_shape[8] = {};
    size_t out_rank = 0;
    const auto start = std::chrono::steady_clock::now();
    CHECK(aibs_ai_run(session, input.data(), shape, 4, &output, out_shape, &out_rank) == AIBS_OK);
    const double ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count();
    CHECK(out_rank == 4 && out_shape[0] == 1 && out_shape[1] == 1 && out_shape[2] == height && out_shape[3] == width);
    if (output != nullptr) {
        double worst = 0;
        for (size_t i = 0; i < plane; ++i) {
            const double expected = 0.2126 * input[i] + 0.7152 * input[plane + i] + 0.0722 * input[2 * plane + i];
            worst = std::max(worst, std::abs(expected - output[i]));
        }
        // GPUs may compute in reduced precision; 1e-3 is well under one 8-bit level.
        CHECK(worst < 1e-3);
        std::printf("timing: %s %lldx%lld on %s took %.1f ms (max error %.2g)\n", label,
                    static_cast<long long>(width), static_cast<long long>(height), provider_name(provider), ms, worst);
    }
    aibs_ai_free(output);
}

void test_cpu_session() {
    const std::vector<uint8_t> model = read_model("test-luma-v1.onnx");
    Session session;
    int32_t used = -1;
    CHECK(aibs_ai_open(model.data(), model.size(), AIBS_PROVIDER_CPU, &session.ptr, &used, nullptr, 0) == AIBS_OK);
    CHECK(used == AIBS_PROVIDER_CPU);
    if (session.ptr != nullptr) {
        check_luma(session.ptr, 7, 5, "luma", used);
        check_luma(session.ptr, 1024, 1024, "luma", used);
    }
    std::printf("info: ONNX Runtime %s\n", aibs_ai_runtime_version());
}

void test_directml_or_fallback() {
    const std::vector<uint8_t> model = read_model("test-luma-v1.onnx");
    Session session;
    int32_t used = -1;
    char note[512] = "x";
    CHECK(aibs_ai_open(model.data(), model.size(), AIBS_PROVIDER_DIRECTML, &session.ptr, &used, note, sizeof(note)) ==
          AIBS_OK);
    // Either DirectML works, or it fell back to CPU and said why.
    CHECK(used == AIBS_PROVIDER_DIRECTML || (used == AIBS_PROVIDER_CPU && note[0] != '\0'));
    std::printf("info: DirectML requested, %s used%s%s\n", provider_name(used), note[0] ? "; " : "", note);
    if (session.ptr != nullptr) {
        check_luma(session.ptr, 7, 5, "luma", used);
        check_luma(session.ptr, 1024, 1024, "luma", used);
    }
}

void test_bad_input_is_rejected() {
    const uint8_t junk[] = {1, 2, 3, 4, 5, 6, 7, 8};
    Session session;
    int32_t used = -1;
    char note[256] = "";
    CHECK(aibs_ai_open(junk, sizeof(junk), AIBS_PROVIDER_CPU, &session.ptr, &used, note, sizeof(note)) ==
          AIBS_ERR_MODEL_INVALID);
    CHECK(session.ptr == nullptr && note[0] != '\0');
    CHECK(aibs_ai_open(junk, sizeof(junk), 7, &session.ptr, &used, nullptr, 0) == AIBS_ERR_INVALID_ARGUMENT);

    const std::vector<uint8_t> model = read_model("test-luma-v1.onnx");
    Session good;
    CHECK(aibs_ai_open(model.data(), model.size(), AIBS_PROVIDER_CPU, &good.ptr, &used, nullptr, 0) == AIBS_OK);
    const float value = 0.5f;
    const int64_t wrong_channels[4] = {1, 1, 1, 1};
    float* output = nullptr;
    int64_t out_shape[8] = {};
    size_t out_rank = 0;
    CHECK(aibs_ai_run(good.ptr, &value, wrong_channels, 4, &output, out_shape, &out_rank) ==
          AIBS_ERR_INFERENCE_FAILED);
    CHECK(output == nullptr);
    const int64_t zero[4] = {1, 3, 0, 4};
    CHECK(aibs_ai_run(good.ptr, &value, zero, 4, &output, out_shape, &out_rank) == AIBS_ERR_INVALID_ARGUMENT);
}

}  // namespace

void run_ai_tests() {
    test_gpu_list();
    test_cpu_session();
    test_directml_or_fallback();
    test_bad_input_is_rejected();
}

#include "aibs/engine.h"
#include "session_internal.h"

// dml_provider_factory.h pulls in windows.h; keep its min/max macros out.
#ifndef NOMINMAX
#define NOMINMAX
#endif

#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <memory>
#include <string>
#include <vector>

// The C++ API's global API pointer is set up by us after the runtime DLL is
// loaded from the right folder (see load_runtime), not at program start.
#define ORT_API_MANUAL_INIT
#include <onnxruntime_cxx_api.h>
#if defined(_WIN32)
#include <dml_provider_factory.h>
#include <windows.h>
#endif

struct aibs_ai_session {
    std::unique_ptr<Ort::Session> session;
    std::vector<std::string> input_names;
    std::vector<std::string> output_names;
};

namespace {

#if defined(_WIN32)
// Folder containing this engine DLL.
std::wstring engine_folder() {
    HMODULE self = nullptr;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(&engine_folder), &self);
    std::wstring path(32768, L'\0');
    const DWORD n = GetModuleFileNameW(self, path.data(), static_cast<DWORD>(path.size()));
    path.resize(n);
    const size_t slash = path.find_last_of(L"\\/");
    return slash == std::wstring::npos ? std::wstring() : path.substr(0, slash + 1);
}
#endif

/*
 * Load ONNX Runtime and DirectML from the engine's own folder.
 *
 * Windows ships its own (older) onnxruntime.dll and DirectML.dll in
 * System32, which the normal DLL search would find before ours whenever
 * the program's folder does not hold them (for example test runners).
 * onnxruntime.dll is delay-loaded by the build; loading both by full path
 * first makes every later lookup by name return our copies.
 */
bool load_runtime() {
    static const bool loaded = [] {
#if defined(_WIN32)
        const std::wstring folder = engine_folder();
        LoadLibraryExW((folder + L"DirectML.dll").c_str(), nullptr, LOAD_WITH_ALTERED_SEARCH_PATH);
        if (LoadLibraryExW((folder + L"onnxruntime.dll").c_str(), nullptr, LOAD_WITH_ALTERED_SEARCH_PATH) == nullptr) {
            return false;
        }
#endif
        const OrtApi* api = OrtGetApiBase()->GetApi(ORT_API_VERSION);
        if (api == nullptr) {
            return false;  // the loaded runtime is older than the headers
        }
        Ort::InitApi(api);
        return true;
    }();
    return loaded;
}

Ort::Env& environment() {
    // One ONNX Runtime environment for the whole process.
    static Ort::Env env(ORT_LOGGING_LEVEL_WARNING, "aibs");
    return env;
}

void copy_note(const std::string& text, char* note, size_t note_len) {
    if (note == nullptr || note_len == 0) {
        return;
    }
    const size_t n = std::min(text.size(), note_len - 1);
    std::memcpy(note, text.data(), n);
    note[n] = '\0';
}

Ort::SessionOptions base_options() {
    Ort::SessionOptions options;
    options.SetGraphOptimizationLevel(GraphOptimizationLevel::ORT_ENABLE_ALL);
    return options;
}

}  // namespace

const char* aibs_ai_runtime_version(void) {
    return load_runtime() ? OrtGetApiBase()->GetVersionString() : "unavailable";
}

int32_t aibs_ai_open(const uint8_t* model, size_t len, int32_t preferred, aibs_ai_session** out,
                     int32_t* used_provider, char* note, size_t note_len) {
    if (out == nullptr || used_provider == nullptr) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    *out = nullptr;
    *used_provider = AIBS_PROVIDER_CPU;
    copy_note("", note, note_len);
    if (model == nullptr || len == 0 ||
        (preferred != AIBS_PROVIDER_CPU && preferred != AIBS_PROVIDER_DIRECTML)) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    if (!load_runtime()) {
        copy_note("ONNX Runtime could not be loaded from the application folder", note, note_len);
        return AIBS_ERR_INTERNAL;
    }

    try {
        std::unique_ptr<Ort::Session> session;
        int32_t used = AIBS_PROVIDER_CPU;

        if (preferred == AIBS_PROVIDER_DIRECTML) {
#if defined(_WIN32)
            try {
                // DirectML requires these two settings.
                Ort::SessionOptions options = base_options();
                options.DisableMemPattern();
                options.SetExecutionMode(ExecutionMode::ORT_SEQUENTIAL);
                Ort::ThrowOnError(OrtSessionOptionsAppendExecutionProvider_DML(options, 0));
                session = std::make_unique<Ort::Session>(environment(), model, len, options);
                used = AIBS_PROVIDER_DIRECTML;
            } catch (const Ort::Exception& e) {
                copy_note(std::string("DirectML unavailable: ") + e.what(), note, note_len);
            }
#else
            copy_note("DirectML is only available on Windows", note, note_len);
#endif
        }
        if (!session) {
            Ort::SessionOptions options = base_options();
            session = std::make_unique<Ort::Session>(environment(), model, len, options);
        }

        if (session->GetInputCount() == 0 || session->GetInputCount() > 8 || session->GetOutputCount() == 0) {
            return AIBS_ERR_MODEL_INVALID;
        }
        Ort::AllocatorWithDefaultOptions allocator;
        auto handle = std::make_unique<aibs_ai_session>();
        for (size_t i = 0; i < session->GetInputCount(); ++i) {
            handle->input_names.push_back(session->GetInputNameAllocated(i, allocator).get());
        }
        for (size_t o = 0; o < session->GetOutputCount(); ++o) {
            handle->output_names.push_back(session->GetOutputNameAllocated(o, allocator).get());
        }
        handle->session = std::move(session);
        *used_provider = used;
        *out = handle.release();
        return AIBS_OK;
    } catch (const Ort::Exception& e) {
        copy_note(e.what(), note, note_len);
        return AIBS_ERR_MODEL_INVALID;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

namespace aibs {

int32_t run_all_outputs(aibs_ai_session* session, const float* input, const int64_t* shape, size_t rank,
                        std::vector<NamedTensor>& outputs) {
    return run_inputs(session, &input, &shape, &rank, 1, outputs);
}

int32_t run_inputs(aibs_ai_session* session, const float* const* inputs, const int64_t* const* shapes,
                   const size_t* ranks, size_t input_count, std::vector<NamedTensor>& outputs) {
    outputs.clear();
    if (session == nullptr || inputs == nullptr || shapes == nullptr || ranks == nullptr ||
        input_count != session->input_names.size()) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    std::vector<size_t> counts(input_count, 1);
    for (size_t k = 0; k < input_count; ++k) {
        if (inputs[k] == nullptr || shapes[k] == nullptr || ranks[k] == 0 || ranks[k] > 8) {
            return AIBS_ERR_INVALID_ARGUMENT;
        }
        for (size_t i = 0; i < ranks[k]; ++i) {
            const int64_t d = shapes[k][i];
            if (d <= 0 || static_cast<uint64_t>(d) > std::numeric_limits<size_t>::max() / counts[k]) {
                return AIBS_ERR_INVALID_ARGUMENT;
            }
            counts[k] *= static_cast<size_t>(d);
        }
    }
    try {
        const Ort::MemoryInfo memory = Ort::MemoryInfo::CreateCpu(OrtArenaAllocator, OrtMemTypeDefault);
        std::vector<Ort::Value> tensors;
        std::vector<const char*> input_names;
        for (size_t k = 0; k < input_count; ++k) {
            tensors.push_back(Ort::Value::CreateTensor<float>(memory, const_cast<float*>(inputs[k]), counts[k], shapes[k],
                                                              ranks[k]));
            input_names.push_back(session->input_names[k].c_str());
        }
        std::vector<const char*> output_names;
        for (const std::string& name : session->output_names) {
            output_names.push_back(name.c_str());
        }
        std::vector<Ort::Value> results =
            session->session->Run(Ort::RunOptions{nullptr}, input_names.data(), tensors.data(), tensors.size(),
                                  output_names.data(), output_names.size());
        for (size_t o = 0; o < results.size(); ++o) {
            if (!results[o].IsTensor()) {
                return AIBS_ERR_INFERENCE_FAILED;
            }
            const Ort::TensorTypeAndShapeInfo info = results[o].GetTensorTypeAndShapeInfo();
            if (info.GetElementType() != ONNX_TENSOR_ELEMENT_DATA_TYPE_FLOAT) {
                return AIBS_ERR_INFERENCE_FAILED;
            }
            NamedTensor named;
            named.name = session->output_names[o];
            named.shape = info.GetShape();
            const float* data = results[o].GetTensorData<float>();
            named.data.assign(data, data + info.GetElementCount());
            outputs.push_back(std::move(named));
        }
        return AIBS_OK;
    } catch (const Ort::Exception&) {
        return AIBS_ERR_INFERENCE_FAILED;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

}  // namespace aibs

namespace {

// Copy the first output into an engine-allocated buffer for the caller.
int32_t first_output(const std::vector<aibs::NamedTensor>& outputs, float** output, int64_t* out_shape,
                     size_t* out_rank) {
    const aibs::NamedTensor& first = outputs.at(0);
    if (first.shape.size() > 8) return AIBS_ERR_INFERENCE_FAILED;
    float* buffer = static_cast<float*>(std::malloc(std::max<size_t>(first.data.size(), 1) * sizeof(float)));
    if (buffer == nullptr) return AIBS_ERR_OUT_OF_MEMORY;
    std::memcpy(buffer, first.data.data(), first.data.size() * sizeof(float));
    for (size_t d = 0; d < first.shape.size(); ++d) out_shape[d] = first.shape[d];
    *out_rank = first.shape.size();
    *output = buffer;
    return AIBS_OK;
}

}  // namespace

int32_t aibs_ai_run_inputs(aibs_ai_session* session, const float* const* inputs, const int64_t* const* shapes,
                           const size_t* ranks, size_t input_count, float** output, int64_t* out_shape,
                           size_t* out_rank) {
    if (output != nullptr) *output = nullptr;
    if (output == nullptr || out_shape == nullptr || out_rank == nullptr) return AIBS_ERR_INVALID_ARGUMENT;
    std::vector<aibs::NamedTensor> outputs;
    const int32_t status = aibs::run_inputs(session, inputs, shapes, ranks, input_count, outputs);
    if (status != AIBS_OK) return status;
    return first_output(outputs, output, out_shape, out_rank);
}

int32_t aibs_ai_run(aibs_ai_session* session, const float* input, const int64_t* shape, size_t rank,
                    float** output, int64_t* out_shape, size_t* out_rank) {
    if (output != nullptr) {
        *output = nullptr;
    }
    if (output == nullptr || out_shape == nullptr || out_rank == nullptr) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    std::vector<aibs::NamedTensor> outputs;
    const int32_t status = aibs::run_all_outputs(session, input, shape, rank, outputs);
    if (status != AIBS_OK) {
        return status;
    }
    // The single-output API returns the first output.
    const aibs::NamedTensor& first = outputs.at(0);
    if (first.shape.size() > 8) {
        return AIBS_ERR_INFERENCE_FAILED;
    }
    float* buffer = static_cast<float*>(std::malloc(std::max<size_t>(first.data.size(), 1) * sizeof(float)));
    if (buffer == nullptr) {
        return AIBS_ERR_OUT_OF_MEMORY;
    }
    std::memcpy(buffer, first.data.data(), first.data.size() * sizeof(float));
    for (size_t d = 0; d < first.shape.size(); ++d) {
        out_shape[d] = first.shape[d];
    }
    *out_rank = first.shape.size();
    *output = buffer;
    return AIBS_OK;
}

void aibs_ai_free(float* data) {
    std::free(data);
}

void aibs_ai_close(aibs_ai_session* session) {
    delete session;
}

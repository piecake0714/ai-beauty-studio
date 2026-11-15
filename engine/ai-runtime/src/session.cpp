#include "aibs/engine.h"

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
    std::string input_name;
    std::string output_name;
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

        if (session->GetInputCount() != 1 || session->GetOutputCount() != 1) {
            return AIBS_ERR_MODEL_INVALID;
        }
        Ort::AllocatorWithDefaultOptions allocator;
        auto handle = std::make_unique<aibs_ai_session>();
        handle->input_name = session->GetInputNameAllocated(0, allocator).get();
        handle->output_name = session->GetOutputNameAllocated(0, allocator).get();
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

int32_t aibs_ai_run(aibs_ai_session* session, const float* input, const int64_t* shape, size_t rank,
                    float** output, int64_t* out_shape, size_t* out_rank) {
    if (output != nullptr) {
        *output = nullptr;
    }
    if (session == nullptr || input == nullptr || shape == nullptr || rank == 0 || rank > 8 ||
        output == nullptr || out_shape == nullptr || out_rank == nullptr) {
        return AIBS_ERR_INVALID_ARGUMENT;
    }
    size_t count = 1;
    for (size_t i = 0; i < rank; ++i) {
        if (shape[i] <= 0 || static_cast<uint64_t>(shape[i]) > std::numeric_limits<size_t>::max() / count) {
            return AIBS_ERR_INVALID_ARGUMENT;
        }
        count *= static_cast<size_t>(shape[i]);
    }

    try {
        const Ort::MemoryInfo memory = Ort::MemoryInfo::CreateCpu(OrtArenaAllocator, OrtMemTypeDefault);
        Ort::Value tensor = Ort::Value::CreateTensor<float>(memory, const_cast<float*>(input), count, shape, rank);
        const char* input_names[] = {session->input_name.c_str()};
        const char* output_names[] = {session->output_name.c_str()};
        std::vector<Ort::Value> results =
            session->session->Run(Ort::RunOptions{nullptr}, input_names, &tensor, 1, output_names, 1);
        if (results.size() != 1 || !results[0].IsTensor()) {
            return AIBS_ERR_INFERENCE_FAILED;
        }
        const Ort::TensorTypeAndShapeInfo info = results[0].GetTensorTypeAndShapeInfo();
        if (info.GetElementType() != ONNX_TENSOR_ELEMENT_DATA_TYPE_FLOAT) {
            return AIBS_ERR_INFERENCE_FAILED;
        }
        const std::vector<int64_t> dims = info.GetShape();
        if (dims.size() > 8) {
            return AIBS_ERR_INFERENCE_FAILED;
        }
        const size_t elements = info.GetElementCount();
        float* buffer = static_cast<float*>(std::malloc(std::max<size_t>(elements, 1) * sizeof(float)));
        if (buffer == nullptr) {
            return AIBS_ERR_OUT_OF_MEMORY;
        }
        std::memcpy(buffer, results[0].GetTensorData<float>(), elements * sizeof(float));
        for (size_t d = 0; d < dims.size(); ++d) {
            out_shape[d] = dims[d];
        }
        *out_rank = dims.size();
        *output = buffer;
        return AIBS_OK;
    } catch (const Ort::Exception&) {
        return AIBS_ERR_INFERENCE_FAILED;
    } catch (const std::bad_alloc&) {
        return AIBS_ERR_OUT_OF_MEMORY;
    } catch (...) {
        return AIBS_ERR_INTERNAL;
    }
}

void aibs_ai_free(float* data) {
    std::free(data);
}

void aibs_ai_close(aibs_ai_session* session) {
    delete session;
}

#include "aibs/engine.h"

#include <cstring>

#if defined(_WIN32)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <dxgi1_4.h>
#include <windows.h>
#include <wrl/client.h>

uint32_t aibs_gpu_list(aibs_gpu_info* out, uint32_t capacity) {
    Microsoft::WRL::ComPtr<IDXGIFactory1> factory;
    if (FAILED(CreateDXGIFactory1(IID_PPV_ARGS(&factory)))) {
        return 0;
    }
    uint32_t count = 0;
    Microsoft::WRL::ComPtr<IDXGIAdapter1> adapter;
    for (UINT index = 0; factory->EnumAdapters1(index, &adapter) != DXGI_ERROR_NOT_FOUND; ++index) {
        DXGI_ADAPTER_DESC1 desc{};
        if (FAILED(adapter->GetDesc1(&desc))) {
            continue;
        }
        if (out != nullptr && count < capacity) {
            aibs_gpu_info& info = out[count];
            std::memset(&info, 0, sizeof(info));
            WideCharToMultiByte(CP_UTF8, 0, desc.Description, -1, info.name, static_cast<int>(sizeof(info.name)),
                                nullptr, nullptr);
            info.name[sizeof(info.name) - 1] = '\0';
            info.vendor_id = desc.VendorId;
            info.is_software = (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE) != 0 ? 1u : 0u;
            info.dedicated_memory = desc.DedicatedVideoMemory;
            info.shared_memory = desc.SharedSystemMemory;
        }
        ++count;
    }
    return count;
}

#else

// Only Windows is supported; other systems report no adapters.
uint32_t aibs_gpu_list(aibs_gpu_info* out, uint32_t capacity) {
    (void)out;
    (void)capacity;
    return 0;
}

#endif

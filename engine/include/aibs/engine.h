/*
 * AI Beauty Studio engine: public C ABI.
 *
 * This header is the only doorway between the Rust core and the C++ engine.
 * No C++ type may appear here. Memory returned by the engine must be released
 * by the engine (aibs_image_free), never by the caller's allocator.
 */
#ifndef AIBS_ENGINE_H
#define AIBS_ENGINE_H

#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32)
#  if defined(AIBS_BUILD)
#    define AIBS_API __declspec(dllexport)
#  else
#    define AIBS_API __declspec(dllimport)
#  endif
#else
#  define AIBS_API __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* Status codes returned by every engine call. */
#define AIBS_OK 0
#define AIBS_ERR_INVALID_ARGUMENT 1
#define AIBS_ERR_DECODE_FAILED 2
#define AIBS_ERR_OUT_OF_MEMORY 3
#define AIBS_ERR_INTERNAL 4

/* An 8-bit RGBA image owned by the engine. */
typedef struct aibs_image {
    uint8_t* data;          /* width * height * 4 bytes, row-major, no padding */
    uint32_t width;         /* size of the returned pixels */
    uint32_t height;
    uint32_t source_width;  /* size of the file's image before any downscale */
    uint32_t source_height;
} aibs_image;

/* Engine version string, statically allocated. */
AIBS_API const char* aibs_engine_version(void);

/*
 * Decode an encoded image (JPEG, PNG, TIFF, WebP) held in memory to RGBA8.
 *
 * max_dimension: when non-zero and the image's longer side exceeds it, the
 * result is downscaled so the longer side equals max_dimension. Zero keeps
 * the full size.
 *
 * The input bytes are only read. On success *out is filled and must be
 * released with aibs_image_free. On failure *out is zeroed.
 */
AIBS_API int32_t aibs_decode_rgba8(const uint8_t* bytes, size_t len,
                                   uint32_t max_dimension, aibs_image* out);

/* Release an image returned by the engine. Safe on a zeroed struct. */
AIBS_API void aibs_image_free(aibs_image* image);

/* Number of colour bands for HSL adjustments, in order: red, orange,
 * yellow, green, aqua, blue, purple, magenta. */
#define AIBS_HSL_BANDS 8

/*
 * Light and colour adjustments; all zero leaves the image unchanged.
 * Values outside their range are clamped; non-numbers count as zero.
 *
 * Order of application: white balance and exposure in linear light; the
 * tone controls (highlights, shadows, whites, blacks) on luminance, keeping
 * hue; then, on display-encoded values, HSL, saturation and vibrance; then
 * brightness and contrast per channel.
 */
typedef struct aibs_adjustments {
    float exposure;     /* stops (EV), -5 to +5; +1 doubles linear light */
    float brightness;   /* -100 to +100; midtones up or down, keeps black and white */
    float contrast;     /* -100 to +100; around middle grey */
    float highlights;   /* -100 to +100; mostly the bright tones */
    float shadows;      /* -100 to +100; mostly the dark tones */
    float whites;       /* -100 to +100; the brightest tones and clipping point */
    float blacks;       /* -100 to +100; the darkest tones and black point */
    float temperature;  /* -100 (cooler, bluer) to +100 (warmer, yellower) */
    float tint;         /* -100 (greener) to +100 (more magenta) */
    float vibrance;     /* -100 to +100; acts more on less saturated colours */
    float saturation;   /* -100 (grey) to +100 */
    float hsl_hue[AIBS_HSL_BANDS];         /* -100 to +100, up to 30 degrees */
    float hsl_saturation[AIBS_HSL_BANDS];  /* -100 to +100 */
    float hsl_luminance[AIBS_HSL_BANDS];   /* -100 to +100 */
} aibs_adjustments;

/* Apply adjustments to pixel_count packed 8-bit RGB pixels, exactly as
 * rendering does. in and out may be the same buffer. */
AIBS_API int32_t aibs_adjust_rgb8(const aibs_adjustments* adjustments,
                                  const uint8_t* in, uint8_t* out, size_t pixel_count);

/*
 * A decoded image kept at full resolution inside the engine, with a
 * precomputed pyramid of smaller copies so any region can be shown quickly
 * at any zoom. Read-only once opened: several threads may render from the
 * same source at once.
 */
typedef struct aibs_source aibs_source;

/* Decode encoded bytes (JPEG, PNG, TIFF, WebP) into a new source. On
 * success *out must be released with aibs_source_free. */
AIBS_API int32_t aibs_source_open(const uint8_t* bytes, size_t len, aibs_source** out);

/* Full-resolution size of the source. */
AIBS_API void aibs_source_size(const aibs_source* source, uint32_t* width, uint32_t* height);

/*
 * Render the region of the source starting at (x, y) with size (w, h),
 * in source pixels, into an out_width x out_height RGBA8 image. The region
 * may extend past the source's edges; output pixels outside the source get
 * alpha 0. Enlarging uses nearest-neighbour so individual pixels stay
 * visible; reducing uses the pyramid and bilinear filtering.
 *
 * adjustments may be NULL (no adjustment). The leftmost original_columns
 * output columns are left unadjusted, for a before/after split view; 0
 * adjusts everything, out_width or more shows the original only.
 */
AIBS_API int32_t aibs_source_render(const aibs_source* source,
                                    double x, double y, double w, double h,
                                    uint32_t out_width, uint32_t out_height,
                                    const aibs_adjustments* adjustments,
                                    uint32_t original_columns,
                                    aibs_image* out);

/* Release a source. Safe on NULL. */
AIBS_API void aibs_source_free(aibs_source* source);

/* ---------------------------------------------------------------------
 * Hardware detection
 * ------------------------------------------------------------------- */

typedef struct aibs_gpu_info {
    char name[256];             /* UTF-8, NUL-terminated */
    uint32_t vendor_id;         /* PCI vendor id, e.g. 0x10DE NVIDIA, 0x1002 AMD, 0x8086 Intel */
    uint32_t is_software;       /* 1 for a software renderer (no real GPU) */
    uint64_t dedicated_memory;  /* bytes of dedicated video memory */
    uint64_t shared_memory;     /* bytes of system memory the GPU may use */
} aibs_gpu_info;

/* Fill up to `capacity` entries with the system's display adapters, in the
 * order the system prefers them, and return how many exist. */
AIBS_API uint32_t aibs_gpu_list(aibs_gpu_info* out, uint32_t capacity);

/* ---------------------------------------------------------------------
 * AI runtime (ONNX Runtime)
 * ------------------------------------------------------------------- */

#define AIBS_PROVIDER_CPU 0
#define AIBS_PROVIDER_DIRECTML 1

#define AIBS_ERR_MODEL_INVALID 10
#define AIBS_ERR_INFERENCE_FAILED 11

/* A loaded model. Runs may be called from several threads. */
typedef struct aibs_ai_session aibs_ai_session;

/* ONNX Runtime version string, statically allocated. */
AIBS_API const char* aibs_ai_runtime_version(void);

/*
 * Load a model from memory. `preferred` is AIBS_PROVIDER_DIRECTML (GPU,
 * falling back to CPU if it cannot be used) or AIBS_PROVIDER_CPU.
 * *used_provider receives the provider actually in use. When DirectML was
 * wanted but not used, `note` (if not NULL) receives the reason, UTF-8,
 * truncated to note_len bytes including the NUL.
 */
AIBS_API int32_t aibs_ai_open(const uint8_t* model, size_t len, int32_t preferred,
                              aibs_ai_session** out, int32_t* used_provider,
                              char* note, size_t note_len);

/*
 * Run a model with one float32 input and one float32 output. `shape` has
 * `rank` dimensions. On success *output points to the result (release with
 * aibs_ai_free) and out_shape receives up to 8 dimensions, *out_rank the
 * count.
 */
AIBS_API int32_t aibs_ai_run(aibs_ai_session* session,
                             const float* input, const int64_t* shape, size_t rank,
                             float** output, int64_t* out_shape, size_t* out_rank);

AIBS_API void aibs_ai_free(float* data);
AIBS_API void aibs_ai_close(aibs_ai_session* session);

#ifdef __cplusplus
}
#endif

#endif /* AIBS_ENGINE_H */

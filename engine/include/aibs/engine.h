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

/*
 * Tone adjustments, all zero = unchanged. Applied per colour channel in this
 * order: exposure in linear light, then brightness and contrast on the
 * display-encoded (sRGB) value.
 */
typedef struct aibs_adjustments {
    float exposure;    /* stops (EV), -5 to +5; +1 doubles linear light */
    float brightness;  /* -100 to +100; lifts or lowers midtones, keeps black and white */
    float contrast;    /* -100 to +100; steepens or flattens tones around middle grey */
} aibs_adjustments;

/* Fill lut[256] with the 8-bit result of applying the adjustments to each
 * 8-bit sRGB input value. Out-of-range values are clamped first. */
AIBS_API int32_t aibs_adjustments_lut(const aibs_adjustments* adjustments, uint8_t* lut);

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

#ifdef __cplusplus
}
#endif

#endif /* AIBS_ENGINE_H */

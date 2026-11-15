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
 * A detected face, in source pixel coordinates. Landmarks are, in order:
 * right eye, left eye, nose tip, right mouth corner, left mouth corner
 * (as seen in the image: "right eye" is the person's right, usually on the
 * image's left), each as x, y.
 */
typedef struct aibs_face {
    float x, y, width, height;
    float landmarks[10];
    float score;          /* 0..1 */
    /* This face's skin colour, measured in YCbCr on the cheeks:
     * mean Cb, mean Cr, variance Cb, variance Cr, covariance. */
    float skin[5];
} aibs_face;

/* Portrait retouching applied while rendering. */
typedef struct aibs_portrait {
    const aibs_face* faces;
    uint32_t face_count;
    float smoothing;      /* 0 (off) to 100 */
} aibs_portrait;

/* A decoded image; see aibs_source_open below. */
typedef struct aibs_source aibs_source;

/*
 * Subject/background separation ("matte"): per-pixel subject coverage,
 * 0 (background) to 255 (subject), for a whole source. Created by
 * aibs_segment (AI) or aibs_matte_create (given values); read-only after.
 */
typedef struct aibs_matte aibs_matte;

#define AIBS_BG_ORIGINAL 0     /* keep the background (adjustable) */
#define AIBS_BG_TRANSPARENT 1  /* remove it: output alpha is the matte */
#define AIBS_BG_COLOR 2        /* a solid colour */
#define AIBS_BG_BLUR 3         /* the original background, blurred */
#define AIBS_BG_IMAGE 4        /* another image, scaled to cover */

/* Background treatment applied while rendering, after all other edits. */
typedef struct aibs_background {
    const aibs_matte* matte;
    uint32_t mode;             /* AIBS_BG_* */
    uint8_t color[3];          /* R, G, B for AIBS_BG_COLOR */
    uint8_t reserved;
    float blur;                /* 0..100, for AIBS_BG_BLUR */
    float brightness;          /* -100..100, background only (-1..+1 EV) */
    float saturation;          /* -100..100, background only */
    float edge;                /* -100 (pull the edge in) .. 100 (push it out) */
    const aibs_source* image;  /* for AIBS_BG_IMAGE */
} aibs_background;

/*
 * The 3D shape of a photo: surface directions estimated from AI depth, for
 * lighting. Created by aibs_estimate_shape or aibs_shape_create.
 */
typedef struct aibs_shape aibs_shape;

#define AIBS_LIGHT_NONE 0
#define AIBS_LIGHT_NATURAL 1    /* broad, warm, like sun or a window */
#define AIBS_LIGHT_DIFFUSE 2    /* very soft and wide, lifts shadows */
#define AIBS_LIGHT_REFLECTOR 3  /* fill: brightens dark areas near it */
#define AIBS_LIGHT_SOFTBOX 4    /* soft but concentrated key light */

/*
 * A light added to the photo. With a shape, surfaces facing the light get
 * brighter and those facing away darker (shading; no cast shadows).
 * Without one, the photo is treated as flat.
 */
typedef struct aibs_light {
    uint32_t type;             /* AIBS_LIGHT_* */
    float x, y;                /* position as a fraction of width / height */
    float intensity;           /* 0..100 */
    float size;                /* 0..100: how far it reaches */
    float warmth;              /* -100 (cool) .. 100 (warm) */
    float height;              /* 0 (grazing, from the side) .. 100 (from the camera) */
    const aibs_matte* subject; /* if not NULL, light only the subject */
    const aibs_shape* shape;   /* may be NULL */
} aibs_light;

/*
 * A decoded image kept at full resolution inside the engine, with a
 * precomputed pyramid of smaller copies so any region can be shown quickly
 * at any zoom. Read-only once opened: several threads may render from the
 * same source at once.
 */

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
 * portrait (may be NULL) retouches skin first; adjustments (may be NULL)
 * follow, then light (may be NULL); background (may be NULL) is
 * composited last. The leftmost original_columns output columns get none
 * of them, for a before/after split view; 0 changes everything, out_width
 * or more shows the original only.
 */
AIBS_API int32_t aibs_source_render(const aibs_source* source,
                                    double x, double y, double w, double h,
                                    uint32_t out_width, uint32_t out_height,
                                    const aibs_portrait* portrait,
                                    const aibs_adjustments* adjustments,
                                    const aibs_background* background,
                                    const aibs_light* light,
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

/* ---------------------------------------------------------------------
 * Portrait
 * ------------------------------------------------------------------- */

/*
 * Find faces with a YuNet face-detection session (opened from the
 * registry's verified model). Faces scoring below min_score are dropped.
 * Fills up to `capacity` faces, most confident first, sets *count to how
 * many were found, and measures each face's skin colour.
 */
AIBS_API int32_t aibs_detect_faces(const aibs_source* source, aibs_ai_session* detector,
                                   float min_score, aibs_face* out, uint32_t capacity,
                                   uint32_t* count);

/* Measure a face's skin colour (fills face->skin) from the source. */
AIBS_API int32_t aibs_measure_skin(const aibs_source* source, aibs_face* face);

/*
 * Suggest adjustments for the whole image: exposure, contrast, tone, white
 * balance and vibrance from its brightness and colour statistics. With
 * faces, exposure aims at well-lit skin. *out is overwritten; HSL fields
 * are set to zero.
 */
AIBS_API int32_t aibs_auto_adjust(const aibs_source* source, const aibs_face* faces,
                                  uint32_t face_count, aibs_adjustments* out);

/* ---------------------------------------------------------------------
 * Background
 * ------------------------------------------------------------------- */

/*
 * Separate subject from background with a BiRefNet session (input
 * 1 x 3 x 1024 x 1024, normalised RGB; output 1 x 1 x 1024 x 1024 logits).
 * Edges are refined against the image. On success *out must be released
 * with aibs_matte_free.
 */
AIBS_API int32_t aibs_segment(const aibs_source* source, aibs_ai_session* model, aibs_matte** out);

/* A matte from given coverage values (width x height bytes, 0..255) that
 * stretch over a source of source_width x source_height pixels. */
AIBS_API int32_t aibs_matte_create(const uint8_t* values, uint32_t width, uint32_t height,
                                   uint32_t source_width, uint32_t source_height, aibs_matte** out);

/* Fraction of the image covered by the subject, 0..1. */
AIBS_API float aibs_matte_coverage(const aibs_matte* matte);

/* Release a matte. Safe on NULL. */
AIBS_API void aibs_matte_free(aibs_matte* matte);

/* ---------------------------------------------------------------------
 * Lighting
 * ------------------------------------------------------------------- */

/*
 * Estimate the photo's 3D shape with a Depth Anything V2 session (input
 * 1 x 3 x H x W normalised RGB, H and W multiples of 14 with the longer
 * 518; output relative inverse depth, larger is closer). On success *out
 * must be released with aibs_shape_free.
 */
AIBS_API int32_t aibs_estimate_shape(const aibs_source* source, aibs_ai_session* model, aibs_shape** out);

/* A shape from given relative inverse depth values (width x height,
 * larger is closer) that stretch over a source of the given size. */
AIBS_API int32_t aibs_shape_create(const float* depth, uint32_t width, uint32_t height,
                                   uint32_t source_width, uint32_t source_height, aibs_shape** out);

/* Release a shape. Safe on NULL. */
AIBS_API void aibs_shape_free(aibs_shape* shape);

#ifdef __cplusplus
}
#endif

#endif /* AIBS_ENGINE_H */

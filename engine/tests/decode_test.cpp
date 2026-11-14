// Engine tests for the decode path. Plain executable run by CTest: returns
// non-zero on the first failed check. A test framework is added when the
// suite grows past what this covers.
#include "aibs/engine.h"

#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>

#ifndef AIBS_FIXTURE_DIR
#error "AIBS_FIXTURE_DIR must be defined by the build"
#endif

static int failures = 0;

#define CHECK(cond)                                                        \
    do {                                                                   \
        if (!(cond)) {                                                     \
            std::printf("FAILED %s:%d: %s\n", __FILE__, __LINE__, #cond);  \
            ++failures;                                                    \
        }                                                                  \
    } while (0)

static std::vector<uint8_t> read_fixture(const std::string& name) {
    const std::string path = std::string(AIBS_FIXTURE_DIR) + "/" + name;
    std::ifstream file(path, std::ios::binary);
    if (!file) {
        std::printf("cannot open fixture %s\n", path.c_str());
        std::exit(2);
    }
    return std::vector<uint8_t>(std::istreambuf_iterator<char>(file),
                                std::istreambuf_iterator<char>());
}

// The PNG fixture is a 64x48 gradient: r = x * 4, g = y * 5, b = 128.
static void test_png_decodes_exact_pixels() {
    const std::vector<uint8_t> bytes = read_fixture("gradient_64x48.png");
    aibs_image image;
    CHECK(aibs_decode_rgba8(bytes.data(), bytes.size(), 0, &image) == AIBS_OK);
    CHECK(image.width == 64 && image.height == 48);
    CHECK(image.source_width == 64 && image.source_height == 48);
    if (image.data != nullptr && image.width == 64 && image.height == 48) {
        const uint8_t* first = image.data;
        CHECK(first[0] == 0 && first[1] == 0 && first[2] == 128 && first[3] == 255);
        const uint8_t* p = image.data + (static_cast<size_t>(10) * 64 + 20) * 4;
        CHECK(p[0] == 80 && p[1] == 50 && p[2] == 128 && p[3] == 255);
    }
    aibs_image_free(&image);
    CHECK(image.data == nullptr);
}

static void test_jpeg_decodes() {
    const std::vector<uint8_t> bytes = read_fixture("gradient_64x48.jpg");
    aibs_image image;
    CHECK(aibs_decode_rgba8(bytes.data(), bytes.size(), 0, &image) == AIBS_OK);
    CHECK(image.width == 64 && image.height == 48);
    if (image.data != nullptr && image.width == 64 && image.height == 48) {
        // Lossy: check the blue channel stays near 128 at the centre.
        const uint8_t* p = image.data + (static_cast<size_t>(24) * 64 + 32) * 4;
        CHECK(p[2] > 108 && p[2] < 148 && p[3] == 255);
    }
    aibs_image_free(&image);
}

static void test_downscale_keeps_aspect_and_reports_source() {
    const std::vector<uint8_t> bytes = read_fixture("gradient_64x48.png");
    aibs_image image;
    CHECK(aibs_decode_rgba8(bytes.data(), bytes.size(), 32, &image) == AIBS_OK);
    CHECK(image.width == 32 && image.height == 24);
    CHECK(image.source_width == 64 && image.source_height == 48);
    aibs_image_free(&image);
}

static void test_corrupted_file_is_rejected() {
    const std::vector<uint8_t> bytes = read_fixture("corrupted.jpg");
    aibs_image image;
    CHECK(aibs_decode_rgba8(bytes.data(), bytes.size(), 0, &image) ==
          AIBS_ERR_DECODE_FAILED);
    CHECK(image.data == nullptr && image.width == 0);
}

static void test_invalid_arguments() {
    aibs_image image;
    const uint8_t byte = 0;
    CHECK(aibs_decode_rgba8(nullptr, 10, 0, &image) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(aibs_decode_rgba8(&byte, 0, 0, &image) == AIBS_ERR_INVALID_ARGUMENT);
    CHECK(aibs_decode_rgba8(&byte, 1, 0, nullptr) == AIBS_ERR_INVALID_ARGUMENT);
    aibs_image_free(nullptr);
}

int main() {
    test_png_decodes_exact_pixels();
    test_jpeg_decodes();
    test_downscale_keeps_aspect_and_reports_source();
    test_corrupted_file_is_rejected();
    test_invalid_arguments();
    if (failures == 0) {
        std::printf("all engine tests passed (engine %s)\n", aibs_engine_version());
        return 0;
    }
    std::printf("%d check(s) failed\n", failures);
    return 1;
}

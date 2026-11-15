// Engine tests for the decode path. Plain executable run by CTest: returns
// non-zero on the first failed check. A test framework is added when the
// suite grows past what this covers.
#include "aibs/engine.h"
#include "check.h"

#include <cstdio>
#include <string>
#include <utility>
#include <vector>

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

// ---- camera RAW (a DNG written here) -------------------------------------

namespace raw_tests {

struct Tiff {
    std::vector<uint8_t> bytes;
    void u16(uint32_t v) {
        bytes.push_back(static_cast<uint8_t>(v));
        bytes.push_back(static_cast<uint8_t>(v >> 8));
    }
    void u32(uint32_t v) {
        for (int i = 0; i < 4; ++i) bytes.push_back(static_cast<uint8_t>(v >> (8 * i)));
    }
    void put32(size_t at, uint32_t v) {
        for (int i = 0; i < 4; ++i) bytes[at + i] = static_cast<uint8_t>(v >> (8 * i));
    }
};

struct Entry {
    uint16_t tag, type;
    uint32_t count;
    std::vector<uint8_t> data;  // value bytes (little endian)
};

Entry shorts(uint16_t tag, std::vector<uint16_t> v) {
    Entry e{tag, 3, static_cast<uint32_t>(v.size()), {}};
    for (const uint16_t x : v) {
        e.data.push_back(static_cast<uint8_t>(x));
        e.data.push_back(static_cast<uint8_t>(x >> 8));
    }
    return e;
}
Entry longs(uint16_t tag, std::vector<uint32_t> v) {
    Entry e{tag, 4, static_cast<uint32_t>(v.size()), {}};
    for (const uint32_t x : v)
        for (int i = 0; i < 4; ++i) e.data.push_back(static_cast<uint8_t>(x >> (8 * i)));
    return e;
}
Entry bytes_of(uint16_t tag, std::vector<uint8_t> v) {
    return Entry{tag, 1, static_cast<uint32_t>(v.size()), v};
}
Entry ascii(uint16_t tag, const std::string& text) {
    std::vector<uint8_t> v(text.begin(), text.end());
    v.push_back(0);
    return Entry{tag, 2, static_cast<uint32_t>(v.size()), v};
}
Entry rationals(uint16_t tag, uint16_t type, std::vector<std::pair<int32_t, int32_t>> v) {
    Entry e{tag, type, static_cast<uint32_t>(v.size()), {}};
    for (const auto& [n, d] : v)
        for (const int32_t x : {n, d})
            for (int i = 0; i < 4; ++i) e.data.push_back(static_cast<uint8_t>(static_cast<uint32_t>(x) >> (8 * i)));
    return e;
}

// An uncompressed 16-bit RGGB DNG, w x h: the left half reddish, the right
// half bluish.
std::vector<uint8_t> make_dng(int w, int h) {
    std::vector<uint8_t> pixels;
    for (int y = 0; y < h; ++y) {
        for (int x = 0; x < w; ++x) {
            const int site = (y % 2) * 2 + (x % 2);  // 0 R, 1 G, 2 G, 3 B
            const bool left = x < w / 2;
            uint16_t v = 20000;
            if (site == 0) v = left ? 50000 : 6000;
            if (site == 3) v = left ? 6000 : 50000;
            pixels.push_back(static_cast<uint8_t>(v));
            pixels.push_back(static_cast<uint8_t>(v >> 8));
        }
    }
    std::vector<Entry> entries = {
        longs(254, {0}),
        longs(256, {static_cast<uint32_t>(w)}),
        longs(257, {static_cast<uint32_t>(h)}),
        shorts(258, {16}),
        shorts(259, {1}),
        shorts(262, {32803}),
        ascii(271, "AIBS"),
        ascii(272, "Test Camera"),
        longs(273, {0}),  // strip offset, filled in below
        shorts(274, {1}),
        shorts(277, {1}),
        longs(278, {static_cast<uint32_t>(h)}),
        longs(279, {static_cast<uint32_t>(pixels.size())}),
        shorts(284, {1}),
        shorts(33421, {2, 2}),
        bytes_of(33422, {0, 1, 1, 2}),
        bytes_of(50706, {1, 4, 0, 0}),
        bytes_of(50707, {1, 1, 0, 0}),
        ascii(50708, "AIBS Test Camera"),
        longs(50714, {0}),
        longs(50717, {65535}),
        rationals(50721, 10, {{1, 1}, {0, 1}, {0, 1}, {0, 1}, {1, 1}, {0, 1}, {0, 1}, {0, 1}, {1, 1}}),
        rationals(50728, 5, {{1, 1}, {1, 1}, {1, 1}}),
        shorts(50778, {21}),
    };
    Tiff t;
    t.bytes = {'I', 'I', 42, 0};
    t.u32(8);
    const size_t ifd = 8;
    const size_t ifd_size = 2 + entries.size() * 12 + 4;
    size_t extra = ifd + ifd_size;  // values that do not fit in 4 bytes go here
    std::vector<uint8_t> overflow;
    t.u16(static_cast<uint32_t>(entries.size()));
    size_t strip_entry = 0;
    for (const Entry& e : entries) {
        t.u16(e.tag);
        t.u16(e.type);
        t.u32(e.count);
        if (e.tag == 273) strip_entry = t.bytes.size();
        if (e.data.size() <= 4) {
            std::vector<uint8_t> v = e.data;
            v.resize(4, 0);
            t.bytes.insert(t.bytes.end(), v.begin(), v.end());
        } else {
            t.u32(static_cast<uint32_t>(extra + overflow.size()));
            overflow.insert(overflow.end(), e.data.begin(), e.data.end());
            if (overflow.size() % 2) overflow.push_back(0);
        }
    }
    t.u32(0);  // no next IFD
    t.bytes.insert(t.bytes.end(), overflow.begin(), overflow.end());
    t.put32(strip_entry, static_cast<uint32_t>(t.bytes.size()));
    t.bytes.insert(t.bytes.end(), pixels.begin(), pixels.end());
    return t.bytes;
}

void test_dng_decodes() {
    const auto dng = make_dng(64, 48);
    aibs_image image{};
    const int32_t status = aibs_decode_rgba8(dng.data(), dng.size(), 0, &image);
    CHECK(status == AIBS_OK);
    if (status != AIBS_OK) return;
    CHECK(image.width == 64 && image.height == 48);
    // Mean R - B on each side, away from the middle.
    const auto redness = [&](int x0, int x1) {
        double sum = 0, n = 0;
        for (uint32_t y = 4; y + 4 < image.height; ++y)
            for (int x = x0; x < x1; ++x) {
                const uint8_t* p = image.data + (static_cast<size_t>(y) * image.width + x) * 4;
                sum += static_cast<double>(p[0]) - p[2];
                n += 1;
            }
        return sum / n;
    };
    const double left = redness(4, 24), right = redness(40, 60);
    std::printf("info: DNG red minus blue: left %.1f, right %.1f\n", left, right);
    CHECK(left > right + 30);
    aibs_image_free(&image);

    // The same file opens as an editable source.
    aibs_source* source = nullptr;
    CHECK(aibs_source_open(dng.data(), dng.size(), &source) == AIBS_OK);
    uint32_t w = 0, h = 0;
    aibs_source_size(source, &w, &h);
    CHECK(w == 64 && h == 48);
    aibs_source_free(source);

    // A RAW file cut off inside its header is rejected cleanly.
    std::vector<uint8_t> cut(dng.begin(), dng.begin() + 40);
    aibs_image broken{};
    CHECK(aibs_decode_rgba8(cut.data(), cut.size(), 0, &broken) != AIBS_OK);
    aibs_image_free(&broken);
}

}  // namespace raw_tests

void run_decode_tests() {
    test_png_decodes_exact_pixels();
    test_jpeg_decodes();
    test_downscale_keeps_aspect_and_reports_source();
    test_corrupted_file_is_rejected();
    test_invalid_arguments();
    raw_tests::test_dng_decodes();
}

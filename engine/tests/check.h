// Minimal check helpers shared by the engine tests. Each test file exposes a
// run_*_tests() function called from main.cpp.
#pragma once

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>

#ifndef AIBS_FIXTURE_DIR
#error "AIBS_FIXTURE_DIR must be defined by the build"
#endif

extern int g_failures;

#define CHECK(cond)                                                        \
    do {                                                                   \
        if (!(cond)) {                                                     \
            std::printf("FAILED %s:%d: %s\n", __FILE__, __LINE__, #cond);  \
            ++g_failures;                                                  \
        }                                                                  \
    } while (0)

inline std::vector<uint8_t> read_fixture(const std::string& name) {
    const std::string path = std::string(AIBS_FIXTURE_DIR) + "/" + name;
    std::ifstream file(path, std::ios::binary);
    if (!file) {
        std::printf("cannot open fixture %s\n", path.c_str());
        std::exit(2);
    }
    return std::vector<uint8_t>(std::istreambuf_iterator<char>(file),
                                std::istreambuf_iterator<char>());
}

void run_decode_tests();
void run_source_tests();
void run_adjust_tests();
void run_ai_tests();
void run_portrait_tests();
void run_background_tests();

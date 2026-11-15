#include "aibs/engine.h"
#include "check.h"

int g_failures = 0;

int main() {
    run_decode_tests();
    run_source_tests();
    run_adjust_tests();
    run_ai_tests();
    run_portrait_tests();
    if (g_failures == 0) {
        std::printf("all engine tests passed (engine %s)\n", aibs_engine_version());
        return 0;
    }
    std::printf("%d check(s) failed\n", g_failures);
    return 1;
}

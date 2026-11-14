cmake_minimum_required(VERSION 3.21)
project(aibs_engine VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_CXX_VISIBILITY_PRESET hidden)
set(CMAKE_VISIBILITY_INLINES_HIDDEN ON)

find_package(OpenCV CONFIG REQUIRED COMPONENTS core imgproc imgcodecs)

# One shared library exposing only the C ABI in include/aibs/. OpenCV is
# linked statically into it, so the application ships a single engine DLL.
add_library(aibs_engine SHARED
    image-engine/src/decode.cpp
)
target_include_directories(aibs_engine PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_compile_definitions(aibs_engine PRIVATE AIBS_BUILD)
target_link_libraries(aibs_engine PRIVATE opencv_core opencv_imgproc opencv_imgcodecs)

if(MSVC)
    target_compile_options(aibs_engine PRIVATE /W4 /WX /utf-8)
else()
    target_compile_options(aibs_engine PRIVATE -Wall -Wextra -Werror)
endif()

install(TARGETS aibs_engine
    RUNTIME DESTINATION bin
    LIBRARY DESTINATION lib
    ARCHIVE DESTINATION lib
)

include(CTest)
if(BUILD_TESTING)
    add_executable(aibs_engine_tests tests/decode_test.cpp)
    target_link_libraries(aibs_engine_tests PRIVATE aibs_engine)
    target_compile_definitions(aibs_engine_tests PRIVATE
        AIBS_FIXTURE_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../tests/fixtures")
    # Put the test next to the engine DLL so Windows can load it.
    set_target_properties(aibs_engine aibs_engine_tests PROPERTIES
        RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)
    add_test(NAME decode COMMAND aibs_engine_tests)
endif()

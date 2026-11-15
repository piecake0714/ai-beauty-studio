cmake_minimum_required(VERSION 3.21)
project(aibs_engine VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_CXX_VISIBILITY_PRESET hidden)
set(CMAKE_VISIBILITY_INLINES_HIDDEN ON)

find_package(OpenCV CONFIG REQUIRED COMPONENTS core imgproc imgcodecs)

# ONNX Runtime with DirectML, from the extracted NuGet packages
# Microsoft.ML.OnnxRuntime.DirectML and Microsoft.AI.DirectML (see CI).
set(AIBS_ORT_DIR "" CACHE PATH "Extracted Microsoft.ML.OnnxRuntime.DirectML package")
set(AIBS_DML_DIR "" CACHE PATH "Extracted Microsoft.AI.DirectML package")
if(NOT AIBS_ORT_DIR OR NOT AIBS_DML_DIR)
    message(FATAL_ERROR "Set AIBS_ORT_DIR and AIBS_DML_DIR to the extracted ONNX Runtime and DirectML packages")
endif()
set(AIBS_ORT_BIN "${AIBS_ORT_DIR}/runtimes/win-x64/native")
add_library(onnxruntime SHARED IMPORTED)
set_target_properties(onnxruntime PROPERTIES
    IMPORTED_LOCATION "${AIBS_ORT_BIN}/onnxruntime.dll"
    IMPORTED_IMPLIB "${AIBS_ORT_BIN}/onnxruntime.lib"
    INTERFACE_INCLUDE_DIRECTORIES "${AIBS_ORT_DIR}/build/native/include")
# Every DLL the engine needs next to it at run time.
file(GLOB AIBS_ORT_DLLS "${AIBS_ORT_BIN}/*.dll")
set(AIBS_RUNTIME_DLLS ${AIBS_ORT_DLLS} "${AIBS_DML_DIR}/bin/x64-win/DirectML.dll")
foreach(dll IN LISTS AIBS_RUNTIME_DLLS)
    if(NOT EXISTS "${dll}")
        message(FATAL_ERROR "Missing runtime DLL: ${dll}")
    endif()
endforeach()

# One shared library exposing only the C ABI in include/aibs/. OpenCV is
# linked statically into it, so the application ships a single engine DLL.
add_library(aibs_engine SHARED
    image-engine/src/decode.cpp
    image-engine/src/source.cpp
    color-engine/src/adjust.cpp
    gpu-engine/src/gpu.cpp
    ai-runtime/src/session.cpp
)
target_include_directories(aibs_engine PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_compile_definitions(aibs_engine PRIVATE AIBS_BUILD)
target_link_libraries(aibs_engine PRIVATE opencv_core opencv_imgproc opencv_imgcodecs onnxruntime dxgi)
if(MSVC)
    # Loaded explicitly from the engine's folder at first use (session.cpp).
    target_link_options(aibs_engine PRIVATE "/DELAYLOAD:onnxruntime.dll")
    target_link_libraries(aibs_engine PRIVATE delayimp)
endif()

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
install(FILES ${AIBS_RUNTIME_DLLS} DESTINATION bin)

include(CTest)
if(BUILD_TESTING)
    add_executable(aibs_engine_tests tests/decode_test.cpp tests/source_test.cpp tests/adjust_test.cpp tests/ai_test.cpp tests/main.cpp)
    # The tests use OpenCV directly to build large synthetic images.
    target_link_libraries(aibs_engine_tests PRIVATE aibs_engine opencv_core opencv_imgcodecs)
    target_compile_definitions(aibs_engine_tests PRIVATE
        AIBS_FIXTURE_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../tests/fixtures"
        AIBS_MODEL_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../models/files")
    # Put the test next to the engine DLL so Windows can load it.
    set_target_properties(aibs_engine aibs_engine_tests PROPERTIES
        RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)
    # The runtime DLLs must sit next to the test executable.
    add_custom_command(TARGET aibs_engine_tests POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different ${AIBS_RUNTIME_DLLS} $<TARGET_FILE_DIR:aibs_engine_tests>)
    add_test(NAME engine COMMAND aibs_engine_tests)
endif()

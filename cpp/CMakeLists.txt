cmake_minimum_required(VERSION 3.16)
project(difi_cpp LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release)
endif()

add_compile_options(-Wall -Wextra -Wpedantic)

# Optional sanitizers for fuzz/hardening runs: cmake -DDIFI_SANITIZE=ON
# Builds the tests with AddressSanitizer + UndefinedBehaviorSanitizer so any
# out-of-bounds read or UB in the parse paths is caught during fuzzing.
option(DIFI_SANITIZE "Build with Address+UB sanitizers" OFF)
if(DIFI_SANITIZE)
  add_compile_options(-fsanitize=address,undefined -fno-omit-frame-pointer -g)
  add_link_options(-fsanitize=address,undefined)
endif()

# Header-only DIFI library.
add_library(difi INTERFACE)
target_include_directories(difi INTERFACE ${CMAKE_CURRENT_SOURCE_DIR}/include)

# CLI tool (interop with the Rust implementation).
add_executable(difi-tool src/difi_tool.cpp)
target_link_libraries(difi-tool PRIVATE difi)

# Unit tests.
enable_testing()
add_executable(difi_tests tests/test_codec.cpp)
target_link_libraries(difi_tests PRIVATE difi)
# The 1.1 backward-compat regression test (task 5) decodes the committed golden packets.
target_compile_definitions(difi_tests PRIVATE
  DIFI_GOLDEN_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../tests/golden")
add_test(NAME codec COMMAND difi_tests)

# Golden-packet conformance: regenerate and diff against the committed shared oracle.
add_test(
  NAME golden_match
  COMMAND ${CMAKE_COMMAND}
          -DDIFI_TOOL=$<TARGET_FILE:difi-tool>
          -DGOLDEN_DIR=${CMAKE_CURRENT_SOURCE_DIR}/../tests/golden
          -DWORK_DIR=${CMAKE_CURRENT_BINARY_DIR}/golden-out
          -P ${CMAKE_CURRENT_SOURCE_DIR}/cmake/CheckGolden.cmake
)

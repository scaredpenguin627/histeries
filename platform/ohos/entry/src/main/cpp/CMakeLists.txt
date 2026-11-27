cmake_minimum_required(VERSION 3.20)
project(histeries_board LANGUAGES C CXX)

if(OHOS_ARCH STREQUAL "arm64-v8a")
    set(rust_target aarch64-unknown-linux-ohos)
    set(clang_target aarch64-linux-ohos)
elseif(OHOS_ARCH STREQUAL "x86_64")
    set(rust_target x86_64-unknown-linux-ohos)
    set(clang_target x86_64-linux-ohos)
else()
    message(FATAL_ERROR "Unsupported HarmonyOS architecture: ${OHOS_ARCH}")
endif()

find_program(cargo cargo REQUIRED)
get_filename_component(renderer "${CMAKE_CURRENT_SOURCE_DIR}/../../../../renderer" ABSOLUTE)
set(cargo_output "${CMAKE_BINARY_DIR}/rust")
set(archive "${cargo_output}/${rust_target}/release/libhisteries_ohos.a")
string(REPLACE "-" "_" env_target "${rust_target}")

# Ask Cargo for freshness each build. Transitive Rust changes, ruleset data and shaders can all affect the frame.
add_custom_target(rust_board
    COMMAND "${CMAKE_COMMAND}" -P "${renderer}/../prepare-wgpu.cmake"
    COMMAND "${CMAKE_COMMAND}" -E env
        "CC_${env_target}=${CMAKE_C_COMPILER}"
        "CXX_${env_target}=${CMAKE_CXX_COMPILER}"
        "AR_${env_target}=${CMAKE_AR}"
        "CC_SHELL_ESCAPED_FLAGS=1"
        "CFLAGS_${env_target}=--target=${clang_target} --sysroot=\"${CMAKE_SYSROOT}\""
        "CXXFLAGS_${env_target}=--target=${clang_target} --sysroot=\"${CMAKE_SYSROOT}\""
        "${cargo}" build --manifest-path "${renderer}/Cargo.toml" --target-dir "${cargo_output}"
        --target "${rust_target}" --release --locked
    BYPRODUCTS "${archive}"
    WORKING_DIRECTORY "${renderer}"
    VERBATIM)

add_library(histeries_board SHARED board.cpp)
add_dependencies(histeries_board rust_board)
target_compile_features(histeries_board PRIVATE cxx_std_17)
target_link_libraries(histeries_board PRIVATE "${archive}" ace_ndk.z ace_napi.z native_window hilog_ndk.z dl m)

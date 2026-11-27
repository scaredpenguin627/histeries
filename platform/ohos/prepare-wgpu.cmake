cmake_minimum_required(VERSION 3.20)

# Both HAP ABIs consume the same pre-release wgpu workspace.
set(revision "8b419de0906f37af69d728f1735132e427a3436f")
set(sha256 "1fb21ed259f48d170ea9ebc9314d3d03056a83157c1ab0fdea51d59b11b9c7f5")
set(cache "${CMAKE_CURRENT_LIST_DIR}/upstream")
set(source "${cache}/wgpu-${revision}")
set(archive "${cache}/wgpu-${revision}.tar.gz")
set(staging "${cache}/extract-${revision}")

file(MAKE_DIRECTORY "${cache}")
file(LOCK "${cache}/prepare.lock" GUARD PROCESS TIMEOUT 600)
if(EXISTS "${source}/.archive-verified" AND EXISTS "${source}/wgpu/Cargo.toml")
    return()
endif()

file(DOWNLOAD "https://codeload.github.com/gfx-rs/wgpu/tar.gz/${revision}" "${archive}"
    EXPECTED_HASH "SHA256=${sha256}" TLS_VERIFY ON TIMEOUT 300)
file(REMOVE_RECURSE "${staging}")
file(MAKE_DIRECTORY "${staging}")
file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${staging}")
file(WRITE "${staging}/wgpu-${revision}/.archive-verified" "${sha256}\n")
file(RENAME "${staging}/wgpu-${revision}" "${source}")
file(REMOVE_RECURSE "${staging}")

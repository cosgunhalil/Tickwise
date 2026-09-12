cmake_minimum_required(VERSION 3.16)
project(tickwise_cpp LANGUAGES CXX)

# C++14, the floor for Unreal 4.26 and classic cocos2d-x. No exceptions
# and no RTTI are required by the wrapper, and the harness builds either
# way, so the default flags of each platform are left alone.
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Where the tickwise-ffi crate lives, and which of its build profiles to
# link. Both default to the sibling crate's release build.
set(TICKWISE_FFI_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../tickwise-ffi" CACHE PATH
    "Path to the tickwise-ffi crate")
set(TICKWISE_FFI_PROFILE "release" CACHE STRING
    "Cargo profile whose target directory holds the built library")
set(TICKWISE_FFI_LIB_DIR "${TICKWISE_FFI_DIR}/target/${TICKWISE_FFI_PROFILE}" CACHE PATH
    "Directory holding the built tickwise_ffi library")

# The wrapper itself: two headers, nothing to compile.
add_library(tickwise_cpp INTERFACE)
target_include_directories(tickwise_cpp INTERFACE
    "${CMAKE_CURRENT_SOURCE_DIR}/include"
    "${TICKWISE_FFI_DIR}/include")

# The native library, found rather than built: cargo owns that step.
if(WIN32)
    set(_tickwise_import_lib "${TICKWISE_FFI_LIB_DIR}/tickwise_ffi.dll.lib")
    set(_tickwise_runtime "${TICKWISE_FFI_LIB_DIR}/tickwise_ffi.dll")
elseif(APPLE)
    set(_tickwise_import_lib "${TICKWISE_FFI_LIB_DIR}/libtickwise_ffi.dylib")
    set(_tickwise_runtime "${_tickwise_import_lib}")
else()
    set(_tickwise_import_lib "${TICKWISE_FFI_LIB_DIR}/libtickwise_ffi.so")
    set(_tickwise_runtime "${_tickwise_import_lib}")
endif()
if(NOT EXISTS "${_tickwise_import_lib}")
    message(FATAL_ERROR
        "tickwise_ffi library not found at ${_tickwise_import_lib}. "
        "Build it first: cd ${TICKWISE_FFI_DIR} && cargo build --${TICKWISE_FFI_PROFILE}")
endif()

add_library(tickwise_ffi SHARED IMPORTED GLOBAL)
set_target_properties(tickwise_ffi PROPERTIES
    IMPORTED_LOCATION "${_tickwise_runtime}"
    IMPORTED_IMPLIB "${_tickwise_import_lib}")
target_link_libraries(tickwise_cpp INTERFACE tickwise_ffi)

# The harness, and the same warning bar the C harness clears.
option(TICKWISE_CPP_BUILD_TESTS "Build the C++ harness" ON)
if(TICKWISE_CPP_BUILD_TESTS)
    enable_testing()
    add_executable(tickwise_cpp_harness tests/harness.cpp)
    target_link_libraries(tickwise_cpp_harness PRIVATE tickwise_cpp)
    if(MSVC)
        target_compile_options(tickwise_cpp_harness PRIVATE /W4 /WX)
    else()
        target_compile_options(tickwise_cpp_harness PRIVATE -Wall -Wextra -Werror -pedantic)
        set_target_properties(tickwise_cpp_harness PROPERTIES
            BUILD_RPATH "${TICKWISE_FFI_LIB_DIR}")
    endif()
    if(WIN32)
        # The DLL must sit beside the executable for the loader to find it.
        add_custom_command(TARGET tickwise_cpp_harness POST_BUILD
            COMMAND "${CMAKE_COMMAND}" -E copy_if_different
                "${_tickwise_runtime}" "$<TARGET_FILE_DIR:tickwise_cpp_harness>")
    endif()

    set(_tickwise_out "${CMAKE_CURRENT_BINARY_DIR}/recordings")
    file(MAKE_DIRECTORY "${_tickwise_out}")
    add_test(NAME harness
        COMMAND tickwise_cpp_harness
            "${_tickwise_out}/clean.rec" "${_tickwise_out}/chaotic.rec" "${_tickwise_out}")
endif()

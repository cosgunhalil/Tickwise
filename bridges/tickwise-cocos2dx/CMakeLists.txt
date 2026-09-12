cmake_minimum_required(VERSION 3.16)
project(tickwise_cocos2dx LANGUAGES CXX)

# C++14, the floor for classic cocos2d-x 3.17 and 4.0.
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# The shared C++ layer and, through it, the native library. Its own
# harness stays out: this project has one of its own.
set(TICKWISE_CPP_BUILD_TESTS OFF CACHE BOOL "" FORCE)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../tickwise-cpp" tickwise_cpp EXCLUDE_FROM_ALL)

# Two ways to build the adapter:
#   - Against a real cocos2d-x tree, when TICKWISE_COCOS2DX_ROOT points at
#     one. This produces a static library to link into a game. Most games
#     will instead drop the two source files into their own project, which
#     the README describes.
#   - Against the shim, the default, for continuous integration. The shim
#     declares only the engine facilities the adapter uses, so the
#     adapter runs end to end through the real native library without an
#     engine present.
set(TICKWISE_COCOS2DX_ROOT "" CACHE PATH "Path to a cocos2d-x checkout, empty for the shim")

add_library(tickwise_cocos2dx STATIC src/TickwiseRecorder.cpp)
target_include_directories(tickwise_cocos2dx PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}/include")
target_link_libraries(tickwise_cocos2dx PUBLIC tickwise_cpp)

if(TICKWISE_COCOS2DX_ROOT)
    target_include_directories(tickwise_cocos2dx PUBLIC
        "${TICKWISE_COCOS2DX_ROOT}/cocos"
        "${TICKWISE_COCOS2DX_ROOT}/cocos/platform"
        "${TICKWISE_COCOS2DX_ROOT}/cocos/base"
        "${TICKWISE_COCOS2DX_ROOT}/external")
    message(STATUS "tickwise_cocos2dx: building against cocos2d-x at ${TICKWISE_COCOS2DX_ROOT}")
else()
    target_include_directories(tickwise_cocos2dx PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}/shim")
    message(STATUS "tickwise_cocos2dx: building against the shim; set TICKWISE_COCOS2DX_ROOT for a real engine")
endif()

if(MSVC)
    target_compile_options(tickwise_cocos2dx PRIVATE /W4 /WX)
else()
    target_compile_options(tickwise_cocos2dx PRIVATE -Wall -Wextra -Werror -pedantic)
endif()

# The harness runs only against the shim: with a real engine the node
# needs a running Director, which is a game's job.
option(TICKWISE_COCOS2DX_BUILD_TESTS "Build the shim harness" ON)
if(TICKWISE_COCOS2DX_BUILD_TESTS AND NOT TICKWISE_COCOS2DX_ROOT)
    enable_testing()
    add_executable(tickwise_cocos2dx_harness tests/harness.cpp)
    target_link_libraries(tickwise_cocos2dx_harness PRIVATE tickwise_cocos2dx)
    if(MSVC)
        target_compile_options(tickwise_cocos2dx_harness PRIVATE /W4 /WX)
    else()
        target_compile_options(tickwise_cocos2dx_harness PRIVATE -Wall -Wextra -Werror -pedantic)
        set_target_properties(tickwise_cocos2dx_harness PROPERTIES
            BUILD_RPATH "${TICKWISE_FFI_LIB_DIR}")
    endif()
    if(WIN32)
        add_custom_command(TARGET tickwise_cocos2dx_harness POST_BUILD
            COMMAND "${CMAKE_COMMAND}" -E copy_if_different
                "$<TARGET_FILE:tickwise_ffi>" "$<TARGET_FILE_DIR:tickwise_cocos2dx_harness>")
    endif()

    set(_tickwise_out "${CMAKE_CURRENT_BINARY_DIR}/recordings")
    file(MAKE_DIRECTORY "${_tickwise_out}")
    add_test(NAME cocos2dx_harness COMMAND tickwise_cocos2dx_harness "${_tickwise_out}")
endif()

cmake_minimum_required(VERSION 3.21)
project(openliero)

if(NOT EMSCRIPTEN)
    message(FATAL_ERROR "The game is only built for the web: configure with emcmake (see README.md).")
endif()

# Emscripten ships its own SDL2 and SDL2_image ports. Wrap them in the same
# target names find_package gives, so the targets below link the same way.
add_library(SDL2::SDL2 INTERFACE IMPORTED)
set_target_properties(SDL2::SDL2 PROPERTIES
    INTERFACE_COMPILE_OPTIONS "-sUSE_SDL=2"
    INTERFACE_LINK_OPTIONS "-sUSE_SDL=2")
add_library(SDL2_image::SDL2_image INTERFACE IMPORTED)
set_target_properties(SDL2_image::SDL2_image PROPERTIES
    INTERFACE_COMPILE_OPTIONS "-sUSE_SDL_IMAGE=2"
    INTERFACE_LINK_OPTIONS "-sUSE_SDL_IMAGE=2;-sSDL2_IMAGE_FORMATS=tga,png")
# The game reports errors with exceptions; emcc only catches them when asked.
add_compile_options(-fexceptions)
add_link_options(-fexceptions)
include_directories(${PROJECT_SOURCE_DIR}/src)

set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++11 -Wall")
set(CMAKE_CXX_FLAGS_DEBUG "-g")
set(CMAKE_CXX_FLAGS_RELEASE "-O2")

set(GAME_SOURCES
    src/game/common.cpp
    src/game/console.cpp
    src/game/constants.cpp
    src/game/filesystem.cpp
    src/game/game.cpp
    src/game/level.cpp
    src/game/math.cpp
    src/game/rand.cpp
    src/game/settings.cpp
    src/game/text.cpp
    src/game/viewport.cpp
    src/game/weapon.cpp
    src/game/worm.cpp
    src/game/gfx/blit.cpp
    src/game/gfx/font.cpp
    src/game/gfx/palette.cpp
    src/game/gfx/renderer.cpp
    src/game/gfx.cpp
    src/game/hashrun.cpp
    src/game/main.cpp
    src/game/replay.cpp
    src/game/screen.cpp
    src/game/sfx.cpp
    src/game/menu/menu.cpp
    src/game/sdlmain.cpp
)
include_directories(src/game)

# Game code ported to Rust (rust/liero-game). cargo builds it as a static
# library for wasm32-unknown-emscripten on every build, and only rebuilds
# what changed.
find_program(CARGO cargo REQUIRED)
set(LIERO_GAME_DIR ${PROJECT_SOURCE_DIR}/rust/liero-game)
set(LIERO_GAME_TARGET_DIR ${CMAKE_BINARY_DIR}/rust)
set(LIERO_GAME_LIB
    ${LIERO_GAME_TARGET_DIR}/wasm32-unknown-emscripten/release/libliero_game.a)

add_custom_target(liero_game_cargo
    COMMAND ${CARGO} build --release
        --manifest-path ${LIERO_GAME_DIR}/Cargo.toml
        --target-dir ${LIERO_GAME_TARGET_DIR}
        --target wasm32-unknown-emscripten
    BYPRODUCTS ${LIERO_GAME_LIB}
    COMMENT "Building rust/liero-game")
add_library(liero_game STATIC IMPORTED)
set_target_properties(liero_game PROPERTIES
    IMPORTED_LOCATION ${LIERO_GAME_LIB}
    INTERFACE_INCLUDE_DIRECTORIES ${LIERO_GAME_DIR}/include)
add_dependencies(liero_game liero_game_cargo)

add_executable(openliero ${GAME_SOURCES})
target_link_libraries(openliero SDL2::SDL2 SDL2_image::SDL2_image liero_game)
# Emscripten writes openliero.html/.js/.wasm/.data.
set_target_properties(openliero PROPERTIES SUFFIX ".html")
# The game looks for TC/ under the current folder, which is / in the browser.
target_link_options(openliero PRIVATE
    "--preload-file=${PROJECT_SOURCE_DIR}/TC/openliero@/TC/openliero")
# The page (web/shell.html) waits for a click before it calls main, so the
# browser lets the game start audio. Before that it mounts IndexedDB (IDBFS,
# part of Emscripten) on the folder the game saves to and loads it.
target_link_options(openliero PRIVATE
    "--shell-file=${PROJECT_SOURCE_DIR}/web/shell.html"
    "-sINVOKE_RUN=0"
    "-lidbfs.js"
    "-sEXPORTED_RUNTIME_METHODS=callMain,FS,addRunDependency,removeRunDependency")
set_property(TARGET openliero APPEND PROPERTY
    LINK_DEPENDS ${PROJECT_SOURCE_DIR}/web/shell.html)
# The web build's stack is 64 KB, and nothing stops a function from
# writing past it into the program's data. With this on, every function
# checks, and an overflow stops the game with an error. CI's hash run uses
# it; the deployed game doesn't, as it's slower.
option(LIERO_STACK_CHECK "Check for stack overflows in the web build" OFF)
if(LIERO_STACK_CHECK)
    target_link_options(openliero PRIVATE "-sSTACK_OVERFLOW_CHECK=2")
endif()

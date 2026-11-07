cmake_minimum_required(VERSION 3.21)
project(openliero)

if(EMSCRIPTEN)
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
else()
    find_package(SDL2 REQUIRED)
    find_package(SDL2_image REQUIRED)
endif()
include_directories(
    ${PROJECT_SOURCE_DIR}/src
    ${PROJECT_SOURCE_DIR}/ffmpeg)
link_directories(${PROJECT_SOURCE_DIR}/ffmpeg/libavcodec ${PROJECT_SOURCE_DIR}/ffmpeg/libavformat ${PROJECT_SOURCE_DIR}/ffmpeg/libavutil ${PROJECT_SOURCE_DIR}/ffmpeg/libswresample ${PROJECT_SOURCE_DIR}/ffmpeg/libswscale ${PROJECT_SOURCE_DIR}/x264)

set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++11 -Wall")
set(CMAKE_CXX_FLAGS_DEBUG "-g")
set(CMAKE_CXX_FLAGS_RELEASE "-O2")
if(NOT EMSCRIPTEN)
    # emcc reads a bare -s as the start of a setting, not as "strip".
    set(CMAKE_EXE_LINKER_FLAGS_RELEASE "-s")
endif()

set(SOURCES
    src/game/bobject.cpp
    src/game/bonus.cpp
    src/game/common.cpp
    src/game/console.cpp
    src/game/constants.cpp
    src/game/filesystem.cpp
    src/game/game.cpp
    src/game/level.cpp
    src/game/math.cpp
    src/game/ninjarope.cpp
    src/game/nobject.cpp
    src/game/rand.cpp
    src/game/settings.cpp
    src/game/sobject.cpp
    src/game/stats_recorder.cpp
    src/game/sys.cpp
    src/game/text.cpp
    src/game/viewport.cpp
    src/game/weapon.cpp
    src/game/worm.cpp
    src/game/ai/predictive_ai.cpp
    src/game/gfx/blit.cpp
    src/game/gfx/font.cpp
    src/game/gfx/palette.cpp
    src/game/gfx/renderer.cpp
    src/game/gfx/sprite.cpp
    src/gvl/zlib/miniz.c
    src/gvl/system/system.c
    src/gvl/support/bits.c
    src/gvl/support/debug.cpp
    src/gvl/support/platform.cpp
    src/gvl/support/profile.cpp
    src/gvl/io2/stream.cpp
    src/gvl/io2/convert.cpp
    src/gvl/containers/list.cpp
)

set(GAME_SOURCES ${SOURCES}
    src/game/gfx.cpp
    src/game/hashmenus.cpp
    src/game/hashrun.cpp
    src/game/hashsettings.cpp
    src/game/hashstats.cpp
    src/game/hashreplays.cpp
    src/game/hashai.cpp
    src/game/hashsfx.cpp
    src/game/hashapp.cpp
    src/game/hashtc.cpp
    src/game/main.cpp
    src/game/replay.cpp
    src/game/screen.cpp
    src/game/sfx.cpp
    src/game/weapsel.cpp
    src/game/controller/commonController.cpp
    src/game/controller/localController.cpp
    src/game/controller/replayController.cpp
    src/game/controller/stats_presenter.cpp
    src/game/menu/booleanSwitchBehavior.cpp
    src/game/menu/enumBehavior.cpp
    src/game/menu/hiddenMenu.cpp
    src/game/menu/integerBehavior.cpp
    src/game/menu/itemBehavior.cpp
    src/game/menu/mainMenu.cpp
    src/game/menu/menu.cpp
    src/game/menu/menuItem.cpp
    src/game/menu/timeBehavior.cpp
)

set(VIDEO_TOOL_SOURCES ${SOURCES}
    src/game/replay.cpp
    src/video_tool/replay_to_video.cpp
    src/video_tool/tools_main.cpp
    src/video_tool/video_recorder.c
)

set(GAME_SOURCES ${GAME_SOURCES} src/game/sdlmain.cpp)
include_directories(src/game)

if(EMSCRIPTEN)
    # The game is only built for the browser.

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
else()
    # Natively, only videotool is built, and only on request. tctool is the
    # Rust tool in tools/tctool.
    add_executable(videotool EXCLUDE_FROM_ALL ${VIDEO_TOOL_SOURCES})
    target_link_libraries(videotool SDL2::SDL2 avcodec avformat avutil dl pthread swresample swscale x264)
endif()

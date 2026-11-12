#ifndef LIERO_SCREEN_HPP
#define LIERO_SCREEN_HPP

// Runs the app's screens, which are in Rust (rust/liero-game/src/screen.rs),
// on the window: what they call here (LieroScreenHost) and the loop.

#include <liero_game.h>

#include <functional>

// Runs `first` as the app, then calls `done`. In the browser this returns
// at once: the browser calls the frame function every frame
// (emscripten_set_main_loop), which runs the 14 ms steps that are due.
// Elsewhere each step runs as soon as the one before it is done, as
// Gfx::flip waits for the next frame, and `done` is called before this
// returns.
void startScreens(LieroScreen first, std::function<void()> done);

// The app: the main menu and what it opens (gfx.cpp). It closes when the
// player quits.
LieroScreen mainScreen();

#endif // LIERO_SCREEN_HPP

#ifndef LIERO_SCREEN_HPP
#define LIERO_SCREEN_HPP

// A screen on the app's stack (rust/liero-game/src/screen.rs), written in C++.
//
// `tick` runs one step of what a blocking loop's body did: read the events,
// update, draw. It returns whether it drew a frame (the stack then calls
// Gfx::flip) and what happens next. A screen waiting for an event stays and
// shows nothing; the stack then calls Gfx::idle, which in a scripted run
// skips to the next event as the old blocking waits did.

#include <liero_game.h>

#include <memory>

struct Screen;

struct Tick
{
	LieroTickKind kind = TickStay;
	bool shown = false;
	int result = 0;
	std::unique_ptr<Screen> screen;

	static Tick stay(bool shown) { Tick t; t.shown = shown; return t; }
	static Tick push(std::unique_ptr<Screen> s, bool shown = false) { Tick t; t.kind = TickPush; t.shown = shown; t.screen = std::move(s); return t; }
	static Tick close(int result = 0, bool shown = false) { Tick t; t.kind = TickClose; t.shown = shown; t.result = result; return t; }
	static Tick replace(std::unique_ptr<Screen> s, bool shown = false) { Tick t; t.kind = TickReplace; t.shown = shown; t.screen = std::move(s); return t; }
	static Tick quit(bool shown = false) { Tick t; t.kind = TickQuit; t.shown = shown; return t; }
};

struct Screen
{
	virtual ~Screen() {}
	virtual Tick tick() = 0;
	// The screen this one opened closed with `result`.
	virtual void resume(int result) {}
};

// Runs `first` and whatever it opens until no screen is left. An exception
// thrown by a screen closes them all and is rethrown here.
void runScreens(std::unique_ptr<Screen> first);

// The app: the main menu and what it opens (gfx.cpp). It closes when the
// player quits.
std::unique_ptr<Screen> mainScreen();

#endif // LIERO_SCREEN_HPP

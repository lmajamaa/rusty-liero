#ifndef LIERO_GFX_HPP
#define LIERO_GFX_HPP

#include <SDL.h>
#include <liero_game.h>

#include <cstdint>
#include <vector>

#include "keys.hpp"

struct Joystick {
	SDL_GameController *sdlGameController;
	bool btnState[MaxJoyButtons];
	void clearState() {
		for ( int i = 0; i < MaxJoyButtons; ++i ) btnState[i] = false;
	}
};

// The window: SDL's windows, events and joysticks. Everything else is the
// Rust app's (LieroApp in rust/liero-game/src/app.rs), which draws into
// its renderers and reads the keys this passes it.
struct Gfx
{
	Gfx();

	LieroApp* app = liero_app();
	// The menus, in Rust; C++ makes them with the window's hooks.
	LieroMenus* menus;

	bool pollEvent(SDL_Event& ev);
	// A screen showed nothing while waiting for an event (screen.hpp).
	void idle();

	void init();
	void setVideoMode();
	void onWindowResize(Uint32 windowId);
	// Reads every event due into the keys, passing them to the app's
	// controller with `toController`.
	void process(bool toController = false);
	void processEvent(SDL_Event& ev, bool toController = false);
	// draws a given surface onto an SDL texture/renderer, using a given
	// renderer of the app's
	void draw(SDL_Surface& surface, SDL_Texture& texture, SDL_Renderer& sdlRenderer, LieroRenderer* renderer);
	void flip();
	void clearKeys();

	void setSpectatorFullscreen(bool newFullscreen);
	// `updateMenu`: show the change in the options menu.
	void setFullscreen(bool newFullscreen, bool updateMenu = true);
	void setDoubleRes(bool newDoubleRes, bool updateMenu = true);

	LieroKeys& dosKeys = *liero_app_keys(app);

	// the window to render into
	SDL_Window* sdlWindow = NULL;
	// the window to render the spectator view into
	SDL_Window* sdlSpectatorWindow = NULL;
	// the SDL renderer to use
	SDL_Renderer* sdlRenderer = NULL;
	// the SDL renderer to use for the spectator window
	SDL_Renderer* sdlSpectatorRenderer = NULL;
	// full window size texture that represents the window
	SDL_Texture* sdlTexture = NULL;
	// full spectator window size texture that represents the spectator window
	SDL_Texture* sdlSpectatorTexture = NULL;
	// a software surface to do the actual drawing into
	SDL_Surface* sdlDrawSurface = NULL;
	// a software surface to do the actual drawing of the spectator view into
	SDL_Surface* sdlSpectatorDrawSurface = NULL;

	bool running;
	bool spectatorFullscreen = false, doubleRes;
	Uint32 lastFrame;
	int windowW, windowH;
	int prevMag; // Previous magnification used for drawing
	std::vector<Joystick> joysticks;
	// The keys pressed since the frame's events were last read, for the
	// menus' typing search.
	SDL_Keysym keyBuf[32], *keyBufPtr;
};

extern Gfx gfx;

#endif // LIERO_GFX_HPP

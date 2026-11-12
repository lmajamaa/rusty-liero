#ifndef LIERO_GFX_HPP
#define LIERO_GFX_HPP

#include <SDL.h>
#include <memory>
#include "geom.hpp"

#include <cstdio>
#include <ctime>
#include <cassert>

#include "gfx/font.hpp"
#include "gfx/blit.hpp"
#include "gfx/color.hpp"
#include "gfx/bitmap.hpp"
#include "gfx/renderer.hpp"
#include "menu/menu.hpp"
#include "menu/menuIds.hpp"
#include "rand.hpp"
#include "keys.hpp"
#include "settings.hpp"
#include "common.hpp"
#include "filesystem.hpp"
#include "appclock.hpp"
#include "screen.hpp"

#include <functional>
#include <memory>

// A message in a box centred on (x, y), as `Gfx::infoBox` shows it.
void drawInfoBox(Common& common, Bitmap& bmp, std::string const& text, int x, int y);

// The text being typed, `str`, in a box at (x, y) over `frozen`, as the
// text box shows it.
void drawInputBox(Common& common, Bitmap& bmp, Bitmap& frozen, std::string const& str, int x, int y, bool centered);

struct Key
{
	Key(int sym, char ch)
	: sym(sym), ch(ch)
	{
	}

	int sym;
	char ch;
};

struct Game;
struct Gfx;

// Where key events go (Gfx::processEvent).
struct KeySink
{
	virtual ~KeySink() {}
	virtual void onKey(int key, bool state) = 0;
};

// A message (Gfx::infoBox) until a key is pressed.
std::unique_ptr<Screen> infoBoxScreen(std::string const& text, int x = 320/2, int y = 200/2, bool clearScreen = true);

struct Joystick {
	SDL_GameController *sdlGameController;
	bool btnState[MaxJoyButtons];

	void clearState() {
		for ( int i = 0; i < MaxJoyButtons; ++i ) btnState[i] = false;
	}
};


// The app. Rust owns what it keeps besides the SDL windows (LieroApp in
// rust/liero-game/src/app.rs); the fields here refer to it.
struct Gfx
{
	Gfx();

	LieroApp* app = liero_app();

	bool pollEvent(SDL_Event& ev);
	// A screen showed nothing while waiting for an event (screen.hpp).
	void idle();

	void init();
	void setVideoMode();
	void onWindowResize(Uint32 windowId);
	void loadMenus();

	void process(KeySink* controller = 0);
	// draws a given surface onto an SDL texture/renderer, using a given Renderer
	void draw(SDL_Surface& surface, SDL_Texture& texture, SDL_Renderer& sdlRenderer, Renderer& renderer);
	void flip();

	void clearKeys();

	// The key state is in Rust (rust/liero-game/src/keys.rs).
	bool testKeyOnce(Uint32 key)
	{
		return liero_keys_test_once(&dosKeys, key);
	}

	bool testKey(Uint32 key)
	{
		return liero_keys_test(&dosKeys, key);
	}

	void releaseKey(Uint32 key)
	{
		liero_keys_set(&dosKeys, key, false);
	}

	void pressKey(Uint32 key)
	{
		liero_keys_set(&dosKeys, key, true);
	}

	void setKey(Uint32 key, bool state)
	{
		liero_keys_set(&dosKeys, key, state);
	}

	void toggleKey(Uint32 key)
	{
		liero_keys_toggle(&dosKeys, key);
	}

	bool testSDLKeyOnce(SDL_Scancode key)
	{
		return liero_keys_test_sdl_once(&dosKeys, key);
	}

	bool testSDLKey(SDL_Scancode key)
	{
		return liero_keys_test_sdl(&dosKeys, key);
	}

	void releaseSDLKey(SDL_Scancode key)
	{
		liero_keys_release_sdl(&dosKeys, key);
	}

	std::string getKeyName(uint32_t key);
	void setSpectatorFullscreen(bool newFullscreen);
	// `updateMenu`: show the change in the options menu.
	void setFullscreen(bool newFullscreen, bool updateMenu = true);
	void setDoubleRes(bool newDoubleRes, bool updateMenu = true);

	// The settings and their files are the Rust app's (app_files.rs).
	void saveSettings(FsNode node);
	bool loadSettings(FsNode node);
	bool loadSettingsLegacy(FsNode node);

	void processEvent(SDL_Event& ev, KeySink* controller = 0);

	// F8 on the menu: the developers' random TC.
	void randomizeTc();



	static void preparePalette(SDL_PixelFormat* format, Color realPal[256], uint32_t (&pal32)[256]);

	static void overlay(
		SDL_PixelFormat* format,
		uint8_t* src, int w, int h, std::size_t srcPitch,
		uint8_t* dest, std::size_t destPitch, int mag);

	// The folders are the Rust app's.
	void setConfigPath(std::string const& path);
	FsNode getConfigNode();

	// The folder of TCs, TC in the config folder unless set.
	void setTcPath(std::string const& path);
	FsNode getTcNode();

	// PRNG for things that don't affect the game
	Rand& rand = static_cast<Rand&>(*liero_app_rand(app));

	// renders everything for actual play
	Renderer playRenderer{liero_app_renderer(app, false)};
	// renders everything on a single screen, for single screen replay and
	// the spectator window
	Renderer singleScreenRenderer{liero_app_renderer(app, true)};

	// the renderer currently in use by the primary window. Usually
	// playRenderer, but is singleScreenRenderer if watching a replay in
	// single screen mode. Rust keeps which (liero_app_set_primary_single).
	Renderer& primaryRenderer()
	{
		return liero_app_primary(app) == singleScreenRenderer.rust() ? singleScreenRenderer : playRenderer;
	}

	// The menus, in Rust, and views of them.
	LieroMenus* menus;
	Menu mainMenu;
	Menu settingsMenu;
	Menu playerMenu;
	Menu hiddenMenu;

	// The menus' settings, which the Rust app owns; loading options
	// replaces them. A view of them, made again when they change.
	std::shared_ptr<Settings> const& settings();
	void setSettings(std::shared_ptr<Settings> s);

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
	// when the menu is open, the ongoing game on the screen is paused and
	// stored in this bitmap
	Bitmap frozenScreen{liero_app_frozen(app, false)};
	// when the menu is open, the ongoing game on the spectator screen is
	// paused and stored in this bitmap
	Bitmap frozenSpectatorScreen{liero_app_frozen(app, true)};

	bool running;
	bool spectatorFullscreen, doubleRes;

	Uint32 lastFrame;
	uint32_t& menuCycles = *liero_app_menu_cycles(app);
	int windowW, windowH;
	int prevMag; // Previous magnification used for drawing
	Rect lastUpdateRect; // Last region that was updated when flipping

	// The menus' TC, which the Rust app owns; choosing a TC replaces it. A
	// view of it, made again when it changes.
	std::shared_ptr<Common> const& common();
	void setCommon(std::shared_ptr<Common> c);

	std::vector<Joystick> joysticks;

	SDL_Keysym keyBuf[32], *keyBufPtr;

	std::vector<std::pair<int, int>> debugPoints;
	std::string debugInfo;

private:
	std::shared_ptr<Settings> settingsView;
	std::shared_ptr<Common> commonView;
};

extern Gfx gfx;

#endif // LIERO_GFX_HPP

#ifndef LIERO_GFX_HPP
#define LIERO_GFX_HPP

#include <SDL.h>
#include <gvl/resman/shared_ptr.hpp>
#include <gvl/math/rect.hpp>

#include <cstdio>
#include <cassert>

#include "gfx/font.hpp"
#include "gfx/blit.hpp"
#include "gfx/color.hpp"
#include "gfx/bitmap.hpp"
#include "gfx/renderer.hpp"
#include "menu/menu.hpp"
#include "menu/hiddenMenu.hpp"
#include "menu/mainMenu.hpp"
#include "rand.hpp"
#include "keys.hpp"
#include "settings.hpp"
#include "common.hpp"
#include "filesystem.hpp"

// The level name, "A vs B" and the worms' colours, as the spectator window
// shows them in the menus and weapon selection. The name is taken from
// `levelFileForName`, which weapon selection takes from other settings.
void drawVersusInfo(Common& common, Bitmap& bmp, Settings const& settings, std::string const& levelFileForName, int centerX, int centerY);

// A message in a box centred on (x, y), as `Gfx::infoBox` shows it.
void drawInfoBox(Common& common, Bitmap& bmp, std::string const& text, int x, int y);

// The text being typed, `str`, in a box at (x, y) over `frozen`, as
// `Gfx::inputString` shows it.
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
struct Controller;
struct Gfx;

struct PlayerMenu : Menu
{
	PlayerMenu(int x, int y)
	: Menu(x, y)
	{
	}

	enum
	{
		PlName,
		PlHealth,
		PlRed, PlGreen, PlBlue,
		PlUp,
		PlDown,
		PlLeft,
		PlRight,
		PlFire,
		PlChange,
		PlJump,
		PlDig,
		PlWeap0,
		PlController = PlWeap0 + 5,
		PlSaveProfile,
		PlSaveProfileAs,
		PlLoadProfile,
		PlLoadedProfile,
	};

	virtual void drawItemOverlay(Common& common, MenuItem& item, int x, int y, bool selected, bool disabled);

	virtual ItemBehavior* getItemBehavior(Common& common, MenuItem& item);

	gvl::shared_ptr<WormSettings> ws;
};

struct SettingsMenu : Menu
{
	enum
	{
		SiGameMode,
		SiLives,
		SiTimeToLose, // Extra
		SiTimeToWin,
		SiZoneTimeout,
		SiFlagsToWin, // Extra
		SiLoadingTimes,
		SiMaxBonuses,
		SiNamesOnBonuses,
		SiMap,
		SiAmountOfBlood,
		SiLevel,
		SiRegenerateLevel,
		SiWeaponOptions,
		LoadOptions,
		SaveOptions,
		LoadChange,
	};

	SettingsMenu(int x, int y)
	: Menu(x, y)
	{
	}

	virtual ItemBehavior* getItemBehavior(Common& common, MenuItem& item);

	virtual void onUpdate();
};

struct Joystick {
	SDL_GameController *sdlGameController;
	bool btnState[MaxJoyButtons];

	void clearState() {
		for ( int i = 0; i < MaxJoyButtons; ++i ) btnState[i] = false;
	}
};


struct Gfx
{
	Gfx();

	void init();
	void setVideoMode();
	void onWindowResize(Uint32 windowId);
	void loadMenus();

	void process(Controller* controller = 0);
	// draws a given surface onto an SDL texture/renderer, using a given Renderer
	void draw(SDL_Surface& surface, SDL_Texture& texture, SDL_Renderer& sdlRenderer, Renderer& renderer);
	void flip();
	void menuFlip(bool quitting = false);

	void clearKeys();

	bool testKeyOnce(Uint32 key)
	{
		bool state = dosKeys[key];
		dosKeys[key] = false;
		return state;
	}

	bool testKey(Uint32 key)
	{
		return dosKeys[key];
	}

	void releaseKey(Uint32 key)
	{
		dosKeys[key] = false;
	}

	void pressKey(Uint32 key)
	{
		dosKeys[key] = true;
	}

	void setKey(Uint32 key, bool state)
	{
		dosKeys[key] = state;
	}

	void toggleKey(Uint32 key)
	{
		dosKeys[key] = !dosKeys[key];
	}

	bool testSDLKeyOnce(SDL_Scancode key)
	{
		Uint32 k = SDLToDOSKey(key);
		return k ? testKeyOnce(k) : false;
	}

	bool testSDLKey(SDL_Scancode key)
	{
		Uint32 k = SDLToDOSKey(key);
		return k ? testKey(k) : false;
	}

	void releaseSDLKey(SDL_Scancode key)
	{
		Uint32 k = SDLToDOSKey(key);
		if(k)
			dosKeys[k] = false;
	}

	SDL_Keysym waitForKey();
	uint32_t waitForKeyEx();
	std::string getKeyName(uint32_t key);
	void setSpectatorFullscreen(bool newFullscreen);
	void setFullscreen(bool newFullscreen);
	void setDoubleRes(bool newDoubleRes);

	void saveSettings(FsNode node);
	bool loadSettings(FsNode node);
	bool loadSettingsLegacy(FsNode node);
	// Writes liero.cfg in the config folder if the settings changed since it
	// was last read or written here. The browser build calls it from the menu
	// loop, since a closed tab never reaches the save at the end of main.
	void saveSettingsIfChanged();

	void processEvent(SDL_Event& ev, Controller* controller = 0);

	int menuLoop();
	void mainLoop();
	void drawBasicMenu(/*int curSel*/);
	void drawSpectatorInfo();
	void playerSettings(int player);
	void openHiddenMenu();

	bool inputString(std::string& dest, std::size_t maxLen, int x, int y, int (*filter)(int) = 0, std::string const& prefix = "", bool centered = true);
	void inputInteger(int& dest, int min, int max, std::size_t maxLen, int x, int y);
	void selectLevel();
	int  selectReplay();
	void selectProfile(WormSettings& ws);
	std::unique_ptr<Common> selectTc();
	void selectOptions();
	void weaponOptions();
	void infoBox(std::string const& text, int x = 320/2, int y = 200/2, bool clearScreen = true);

	static void preparePalette(SDL_PixelFormat* format, Color realPal[256], uint32_t (&pal32)[256]);

	static void overlay(
		SDL_PixelFormat* format,
		uint8_t* src, int w, int h, std::size_t srcPitch,
		uint8_t* dest, std::size_t destPitch, int mag);

	void setConfigPath(std::string const& path)
	{
		configNode = FsNode(path);
	}

	FsNode getConfigNode()
	{
		return configNode;
	}

	// The folder of TCs, TC in the config folder unless set.
	void setTcPath(std::string const& path)
	{
		tcNode = FsNode(path);
	}

	FsNode getTcNode()
	{
		return tcNode;
	}

	// PRNG for things that don't affect the game
	Rand rand;

	// renders everything for actual play
	Renderer playRenderer;
	// renders everything on a single screen, for single screen replay and
	// the spectator window
	Renderer singleScreenRenderer;

	// the renderer currently in use by the primary window. Usually
	// playRenderer, but is singleScreenRenderer if watching a replay in
	// single screen mode.
	Renderer* primaryRenderer;

	FsNode configNode;
	FsNode tcNode;
	// liero.cfg as saveSettingsIfChanged last read or wrote it.
	std::string savedSettingsText;

	MainMenu mainMenu;
	SettingsMenu settingsMenu;
	PlayerMenu playerMenu;
	HiddenMenu hiddenMenu;

	Menu* curMenu;
	std::string prevSelectedReplayPath;
	FsNode settingsNode; // Currently loaded settings file. TODO: This is only used for display. We could just remember the name.
	gvl::shared_ptr<Settings> settings;

	bool dosKeys[177];
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
	Bitmap frozenScreen;
	// when the menu is open, the ongoing game on the spectator screen is
	// paused and stored in this bitmap
	Bitmap frozenSpectatorScreen;

	bool running;
	bool spectatorFullscreen, doubleRes;

	Uint32 lastFrame;
	unsigned menuCycles;
	int windowW, windowH;
	int prevMag; // Previous magnification used for drawing
	gvl::rect lastUpdateRect; // Last region that was updated when flipping
	gvl::shared_ptr<Common> common;
	std::unique_ptr<Controller> controller;

	std::vector<Joystick> joysticks;

	SDL_Keysym keyBuf[32], *keyBufPtr;

	std::vector<std::pair<int, int>> debugPoints;
	std::string debugInfo;
};

extern Gfx gfx;

#endif // LIERO_GFX_HPP

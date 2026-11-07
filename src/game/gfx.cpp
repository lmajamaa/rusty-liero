#include <SDL.h>
#include <SDL_image.h>
#include <cstring>
#include <cassert>
#include <cstdlib>
#include <cctype>
#include <vector>
#include <utility>
#include <algorithm>
#include <cstdio>
#include <memory>
#include <limits>
#include <random>
#ifdef __EMSCRIPTEN__
#include <emscripten.h>
#endif

#include "gfx.hpp"
#include "screen.hpp"
#include "reader.hpp"
#include "game.hpp"
#include "sfx.hpp"
#include "text.hpp"
#include "keys.hpp"
#include "filesystem.hpp"

#include <gvl/io2/fstream.hpp>
#include <gvl/system/system.hpp>

#include "controller/replayController.hpp"
#include "controller/localController.hpp"
#include "controller/controller.hpp"

#include "gfx/macros.hpp"

#include "menu/arrayEnumBehavior.hpp"
#include "menu/fileSelector.hpp"

Gfx gfx;

struct KeyBehavior : ItemBehavior
{
	KeyBehavior(Common& common, uint32_t& key, uint32_t& keyEx, bool extended = false)
	: common(common)
	, key(key)
	, keyEx(keyEx)
	, extended(extended)
	{
	}

	int onEnter(Menu& menu, MenuItem& item)
	{
		sfx.play(common, 27);
		KeyBehavior self = *this;
		gfx.open(keyChoiceScreen(extended), [self, &menu, &item](int result) mutable
		{
			uint32_t k = uint32_t(result);
			if(k != DkEscape)
			{
				if(!isExtendedKey(k))
					self.key = k;
				self.keyEx = k;

				self.onUpdate(menu, item);
			}

			gfx.clearKeys();
			return -1;
		});
		return -1;
	}

	void onUpdate(Menu& menu, MenuItem& item)
	{
		item.value = gfx.getKeyName(extended ? keyEx : key);
		item.hasValue = true;
	}

	Common& common;
	uint32_t& key;
	uint32_t& keyEx;
	bool extended;
};

struct WormNameBehavior : ItemBehavior
{
	WormNameBehavior(Common& common, WormSettings& ws)
	: common(common)
	, ws(ws)
	{
	}

	int onEnter(Menu& menu, MenuItem& item)
	{
		sfx.play(common, 27);

		ws.randomName = false;

		int x, y;
		if(!menu.itemPosition(item, x, y))
			return -1;

		x += menu.valueOffsetX + 2;

		WormNameBehavior self = *this;
		gfx.openInputString(ws.name, 20, x, y, [self, &menu, &item](bool ok, std::string const& name) mutable
		{
			if (ok)
				self.ws.name = name;

			if(self.ws.name.empty())
			{
				Settings::generateName(self.ws, gfx.rand);
			}
			sfx.play(self.common, 27);
			self.onUpdate(menu, item);
			return -1;
		});
		return -1;
	}

	void onUpdate(Menu& menu, MenuItem& item)
	{
		item.value = ws.name;
		item.hasValue = true;
	}

	Common& common;
	WormSettings& ws;
};


struct ProfileSaveBehavior : ItemBehavior
{
	ProfileSaveBehavior(Common& common, WormSettings& ws, bool saveAs = false)
	: common(common)
	, ws(ws)
	, saveAs(saveAs)
	{
	}

	int onEnter(Menu& menu, MenuItem& item)
	{
		sfx.play(common, 27);

		int x, y;
		if(!menu.itemPosition(item, x, y))
			return -1;

		x += menu.valueOffsetX + 2;

		if(saveAs)
		{
			Common& common = this->common;
			WormSettings& ws = this->ws;
			gfx.openInputString("", 30, x, y, [&common, &ws, &menu](bool ok, std::string const& name)
			{
				if(ok && !name.empty())
				{
					//ws.saveProfile(joinPath(joinPath(configRoot, "Profiles"), name));
					ws.saveProfile(gfx.getConfigNode() / "Profiles" / (name + ".lpf"));
				}

				sfx.play(common, 27);
				menu.updateItems(common);
				return -1;
			});
			return -1;
		}
		else
			ws.saveProfile(ws.profileNode);

		menu.updateItems(common);
		return -1;
	}

	void onUpdate(Menu& menu, MenuItem& item)
	{
		if(!saveAs)
		{
			item.visible = (bool)ws.profileNode;
		}
	}

	Common& common;
	WormSettings& ws;
	bool saveAs;
};

struct ProfileLoadedBehavior : ItemBehavior
{
	ProfileLoadedBehavior(Common& common, WormSettings& ws)
	: common(common)
	, ws(ws)
	{
	}

	void onUpdate(Menu& menu, MenuItem& item)
	{
		if (ws.profileNode)
		{
			item.value = getBasename(getLeaf(ws.profileNode.fullPath()));
			item.visible = true;
		}
		else
		{
			item.value.clear();
			item.visible = false;
		}

		item.hasValue = true;
	}

	Common& common;
	WormSettings& ws;
};

struct WeaponEnumBehavior : EnumBehavior
{
	WeaponEnumBehavior(Common& common, uint32_t& v)
	: EnumBehavior(common, v, 1, (uint32_t)common.weapons.size(), false)
	{
	}

	void onUpdate(Menu& menu, MenuItem& item)
	{
		item.value = common.weapons[common.weapOrder[v - 1]].name;
		item.hasValue = true;
	}

	int onEnter(Menu& menu, MenuItem& item)
	{
		sfx.play(common, 27);

		int x, y;
		if(!menu.itemPosition(item, x, y))
			return -1;

		x += menu.valueOffsetX + 2;

		WeaponEnumBehavior self = *this;
		gfx.openInputString("", 10, x, y, [self, &menu](bool ok, std::string const& search) mutable
		{
			if (ok)
				self.chooseClosest(menu, search);
			return -1;
		});

		return -1;
	}

	void chooseClosest(Menu& menu, std::string const& search)
	{
		{
			std::vector<LieroStr> names;
			for (uint32_t i = min; i <= max; ++i)
			{
				std::string& name = common.weapons[common.weapOrder[i - 1]].name;
				names.push_back({ reinterpret_cast<uint8_t const*>(name.data()), name.size() });
			}

			int closest = liero_menu_closest_name(names.data(), int32_t(names.size()),
				reinterpret_cast<uint8_t const*>(search.data()), search.size());
			if (closest >= 0)
				v = min + uint32_t(closest);
			menu.updateItems(common);
		}
	}
};

Gfx::Gfx()
: mainMenu(53, 20)
, settingsMenu(178, 20)
, playerMenu(178, 20)
, hiddenMenu(178, 20)
, curMenu(0)
, sdlDrawSurface(0)
, running(true)
, doubleRes(true)
, menuCycles(0)
, windowW(320 * 2)
, windowH(200 * 2)
, prevMag(0)
, keyBufPtr(keyBuf)
{
	clearKeys();
	primaryRenderer = &playRenderer;
}

void Gfx::init()
{
	SDL_ShowCursor(SDL_DISABLE);
	lastFrame = SDL_GetTicks();

	playRenderer.init(320, 200);
	singleScreenRenderer.init(640, 400);
	// Joystick init
	SDL_GameControllerEventState(SDL_ENABLE);
	int numJoysticks = SDL_NumJoysticks();
	joysticks.resize(numJoysticks);
	for ( int i = 0; i < numJoysticks; ++i ) {
		joysticks[i].sdlGameController= SDL_GameControllerOpen(i);
		joysticks[i].clearState();
	}
}

void Gfx::setVideoMode()
{
	int flags;

#ifdef __EMSCRIPTEN__
	// The page has one canvas, so there is no second window to show.
	settings->spectatorWindow = false;
#endif

	if (sdlSpectatorRenderer)
	{
		SDL_DestroyRenderer(sdlSpectatorRenderer);
		sdlSpectatorRenderer = NULL;
	}
	if (settings->spectatorWindow)
	{
		flags = SDL_WINDOW_RESIZABLE | SDL_WINDOW_ALLOW_HIGHDPI;
		if (spectatorFullscreen)
		{
			flags |= SDL_WINDOW_FULLSCREEN_DESKTOP;
		}
		if (!sdlSpectatorWindow)
		{
			sdlSpectatorWindow = SDL_CreateWindow("Liero Spectator Window",
				SDL_WINDOWPOS_UNDEFINED, SDL_WINDOWPOS_UNDEFINED, windowW, windowH, flags);
		}
		else
		{
			if (spectatorFullscreen)
			{
				SDL_SetWindowFullscreen(sdlSpectatorWindow,
					SDL_WINDOW_FULLSCREEN_DESKTOP);
			}
			else
			{
				SDL_SetWindowFullscreen(sdlSpectatorWindow, 0);
			}
		}
		sdlSpectatorRenderer = SDL_CreateRenderer(sdlSpectatorWindow, -1, 0/*SDL_RENDERER_PRESENTVSYNC*/);
		onWindowResize(SDL_GetWindowID(sdlSpectatorWindow));
	}
	else
	{
		if (sdlSpectatorTexture)
		{
			SDL_DestroyTexture(sdlSpectatorTexture);
			sdlSpectatorTexture = NULL;
		}
		if (sdlSpectatorDrawSurface)
		{
			SDL_FreeSurface(sdlSpectatorDrawSurface);
			sdlSpectatorDrawSurface = NULL;
		}
		if (sdlSpectatorWindow)
		{
			SDL_DestroyWindow(sdlSpectatorWindow);
			sdlSpectatorWindow = NULL;
		}
	}

	flags = SDL_WINDOW_RESIZABLE | SDL_WINDOW_ALLOW_HIGHDPI;

	if (settings->fullscreen)
	{
		flags |= SDL_WINDOW_FULLSCREEN_DESKTOP;
	}

	if (!sdlWindow)
	{
		int x = SDL_WINDOWPOS_UNDEFINED;
		int y = SDL_WINDOWPOS_UNDEFINED;
		if (sdlSpectatorWindow)
		{
			SDL_GetWindowPosition(sdlSpectatorWindow, &x, &y);
		}
		sdlWindow = SDL_CreateWindow("Liero 1.39", x + 100, y + 50, windowW, windowH, flags);

		// The Mac app will automatically use the .icns icon file located in the
		// .app bundle, so don't override that here.
#ifndef __APPLE__
		std::string s = joinPath(getConfigNode().fullPath(), "icon.png");
		SDL_Surface *icon = IMG_Load(s.c_str());
		if (icon)
		{
			SDL_SetWindowIcon(sdlWindow, icon);
			SDL_FreeSurface(icon);
		}
#endif
	}
	else
	{
		if (settings->fullscreen)
		{
			SDL_SetWindowFullscreen(sdlWindow, SDL_WINDOW_FULLSCREEN_DESKTOP);
		}
		else
		{
			SDL_SetWindowFullscreen(sdlWindow, 0);
		}
	}
	if (sdlRenderer)
	{
		SDL_DestroyRenderer(sdlRenderer);
		sdlRenderer = NULL;
	}
	// vertical sync is always disabled. Frame limiting is done manually below,
	// to keep the correct speed
	sdlRenderer = SDL_CreateRenderer(sdlWindow, -1, 0 /*SDL_RENDERER_PRESENTVSYNC*/);
	onWindowResize(SDL_GetWindowID(sdlWindow));

	// Set the spectator window's icon after the main window has been initialized.
	// On Windows, this makes sure the icon in the stacked taskbar is the main icon.
	// On MacOS this is commented out, because it only allows one icon and the spectator icon
	// will override the main icon
#ifndef __APPLE__
	if (sdlSpectatorWindow)
	{
		std::string s = joinPath(getConfigNode().fullPath(), "spectator_icon.png");
		SDL_Surface *spectator_icon = IMG_Load(s.c_str());
		if (spectator_icon)
		{
			SDL_SetWindowIcon(sdlSpectatorWindow, spectator_icon);
			SDL_FreeSurface(spectator_icon);
		}
	}
#endif
}

void Gfx::onWindowResize(Uint32 windowID)
{
	if (windowID == SDL_GetWindowID(sdlWindow))
	{
		if (sdlTexture)
		{
			SDL_DestroyTexture(sdlTexture);
			sdlTexture = NULL;
		}
		sdlTexture = SDL_CreateTexture(sdlRenderer, SDL_PIXELFORMAT_ARGB8888,
			                           SDL_TEXTUREACCESS_STREAMING,
			                           doubleRes ? 640 : 320,
		                         	   doubleRes ? 400 : 200);

		if (sdlDrawSurface)
		{
			SDL_FreeSurface(sdlDrawSurface);
			sdlDrawSurface = NULL;
		}
		sdlDrawSurface = SDL_CreateRGBSurface(0, doubleRes ? 640 : 320,
		                         doubleRes ? 400 : 200, 32, 0, 0, 0, 0);
#ifdef __EMSCRIPTEN__
		// The canvas fills the window, so keep the pixels sharp when scaling up.
		SDL_SetHint(SDL_HINT_RENDER_SCALE_QUALITY, "nearest");
#else
		// linear for that old-school chunky look, but consider adding a user
		// option for this
		SDL_SetHint(SDL_HINT_RENDER_SCALE_QUALITY, "linear");
#endif
		SDL_RenderSetLogicalSize(sdlRenderer, doubleRes ? 640 : 320,
		                         doubleRes ? 400 : 200);
	}
	else
	{
		if (sdlSpectatorTexture)
		{
			SDL_DestroyTexture(sdlSpectatorTexture);
			sdlSpectatorTexture = NULL;
		}
		if (sdlSpectatorDrawSurface)
		{
			SDL_FreeSurface(sdlSpectatorDrawSurface);
			sdlSpectatorDrawSurface = NULL;
		}

		if (settings->spectatorWindow)
		{
			sdlSpectatorTexture = SDL_CreateTexture(sdlSpectatorRenderer,
			                                        SDL_PIXELFORMAT_ARGB8888,
				                           			SDL_TEXTUREACCESS_STREAMING,
				                           			640, 400);
			sdlSpectatorDrawSurface = SDL_CreateRGBSurface(0, 640, 400, 32, 0,
			                                               0, 0, 0);
			SDL_RenderSetLogicalSize(sdlSpectatorRenderer, 640, 400);
		}
	}
}

void Gfx::loadMenus()
{
	hiddenMenu.addItem(MenuItem(48, 7, "FULLSCREEN (F11)", HiddenMenu::Fullscreen));
	hiddenMenu.addItem(MenuItem(48, 7, "DOUBLE SIZE", HiddenMenu::DoubleRes));
	hiddenMenu.addItem(MenuItem(48, 7, "POWERLEVEL PALETTES", HiddenMenu::LoadPowerLevels));
	hiddenMenu.addItem(MenuItem(48, 7, "SHADOWS", HiddenMenu::Shadows));
	hiddenMenu.addItem(MenuItem(48, 7, "AUTO-RECORD REPLAYS", HiddenMenu::RecordReplays));
	hiddenMenu.addItem(MenuItem(48, 7, "AI FRAMES", HiddenMenu::AiFrames));
	hiddenMenu.addItem(MenuItem(48, 7, "AI MUTATIONS", HiddenMenu::AiMutations));
	hiddenMenu.addItem(MenuItem(48, 7, "AI PARALLELS", HiddenMenu::AiParallels));
	hiddenMenu.addItem(MenuItem(48, 7, "AI TRACES", HiddenMenu::AiTraces));
	hiddenMenu.addItem(MenuItem(48, 7, "PALETTE", HiddenMenu::PaletteSelect));
	hiddenMenu.addItem(MenuItem(48, 7, "BOT WEAPONS", HiddenMenu::SelectBotWeapons));
	hiddenMenu.addItem(MenuItem(48, 7, "SEE SPAWN POINT", HiddenMenu::AllowViewingSpawnPoint));
	hiddenMenu.addItem(MenuItem(48, 7, "SINGLE SCREEN REPLAY", HiddenMenu::SingleScreenReplay));
#ifndef __EMSCRIPTEN__
	hiddenMenu.addItem(MenuItem(48, 7, "SPECTATOR WINDOW", HiddenMenu::SpectatorWindow));
#endif

	playerMenu.addItem(MenuItem(3, 7, "PROFILE LOADED", PlayerMenu::PlLoadedProfile));
	playerMenu.addItem(MenuItem(3, 7, "SAVE PROFILE", PlayerMenu::PlSaveProfile));
	playerMenu.addItem(MenuItem(3, 7, "SAVE PROFILE AS...", PlayerMenu::PlSaveProfileAs));
	playerMenu.addItem(MenuItem(3, 7, "LOAD PROFILE", PlayerMenu::PlLoadProfile));
	playerMenu.addItem(MenuItem(48, 7, "NAME", PlayerMenu::PlName));
	playerMenu.addItem(MenuItem(48, 7, "HEALTH", PlayerMenu::PlHealth));
	playerMenu.addItem(MenuItem(48, 7, "Red", PlayerMenu::PlRed));
	playerMenu.addItem(MenuItem(48, 7, "Green", PlayerMenu::PlGreen));
	playerMenu.addItem(MenuItem(48, 7, "Blue", PlayerMenu::PlBlue));
	playerMenu.addItem(MenuItem(48, 7, "AIM UP", PlayerMenu::PlUp));
	playerMenu.addItem(MenuItem(48, 7, "AIM DOWN", PlayerMenu::PlDown));
	playerMenu.addItem(MenuItem(48, 7, "MOVE LEFT", PlayerMenu::PlLeft));
	playerMenu.addItem(MenuItem(48, 7, "MOVE RIGHT", PlayerMenu::PlRight));
	playerMenu.addItem(MenuItem(48, 7, "FIRE", PlayerMenu::PlFire));
	playerMenu.addItem(MenuItem(48, 7, "CHANGE", PlayerMenu::PlChange));
	playerMenu.addItem(MenuItem(48, 7, "JUMP", PlayerMenu::PlJump));
	playerMenu.addItem(MenuItem(48, 7, "DIG", PlayerMenu::PlDig));

	for (int i = 0; i < 5; ++i)
		playerMenu.addItem(MenuItem(48, 7, std::string("WEAPON ") + (char)(i + '1'), PlayerMenu::PlWeap0 + i));

	playerMenu.addItem(MenuItem(48, 7, "CONTROLLER", PlayerMenu::PlController));

	settingsMenu.addItem(MenuItem(48, 7, "GAME MODE", SettingsMenu::SiGameMode));
	settingsMenu.addItem(MenuItem(48, 7, "TIME TO LOSE", SettingsMenu::SiTimeToLose));
	settingsMenu.addItem(MenuItem(48, 7, "TIME TO WIN", SettingsMenu::SiTimeToWin));
	settingsMenu.addItem(MenuItem(48, 7, "ZONE TIMEOUT", SettingsMenu::SiZoneTimeout));
	settingsMenu.addItem(MenuItem(48, 7, "FLAGS TO WIN", SettingsMenu::SiFlagsToWin));
	settingsMenu.addItem(MenuItem(48, 7, "LIVES", SettingsMenu::SiLives));
	settingsMenu.addItem(MenuItem(48, 7, "LEVEL", SettingsMenu::SiLevel));
	settingsMenu.addItem(MenuItem(48, 7, "LOADING TIMES", SettingsMenu::SiLoadingTimes));
	settingsMenu.addItem(MenuItem(48, 7, "WEAPON OPTIONS", SettingsMenu::SiWeaponOptions));
	settingsMenu.addItem(MenuItem(48, 7, "MAX BONUSES", SettingsMenu::SiMaxBonuses));
	settingsMenu.addItem(MenuItem(48, 7, "NAMES ON BONUSES", SettingsMenu::SiNamesOnBonuses));
	settingsMenu.addItem(MenuItem(48, 7, "MAP", SettingsMenu::SiMap));
	settingsMenu.addItem(MenuItem(48, 7, "AMOUNT OF BLOOD", SettingsMenu::SiAmountOfBlood));
	settingsMenu.addItem(MenuItem(48, 7, "LOAD+CHANGE", SettingsMenu::LoadChange));
	settingsMenu.addItem(MenuItem(48, 7, "REGENERATE LEVEL", SettingsMenu::SiRegenerateLevel));
	settingsMenu.addItem(MenuItem(48, 7, "SAVE SETUP AS...", SettingsMenu::SaveOptions));
	settingsMenu.addItem(MenuItem(48, 7, "LOAD SETUP", SettingsMenu::LoadOptions));

	mainMenu.addItem(MenuItem(10, 10, "", MainMenu::MaResumeGame)); // string set in menuLoop
	mainMenu.addItem(MenuItem(10, 10, "", MainMenu::MaNewGame)); // string set in menuLoop
	mainMenu.addItem(MenuItem(48, 48, "OPTIONS (F2)", MainMenu::MaAdvanced));
	mainMenu.addItem(MenuItem(48, 48, "REPLAYS (F3)", MainMenu::MaReplays));
	mainMenu.addItem(MenuItem(48, 48, "TC", MainMenu::MaTc));
	mainMenu.addItem(MenuItem(6, 6, "QUIT TO OS", MainMenu::MaQuit));
	mainMenu.addItem(MenuItem::space());
	mainMenu.addItem(MenuItem(48, 48, "LEFT PLAYER (F5)", MainMenu::MaPlayer1Settings));
	mainMenu.addItem(MenuItem(48, 48, "RIGHT PLAYER (F6)", MainMenu::MaPlayer2Settings));
	mainMenu.addItem(MenuItem(48, 48, "MATCH SETUP (F7)", MainMenu::MaSettings));

	settingsMenu.valueOffsetX = 100;
	playerMenu.valueOffsetX = 95;
	hiddenMenu.valueOffsetX = 120;
}

void Gfx::setSpectatorFullscreen(bool newFullscreen)
{
	if (newFullscreen == spectatorFullscreen)
		return;
	spectatorFullscreen = newFullscreen;

	if (!spectatorFullscreen)
	{
		if (doubleRes)
		{
			windowW = 640;
			windowH = 400;
		}
		else
		{
			windowW = 320;
			windowH = 200;
		}
	}
	setVideoMode();
}

void Gfx::setFullscreen(bool newFullscreen)
{
	if (newFullscreen == settings->fullscreen)
		return;
	settings->fullscreen = newFullscreen;

	// fullscreen will automatically set window size
	if (!settings->fullscreen)
	{
		if (doubleRes)
		{
			windowW = 640;
			windowH = 400;
		}
		else
		{
			windowW = 320;
			windowH = 200;
		}
	}
	setVideoMode();
	hiddenMenu.updateItems(*common);
}

void Gfx::setDoubleRes(bool newDoubleRes)
{
	if (newDoubleRes == doubleRes)
		return;
	doubleRes = newDoubleRes;

	if (!newDoubleRes)
	{
		windowW = 320;
		windowH = 200;
	}
	else
	{
		windowW = 640;
		windowH = 400;
	}
	setVideoMode();
	hiddenMenu.updateItems(*common);
}

namespace
{

// Passes joystick buttons that changed to the controller.
void sendKeys(Controller* controller, LieroKeyChanges const& changes)
{
	if (!controller)
		return;
	for (uint32_t i = 0; i < changes.count; ++i)
		controller->onKey(changes.keys[i], changes.states[i]);
}

} // namespace

void Gfx::processEvent(SDL_Event& ev, Controller* controller)
{
	switch(ev.type)
	{
		case SDL_KEYDOWN:
		{

			SDL_Scancode s = ev.key.keysym.scancode;

			if (keyBufPtr < keyBuf + 32)
				*keyBufPtr++ = ev.key.keysym;

			Uint32 dosScan = liero_keys_event(&dosKeys, s, true);
			if(dosScan && controller)
				controller->onKey(dosScan, true);

			if(s == SDL_SCANCODE_F11)
			{
				if (SDL_GetWindowFromID(ev.key.windowID) == sdlWindow)
				{
					setFullscreen(!settings->fullscreen);
				}
				else
				{
					setSpectatorFullscreen(!spectatorFullscreen);
				}

			}
		}
		break;

		case SDL_KEYUP:
		{
			SDL_Scancode s = ev.key.keysym.scancode;

			Uint32 dosScan = liero_keys_event(&dosKeys, s, false);
			if(dosScan && controller)
				controller->onKey(dosScan, false);
		}
		break;

		case SDL_WINDOWEVENT:
		{
			switch (ev.window.event)
			{
				case SDL_WINDOWEVENT_RESIZED:
				{
					onWindowResize(ev.window.windowID);
				}
				break;

				default:
				break;
			}
		}
		break;

		case SDL_QUIT:
		{
			running = false;
		}
		break;

		case SDL_JOYAXISMOTION:
			sendKeys(controller, liero_joy_axis(joysticks[ev.jaxis.which].btnState, ev.jaxis.which, ev.jaxis.axis, ev.jaxis.value));
		break;

		case SDL_JOYHATMOTION:
			sendKeys(controller, liero_joy_hat(joysticks[ev.jhat.which].btnState, ev.jhat.which, ev.jhat.value));
		break;

		case SDL_JOYBUTTONDOWN:
		case SDL_JOYBUTTONUP: /* Fall-through */
			sendKeys(controller, liero_joy_button(joysticks[ev.jbutton.which].btnState, ev.jbutton.which, ev.jbutton.button, ev.jbutton.state == SDL_PRESSED));
		break;

		default:
			break;
	}
}

uint32_t appTicks()
{
	return gfx.driver ? gfx.driver->ticks() : SDL_GetTicks();
}

uint64_t appHiresTicks()
{
	return gfx.driver ? uint64_t(gfx.driver->ticks()) * 1000000 : gvl::get_hires_ticks();
}

std::time_t appTime()
{
	return gfx.driver ? gfx.driver->time() : std::time(0);
}

bool Gfx::pollEvent(SDL_Event& ev)
{
	return driver ? driver->pollEvent(ev) : SDL_PollEvent(&ev) != 0;
}

void Gfx::idle()
{
	if (driver)
	{
		driver->idle();
		return;
	}

	// Until the frame function runs from the browser's main loop (PLAN.md,
	// "Replacing the main loop", step 4), runScreens is a loop of its own:
	// a screen waiting for an event gives the browser a frame's time to
	// deliver it, as the SDL_WaitEvent it replaces did.
#ifdef __EMSCRIPTEN__
	emscripten_sleep(14);
#else
	SDL_Delay(14);
#endif
	lastFrame = SDL_GetTicks();
}

void Gfx::process(Controller* controller)
{
	SDL_Event ev;
	keyBufPtr = keyBuf;
	while(pollEvent(ev))
	{
		processEvent(ev, controller);
	}
}

std::string Gfx::getKeyName(uint32_t key)
{
	int joy, button;
	switch (liero_key_name_kind(key, &joy, &button))
	{
	case 0: return common->texts.keyNames[key];
	case 1: return "J" + toString(joy) + "_" + toString(button);
	default: return "";
	}
}

void Gfx::clearKeys()
{
	liero_keys_clear(&dosKeys);
}

void Gfx::preparePalette(SDL_PixelFormat* format, Color realPal[256], uint32_t (&pal32)[256])
{
	for(int i = 0; i < 256; ++i)
	{
		pal32[i] = SDL_MapRGB(format, realPal[i].r, realPal[i].g, realPal[i].b);
	}
}

void Gfx::menuFade(bool quitting)
{
	if (playRenderer.fadeValue < 32 && !quitting)
		++playRenderer.fadeValue;
	if (singleScreenRenderer.fadeValue < 32 && !quitting)
		++singleScreenRenderer.fadeValue;

	++menuCycles;
	playRenderer.pal = playRenderer.origpal;
	playRenderer.pal.rotateFrom(playRenderer.origpal, 168, 174, menuCycles);
	playRenderer.pal.setWormColours(*settings);
	playRenderer.pal.fade(playRenderer.fadeValue);
	singleScreenRenderer.pal = singleScreenRenderer.origpal;
	singleScreenRenderer.pal.rotateFrom(singleScreenRenderer.origpal, 168, 174, menuCycles);
	singleScreenRenderer.pal.setWormColours(*settings);
	singleScreenRenderer.pal.fade(singleScreenRenderer.fadeValue);
}

void Gfx::draw(SDL_Surface& surface, SDL_Texture& texture, SDL_Renderer& sdlRenderer, Renderer& renderer)
{
	gvl::rect updateRect;
	Color realPal[256];
	renderer.pal.activate(realPal);
	int offsetX, offsetY;
	int mag = fitScreen(surface.w, surface.h,
						renderer.renderResX, renderer.renderResY, offsetX, offsetY);

	gvl::rect newRect(offsetX, offsetY, renderer.renderResX * mag, renderer.renderResY * mag);

	if(mag != prevMag)
	{
		// Clear background if magnification is decreased to
		// avoid leftovers.
		SDL_FillRect(&surface, 0, 0);
		updateRect = lastUpdateRect | newRect;
	}
	else
		updateRect = newRect;
	prevMag = mag;

	std::size_t destPitch = surface.pitch;
	std::size_t srcPitch = renderer.bmp.pitch;

	PalIdx* dest = reinterpret_cast<PalIdx*>(surface.pixels) + offsetY * destPitch + offsetX * surface.format->BytesPerPixel;
	PalIdx* src = renderer.bmp.pixels;

	uint32_t pal32[256];
	preparePalette(surface.format, realPal, pal32);
	scaleDraw(src, renderer.renderResX, renderer.renderResY, srcPitch, dest, destPitch, mag, pal32);

	SDL_UpdateTexture(&texture, NULL, surface.pixels, surface.w * 4);
	SDL_RenderClear(&sdlRenderer);
	SDL_RenderCopy(&sdlRenderer, &texture, NULL, NULL);
	SDL_RenderPresent(&sdlRenderer);

	lastUpdateRect = updateRect;
}

void Gfx::flip()
{
	if (driver)
	{
		driver->frame();
		return;
	}

	// draw into the play window. This uses either the normal split screen renderer
	// or the single screen renderer if this is a replay and single screen replay
	// is turned on
	draw(*sdlDrawSurface, *sdlTexture, *sdlRenderer, *primaryRenderer);
	if (settings->spectatorWindow)
	{
		draw(*sdlSpectatorDrawSurface, *sdlSpectatorTexture, *sdlSpectatorRenderer, singleScreenRenderer);
	}

	static unsigned int const delay = 14u;

	uint32_t wantedTime = lastFrame + delay;

#ifdef __EMSCRIPTEN__
	// The browser only shows the frame and delivers input once we return to it,
	// so yield (through ASYNCIFY) every frame, even when we're behind.
	uint32_t now = SDL_GetTicks();
	emscripten_sleep(now < wantedTime ? wantedTime - now : 0);
#else
	while(true)
	{
		uint32_t now = SDL_GetTicks();
		if(now >= wantedTime)
			break;

		SDL_Delay(wantedTime - now);
	}
#endif

	lastFrame = wantedTime;
}

void resetLeftRight()
{
	gfx.releaseSDLKey(SDL_SCANCODE_LEFT);
	gfx.releaseSDLKey(SDL_SCANCODE_RIGHT);
}

struct ProfileLoadBehavior : ItemBehavior
{
	ProfileLoadBehavior(Common& common, WormSettings& ws)
	: common(common)
	, ws(ws)
	{
	}

	int onEnter(Menu& menu, MenuItem& item)
	{
		sfx.play(common, 27);
		Common& common = this->common;
		gfx.open(selectProfileScreen(ws), [&common, &menu](int)
		{
			sfx.play(common, 27);
			menu.updateItems(common);
			return -1;
		});
		return -1;
	}

	Common& common;
	WormSettings& ws;
};



struct PlayerSettingsBehavior : ItemBehavior
{
	PlayerSettingsBehavior(Common& common, int player)
	: common(common)
	, player(player)
	{
	}

	int onEnter(Menu& menu, MenuItem& item)
	{
		sfx.play(common, 27);
		gfx.playerSettings(player);
		return -1;
	}

	Common& common;
	int player;
};

struct LevelSelectBehavior : ItemBehavior
{
	LevelSelectBehavior(Common& common)
	: common(common)
	{
	}

	int onEnter(Menu& menu, MenuItem& item)
	{
		sfx.play(common, 27);
		Common& common = this->common;
		gfx.open(selectLevelScreen(), [&common, &menu, &item](int)
		{
			sfx.play(common, 27);
			LevelSelectBehavior(common).onUpdate(menu, item);
			return -1;
		});
		return -1;
	}

	void onUpdate(Menu& menu, MenuItem& item)
	{
		item.hasValue = true;
		if(!gfx.settings->randomLevel)
		{
			item.value = '"' + getBasename(getLeaf(gfx.settings->levelFile)) + '"';
			menu.itemFromId(SettingsMenu::SiRegenerateLevel)->string = LS(ReloadLevel); // Not string?
		}
		else
		{
			item.value = LS(Random2);
			menu.itemFromId(SettingsMenu::SiRegenerateLevel)->string = LS(RegenLevel);
		}
	}

	Common& common;
};

struct WeaponOptionsBehavior : ItemBehavior
{
	WeaponOptionsBehavior(Common& common)
	: common(common)
	{
	}

	int onEnter(Menu& menu, MenuItem& item)
	{
		sfx.play(common, 27);
		Common& common = this->common;
		gfx.open(weaponOptionsScreen(), [&common](int)
		{
			sfx.play(common, 27);
			return -1;
		});
		return -1;
	}

	Common& common;
};

struct OptionsSaveBehavior : ItemBehavior
{
	OptionsSaveBehavior(Common& common)
	: common(common)
	{
	}

	int onEnter(Menu& menu, MenuItem& item)
	{
		sfx.play(common, 27);

		int x, y;
		if(!menu.itemPosition(item, x, y))
			return -1;

		x += menu.valueOffsetX + 2;

		std::string name = getBasename(getLeaf(gfx.settingsNode.fullPath()));
		OptionsSaveBehavior self = *this;
		gfx.openInputString(name, 30, x, y, [self, &menu, &item](bool ok, std::string const& name) mutable
		{
			if(ok && !name.empty())
			{
				//gfx.saveSettings(joinPath(configRoot, name + ".cfg"));
				gfx.saveSettings(gfx.getConfigNode() / (name + ".cfg"));
			}

			sfx.play(self.common, 27);

			self.onUpdate(menu, item);
			return -1;
		});
		return -1;
	}

	void onUpdate(Menu& menu, MenuItem& item)
	{
		item.value = getBasename(getLeaf(gfx.settingsNode.fullPath()));
		item.hasValue = true;
	}

	Common& common;
};

struct OptionsSelectBehavior : ItemBehavior
{
	OptionsSelectBehavior(Common& common)
	: common(common)
	{
	}

	int onEnter(Menu& menu, MenuItem& item)
	{
		sfx.play(common, 27);
		Common& common = this->common;
		gfx.open(selectOptionsScreen(), [&common, &menu](int)
		{
			sfx.play(common, 27);
			menu.updateItems(common);
			return -1;
		});
		return -1;
	}

	Common& common;
};

ItemBehavior* SettingsMenu::getItemBehavior(Common& common, MenuItem& item)
{
	switch(item.id)
	{
		case SiNamesOnBonuses:
			return new BooleanSwitchBehavior(common, gfx.settings->namesOnBonuses);
		case SiMap:
			return new BooleanSwitchBehavior(common, gfx.settings->map);
		case SiRegenerateLevel:
			return new BooleanSwitchBehavior(common, gfx.settings->regenerateLevel);
		case SiLoadingTimes:
			return new IntegerBehavior(common, gfx.settings->loadingTime, 0, 9999, 1, true);
		case SiMaxBonuses:
			return new IntegerBehavior(common, gfx.settings->maxBonuses, 0, 99, 1);
		case SiAmountOfBlood:
		{
			IntegerBehavior* ret = new IntegerBehavior(common, gfx.settings->blood, 0, LC(BloodLimit), LC(BloodStepUp), true);
			ret->allowEntry = false;
			return ret;
		}

		case SiLives:
			return new IntegerBehavior(common, gfx.settings->lives, 1, 999, 1);
		case SiTimeToLose:
		case SiTimeToWin:
			return new TimeBehavior(common, gfx.settings->timeToLose, 60, 3600, 10);
		case SiZoneTimeout:
			return new TimeBehavior(common, gfx.settings->zoneTimeout, 10, 3600, 10);
		case SiFlagsToWin:
			return new IntegerBehavior(common, gfx.settings->flagsToWin, 1, 999, 1);

		case SiLevel:
			return new LevelSelectBehavior(common);

		case SiGameMode:
			return new ArrayEnumBehavior(common, gfx.settings->gameMode, common.texts.gameModes);
		case SiWeaponOptions:
			return new WeaponOptionsBehavior(common);
		case LoadOptions:
			return new OptionsSelectBehavior(common);
		case SaveOptions:
			return new OptionsSaveBehavior(common);
		case LoadChange:
			return new BooleanSwitchBehavior(common, gfx.settings->loadChange);
		default:
			return Menu::getItemBehavior(common, item);
	}
}

void SettingsMenu::onUpdate()
{
	setVisibility(SiLives, false);
	setVisibility(SiTimeToLose, false);
	setVisibility(SiTimeToWin, false);
	setVisibility(SiZoneTimeout, false);
	setVisibility(SiFlagsToWin, false);

	switch(gfx.settings->gameMode)
	{
		case Settings::GMKillEmAll:
		case Settings::GMScalesOfJustice:
			setVisibility(SiLives, true);
		break;

		case Settings::GMGameOfTag:
			setVisibility(SiTimeToLose, true);
		break;

		case Settings::GMHoldazone:
			setVisibility(SiTimeToWin, true);
			setVisibility(SiZoneTimeout, true);
		break;
	}
}

using std::string;
using std::vector;
using std::pair;
using gvl::shared_ptr;

namespace
{

// A file selector over the frozen menu. Each tick reads the events of the
// frame before it, draws, and closes on Escape or a chosen file.
struct SelectorScreen : Screen
{
	FileSelector sel;
	bool shown = false;

	explicit SelectorScreen(int x = 178)
	: sel(*gfx.common, x)
	{
	}

	Tick tick() override
	{
		if (shown)
			gfx.process();

		gfx.playRenderer.bmp.copy(gfx.frozenScreen);
		drawTitle();
		sel.draw();

		if (!sel.process())
			return Tick::close(-1);

		if(gfx.testSDLKeyOnce(SDL_SCANCODE_RETURN)
		|| gfx.testSDLKeyOnce(SDL_SCANCODE_KP_ENTER))
		{
			int result = -1;
			if (enter(result))
				return Tick::close(result);
		}

		gfx.menuFade();
		shown = true;
		return Tick::stay(true);
	}

	// The title, and anything else drawn before the list.
	virtual void drawTitle() = 0;
	// Enter was pressed: true to close with `result`.
	virtual bool enter(int& result) = 0;

	void drawFramedTitle(string title)
	{
		if (!sel.currentNode->fullPath.empty())
		{
			title += ' ';
			title += sel.currentNode->fullPath;
		}

		gfx.common->font.drawFramedText(gfx.playRenderer.bmp, title, 178, 20, 50);
	}
};

struct LevelSelectScreen : SelectorScreen
{
	shared_ptr<FileNode> random;
	FileNode* previewNode = 0;

	LevelSelectScreen()
	{
		Common& common = *gfx.common;
		random.reset(new FileNode(LS(Random), "", "", false, &sel.rootNode));

		sel.fill(gfx.getConfigNode(), [](string const& name, string const& ext) { return ciCompare(ext, "LEV"); });

		random->id = 1;
		sel.rootNode.children.insert(sel.rootNode.children.begin(), random);
		sel.setFolder(sel.rootNode);
		sel.select(gfx.settings->levelFile);
	}

	void drawTitle() override
	{
		Common& common = *gfx.common;
		string title = LS(SelLevel);
		if (!sel.currentNode->fullPath.empty())
		{
			title += ' ';
			title += sel.currentNode->fullPath;
		}

		int wid = common.font.getDims(title);

		drawRoundedBox(gfx.playRenderer.bmp, 178, 20, 0, 7, wid);
		common.font.drawText(gfx.playRenderer.bmp, title, 180, 21, 50);

		FileNode* cur = sel.curSel();
		if (previewNode != cur && cur && cur != random.get() && !cur->folder)
		{
			Level level(common);

			try
			{
				if (level.load(common, *gfx.settings, cur->getFsNode().toOctetReader()))
				{
					int centerX = gfx.singleScreenRenderer.renderResX / 2;

					level.drawMiniature(gfx.frozenScreen, 134, 162, 10);
					level.drawMiniature(gfx.frozenSpectatorScreen, centerX - 126, gfx.singleScreenRenderer.renderResY - 208, 2);
				}
			}
			catch (std::runtime_error&)
			{
				// Ignore
			}

			previewNode = cur;
		}
	}

	bool enter(int& result) override
	{
		sfx.play(*gfx.common, 27);

		auto* chosen = sel.enter();
		if (!chosen)
			return false;

		if (chosen == random.get())
		{
			gfx.settings->randomLevel = true;
			gfx.settings->levelFile.clear();
		}
		else
		{
			gfx.settings->randomLevel = false;
			gfx.settings->levelFile = chosen->fullPath;
		}
		return true;
	}
};

struct ProfileSelectScreen : SelectorScreen
{
	WormSettings& ws;

	explicit ProfileSelectScreen(WormSettings& ws)
	: SelectorScreen(28)
	, ws(ws)
	{
		sel.fill(gfx.getConfigNode(), [](string const& name, string const& ext) { return ciCompare(ext, "LPF"); });

		sel.setFolder(sel.rootNode);
		sel.select(joinPath(gfx.getConfigNode().fullPath(), "Profiles"));
	}

	void drawTitle() override
	{
		drawFramedTitle("Select profile:");
	}

	bool enter(int& result) override
	{
		auto* chosen = sel.enter();
		if (!chosen)
			return false;
		ws.loadProfile(chosen->getFsNode());
		return true;
	}
};

struct ReplaySelectScreen : SelectorScreen
{
	ReplaySelectScreen()
	: SelectorScreen(28)
	{
		sel.fill(gfx.getConfigNode(), [](string const& name, string const& ext) { return ciCompare(ext, "LRP"); });

		sel.setFolder(sel.rootNode);
		if (gfx.prevSelectedReplayPath.empty()
		  || !sel.select(gfx.prevSelectedReplayPath))
		{
			sel.select(joinPath(gfx.getConfigNode().fullPath(), "Replays"));
		}
	}

	void drawTitle() override
	{
		drawFramedTitle("Select replay:");
	}

	bool enter(int& result) override
	{
		auto* chosen = sel.enter();
		if (!chosen)
			return false;

		gfx.prevSelectedReplayPath = chosen->fullPath;

		// Reset controller before opening the replay, since we may be recording it
		gfx.controller.reset();

		gfx.controller.reset(new ReplayController(gfx.common, chosen->getFsNode().toSource()));

		result = MainMenu::MaReplay;
		return true;
	}
};

struct OptionsSelectScreen : SelectorScreen
{
	OptionsSelectScreen()
	: SelectorScreen(28)
	{
		sel.fill(gfx.getConfigNode(), [](string const& name, string const& ext) {
			return ciCompare(ext, "CFG");
		});

		sel.setFolder(sel.rootNode);
	}

	void drawTitle() override
	{
		drawFramedTitle("Select options:");
	}

	bool enter(int& result) override
	{
		auto* chosen = sel.enter();
		if (!chosen)
			return false;
		gfx.loadSettings(chosen->getFsNode());
		return true;
	}
};

struct TcSelectScreen : SelectorScreen
{
	std::shared_ptr<std::unique_ptr<Common>> chosenTc;

	explicit TcSelectScreen(std::shared_ptr<std::unique_ptr<Common>> chosenTc)
	: SelectorScreen(28)
	, chosenTc(chosenTc)
	{
		sel.fill(gfx.getTcNode(), 0);

		sel.setFolder(sel.rootNode);

		auto end = std::remove_if(sel.rootNode.children.begin(), sel.rootNode.children.end(), [](shared_ptr<FileNode> const& n) {
			auto tc = n->getFsNode() / "tc.cfg";
			return !tc.exists();
		});

		sel.rootNode.children.erase(end, sel.rootNode.children.end());

		for (auto& c : sel.rootNode.children)
		{
			c->folder = false;
		}
	}

	void drawTitle() override
	{
		drawFramedTitle("Select TC:");
	}

	bool enter(int& result) override
	{
		auto* chosen = sel.enter();
		if (!chosen)
			return false;

		std::unique_ptr<Common> common(new Common());
		common->load(chosen->getFsNode());
		gfx.settings->tc = chosen->name;
		*chosenTc = std::move(common);
		return true;
	}
};

struct WeaponMenu : Menu
{
	WeaponMenu(int x, int y)
	: Menu(x, y)
	{
	}

	ItemBehavior* getItemBehavior(Common& common, MenuItem& item)
	{
		int index = common.weapOrder[item.id];
		return new ArrayEnumBehavior(common, gfx.settings->weapTable[index], common.texts.weapStates);
	}
};

// Which weapons a game has. Escape closes it while one weapon or more is on.
struct WeaponOptionsScreen : Screen
{
	WeaponMenu weaponMenu;
	bool shown = false;
	bool resumed = false;

	void resume(int) override
	{
		resumed = true;
	}

	WeaponOptionsScreen()
	: weaponMenu(179, 28)
	{
		Common& common = *gfx.common;

		weaponMenu.setHeight(14);
		weaponMenu.valueOffsetX = 89;

		for(int i = 0; i < (int)common.weapons.size(); ++i)
		{
			int index = common.weapOrder[i];
			weaponMenu.addItem(MenuItem(48, 7, common.weapons[index].name, i));
		}

		weaponMenu.moveToFirstVisible();
		weaponMenu.updateItems(common);
	}

	Tick tick() override
	{
		Gfx& g = gfx;
		Common& common = *g.common;

		// Back from the message: it read the events.
		if (shown && !resumed)
		{
			g.process();

			if(g.testSDLKeyOnce(SDL_SCANCODE_ESCAPE))
			{
				int count = 0;

				for(int i = 0; i < 40; ++i)
				{
					if(g.settings->weapTable[i] == 0)
						++count;
				}

				if(count > 0)
					return Tick::close(); // Enough weapons available

				return Tick::push(infoBoxScreen(LS(NoWeaps), 223, 68, false));
			}
		}
		resumed = false;

		g.playRenderer.bmp.copy(g.frozenScreen);

		g.drawBasicMenu();

		drawRoundedBox(g.playRenderer.bmp, 179, 20, 0, 7, common.font.getDims(LS(Weapon)));
		drawRoundedBox(g.playRenderer.bmp, 249, 20, 0, 7, common.font.getDims(LS(Availability)));

		common.font.drawText(g.playRenderer.bmp, LS(Weapon), 181, 21, 50);
		common.font.drawText(g.playRenderer.bmp, LS(Availability), 251, 21, 50);

		weaponMenu.draw(common, g.playRenderer, false);

		if(g.testSDLKeyOnce(SDL_SCANCODE_UP))
		{
			sfx.play(common, 26);
			weaponMenu.movement(-1);
		}

		if(g.testSDLKeyOnce(SDL_SCANCODE_DOWN))
		{
			sfx.play(common, 25);
			weaponMenu.movement(1);
		}

		if(g.testSDLKeyOnce(SDL_SCANCODE_LEFT))
		{
			weaponMenu.onLeftRight(common, -1);
		}
		if(g.testSDLKeyOnce(SDL_SCANCODE_RIGHT))
		{
			weaponMenu.onLeftRight(common, 1);
		}

		if(g.settings->extensions)
		{
			if(g.testSDLKeyOnce(SDL_SCANCODE_PAGEUP))
			{
				sfx.play(common, 26);

				weaponMenu.movementPage(-1);
			}

			if(g.testSDLKeyOnce(SDL_SCANCODE_PAGEDOWN))
			{
				sfx.play(common, 25);

				weaponMenu.movementPage(1);
			}
		}

		weaponMenu.onKeys(g.keyBuf, g.keyBufPtr);

		g.menuFade();
		shown = true;
		return Tick::stay(true);
	}
};

}

std::unique_ptr<Screen> selectLevelScreen()
{
	return std::unique_ptr<Screen>(new LevelSelectScreen);
}

std::unique_ptr<Screen> selectProfileScreen(WormSettings& ws)
{
	return std::unique_ptr<Screen>(new ProfileSelectScreen(ws));
}

std::unique_ptr<Screen> selectReplayScreen()
{
	return std::unique_ptr<Screen>(new ReplaySelectScreen);
}

std::unique_ptr<Screen> selectOptionsScreen()
{
	return std::unique_ptr<Screen>(new OptionsSelectScreen);
}

std::unique_ptr<Screen> selectTcScreen(std::shared_ptr<std::unique_ptr<Common>> chosen)
{
	return std::unique_ptr<Screen>(new TcSelectScreen(chosen));
}

std::unique_ptr<Screen> weaponOptionsScreen()
{
	return std::unique_ptr<Screen>(new WeaponOptionsScreen);
}

static LieroStr str(std::string const& s)
{
	LieroStr r = { reinterpret_cast<uint8_t const*>(s.data()), s.size() };
	return r;
}

// The boxes are drawn in Rust (rust/liero-game/src/menu.rs).

void drawInfoBox(Common& common, Bitmap& bmp, std::string const& text, int x, int y)
{
	LieroBitmap scr = bmp.view();
	LieroFont font = common.font.view();
	liero_menu_draw_info_box(&scr, &font, str(text), x, y);
}

void drawInputBox(Common& common, Bitmap& bmp, Bitmap& frozen, std::string const& text, int x, int y, bool centered)
{
	LieroBitmap scr = bmp.view();
	LieroBitmap from = frozen.view();
	LieroFont font = common.font.view();
	liero_menu_draw_input_box(&scr, &from, &font, str(text), x, y, centered);
}

namespace
{

// A message over the screen, until a key is pressed.
struct InfoBoxScreen : Screen
{
	std::string text;
	int x, y;
	bool clearScreen;
	bool shown = false;
	// The events that came with the box are read.
	bool waiting = false;

	InfoBoxScreen(std::string const& text, int x, int y, bool clearScreen)
	: text(text), x(x), y(y), clearScreen(clearScreen)
	{
	}

	Tick tick() override
	{
		Gfx& g = gfx;
		static int const bgColor = 0;

		if (!shown)
		{
			if(clearScreen)
			{
				g.playRenderer.pal = g.common->exepal;
				fill(g.playRenderer.bmp, bgColor);
			}

			drawInfoBox(*g.common, g.playRenderer.bmp, text, x, y);
			shown = true;
			return Tick::stay(true);
		}

		if (!waiting)
		{
			g.process();
			waiting = true;
		}

		SDL_Event ev;
		while (g.pollEvent(ev))
		{
			g.processEvent(ev);
			if(ev.type == SDL_KEYDOWN)
			{
				g.clearKeys();

				if(clearScreen)
					fill(g.playRenderer.bmp, bgColor);
				return Tick::close();
			}
		}
		return Tick::stay(false);
	}
};

// The key the next key press, joystick button or stick move gives. Unless
// `extended`, keys only the extended controls have are passed over.
struct KeyChoiceScreen : Screen
{
	bool extended;

	explicit KeyChoiceScreen(bool extended)
	: extended(extended)
	{
	}

	Tick tick() override
	{
		Gfx& g = gfx;
		SDL_Event ev;
		while (g.pollEvent(ev))
		{
			g.processEvent(ev);
			uint32_t k;
			if (keyOf(ev, k) && (extended || !isExtendedKey(k)))
				return Tick::close(int(k));
		}
		return Tick::stay(false);
	}

	// False if the event isn't a key.
	static bool keyOf(SDL_Event& ev, uint32_t& k)
	{
		switch (ev.type)
		{
		case SDL_KEYDOWN:
			k = SDLToDOSKey(ev.key.keysym);
			return true;

		case SDL_JOYAXISMOTION:
			k = liero_joy_axis_key(ev.jaxis.which, ev.jaxis.axis, ev.jaxis.value);
			return k != 0;
		case SDL_JOYHATMOTION:
			k = liero_joy_hat_key(ev.jhat.which, ev.jhat.value);
			return k != 0;
		case SDL_JOYBUTTONDOWN:
			k = liero_joy_button_key(ev.jbutton.which, ev.jbutton.button);
			return true;
		default:
			return false;
		}
	}
};

// Typing a line into a box over the frozen screen. It shows a frame for each
// event. Enter closes it with 1 and the text in `*text`, Escape with 0.
struct InputStringScreen : Screen
{
	std::shared_ptr<std::string> text;
	std::string buffer;
	std::size_t maxLen;
	int x, y;
	int (*filter)(int);
	std::string prefix;
	bool centered;
	bool shown = false;

	InputStringScreen(std::shared_ptr<std::string> text, std::size_t maxLen, int x, int y, int (*filter)(int), std::string const& prefix, bool centered)
	: text(text), buffer(*text), maxLen(maxLen), x(x), y(y), filter(filter), prefix(prefix), centered(centered)
	{
	}

	~InputStringScreen()
	{
#ifdef __EMSCRIPTEN__
		// Text input stays off outside the box, so that SDL calls
		// preventDefault on every key the game gets (see main.cpp).
		SDL_StopTextInput();
#endif
	}

	Tick tick() override
	{
		Gfx& g = gfx;

		if (shown)
		{
			SDL_Event ev;
			if (!g.pollEvent(ev))
				return Tick::stay(false);
			g.processEvent(ev);

			switch (ev.type)
			{
				case SDL_KEYDOWN:
					switch (ev.key.keysym.scancode)
					{
						case SDL_SCANCODE_BACKSPACE:
							if(!buffer.empty())
							{
								buffer.erase(buffer.size() - 1);
							}
							break;

						case SDL_SCANCODE_RETURN:
						case SDL_SCANCODE_KP_ENTER:
							*text = buffer;
							sfx.play(*g.common, 27);
							g.clearKeys();
							return Tick::close(1);

						case SDL_SCANCODE_ESCAPE:
							g.clearKeys();
							return Tick::close(0);

						default:
							break;
					}
					break;

				case SDL_TEXTINPUT:
				{
					int k = utf8ToDOS(ev.text.text);
					if (k && buffer.size() < maxLen &&
						(!filter || (k = filter(k))))
					{
						buffer += char(k);
					}
					break;
				}
				case SDL_TEXTEDITING:
					// since there's no support for any characters that can use a
					// complex IME input (like East Asian languages), we naively
					// discard this event
					break;

				default:
					break;
			}
		}

		std::string str = prefix + buffer + '_';
		drawInputBox(*g.common, g.playRenderer.bmp, g.frozenScreen, str, x, y, centered);
		SDL_StartTextInput();
		shown = true;
		return Tick::stay(true);
	}
};

}

std::unique_ptr<Screen> infoBoxScreen(std::string const& text, int x, int y, bool clearScreen)
{
	return std::unique_ptr<Screen>(new InfoBoxScreen(text, x, y, clearScreen));
}

std::unique_ptr<Screen> keyChoiceScreen(bool extended)
{
	return std::unique_ptr<Screen>(new KeyChoiceScreen(extended));
}

std::unique_ptr<Screen> inputStringScreen(std::shared_ptr<std::string> text, std::size_t maxLen, int x, int y, int (*filter)(int), std::string const& prefix, bool centered)
{
	return std::unique_ptr<Screen>(new InputStringScreen(text, maxLen, x, y, filter, prefix, centered));
}

void Gfx::infoBox(std::string const& text, int x, int y, bool clearScreen)
{
	open(infoBoxScreen(text, x, y, clearScreen), [](int) { return -1; });
}

void Gfx::openInputString(std::string const& text, std::size_t maxLen, int x, int y, std::function<int(bool, std::string const&)> then, int (*filter)(int))
{
	std::shared_ptr<std::string> typed(new std::string(text));
	open(inputStringScreen(typed, maxLen, x, y, filter, "", true), [typed, then](int ok)
	{
		return then(ok != 0, *typed);
	});
}

int filterDigits(int k)
{
	return std::isdigit(k) ? k : 0;
}

void Gfx::openInputInteger(int& dest, int min, int max, std::size_t maxLen, int x, int y, std::function<void()> then)
{
	openInputString(toString(dest), maxLen, x, y, [&dest, min, max, then](bool ok, std::string const& str)
	{
		if(ok && !str.empty())
		{
			dest = std::atoi(str.c_str());
			if(dest < min)
				dest = min;
			else if(dest > max)
				dest = max;
		}
		then();
		return -1;
	}, filterDigits);
}

void PlayerMenu::drawItemOverlay(Common& common, MenuItem& item, int x, int y, bool selected, bool disabled)
{
	if(item.id >= PlayerMenu::PlRed && item.id <= PlayerMenu::PlBlue) //Color settings
	{
		int rgbcol = item.id - PlayerMenu::PlRed;
		LieroBitmap scr = gfx.playRenderer.bmp.view();
		liero_menu_draw_rgb_overlay(&scr, x, y, selected, ws->rgb[rgbcol], ws->color);
	} // CED9
}

ItemBehavior* PlayerMenu::getItemBehavior(Common& common, MenuItem& item)
{
	if (item.id >= PlWeap0 && item.id < PlWeap0 + 5)
		return new WeaponEnumBehavior(common, ws->weapons[item.id - PlWeap0]);

	switch(item.id)
	{
		case PlName:
			return new WormNameBehavior(common, *ws);
		case PlHealth:
		{
			auto* b = new IntegerBehavior(common, ws->health, 1, 10000, 1, true);
			b->scrollInterval = 4;
			return b;
		}

		case PlRed:
		case PlGreen:
		case PlBlue:
		{
			auto* b = new IntegerBehavior(common, ws->rgb[item.id - PlRed], 0, 63, 1, false);
			b->scrollInterval = 4;
			return b;
		}

		case PlUp: // D2AB
		case PlDown:
		case PlLeft:
		case PlRight:
		case PlFire:
		case PlChange:
		case PlJump:
			return new KeyBehavior(common, ws->controls[item.id - PlUp], ws->controlsEx[item.id - PlUp], gfx.settings->extensions);

		case PlDig: // Controls Extension
			return new KeyBehavior(common, ws->controlsEx[item.id - PlUp], ws->controlsEx[item.id - PlUp], gfx.settings->extensions);


		case PlController: // Controller
			return new ArrayEnumBehavior(common, ws->controller, common.texts.controllers);

		case PlSaveProfile: // Save profile
			return new ProfileSaveBehavior(common, *ws, false);

		case PlSaveProfileAs: // Save profile as
			return new ProfileSaveBehavior(common, *ws, true);

		case PlLoadProfile:
			return new ProfileLoadBehavior(common, *ws);

		case PlLoadedProfile:
			return new ProfileLoadedBehavior(common, *ws);

		default:
			return Menu::getItemBehavior(common, item);
	}
}

void Gfx::playerSettings(int player)
{
	playerMenu.ws = settings->wormSettings[player];

	playerMenu.updateItems(*common);
	playerMenu.moveToFirstVisible();

	curMenu = &playerMenu;
	return;
}

namespace
{

std::unique_ptr<Screen> menuScreen();

// A game, until the controller stops it. What the controller opens while it
// runs (the stats screen, a message) goes on top once its call returns.
struct GameScreen : Screen
{
	enum { Focus, Process, Draw, Finish } next = Focus;
	bool shown = false;
	bool resumed = false;
	std::function<int(int)> then;

	Tick tick() override
	{
		Gfx& g = gfx;

		// Each tick reads the events that came with the frame before it,
		// unless a screen the controller opened just closed: it read them.
		if (resumed)
			resumed = false;
		else if (shown)
			g.process(g.controller.get());

		if (next == Focus)
		{
			g.controller->focus();
			next = Process;
			if (g.opening)
				return openPending();
		}

		if (next == Process)
		{
			next = g.controller->process() ? Draw : Finish;
			if (g.opening)
				return openPending();
		}

		if (next == Finish)
		{
			g.primaryRenderer = &g.playRenderer;

			g.controller->unfocus();

			g.clearKeys();
			return Tick::close();
		}

		g.playRenderer.clear();
		g.controller->draw(g.playRenderer, false);

		g.singleScreenRenderer.clear();
		g.controller->draw(g.singleScreenRenderer, true);

		++g.menuCycles;

		next = Process;
		shown = true;
		return Tick::stay(true);
	}

	Tick openPending()
	{
		Gfx& g = gfx;
		then = std::move(g.openingThen);
		g.openingThen = nullptr;
		return Tick::push(std::move(g.opening));
	}

	void resume(int result) override
	{
		if (then)
			then(result);
		then = nullptr;
		resumed = true;
	}
};

// The app: a level behind the main menu, then whatever the menu chose.
struct MainScreen : Screen
{
	// What's open on top of it.
	enum { Nothing, InMenu, InGame } open = Nothing;
	int selection = -1;

	Tick tick() override
	{
		if (open == Nothing)
			restart();
		else if (open == InMenu)
		{
			switch (afterMenu())
			{
			case Quit:
				gfx.controller.reset();
				return Tick::close();
			case Play:
				open = InGame;
				return Tick::push(std::unique_ptr<Screen>(new GameScreen));
			default:
				break;
			}
		}

		// The level, or the game left off, behind the menu.
		gfx.playRenderer.clear();
		gfx.controller->draw(gfx.playRenderer, false);

		gfx.singleScreenRenderer.clear();
		gfx.controller->draw(gfx.singleScreenRenderer, true);

		open = InMenu;
		return Tick::push(menuScreen());
	}

	void resume(int result) override
	{
		selection = result;
	}

	void restart()
	{
		Gfx& g = gfx;
		g.controller.reset(new LocalController(g.common, g.settings));

		{
			Level newLevel(*g.common);
			newLevel.generateFromSettings(*g.common, *g.settings, g.rand);
			g.controller->swapLevel(newLevel);
		}

		g.controller->currentGame()->focus(g.playRenderer);
		g.controller->currentGame()->focus(g.singleScreenRenderer);

		// TODO: Unfocus game when necessary
	}

	enum { Quit, Restart, Play };

	// Does what the menu chose: quits, starts over with a new TC, or plays.
	int afterMenu()
	{
		Gfx& g = gfx;
		auto& settings = g.settings;
		auto& controller = g.controller;

		if(selection == MainMenu::MaNewGame)
		{
			std::unique_ptr<Controller> newController(new LocalController(g.common, settings));

			Level* oldLevel = controller->currentLevel();

			if(oldLevel
			&& !settings->regenerateLevel
			&& settings->randomLevel == oldLevel->oldRandomLevel
			&& settings->levelFile == oldLevel->oldLevelFile)
			{
				// Take level and palette from old game
				newController->swapLevel(*oldLevel);
			}
			else
			{
				Level newLevel(*g.common);
				newLevel.generateFromSettings(*g.common, *settings, g.rand);
				newController->swapLevel(newLevel);
			}

			controller = std::move(newController);
		}
		else if(selection == MainMenu::MaResumeGame)
		{
			if (controller->isReplay())
			{
				g.primaryRenderer = &g.singleScreenRenderer;
			}
		}
		else if(selection == MainMenu::MaQuit) // QUIT TO OS
		{
			return Quit;
		}
		else if(selection == MainMenu::MaReplay)
		{
			if (settings->singleScreenReplay)
			{
				g.primaryRenderer = &g.singleScreenRenderer;
			}
		}
		else if (selection == MainMenu::MaTc)
		{
			restart();
			return Restart;
		}

		return Play;
	}
};

}

std::unique_ptr<Screen> mainScreen()
{
	return std::unique_ptr<Screen>(new MainScreen);
}

void Gfx::saveSettings(FsNode node)
{
	settingsNode = node;
	settings->save(node, rand);
}

void Gfx::saveSettingsIfChanged()
{
	std::string text = settings->text();
	if (text == savedSettingsText)
		return;
	savedSettingsText = text;
	try
	{
		auto writer = (configNode / "liero.cfg").toOctetWriter();
		for (char c : text)
			writer.put(uint8_t(c));
	}
	catch (std::runtime_error&)
	{
		// Not saving is no reason to stop the game.
	}
}

bool Gfx::loadSettings(FsNode node)
{
	settingsNode = node;
	settings.reset(new Settings);
	return settings->load(node, rand);
}

bool Gfx::loadSettingsLegacy(FsNode node)
{
	settings.reset(new Settings);
	return settings->loadLegacy(node, rand);
}

void Gfx::drawBasicMenu(/*int curSel*/)
{
	playRenderer.bmp.copy(frozenScreen);

	mainMenu.draw(*common, playRenderer, curMenu != &mainMenu, -1, true);
}

void drawVersusInfo(Common& common, Bitmap& bmp, Settings const& settings, std::string const& levelFileForName, int centerX, int centerY)
{
	std::string level = settings.levelFile.empty()
		? LS(LevelRandom)
		: LS(LevelIs1) + getBasename(getLeaf(levelFileForName)) + LS(LevelIs2);

	LieroBitmap scr = bmp.view();
	LieroFont font = common.font.view();
	WormSettings const& a = *settings.wormSettings[0];
	WormSettings const& b = *settings.wormSettings[1];
	liero_menu_draw_versus(&scr, &font, str(level), str(a.name), str(b.name), a.color, b.color, centerX, centerY);
}

void Gfx::drawSpectatorInfo()
{
	Common& common = *this->common;
	int centerX = singleScreenRenderer.renderResX / 2;
	int centerY = singleScreenRenderer.renderResY / 4;

	singleScreenRenderer.bmp.copy(frozenSpectatorScreen);
	drawVersusInfo(common, singleScreenRenderer.bmp, *settings, gfx.settings->levelFile, centerX, centerY);

	if (controller->running())
	{
		common.font.drawCenteredText(singleScreenRenderer.bmp, "PAUSED", centerX, centerY + 48, 7, 2);
	}
	else
	{
		common.font.drawCenteredText(singleScreenRenderer.bmp, "SETUP", centerX, centerY + 48, 7, 2);
	}
}

int upperCaseOnly(int k)
{
	k = std::toupper(k);

	if((k >= 'A' && k <= 'Z')
	|| (k == 0x8f || k == 0x8e || k == 0x99) // � �and �
	|| (k >= '0' && k <= '9'))
		return k;

	return 0;
}

void Gfx::openHiddenMenu()
{
	if (curMenu == &hiddenMenu)
		return;
	curMenu = &hiddenMenu;
	curMenu->updateItems(*common);
	curMenu->moveToFirstVisible();
}

int Gfx::menuStart()
{
	Common& common = *this->common;
	int centerX = singleScreenRenderer.renderResX / 2;

	fillRect(playRenderer.bmp, 0, 151, 160, 7, 0);
	common.font.drawText(playRenderer.bmp, LS(Copyright2), 2, 152, 19);

	int startItemId;
	if (controller->running())
	{
		mainMenu.setVisibility(MainMenu::MaResumeGame, true);
		mainMenu.itemFromId(MainMenu::MaResumeGame)->string = "RESUME GAME (F1)";
		mainMenu.itemFromId(MainMenu::MaNewGame)->string = "NEW GAME";
		startItemId = MainMenu::MaResumeGame;
	}
	else
	{
		mainMenu.setVisibility(MainMenu::MaResumeGame, false);
		mainMenu.itemFromId(MainMenu::MaNewGame)->string = "NEW GAME (F1)";
		startItemId = MainMenu::MaNewGame;
	}

	mainMenu.moveToFirstVisible();
	settingsMenu.moveToFirstVisible();
	settingsMenu.updateItems(common);

	playRenderer.fadeValue = 0;
	singleScreenRenderer.fadeValue = 0;
	curMenu = &mainMenu;

	frozenScreen.copy(playRenderer.bmp);
	singleScreenRenderer.clear();
	if (controller->currentLevel())
	{
		controller->currentLevel()->drawMiniature(singleScreenRenderer.bmp, centerX - 126, singleScreenRenderer.renderResY - 208, 2);
	}
	frozenSpectatorScreen.copy(singleScreenRenderer.bmp);

	menuCycles = 0;
	return startItemId;
}

int Gfx::menuStep(int startItemId, int& selected, MenuPass& pass, int entered)
{
	Common& common = *this->common;
	bool closeNow = false;

	// What an item's onEnter returned, put to `how`.
	auto use = [&](MenuPass::Use how, int r)
	{
		if (how == MenuPass::Select)
			selected = r;
		else if (how == MenuPass::Tc && r == MainMenu::MaTc)
		{
			// A new TC: no fade-out.
			selected = MainMenu::MaTc;
			closeNow = true;
		}
	};

	// True if the item opened a screen: the pass stops, and goes on when the
	// screen closes.
	auto enter = [&](Menu& menu, MenuPass::Use how)
	{
		int r = menu.onEnter(common);
		if (opening)
		{
			pass.use = how;
			return true;
		}
		use(how, r);
		return false;
	};

	// Stops the pass at `part` while the screen an item opened is up.
	auto stop = [&](MenuPass::Part part)
	{
		pass.part = part;
		return int(MenuOpened);
	};

	if (pass.part != MenuPass::Top)
	{
		use(pass.use, entered);
		if (closeNow)
			return MenuClose;
	}

	if (pass.part == MenuPass::Top)
	{
#ifdef __EMSCRIPTEN__
		saveSettingsIfChanged();
#endif
		drawBasicMenu();
		drawSpectatorInfo();

		if(curMenu == &mainMenu)
			settingsMenu.draw(common, playRenderer, true);
		else
			curMenu->draw(common, playRenderer, false);

		if(testSDLKeyOnce(SDL_SCANCODE_ESCAPE))
		{
			if(curMenu == &mainMenu)
				mainMenu.moveToId(MainMenu::MaQuit);
			else
				curMenu = &mainMenu;
		}

		if(testSDLKeyOnce(SDL_SCANCODE_UP))
		{
			sfx.play(common, 26);
			curMenu->movement(-1);
		}

		if(testSDLKeyOnce(SDL_SCANCODE_DOWN))
		{
			sfx.play(common, 25);
			curMenu->movement(1);
		}

		if(testSDLKeyOnce(SDL_SCANCODE_RETURN)
		|| testSDLKeyOnce(SDL_SCANCODE_KP_ENTER))
		{
			if(curMenu == &mainMenu)
			{
				sfx.play(common, 27);

				int s = mainMenu.selectedId();
				switch (s)
				{
					case MainMenu::MaSettings:
					{
						curMenu = &settingsMenu; // Go into settings menu
						break;
					}

					case MainMenu::MaPlayer1Settings:
					case MainMenu::MaPlayer2Settings:
					{
						playerSettings(s - MainMenu::MaPlayer1Settings);
						break;
					}

					case MainMenu::MaAdvanced:
					{
						openHiddenMenu();
						break;
					}

					case MainMenu::MaReplays:
					{
						if (enter(*curMenu, MenuPass::Select))
							return stop(MenuPass::AfterEnter);
						break;
					}

					case MainMenu::MaTc:
					{
						if (enter(*curMenu, MenuPass::Tc))
							return stop(MenuPass::AfterEnter);
						if (closeNow)
							return MenuClose;
						break;
					}

					default:
					{
						curMenu = &mainMenu;
						selected = s;
					}
				}
			}
			else if(curMenu == &settingsMenu)
			{
				if (enter(settingsMenu, MenuPass::Ignore))
					return stop(MenuPass::AfterEnter);
			}
			else
			{
				if (enter(*curMenu, MenuPass::Select))
					return stop(MenuPass::AfterEnter);
			}
		}
	}

	if (pass.part != MenuPass::AfterF3)
	{
		if(testSDLKeyOnce(SDL_SCANCODE_F1))
		{
			curMenu = &mainMenu;
			mainMenu.moveToId(startItemId);
			selected = startItemId;
		}
		if(testSDLKeyOnce(SDL_SCANCODE_F2))
		{
			mainMenu.moveToId(MainMenu::MaAdvanced);
			openHiddenMenu();
		}
		if(testSDLKeyOnce(SDL_SCANCODE_F3))
		{
			curMenu = &mainMenu;
			mainMenu.moveToId(MainMenu::MaReplays);
			if (enter(*curMenu, MenuPass::Select))
				return stop(MenuPass::AfterF3);
		}
	}

	if (testSDLKeyOnce(SDL_SCANCODE_F5))
	{
		mainMenu.moveToId(MainMenu::MaPlayer1Settings);
		playerSettings(0);
	}
	if (testSDLKeyOnce(SDL_SCANCODE_F6))
	{
		mainMenu.moveToId(MainMenu::MaPlayer2Settings);
		playerSettings(1);
	}
	if (testSDLKeyOnce(SDL_SCANCODE_F7))
	{
		mainMenu.moveToId(MainMenu::MaSettings);
		curMenu = &settingsMenu; // Go into settings menu
	}

#if 1
	if (testSDLKeyOnce(SDL_SCANCODE_F8))
	{
		uint32_t s = 14;

		Rand r;
		r.seed(s);

		Common& common = *this->common;

		vector<std::size_t> nobjMap;

		for (std::size_t i = 0; i < common.nobjectTypes.size(); ++i)
		{
			nobjMap.push_back(i);
		}
		std::random_device rd;
		std::mt19937 g(rd());
		std::shuffle(nobjMap.begin(), nobjMap.end(), g);

		for (auto& w : common.weapons)
		{
			w.addSpeed = r(30) - 5;
			w.affectByExplosions = r(2) == 0;
			w.affectByWorm = r(3) == 0;
			w.ammo = r(20) + 1;
			w.bloodOnHit = r(50);
			w.blowAway = r(10);
			w.bounce = r(90);
			w.collideWithObjects = r(10) == 0;
			w.colorBullets = 3 + r(250);
			w.createOnExp = r(common.sobjectTypes.size());
			w.delay = r(70);
			w.detectDistance = r(20);
			w.dirtEffect = r(9);
			w.distribution = r(5000) - 2500;
			w.explGround = r(2) == 0;
			w.exploSound = r(common.sounds.size());
			w.fireCone = r(10);
			w.gravity = r(2000) - 1000;
			w.hitDamage = r(20);
			w.laserSight = r(5) == 0;
			w.launchSound = r(common.sounds.size());
			w.leaveShellDelay = r(30);
			w.leaveShells = r(1) == 0;
			w.loadingTime = r(70 * 3);
			w.loopAnim = r(10) == 0;
			w.loopSound = false;
			w.multSpeed = r(2) ? 100 : 99 + r(5);
			w.objTrailDelay = 10 + r(70);
			w.objTrailType = r(4) == 0 ? r(common.sobjectTypes.size()) : -1;
			w.parts = r(2) == 0 ? r(10) : 1;
			w.partTrailDelay = 10 + r(70);
			w.partTrailObj = r(4) == 0 ? r(common.nobjectTypes.size()) : -1;
			w.partTrailType = r(2);
			w.playReloadSound = r(2) == 0;
			w.recoil = r(20);
			w.shadow = r(2) == 0;
			w.shotType = r(5);
			w.speed = r(200);
			w.splinterAmount = r(5) == 0 ? r(10) : 0;
			w.splinterColour = r(256);
			w.splinterScatter = r(2);
			w.splinterType = r(common.nobjectTypes.size());
			w.startFrame = r((uint32_t)common.smallSprites.count - 13);
			w.numFrames = r(5);
			w.timeToExplo = 50 + r(200);
			w.timeToExploV = 10 + r(50);
			w.wormCollide = r(3) > 0;
			w.wormExplode = r(3) > 0;
		}

		//for (auto& n : common.nobjectTypes)
		for (std::size_t idx = 0; idx < common.nobjectTypes.size(); ++idx)
		{
			auto& n = common.nobjectTypes[nobjMap[idx]];
			n.affectByExplosions = r(5) == 0;
			n.bloodOnHit = r(5);
			n.bloodTrail = r(10) == 0;
			n.bloodTrailDelay = r(20) + 3;
			n.blowAway = r(10);
			n.bounce = r(90);
			n.colorBullets = 3 + r(250);
			n.createOnExp = r(3) == 0 ? r(common.sobjectTypes.size()) : -1;
			n.detectDistance = r(20);
			n.dirtEffect = r(9);
			n.distribution = r(5000) - 2500;
			n.drawOnMap = r(20) == 0;
			n.explGround = r(4) > 0;
			n.gravity = r(2000) - 1000;
			n.hitDamage = r(10);
			n.leaveObj = r(5) == 0 ? r(common.sobjectTypes.size()) : -1;
			n.leaveObjDelay = 10 + r(80);
			n.startFrame = r((uint32_t)common.smallSprites.count - 13);
			n.numFrames = r(5);
			n.speed = r(150);
			n.splinterAmount = idx > 0 && r(5) == 0 ? r(10) : 0;
			n.splinterColour = r(256);
			n.splinterType = idx > 0 ? nobjMap[r(idx)] : 0;
			n.timeToExplo = 50 + r(70 * 3);
			n.timeToExploV = r(30);
			n.wormDestroy = r(3) == 0;
			n.wormExplode = r(2) == 0;
		}

		for (auto& s : common.sobjectTypes)
		{
			s.animDelay = 1 + r(10);
			s.blowAway = r(2) == 0 ? r(10000) : 0;
			s.damage = r(30);
			s.detectRange = r(20);
			s.dirtEffect = r(9);
			s.flash = r(5);
			s.startFrame = r((uint32_t)common.largeSprites.count - 7);
			s.numFrames = r(7);
			s.startSound = r(common.sounds.size());
			s.shake = r(10);
			s.shadow = r(2);
			s.numSounds = 1;
		}
	}
#endif

	if(testSDLKey(SDL_SCANCODE_LEFT))
	{
		if(!curMenu->onLeftRight(common, -1))
			resetLeftRight();
	}
	if(testSDLKey(SDL_SCANCODE_RIGHT))
	{
		if(!curMenu->onLeftRight(common, 1))
			resetLeftRight();
	}

	if(testSDLKeyOnce(SDL_SCANCODE_PAGEUP))
	{
		sfx.play(common, 26);

		curMenu->movementPage(-1);
	}

	if(testSDLKeyOnce(SDL_SCANCODE_PAGEDOWN))
	{
		sfx.play(common, 25);

		curMenu->movementPage(1);
	}

	pass.part = MenuPass::Top;
	return MenuDone;
}

namespace
{

// The main menu, then its fade-out. It closes with the item chosen.
struct MenuScreen : Screen
{
	enum { Start, Setup, Menu, FadeOut } phase = Start;
	int startItemId = 0;
	int selected = -1;
	MenuPass pass;
	// What the item that opened a screen does when it closes.
	std::function<int(int)> then;
	int entered = -1;
	bool resumed = false;

	Tick tick() override
	{
		Gfx& g = gfx;

		// The pass an item stopped goes on; the screen that closed read the
		// events.
		if (resumed)
		{
			resumed = false;
			return step();
		}

		// Each tick reads the events that came with the frame before it.
		if (phase != Start)
			g.process();

		switch (phase)
		{
		case Start:
			std::memset(g.playRenderer.pal.entries, 0, sizeof(g.playRenderer.pal.entries));
			std::memset(g.singleScreenRenderer.pal.entries, 0, sizeof(g.singleScreenRenderer.pal.entries));
			phase = Setup;
			return Tick::stay(true);

		case Setup:
			startItemId = g.menuStart();
			selected = -1;
			phase = Menu;
			return step();

		case Menu:
			if (selected < 0)
				return step();
			phase = FadeOut;
			g.playRenderer.fadeValue = 32;
			g.menuFade(true);
			return Tick::stay(true);

		case FadeOut:
		default:
			if (--g.playRenderer.fadeValue > 0)
			{
				g.menuFade(true);
				return Tick::stay(true);
			}
			return Tick::close(selected);
		}
	}

	void resume(int result) override
	{
		entered = then(result);
		then = nullptr;
		resumed = true;
	}

	Tick step()
	{
		Gfx& g = gfx;
		switch (g.menuStep(startItemId, selected, pass, entered))
		{
		case Gfx::MenuClose:
			return Tick::close(selected);
		case Gfx::MenuOpened:
			then = std::move(g.openingThen);
			g.openingThen = nullptr;
			return Tick::push(std::move(g.opening));
		default:
			g.menuFade();
			return Tick::stay(true);
		}
	}
};

std::unique_ptr<Screen> menuScreen()
{
	return std::unique_ptr<Screen>(new MenuScreen);
}

}

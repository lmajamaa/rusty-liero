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

#include "gfx.hpp"
#include "screen.hpp"
#include "game.hpp"
#include "sfx.hpp"
#include "text.hpp"
#include "keys.hpp"
#include "filesystem.hpp"

#include "ticks.hpp"


#include "gfx/macros.hpp"


Gfx gfx;

namespace
{
LieroMenusHooks menusHooks();
}

Gfx::Gfx()
: menus(liero_menus_new(menusHooks()))
, mainMenu(liero_menus_box(menus, LIERO_MENU_MAIN), false)
, settingsMenu(liero_menus_box(menus, LIERO_MENU_SETTINGS), false)
, playerMenu(liero_menus_box(menus, LIERO_MENU_PLAYER), false)
, hiddenMenu(liero_menus_box(menus, LIERO_MENU_HIDDEN), false)
, sdlDrawSurface(0)
, running(true)
, doubleRes(true)
, windowW(320 * 2)
, windowH(200 * 2)
, prevMag(0)
, keyBufPtr(keyBuf)
{
	clearKeys();
	liero_app_set_primary_single(app, false);
	liero_app_set_menus(app, menus);
}

namespace
{

LieroStr lstr(std::string const& s)
{
	LieroStr r = { reinterpret_cast<uint8_t const*>(s.data()), s.size() };
	return r;
}

}

std::shared_ptr<Settings> const& Gfx::settings()
{
	LieroSettingsBox* box = liero_app_settings(app);
	if (!box)
		settingsView.reset();
	else if (!settingsView || settingsView->rust() != box)
		settingsView = Settings::of(box);
	return settingsView;
}

void Gfx::setSettings(std::shared_ptr<Settings> s)
{
	liero_app_set_settings(app, s ? s->rust() : nullptr);
	settingsView = std::move(s);
}

std::shared_ptr<Common> const& Gfx::common()
{
	LieroTc* tc = liero_app_tc(app);
	if (!tc)
		commonView.reset();
	else if (!commonView || commonView->tc != tc)
		commonView = Common::of(tc);
	return commonView;
}

void Gfx::setCommon(std::shared_ptr<Common> c)
{
	liero_app_set_tc(app, c ? c->tc : nullptr);
	commonView = std::move(c);
}

void Gfx::setConfigPath(std::string const& path)
{
	liero_app_set_config_path(app, lstr(path));
}

FsNode Gfx::getConfigNode()
{
	return FsNode::adopt(liero_app_config(app));
}

void Gfx::setTcPath(std::string const& path)
{
	liero_app_set_tc_path(app, lstr(path));
}

FsNode Gfx::getTcNode()
{
	return FsNode::adopt(liero_app_tc_root(app));
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
	settings()->spectatorWindow = false;
#endif

	if (sdlSpectatorRenderer)
	{
		SDL_DestroyRenderer(sdlSpectatorRenderer);
		sdlSpectatorRenderer = NULL;
	}
	if (settings()->spectatorWindow)
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

	if (settings()->fullscreen)
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
		if (settings()->fullscreen)
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

		if (settings()->spectatorWindow)
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
	liero_menus_load(menus);
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

void Gfx::setFullscreen(bool newFullscreen, bool updateMenu)
{
	if (newFullscreen == settings()->fullscreen)
		return;
	settings()->fullscreen = newFullscreen;

	// fullscreen will automatically set window size
	if (!settings()->fullscreen)
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
	if (updateMenu)
		liero_menus_update_items(menus, LIERO_MENU_HIDDEN);
}

void Gfx::setDoubleRes(bool newDoubleRes, bool updateMenu)
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
	if (updateMenu)
		liero_menus_update_items(menus, LIERO_MENU_HIDDEN);
}

namespace
{

// Passes joystick buttons that changed to the controller.
void sendKeys(KeySink* controller, LieroKeyChanges const& changes)
{
	if (!controller)
		return;
	for (uint32_t i = 0; i < changes.count; ++i)
		controller->onKey(changes.keys[i], changes.states[i]);
}

} // namespace

void Gfx::processEvent(SDL_Event& ev, KeySink* controller)
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
					setFullscreen(!settings()->fullscreen);
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
	return SDL_GetTicks();
}

uint64_t appHiresTicks()
{
	return hiresTicks();
}

std::time_t appTime()
{
	return std::time(0);
}

bool Gfx::pollEvent(SDL_Event& ev)
{
	return SDL_PollEvent(&ev) != 0;
}

void Gfx::idle()
{
	// In the browser the next step comes with a later frame, by which time the
	// event may have come. Natively runScreens is a loop of its own, so a
	// screen waiting for an event gives it a step's time, as the SDL_WaitEvent
	// it replaces did.
#ifndef __EMSCRIPTEN__
	SDL_Delay(14);
	lastFrame = SDL_GetTicks();
#endif
}

void Gfx::process(KeySink* controller)
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
	case 0: return common()->texts.keyNames[key];
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

void Gfx::draw(SDL_Surface& surface, SDL_Texture& texture, SDL_Renderer& sdlRenderer, Renderer& renderer)
{
	Rect updateRect;
	Color realPal[256];
	renderer.pal.activate(realPal);
	int offsetX, offsetY;
	int mag = fitScreen(surface.w, surface.h,
						renderer.renderResX, renderer.renderResY, offsetX, offsetY);

	Rect newRect(offsetX, offsetY, renderer.renderResX * mag, renderer.renderResY * mag);

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
	// draw into the play window. This uses either the normal split screen renderer
	// or the single screen renderer if this is a replay and single screen replay
	// is turned on
	draw(*sdlDrawSurface, *sdlTexture, *sdlRenderer, primaryRenderer());
	if (settings()->spectatorWindow)
	{
		draw(*sdlSpectatorDrawSurface, *sdlSpectatorTexture, *sdlSpectatorRenderer, singleScreenRenderer);
	}

#ifndef __EMSCRIPTEN__
	// In the browser the frame function (startScreens) runs the steps that
	// are due; natively each frame waits for its time here.
	static unsigned int const delay = 14u;

	uint32_t wantedTime = lastFrame + delay;

	while(true)
	{
		uint32_t now = SDL_GetTicks();
		if(now >= wantedTime)
			break;

		SDL_Delay(wantedTime - now);
	}

	lastFrame = wantedTime;
#endif
}

// The boxes are drawn in Rust (rust/liero-game/src/menu.rs).

void drawInfoBox(Common& common, Bitmap& bmp, std::string const& text, int x, int y)
{
	LieroBitmap scr = bmp.view();
	LieroFont font = common.font.view();
	liero_menu_draw_info_box(&scr, &font, lstr(text), x, y);
}

void drawInputBox(Common& common, Bitmap& bmp, Bitmap& frozen, std::string const& text, int x, int y, bool centered)
{
	LieroBitmap scr = bmp.view();
	LieroBitmap from = frozen.view();
	LieroFont font = common.font.view();
	liero_menu_draw_input_box(&scr, &from, &font, lstr(text), x, y, centered);
}

// The boxes and key choice are screens in Rust (rust/liero-game/src/boxes.rs).

std::unique_ptr<Screen> infoBoxScreen(std::string const& text, int x, int y, bool clearScreen)
{
	return fromFfi(liero_info_box_screen(lstr(text), x, y, clearScreen));
}

namespace
{

std::unique_ptr<Screen> menuScreen();

// C++ called from Rust: an exception waits until the tick returns.
template<typename F>
auto guarded(F f, decltype(f()) fallback) -> decltype(f())
{
	try
	{
		return f();
	}
	catch (...)
	{
		deferException();
		return fallback;
	}
}

// The app's screens and the menu's choices are in Rust
// (rust/liero-game/src/app_screens.rs); these are what they call in C++.

// The frame's keys go to the controller.
struct ControllerKeys : KeySink
{
	void onKey(int key, bool state) override
	{
		if (LieroController* c = liero_app_controller(gfx.app))
			liero_controller_on_key(c, key, state);
	}
};

void appProcess()
{
	guarded([]
	{
		ControllerKeys keys;
		gfx.process(&keys);
		return 0;
	}, 0);
}

LieroScreen appMenuScreen() { return screenToFfi(menuScreen()); }

}

std::unique_ptr<Screen> mainScreen()
{
	LieroAppHooks h;
	h.process = appProcess;
	h.menu_screen = appMenuScreen;
	return fromFfi(liero_main_screen(h));
}

void Gfx::saveSettings(FsNode node)
{
	if (!node.rust() || !liero_app_save_settings(app, node.rust()))
		throw std::runtime_error("Could not write " + node.fullPath());
}

bool Gfx::loadSettings(FsNode node)
{
	return liero_app_load_settings(app, node.rust(), false);
}

bool Gfx::loadSettingsLegacy(FsNode node)
{
	return liero_app_load_settings(app, node.rust(), true);
}

// F8 on the menu: the developers' random TC, in Rust (liero_tc_randomize).
void Gfx::randomizeTc()
{
	if (LieroTc* tc = liero_app_tc(app))
		liero_tc_randomize(tc, std::random_device()());
}

namespace
{

// The menus' hooks (rust/liero-game/src/menus.rs).

uint32_t menusMenuCycles() { return gfx.menuCycles; }
bool* menusDoubleRes() { return &gfx.doubleRes; }
// The options menu shows the change itself.
void menusSetDoubleRes(bool v) { guarded([v] { gfx.setDoubleRes(v, false); return 0; }, 0); }
void menusSetFullscreen(bool v) { guarded([v] { gfx.setFullscreen(v, false); return 0; }, 0); }

LieroMenusHooks menusHooks()
{
	LieroMenusHooks h;
	h.menu_cycles = menusMenuCycles;
	h.double_res = menusDoubleRes;
	h.set_double_res = menusSetDoubleRes;
	h.set_fullscreen = menusSetFullscreen;
	return h;
}

void hookRandomizeTc() { guarded([] { gfx.randomizeTc(); return 0; }, 0); }

std::unique_ptr<Screen> menuScreen()
{
	LieroMenuScreenHooks h;
	h.menus = gfx.menus;
	h.randomize_tc = hookRandomizeTc;
	return fromFfi(liero_menu_screen(h));
}

}

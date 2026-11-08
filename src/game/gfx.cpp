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

#include "controller/replayController.hpp"
#include "controller/localController.hpp"
#include "controller/controller.hpp"

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
	primaryRenderer = &playRenderer;
	liero_app_set_menu_settings(app, []() { return gfx.settings->rust(); });
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
	return gfx.driver ? uint64_t(gfx.driver->ticks()) * 1000000 : hiresTicks();
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

	// In the browser the next step comes with a later frame, by which time the
	// event may have come. Natively runScreens is a loop of its own, so a
	// screen waiting for an event gives it a step's time, as the SDL_WaitEvent
	// it replaces did.
#ifndef __EMSCRIPTEN__
	SDL_Delay(14);
	lastFrame = SDL_GetTicks();
#endif
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

void resetLeftRight()
{
	gfx.releaseSDLKey(SDL_SCANCODE_LEFT);
	gfx.releaseSDLKey(SDL_SCANCODE_RIGHT);
}

using std::string;
using std::vector;
using std::pair;
using std::shared_ptr;

namespace
{

// The TC the TC selector loaded, until the menu switches to it.
std::unique_ptr<Common> chosenTc;

// The file selectors are screens in Rust (rust/liero-game/src/selector.rs).
// What they still need from C++: listing a folder, which reads .zip files as
// folders, and what choosing a file does.
struct SelectorContext
{
	enum Kind { Level, Profile, Replay, Options, Tc };

	Kind kind;
	FsNode root;
	WormSettings* ws = nullptr;

	SelectorContext(Kind kind, FsNode root)
	: kind(kind), root(std::move(root))
	{
	}

	FsNode node(LieroStr rel) const
	{
		FsNode n = root;
		std::string path(reinterpret_cast<char const*>(rel.ptr), rel.len);
		std::size_t start = 0;
		while (start < path.size())
		{
			std::size_t end = path.find('/', start);
			if (end == std::string::npos)
				end = path.size();
			n = n / path.substr(start, end - start);
			start = end + 1;
		}
		return n;
	}
};

std::string toString(LieroStr s)
{
	return std::string(reinterpret_cast<char const*>(s.ptr), s.len);
}

LieroStr lstr(std::string const& s)
{
	LieroStr r = { reinterpret_cast<uint8_t const*>(s.data()), s.size() };
	return r;
}

void selectorList(void* ctx, LieroStr rel, void* list, void (*add)(void*, LieroStr, bool))
{
	try
	{
		DirectoryListing di(static_cast<SelectorContext*>(ctx)->node(rel).iter());
		for (auto const& name : di)
			add(list, lstr(name.name), name.isDir);
	}
	catch (...)
	{
		deferException();
	}
}

bool selectorExists(void* ctx, LieroStr rel)
{
	try
	{
		return static_cast<SelectorContext*>(ctx)->node(rel).exists();
	}
	catch (...)
	{
		deferException();
		return false;
	}
}

// The level under the cursor, in miniature on the menu and the spectator
// screen.
void selectorPreview(void* ctx, LieroStr rel)
{
	Common& common = *gfx.common;
	Level level;

	try
	{
		if (level.load(common, *gfx.settings, ByteReader(static_cast<SelectorContext*>(ctx)->node(rel).read())))
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
	catch (...)
	{
		deferException();
	}
}

int32_t selectorChoose(void* ctx, LieroStr rel, LieroStr fullPath, LieroStr name, bool random)
{
	SelectorContext& c = *static_cast<SelectorContext*>(ctx);
	try
	{
		switch (c.kind)
		{
		case SelectorContext::Level:
			if (random)
			{
				gfx.settings->randomLevel = true;
				gfx.settings->levelFile.clear();
			}
			else
			{
				gfx.settings->randomLevel = false;
				gfx.settings->levelFile = toString(fullPath);
			}
			return -1;

		case SelectorContext::Profile:
			c.ws->loadProfile(c.node(rel));
			return -1;

		case SelectorContext::Replay:
			gfx.prevSelectedReplayPath = toString(fullPath);

			// Reset controller before opening the replay, since we may be recording it
			gfx.controller.reset();

			gfx.controller.reset(new ReplayController(gfx.common, c.node(rel)));
			return MainMenu::MaReplay;

		case SelectorContext::Options:
			gfx.loadSettings(c.node(rel));
			return -1;

		case SelectorContext::Tc:
		{
			std::unique_ptr<Common> common(new Common());
			common->load(c.node(rel));
			gfx.settings->tc = toString(name);
			chosenTc = std::move(common);
			return -1;
		}
		}
	}
	catch (...)
	{
		deferException();
	}
	return -1;
}

void selectorFree(void* ctx)
{
	delete static_cast<SelectorContext*>(ctx);
}

struct SelectorStrings
{
	std::string root, filter, title, randomName, select, fallback;
};

std::unique_ptr<Screen> selectorScreen(SelectorContext* ctx, SelectorStrings const& s, bool hasSelect, bool tcs, bool preview)
{
	LieroSelectorSpec spec;
	spec.root = lstr(s.root);
	spec.filter = lstr(s.filter);
	spec.title = lstr(s.title);
	spec.random_name = lstr(s.randomName);
	spec.select = lstr(s.select);
	spec.has_select = hasSelect;
	spec.fallback = lstr(s.fallback);
	spec.tcs = tcs;
	spec.enter_sound = ctx->kind == SelectorContext::Level;

	LieroSelectorHooks hooks;
	hooks.ctx = ctx;
	hooks.list = selectorList;
	hooks.exists = selectorExists;
	hooks.preview = preview ? selectorPreview : nullptr;
	hooks.choose = selectorChoose;
	hooks.free = selectorFree;

	LieroScreen screen = liero_selector_screen(&spec, hooks);
	// Listing the folders at the start may have thrown.
	rethrowDeferred();
	return fromFfi(screen);
}

std::unique_ptr<Screen> configSelector(SelectorContext::Kind kind, std::string const& filter, std::string const& title,
	std::string const& select, bool hasSelect, std::string const& fallback = "", WormSettings* ws = nullptr)
{
	FsNode root = gfx.getConfigNode();
	SelectorContext* ctx = new SelectorContext(kind, root);
	ctx->ws = ws;
	SelectorStrings s;
	s.root = root.fullPath();
	s.filter = filter;
	s.title = title;
	s.select = select;
	s.fallback = fallback;
	return selectorScreen(ctx, s, hasSelect, false, false);
}

}

std::unique_ptr<Screen> selectLevelScreen()
{
	Common& common = *gfx.common;
	FsNode root = gfx.getConfigNode();
	SelectorStrings s;
	s.root = root.fullPath();
	s.filter = "LEV";
	s.title = LS(SelLevel);
	s.randomName = LS(Random);
	s.select = gfx.settings->levelFile.str();
	return selectorScreen(new SelectorContext(SelectorContext::Level, root), s, true, false, true);
}

std::unique_ptr<Screen> selectProfileScreen(WormSettings& ws)
{
	return configSelector(SelectorContext::Profile, "LPF", "Select profile:",
		joinPath(gfx.getConfigNode().fullPath(), "Profiles"), true, "", &ws);
}

std::unique_ptr<Screen> selectReplayScreen()
{
	return configSelector(SelectorContext::Replay, "LRP", "Select replay:",
		gfx.prevSelectedReplayPath, !gfx.prevSelectedReplayPath.empty(),
		joinPath(gfx.getConfigNode().fullPath(), "Replays"));
}

std::unique_ptr<Screen> selectOptionsScreen()
{
	return configSelector(SelectorContext::Options, "CFG", "Select options:", "", false);
}

std::unique_ptr<Screen> selectTcScreen()
{
	FsNode root = gfx.getTcNode();
	SelectorContext* ctx = new SelectorContext(SelectorContext::Tc, root);
	SelectorStrings s;
	s.root = root.fullPath();
	s.title = "Select TC:";
	return selectorScreen(ctx, s, false, true, false);
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

// The boxes and key choice are screens in Rust (rust/liero-game/src/boxes.rs).

std::unique_ptr<Screen> infoBoxScreen(std::string const& text, int x, int y, bool clearScreen)
{
	return fromFfi(liero_info_box_screen(str(text), x, y, clearScreen));
}

void Gfx::infoBox(std::string const& text, int x, int y, bool clearScreen)
{
	open(infoBoxScreen(text, x, y, clearScreen), [](int) { return -1; });
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

// The app's screens are in Rust (rust/liero-game/src/app_screens.rs); these
// are the controller and the menu's choices they call.

// A new controller with a new level behind the menu.
void restartApp()
{
	Gfx& g = gfx;
	g.controller.reset(new LocalController(g.common, g.settings));

	{
		Level newLevel;
		newLevel.generateFromSettings(*g.common, *g.settings, g.rand);
		g.controller->swapLevel(newLevel);
	}

	g.controller->currentGame()->focus(g.playRenderer);
	g.controller->currentGame()->focus(g.singleScreenRenderer);

	// TODO: Unfocus game when necessary
}

// Does what the menu chose: quits, starts over with a new TC, or plays.
int afterMenu(int selection)
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
			Level newLevel;
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
		return LIERO_AFTER_MENU_QUIT;
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
		restartApp();
		return LIERO_AFTER_MENU_RESTART;
	}

	return LIERO_AFTER_MENU_PLAY;
}

// What the controller does with the result of the screen it opened.
std::function<int(int)> gameThen;

void appProcess() { guarded([] { gfx.process(gfx.controller.get()); return 0; }, 0); }
void appFocus() { guarded([] { gfx.controller->focus(); return 0; }, 0); }
bool appRun() { return guarded([] { return gfx.controller->process(); }, false); }

void appFinish()
{
	guarded([]
	{
		Gfx& g = gfx;
		g.primaryRenderer = &g.playRenderer;
		g.controller->unfocus();
		g.clearKeys();
		return 0;
	}, 0);
}

void appDraw(bool count)
{
	guarded([count]
	{
		Gfx& g = gfx;
		g.playRenderer.clear();
		g.controller->draw(g.playRenderer, false);
		g.singleScreenRenderer.clear();
		g.controller->draw(g.singleScreenRenderer, true);
		if (count)
			++g.menuCycles;
		return 0;
	}, 0);
}

bool appTakeOpened(LieroScreen* out)
{
	if (!gfx.opening)
		return false;
	gameThen = std::move(gfx.openingThen);
	gfx.openingThen = nullptr;
	*out = screenToFfi(std::move(gfx.opening));
	return true;
}

void appThen(int32_t result)
{
	std::function<int(int)> then = std::move(gameThen);
	gameThen = nullptr;
	if (then)
		guarded([&] { return then(result); }, 0);
}

void appRestart() { guarded([] { restartApp(); return 0; }, 0); }
int32_t appAfterMenu(int32_t selection) { return guarded([selection] { return afterMenu(selection); }, LIERO_AFTER_MENU_QUIT); }
void appQuit() { gfx.controller.reset(); }
LieroScreen appMenuScreen() { return screenToFfi(menuScreen()); }

}

std::unique_ptr<Screen> mainScreen()
{
	LieroAppHooks h;
	h.process = appProcess;
	h.focus = appFocus;
	h.run = appRun;
	h.finish = appFinish;
	h.draw = appDraw;
	h.take_opened = appTakeOpened;
	h.then = appThen;
	h.restart = appRestart;
	h.after_menu = appAfterMenu;
	h.quit = appQuit;
	h.menu_screen = appMenuScreen;
	return fromFfi(liero_main_screen(h));
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
		(configNode / "liero.cfg").write(text);
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

	mainMenu.draw(*common, playRenderer, liero_menus_cur(menus) != LIERO_MENU_MAIN, -1, true);
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
	liero_menu_draw_versus(&scr, &font, str(level), a.name.view(), b.name.view(), a.color, b.color, centerX, centerY);
}

void Gfx::drawSpectatorInfo()
{
	Common& common = *this->common;
	int centerX = singleScreenRenderer.renderResX / 2;
	int centerY = singleScreenRenderer.renderResY / 4;

	singleScreenRenderer.bmp.copy(frozenSpectatorScreen);
	drawVersusInfo(common, singleScreenRenderer.bmp, *settings, gfx.settings->levelFile.str(), centerX, centerY);

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
		mainMenu.setItemString(mainMenu.indexFromId(MainMenu::MaResumeGame), "RESUME GAME (F1)");
		mainMenu.setItemString(mainMenu.indexFromId(MainMenu::MaNewGame), "NEW GAME");
		startItemId = MainMenu::MaResumeGame;
	}
	else
	{
		mainMenu.setVisibility(MainMenu::MaResumeGame, false);
		mainMenu.setItemString(mainMenu.indexFromId(MainMenu::MaNewGame), "NEW GAME (F1)");
		startItemId = MainMenu::MaNewGame;
	}

	mainMenu.moveToFirstVisible();
	settingsMenu.moveToFirstVisible();
	liero_menus_update_items(menus, LIERO_MENU_SETTINGS);

	playRenderer.fadeValue = 0;
	singleScreenRenderer.fadeValue = 0;
	liero_menus_set_cur(menus, LIERO_MENU_MAIN);

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

// F8 on the menu: the developers' random TC, in Rust (liero_tc_randomize).
void Gfx::randomizeTc()
{
	if (common->tc)
		liero_tc_randomize(common->tc, std::random_device()());
}

namespace
{

// The menus' hooks (rust/liero-game/src/menus.rs).

LieroSettingsBox* menusSettings() { return gfx.settings->rust(); }
LieroWormSettingsBox* menusWormSettings(int32_t player) { return gfx.settings->wormSettings[player]->rust(); }
LieroTcData const* menusTc() { return liero_tc_data(gfx.common->tc); }
uint32_t menusMenuCycles() { return gfx.menuCycles; }
bool* menusDoubleRes() { return &gfx.doubleRes; }
// The options menu shows the change itself.
void menusSetDoubleRes(bool v) { guarded([v] { gfx.setDoubleRes(v, false); return 0; }, 0); }
void menusSetFullscreen(bool v) { guarded([v] { gfx.setFullscreen(v, false); return 0; }, 0); }

LieroScreen menusOpenSelector(int32_t kind, int32_t player)
{
	return guarded([kind, player]
	{
		switch (kind)
		{
		case LIERO_SELECTOR_LEVEL: return screenToFfi(selectLevelScreen());
		case LIERO_SELECTOR_PROFILE: return screenToFfi(selectProfileScreen(*gfx.settings->wormSettings[player]));
		case LIERO_SELECTOR_REPLAY: return screenToFfi(selectReplayScreen());
		case LIERO_SELECTOR_OPTIONS: return screenToFfi(selectOptionsScreen());
		default: return screenToFfi(selectTcScreen());
		}
	}, LieroScreen{ nullptr, nullptr, nullptr, nullptr });
}

bool menusTakeChosenTc()
{
	if (!chosenTc)
		return false;
	// TODO: mixer may still be using sounds from the old common
	gfx.common.reset(chosenTc.release());
	return true;
}

LieroStr menusSettingsPath() { return lstr(gfx.settingsNode.fullPath()); }

bool menusSaveOptions(LieroStr name)
{
	return guarded([name] { gfx.saveSettings(gfx.getConfigNode() / (toString(name) + ".cfg")); return true; }, false);
}

bool menusSaveProfileAs(int32_t player, LieroStr name)
{
	return guarded([player, name]
	{
		gfx.settings->wormSettings[player]->saveProfile(gfx.getConfigNode() / "Profiles" / (toString(name) + ".lpf"));
		return true;
	}, false);
}

LieroMenusHooks menusHooks()
{
	LieroMenusHooks h;
	h.settings = menusSettings;
	h.worm_settings = menusWormSettings;
	h.tc = menusTc;
	h.menu_cycles = menusMenuCycles;
	h.double_res = menusDoubleRes;
	h.set_double_res = menusSetDoubleRes;
	h.set_fullscreen = menusSetFullscreen;
	h.open_selector = menusOpenSelector;
	h.take_chosen_tc = menusTakeChosenTc;
	h.settings_path = menusSettingsPath;
	h.save_options = menusSaveOptions;
	h.save_profile_as = menusSaveProfileAs;
	return h;
}

void hookClearPalettes()
{
	std::memset(gfx.playRenderer.pal.entries, 0, sizeof(gfx.playRenderer.pal.entries));
	std::memset(gfx.singleScreenRenderer.pal.entries, 0, sizeof(gfx.singleScreenRenderer.pal.entries));
}

int32_t hookStart() { return guarded([] { return gfx.menuStart(); }, 0); }
void hookFadeOut() { gfx.menuFade(true); }
int32_t hookFadeValue() { return gfx.playRenderer.fadeValue; }
void hookSetFadeValue(int32_t v) { gfx.playRenderer.fadeValue = v; }

void hookSaveSettingsIfChanged()
{
#ifdef __EMSCRIPTEN__
	guarded([] { gfx.saveSettingsIfChanged(); return 0; }, 0);
#endif
}

void hookDraw()
{
	guarded([]
	{
		Gfx& g = gfx;
		g.drawBasicMenu();
		g.drawSpectatorInfo();
		int cur = liero_menus_cur(g.menus);
		LieroBitmap scr = g.playRenderer.bmp.view();
		LieroFont font = g.common->font.view();
		if (cur == LIERO_MENU_MAIN)
			liero_menus_draw(g.menus, LIERO_MENU_SETTINGS, &scr, &font, true, -1, false);
		else
			liero_menus_draw(g.menus, cur, &scr, &font, false, -1, false);
		return 0;
	}, 0);
}

void hookRandomizeTc() { guarded([] { gfx.randomizeTc(); return 0; }, 0); }
void hookReleaseLeftRight() { resetLeftRight(); }

std::unique_ptr<Screen> menuScreen()
{
	LieroMenuScreenHooks h;
	h.menus = gfx.menus;
	h.clear_palettes = hookClearPalettes;
	h.start = hookStart;
	h.fade_out = hookFadeOut;
	h.fade_value = hookFadeValue;
	h.set_fade_value = hookSetFadeValue;
	h.save_settings_if_changed = hookSaveSettingsIfChanged;
	h.draw = hookDraw;
	h.randomize_tc = hookRandomizeTc;
	h.release_left_right = hookReleaseLeftRight;
	return fromFfi(liero_menu_screen(h));
}

}

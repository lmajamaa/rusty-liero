#include <SDL.h>
#include <SDL_image.h>
#include <cstring>
#include <random>
#include <string>

#include "gfx.hpp"

#include "screen.hpp"

Gfx gfx;

namespace
{
LieroMenusHooks menusHooks();

std::string str(LieroStr s)
{
	return std::string(reinterpret_cast<char const*>(s.ptr), s.len);
}

// The icon `name` in the config folder.
std::string iconPath(char const* name)
{
	std::string p = str(liero_shell_config_path(gfx.app));
	if (!p.empty() && p.back() != '/' && p.back() != '\\')
		p += '/';
	return p + name;
}
}

Gfx::Gfx()
: menus(liero_menus_new(menusHooks()))
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

void Gfx::init()
{
	SDL_ShowCursor(SDL_DISABLE);
	lastFrame = SDL_GetTicks();

	liero_renderer_set_render_resolution(liero_app_renderer(app, false), 320, 200);
	liero_renderer_set_render_resolution(liero_app_renderer(app, true), 640, 400);
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
	liero_shell_set_spectator_window(app, false);
#endif

	if (sdlSpectatorRenderer)
	{
		SDL_DestroyRenderer(sdlSpectatorRenderer);
		sdlSpectatorRenderer = NULL;
	}
	if (liero_shell_spectator_window(app))
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

	if (liero_shell_fullscreen(app))
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
		std::string s = iconPath("icon.png");
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
		if (liero_shell_fullscreen(app))
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
		std::string s = iconPath("spectator_icon.png");
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

		if (liero_shell_spectator_window(app))
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
	if (newFullscreen == liero_shell_fullscreen(app))
		return;
	liero_shell_set_fullscreen(app, newFullscreen);

	// fullscreen will automatically set window size
	if (!liero_shell_fullscreen(app))
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

// Passes a key to the app's controller, if it has one.
void sendKey(bool toController, uint32_t key, bool state)
{
	if (!toController)
		return;
	if (LieroController* c = liero_app_controller(gfx.app))
		liero_controller_on_key(c, int32_t(key), state);
}

// Passes joystick buttons that changed to the controller.
void sendKeys(bool toController, LieroKeyChanges const& changes)
{
	for (uint32_t i = 0; i < changes.count; ++i)
		sendKey(toController, changes.keys[i], changes.states[i]);
}

} // namespace

void Gfx::processEvent(SDL_Event& ev, bool toController)
{
	switch(ev.type)
	{
		case SDL_KEYDOWN:
		{

			SDL_Scancode s = ev.key.keysym.scancode;

			if (keyBufPtr < keyBuf + 32)
				*keyBufPtr++ = ev.key.keysym;

			Uint32 dosScan = liero_keys_event(&dosKeys, s, true);
			if(dosScan)
				sendKey(toController, dosScan, true);

			if(s == SDL_SCANCODE_F11)
			{
				if (SDL_GetWindowFromID(ev.key.windowID) == sdlWindow)
				{
					setFullscreen(!liero_shell_fullscreen(app));
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
			if(dosScan)
				sendKey(toController, dosScan, false);
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
			sendKeys(toController, liero_joy_axis(joysticks[ev.jaxis.which].btnState, ev.jaxis.which, ev.jaxis.axis, ev.jaxis.value));
		break;

		case SDL_JOYHATMOTION:
			sendKeys(toController, liero_joy_hat(joysticks[ev.jhat.which].btnState, ev.jhat.which, ev.jhat.value));
		break;

		case SDL_JOYBUTTONDOWN:
		case SDL_JOYBUTTONUP: /* Fall-through */
			sendKeys(toController, liero_joy_button(joysticks[ev.jbutton.which].btnState, ev.jbutton.which, ev.jbutton.button, ev.jbutton.state == SDL_PRESSED));
		break;

		default:
			break;
	}
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

void Gfx::process(bool toController)
{
	SDL_Event ev;
	keyBufPtr = keyBuf;
	while(pollEvent(ev))
	{
		processEvent(ev, toController);
	}
}

void Gfx::clearKeys()
{
	liero_keys_clear(&dosKeys);
}

void Gfx::draw(SDL_Surface& surface, SDL_Texture& texture, SDL_Renderer& sdlRenderer, LieroRenderer* renderer)
{
	LieroRendererData const& d = *liero_renderer_data(renderer);
	LieroBitmap const& bmp = *liero_bitmap_box_view(liero_renderer_bitmap(renderer));
	LieroColor realPal[256];
	liero_palette_activate(d.pal, realPal);
	int32_t offsetX, offsetY;
	int mag = liero_gfx_fit_screen(surface.w, surface.h,
						d.render_res_x, d.render_res_y, &offsetX, &offsetY);

	if(mag != prevMag)
	{
		// Clear background if magnification is decreased to
		// avoid leftovers.
		SDL_FillRect(&surface, 0, 0);
	}
	prevMag = mag;

	std::size_t destPitch = surface.pitch;
	uint8_t* dest = reinterpret_cast<uint8_t*>(surface.pixels) + offsetY * destPitch + offsetX * surface.format->BytesPerPixel;

	uint32_t pal32[256];
	for(int i = 0; i < 256; ++i)
		pal32[i] = SDL_MapRGB(surface.format, realPal[i].r, realPal[i].g, realPal[i].b);
	liero_gfx_scale_draw(bmp.pixels, d.render_res_x, d.render_res_y, std::size_t(bmp.pitch), dest, destPitch, mag, pal32);

	SDL_UpdateTexture(&texture, NULL, surface.pixels, surface.w * 4);
	SDL_RenderClear(&sdlRenderer);
	SDL_RenderCopy(&sdlRenderer, &texture, NULL, NULL);
	SDL_RenderPresent(&sdlRenderer);
}

void Gfx::flip()
{
	// draw into the play window. This uses either the normal split screen renderer
	// or the single screen renderer if this is a replay and single screen replay
	// is turned on
	draw(*sdlDrawSurface, *sdlTexture, *sdlRenderer, liero_app_primary(app));
	if (liero_shell_spectator_window(app))
	{
		draw(*sdlSpectatorDrawSurface, *sdlSpectatorTexture, *sdlSpectatorRenderer, liero_app_renderer(app, true));
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

namespace
{

// The app's screens are in Rust (rust/liero-game/src/app_screens.rs); these
// are what they call here.

void appProcess() { gfx.process(true); }

void hookRandomizeTc()
{
	// F8 on the menu: the developers' random TC.
	if (LieroTc* tc = liero_app_tc(gfx.app))
		liero_tc_randomize(tc, std::random_device()());
}

LieroScreen appMenuScreen()
{
	LieroMenuScreenHooks h;
	h.menus = gfx.menus;
	h.randomize_tc = hookRandomizeTc;
	return liero_menu_screen(h);
}

// The menus' hooks (rust/liero-game/src/menus.rs).

uint32_t menusMenuCycles() { return *liero_app_menu_cycles(gfx.app); }
bool* menusDoubleRes() { return &gfx.doubleRes; }
// The options menu shows the change itself.
void menusSetDoubleRes(bool v) { gfx.setDoubleRes(v, false); }
void menusSetFullscreen(bool v) { gfx.setFullscreen(v, false); }

LieroMenusHooks menusHooks()
{
	LieroMenusHooks h;
	h.menu_cycles = menusMenuCycles;
	h.double_res = menusDoubleRes;
	h.set_double_res = menusSetDoubleRes;
	h.set_fullscreen = menusSetFullscreen;
	return h;
}

}

LieroScreen mainScreen()
{
	LieroAppHooks h;
	h.process = appProcess;
	h.menu_screen = appMenuScreen;
	return liero_main_screen(h);
}

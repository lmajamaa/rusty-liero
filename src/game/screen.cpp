#include "screen.hpp"

#include "gfx.hpp"
#include "keys.hpp"
#include "sfx.hpp"
#include "text.hpp"
#include "gfx/blit.hpp"
#include "appclock.hpp"
#include "filesystem.hpp"

#include <cstdio>
#include <cstdlib>
#include <exception>
#include <ctime>

#ifdef __EMSCRIPTEN__
#include <emscripten.h>
#include <SDL.h>
#endif

namespace
{

// An exception can't unwind through Rust, so a tick that throws stores it,
// quits the stack, and runScreens rethrows it.
std::exception_ptr pending;

LieroScreen toFfi(std::unique_ptr<Screen> s);

void ffiTick(void* data, LieroTick* out, LieroScreen* next)
{
	try
	{
		Tick t = static_cast<Screen*>(data)->tick();
		out->kind = t.kind;
		out->shown = t.shown;
		out->result = t.result;
		if (t.screen)
			*next = toFfi(std::move(t.screen));
	}
	catch (...)
	{
		pending = std::current_exception();
		out->kind = TickQuit;
		out->shown = false;
	}
}

void ffiResume(void* data, int32_t result)
{
	try
	{
		static_cast<Screen*>(data)->resume(result);
	}
	catch (...)
	{
		if (!pending)
			pending = std::current_exception();
	}
}

void ffiFree(void* data)
{
	delete static_cast<Screen*>(data);
}

LieroScreen toFfi(std::unique_ptr<Screen> s)
{
	LieroScreen r;
	r.data = s.release();
	r.tick = ffiTick;
	r.resume = ffiResume;
	r.free = ffiFree;
	return r;
}

void hostShow(void*)
{
	gfx.flip();
}

void hostIdle(void*)
{
	gfx.idle();
}

std::exception_ptr deferred;

// A Rust screen inside C++'s Screen, so C++ can open it.
struct ForeignScreen : Screen
{
	LieroScreen s;

	explicit ForeignScreen(LieroScreen s) : s(s) {}

	~ForeignScreen()
	{
		if (s.free)
			s.free(s.data);
	}

	Tick tick() override
	{
		LieroTick out = { TickStay, false, 0 };
		LieroScreen next = { nullptr, nullptr, nullptr, nullptr };
		s.tick(s.data, &out, &next);
		if (deferred)
		{
			if (next.free)
				next.free(next.data);
			rethrowDeferred();
		}
		Tick t;
		t.kind = out.kind;
		t.shown = out.shown;
		t.result = out.result;
		if (next.tick)
			t.screen = fromFfi(next);
		return t;
	}

	void resume(int result) override
	{
		if (s.resume)
			s.resume(s.data, result);
		rethrowDeferred();
	}
};

// What Rust screens call in C++ (LieroScreenHost).

// The DOS key an event gives, if it's one a control can be set to.
bool choiceKeyOf(SDL_Event& ev, uint32_t& k)
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

bool hostPollEvent(LieroEvent* out)
{
	SDL_Event ev;
	if (!gfx.pollEvent(ev))
		return false;
	gfx.processEvent(ev);

	*out = LieroEvent();
	out->key_down = ev.type == SDL_KEYDOWN;
	if (out->key_down)
		out->scancode = ev.key.keysym.scancode;
	uint32_t k = 0;
	out->choice = choiceKeyOf(ev, k);
	out->key = k;
	if (ev.type == SDL_TEXTINPUT)
		out->text = uint8_t(utf8ToDOS(ev.text.text));
	return true;
}

void hostProcess() { gfx.process(); }
void hostClearKeys() { gfx.clearKeys(); }
void hostPlaySound(int32_t id) { sfx.play(*gfx.common, id); }

void hostTextInput(bool on)
{
	if (on)
		SDL_StartTextInput();
#ifdef __EMSCRIPTEN__
	// Text input stays off outside the text box, so that SDL calls
	// preventDefault on every key the game gets (see main.cpp).
	else
		SDL_StopTextInput();
#endif
}

bool hostTestKeyOnce(uint32_t scancode) { return gfx.testSDLKeyOnce(SDL_Scancode(scancode)); }
bool hostTestKey(uint32_t scancode) { return gfx.testSDLKey(SDL_Scancode(scancode)); }

size_t hostTypedKeys(LieroTypedKey* out, size_t cap)
{
	size_t n = 0;
	for (SDL_Keysym* k = gfx.keyBuf; k != gfx.keyBufPtr && n < cap; ++k, ++n)
	{
		out[n].sym = k->sym;
		out[n].scancode = k->scancode;
	}
	return n;
}

uint32_t hostTicks() { return appTicks(); }
int64_t hostTime() { return int64_t(appTime()); }
uint64_t hostHiresTicks() { return appHiresTicks(); }

size_t hostReplayTime(int64_t t, uint8_t* out, size_t cap)
{
	std::time_t ticks = std::time_t(t);
	// The hash run names replays the same in every time zone.
	std::tm* now = gfx.driver ? std::gmtime(&ticks) : std::localtime(&ticks);
	return std::strftime(reinterpret_cast<char*>(out), cap, "%Y-%m-%d %H.%M.%S", now);
}

LieroFsNode const* hostConfigNode() { return gfx.configNode.rust(); }

LieroSoundOut hostSound()
{
	return DefaultSoundPlayer(*gfx.common).out();
}

void setScreenHost()
{
	LieroScreenHost h = {};
	h.poll_event = hostPollEvent;
	h.process = hostProcess;
	h.clear_keys = hostClearKeys;
	h.play_sound = hostPlaySound;
	h.text_input = hostTextInput;
	h.test_key_once = hostTestKeyOnce;
	h.typed_keys = hostTypedKeys;
	h.ticks = hostTicks;
	h.test_key = hostTestKey;
	h.time = hostTime;
	h.hires_ticks = hostHiresTicks;
	h.replay_time = hostReplayTime;
	h.config_node = hostConfigNode;
	h.sound = hostSound;
	h.app_drawing = true;
	liero_screen_set_host(&h);
}

LieroAppHost appHost()
{
	LieroAppHost host;
	host.data = nullptr;
	host.show = hostShow;
	host.idle = hostIdle;
	return host;
}

#ifdef __EMSCRIPTEN__
struct Running
{
	LieroApp* app;
	std::function<void()> done;
};

void browserFrame(void* arg)
{
	Running* r = static_cast<Running*>(arg);
	LieroAppHost host = appHost();
	if (liero_app_frame(r->app, SDL_GetTicks(), &host) && !pending)
		return;

	emscripten_cancel_main_loop();
	liero_app_free(r->app);
	std::function<void()> done = std::move(r->done);
	delete r;

	if (pending)
	{
		// Nothing is below the browser's main loop to catch it.
		try
		{
			std::rethrow_exception(pending);
		}
		catch (std::exception& e)
		{
			std::fprintf(stderr, "The game stopped: %s\n", e.what());
		}
		catch (...)
		{
		}
		std::abort();
	}
	done();
}
#endif

}

void deferException()
{
	if (!deferred)
		deferred = std::current_exception();
}

void rethrowDeferred()
{
	if (deferred)
	{
		std::exception_ptr e = deferred;
		deferred = nullptr;
		std::rethrow_exception(e);
	}
}

LieroScreen screenToFfi(std::unique_ptr<Screen> s)
{
	return toFfi(std::move(s));
}

std::unique_ptr<Screen> fromFfi(LieroScreen s)
{
	return std::unique_ptr<Screen>(new ForeignScreen(s));
}

void runScreens(std::unique_ptr<Screen> first)
{
	setScreenHost();
	LieroApp* app = liero_app_new();
	liero_app_push(app, toFfi(std::move(first)));

	LieroAppHost host = appHost();

	pending = nullptr;
	while (liero_app_step(app, &host))
	{
		// A resume that threw leaves the stack running; stop it here.
		if (pending)
			break;
	}
	liero_app_free(app);

	if (pending)
	{
		std::exception_ptr e = pending;
		pending = nullptr;
		std::rethrow_exception(e);
	}
}

void startScreens(std::unique_ptr<Screen> first, std::function<void()> done)
{
#ifdef __EMSCRIPTEN__
	setScreenHost();
	pending = nullptr;
	Running* r = new Running{ liero_app_new(), std::move(done) };
	liero_app_push(r->app, toFfi(std::move(first)));
	// 0 fps: every animation frame. Not an infinite loop: this returns, and so
	// does main.
	emscripten_set_main_loop_arg(browserFrame, r, 0, false);
#else
	runScreens(std::move(first));
	done();
#endif
}

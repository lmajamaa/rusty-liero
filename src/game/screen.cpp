#include "screen.hpp"

#include "gfx.hpp"
#include "sfx.hpp"
#include "ticks.hpp"

#include <SDL.h>

#include <cstdio>
#include <ctime>

#ifdef __EMSCRIPTEN__
#include <emscripten.h>
#endif

namespace
{

void hostShow(void*)
{
	gfx.flip();
}

void hostIdle(void*)
{
	gfx.idle();
}

// What Rust screens call here (LieroScreenHost).

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

// The character typed, in the DOS code page: ASCII, the Nordic letters, or
// '?'.
uint8_t utf8ToDOS(char const* str)
{
	if (str[1] == 0) {
		return uint8_t(str[0]);
	}
	char const table[][3] =
	{
		{(char)0xc3, (char)0xa5, (char)0x86}, // å
		{(char)0xc3, (char)0xa4, (char)0x84}, // ä
		{(char)0xc3, (char)0xb6, (char)0x94}, // ö
		{(char)0xc3, (char)0x85, (char)0x8f}, // Å
		{(char)0xc3, (char)0x84, (char)0x8e}, // Ä
		{(char)0xc3, (char)0x96, (char)0x99}, // Ö
	};

	for(std::size_t i = 0; i < sizeof(table) / sizeof(*table); ++i)
	{
		if(table[i][0] == str[0] && table[i][1] == str[1])
			return uint8_t(table[i][2]);
	}
	return '?';
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
		out->text = utf8ToDOS(ev.text.text);
	return true;
}

void hostProcess() { gfx.process(); }
void hostClearKeys() { gfx.clearKeys(); }
void hostPlaySound(int32_t id) { liero_shell_play_sound(gfx.app, sfx.out().mixer, id); }

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

bool hostTestKeyOnce(uint32_t scancode) { return liero_keys_test_sdl_once(&gfx.dosKeys, int32_t(scancode)); }
bool hostTestKey(uint32_t scancode) { return liero_keys_test_sdl(&gfx.dosKeys, int32_t(scancode)); }

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

uint32_t hostTicks() { return SDL_GetTicks(); }
int64_t hostTime() { return int64_t(std::time(0)); }
uint64_t hostHiresTicks() { return hiresTicks(); }

size_t hostReplayTime(int64_t t, uint8_t* out, size_t cap)
{
	std::time_t ticks = std::time_t(t);
	std::tm* now = std::localtime(&ticks);
	return std::strftime(reinterpret_cast<char*>(out), cap, "%Y-%m-%d %H.%M.%S", now);
}

LieroSoundOut hostSound()
{
	return sfx.out();
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
	if (liero_app_frame(r->app, SDL_GetTicks(), &host))
		return;

	emscripten_cancel_main_loop();
	liero_app_free(r->app);
	std::function<void()> done = std::move(r->done);
	delete r;
	done();
}
#endif

}

void startScreens(LieroScreen first, std::function<void()> done)
{
	setScreenHost();
	LieroApp* app = liero_app_new();
	liero_app_push(app, first);
#ifdef __EMSCRIPTEN__
	Running* r = new Running{ app, std::move(done) };
	// 0 fps: every animation frame. Not an infinite loop: this returns, and so
	// does main.
	emscripten_set_main_loop_arg(browserFrame, r, 0, false);
#else
	LieroAppHost host = appHost();
	while (liero_app_step(app, &host))
	{
	}
	liero_app_free(app);
	done();
#endif
}

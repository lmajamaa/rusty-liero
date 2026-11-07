#include "screen.hpp"

#include "gfx.hpp"

#include <cstdio>
#include <cstdlib>
#include <exception>

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

void runScreens(std::unique_ptr<Screen> first)
{
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

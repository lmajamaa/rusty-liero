#include "screen.hpp"

#include "gfx.hpp"

#include <exception>

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

struct LegacyMainScreen : Screen
{
	Tick tick() override
	{
		gfx.mainLoop();
		return Tick::close();
	}
};

}

void runScreens(std::unique_ptr<Screen> first)
{
	LieroApp* app = liero_app_new();
	liero_app_push(app, toFfi(std::move(first)));

	LieroAppHost host;
	host.data = nullptr;
	host.show = hostShow;
	host.idle = hostIdle;

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

std::unique_ptr<Screen> legacyMainScreen()
{
	return std::unique_ptr<Screen>(new LegacyMainScreen);
}

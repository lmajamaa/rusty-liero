// The whole app driven by scripted keys, for replacing the main loop.
//
// The menus, file selectors, text boxes, games and stats screen each run
// their own loop that calls Gfx::flip() for every frame they show. This part
// of the hash run starts the app's screens as the game does, with an AppDriver
// (gfx.hpp) in place of the window: each flip prints a hash of the screen
// and its palette (lines "app N hash frame"), and the events come from a
// timeline of key presses and typing fixed before the run. Nothing about it
// depends on how the loops are written, so a version that runs the same
// screens one frame at a time from a single loop must show the same frames.
//
// Time only moves when a frame is shown, or when a screen waits for an event
// (a text box, a message, choosing a key), which skips to that event.
//
// The script counts key presses to reach each menu item, so a change that
// moves the menus (an item added, a different start) shows as different
// frames here; check where the script goes before re-recording.

#include "hashrun.hpp"

#include "gfx.hpp"
#include "screen.hpp"
#include "filesystem.hpp"

#include <SDL.h>
#include <sys/stat.h>

#include <algorithm>
#include <cstdio>
#include <deque>
#include <stdexcept>
#include <string>

namespace
{

struct Step
{
	int at;
	SDL_Event ev;
};

struct ScriptDriver : AppDriver
{
	std::deque<Step> steps;
	// Frames shown, plus the frames skipped while waiting for events.
	int now = 0;
	int shown = 0;
	int limit = 0;

	void frame() override
	{
		Fnv f;
		Renderer& r = *gfx.primaryRenderer;
		for (int y = 0; y < r.bmp.h; ++y)
			f.bytes(&r.bmp.getPixel(0, y), r.bmp.w);
		for (Color const& c : r.pal.entries)
		{
			f.add(uint32_t(c.r));
			f.add(uint32_t(c.g));
			f.add(uint32_t(c.b));
		}
		std::printf("app %d %016llx frame\n", shown, (unsigned long long)f.h);
		++shown;
		++now;
		if (shown > limit)
			throw std::runtime_error("the app ran past the end of its script");
	}

	bool pollEvent(SDL_Event& ev) override
	{
		if (steps.empty() || steps.front().at > now)
			return false;
		ev = steps.front().ev;
		steps.pop_front();
		return true;
	}

	void idle() override
	{
		if (steps.empty())
			throw std::runtime_error("the app waited for an event after its script ended");
		now = std::max(now, steps.front().at);
	}

	uint32_t ticks() override
	{
		return uint32_t(now) * 14;
	}

	std::time_t time() override
	{
		// 2001-09-09, then 70 frames a second.
		return std::time_t(1000000000 + now / 70);
	}
};

// Builds the timeline: `t` is the frame the next event happens on.
struct Script
{
	ScriptDriver& d;
	int t = 0;

	explicit Script(ScriptDriver& d) : d(d) {}

	void key(SDL_Scancode sc, bool down)
	{
		SDL_Event ev = {};
		ev.type = down ? SDL_KEYDOWN : SDL_KEYUP;
		ev.key.state = down ? SDL_PRESSED : SDL_RELEASED;
		ev.key.keysym.scancode = sc;
		// What SDL's default keymap gives, which the menus' typing search reads.
		if (sc >= SDL_SCANCODE_A && sc <= SDL_SCANCODE_Z)
			ev.key.keysym.sym = SDLK_a + (sc - SDL_SCANCODE_A);
		else if (sc >= SDL_SCANCODE_1 && sc <= SDL_SCANCODE_9)
			ev.key.keysym.sym = SDLK_1 + (sc - SDL_SCANCODE_1);
		else if (sc == SDL_SCANCODE_0)
			ev.key.keysym.sym = SDLK_0;
		else
			ev.key.keysym.sym = SDL_SCANCODE_TO_KEYCODE(sc);
		d.steps.push_back(Step{ t, ev });
	}

	// Held for `hold` frames, then `gap` frames before the next event.
	void press(SDL_Scancode sc, int times = 1, int hold = 3, int gap = 5)
	{
		for (int i = 0; i < times; ++i)
		{
			key(sc, true);
			t += hold;
			key(sc, false);
			t += gap;
		}
	}

	void type(char const* text)
	{
		for (char const* p = text; *p; ++p)
		{
			SDL_Event ev = {};
			ev.type = SDL_TEXTINPUT;
			ev.text.text[0] = *p;
			d.steps.push_back(Step{ t, ev });
			t += 4;
		}
	}

	void wait(int frames)
	{
		t += frames;
	}
};

void writeFile(std::string const& path, std::string const& data)
{
	FILE* f = std::fopen(path.c_str(), "wb");
	if (!f)
		throw std::runtime_error("can't write " + path);
	std::fwrite(data.data(), 1, data.size(), f);
	std::fclose(f);
}

void buildScript(Script& s)
{
	// The main menu fades in.
	s.wait(40);

	// Match setup: one life each, then the level and weapon screens, then a
	// saved and reloaded setup.
	s.press(SDL_SCANCODE_F7);
	s.press(SDL_SCANCODE_DOWN);              // LIVES
	s.press(SDL_SCANCODE_RETURN);            // typed in a box
	s.press(SDL_SCANCODE_BACKSPACE, 2);
	s.type("1");
	s.press(SDL_SCANCODE_RETURN);
	s.press(SDL_SCANCODE_DOWN);              // LEVEL
	s.press(SDL_SCANCODE_RETURN);
	s.wait(20);
	s.press(SDL_SCANCODE_DOWN);
	s.press(SDL_SCANCODE_UP);
	s.press(SDL_SCANCODE_ESCAPE);
	s.press(SDL_SCANCODE_DOWN, 2);           // WEAPON OPTIONS
	s.press(SDL_SCANCODE_RETURN);
	s.wait(20);
	s.press(SDL_SCANCODE_DOWN, 3);
	s.press(SDL_SCANCODE_RIGHT);
	s.press(SDL_SCANCODE_ESCAPE);
	s.press(SDL_SCANCODE_DOWN, 7);           // SAVE SETUP AS...
	s.press(SDL_SCANCODE_RETURN);
	s.type("HASHAPP");
	s.press(SDL_SCANCODE_RETURN);
	s.press(SDL_SCANCODE_DOWN);              // LOAD SETUP
	s.press(SDL_SCANCODE_RETURN);
	s.wait(20);
	s.press(SDL_SCANCODE_DOWN);
	s.press(SDL_SCANCODE_RETURN);
	s.press(SDL_SCANCODE_ESCAPE);

	// Bots get random weapons, so the weapon screen is skipped. Games are
	// recorded, which is on by default.
	s.press(SDL_SCANCODE_F2);
	s.press(SDL_SCANCODE_DOWN, 10);          // BOT WEAPONS
	s.press(SDL_SCANCODE_LEFT);
	s.press(SDL_SCANCODE_ESCAPE);

	// The left player: a name, a key, a saved and reloaded profile, and the
	// CPU bot.
	s.press(SDL_SCANCODE_F5);
	s.wait(10);
	s.press(SDL_SCANCODE_DOWN, 2);           // NAME
	s.press(SDL_SCANCODE_RETURN);
	s.press(SDL_SCANCODE_BACKSPACE, 3);
	s.type("BOT A");
	s.press(SDL_SCANCODE_RETURN);
	s.press(SDL_SCANCODE_DOWN);              // HEALTH, low so a game ends soon
	s.press(SDL_SCANCODE_RETURN);
	s.press(SDL_SCANCODE_BACKSPACE, 3);
	s.type("5");
	s.press(SDL_SCANCODE_RETURN);
	s.press(SDL_SCANCODE_DOWN, 4);           // AIM UP
	s.press(SDL_SCANCODE_RETURN);
	s.wait(30);
	s.press(SDL_SCANCODE_W);
	s.press(SDL_SCANCODE_UP, 7);             // SAVE PROFILE AS...
	s.press(SDL_SCANCODE_RETURN);
	s.type("BOTA");
	s.press(SDL_SCANCODE_RETURN);
	s.press(SDL_SCANCODE_DOWN);              // LOAD PROFILE
	s.press(SDL_SCANCODE_RETURN);
	s.wait(20);
	s.press(SDL_SCANCODE_DOWN);
	s.press(SDL_SCANCODE_RETURN);
	s.press(SDL_SCANCODE_UP, 4);             // CONTROLLER, past the profile items
	s.press(SDL_SCANCODE_RIGHT);
	s.press(SDL_SCANCODE_ESCAPE);

	// The right player is a CPU bot too.
	s.press(SDL_SCANCODE_F6);
	s.press(SDL_SCANCODE_UP);
	s.press(SDL_SCANCODE_RIGHT);
	s.press(SDL_SCANCODE_ESCAPE);

	// A game until one bot is out of lives, then the stats screen.
	s.press(SDL_SCANCODE_F1);
	s.wait(3000);
	s.press(SDL_SCANCODE_RIGHT);
	s.wait(30);
	s.press(SDL_SCANCODE_ESCAPE);
	s.wait(60);

	// The replay just recorded, and a broken one, which shows a message.
	s.press(SDL_SCANCODE_F3);
	s.wait(20);
	s.press(SDL_SCANCODE_RETURN);
	s.wait(300);
	s.press(SDL_SCANCODE_ESCAPE);
	s.wait(60);
	s.press(SDL_SCANCODE_F3);
	s.wait(20);
	s.press(SDL_SCANCODE_DOWN);
	s.press(SDL_SCANCODE_RETURN);
	// The message shows 34 frames after the Enter, past the menu's fade-out.
	// A key pressed on its first frame is dropped, and the next closes it.
	s.wait(34 - 8);
	s.press(SDL_SCANCODE_SPACE);
	s.wait(30);
	s.press(SDL_SCANCODE_SPACE);
	s.wait(60);

	// Choosing the TC restarts the main loop.
	s.press(SDL_SCANCODE_DOWN, 3);           // TC
	s.press(SDL_SCANCODE_RETURN);
	s.wait(20);
	s.press(SDL_SCANCODE_RETURN);
	s.wait(60);

	// Quit.
	s.press(SDL_SCANCODE_ESCAPE);
	s.press(SDL_SCANCODE_RETURN);
	s.wait(60);
}

}

void hashApp(gvl::shared_ptr<Common> common)
{
	// A config folder of its own, with a broken replay to fail on.
	std::string const root = "/hashapp";
	mkdir(root.c_str(), 0777);
	mkdir((root + "/Profiles").c_str(), 0777);
	mkdir((root + "/Replays").c_str(), 0777);
	writeFile(root + "/Replays/BROKEN.lrp", "This is not a replay.");

	// Fresh menus, as the game starts with: the menu checks before this one
	// leave items hidden and selected.
	gfx.mainMenu = MainMenu(53, 20);
	gfx.settingsMenu = SettingsMenu(178, 20);
	gfx.playerMenu = PlayerMenu(178, 20);
	gfx.hiddenMenu = HiddenMenu(178, 20);
	gfx.curMenu = nullptr;
	gfx.settings.reset(new Settings);
	gfx.common = common;
	gfx.setConfigPath(root);
	gfx.setTcPath("TC");
	gfx.settingsNode = FsNode(root + "/liero.cfg");
	gfx.rand.seed(77);
	gfx.playRenderer.init(320, 200);
	gfx.singleScreenRenderer.init(640, 400);
	gfx.playRenderer.loadPalette(*common);
	gfx.singleScreenRenderer.loadPalette(*common);
	gfx.primaryRenderer = &gfx.playRenderer;
	gfx.settings->spectatorWindow = false;
	gfx.clearKeys();
	gfx.loadMenus();

	ScriptDriver driver;
	Script script(driver);
	buildScript(script);
	driver.limit = script.t + 2000;

	gfx.driver = &driver;
	try
	{
		runScreens(mainScreen());
		std::printf("app-end %d %016llx\n", driver.shown, (unsigned long long)driver.steps.size());
	}
	catch (std::exception& e)
	{
		std::fprintf(stderr, "app: %s after %d frames\n", e.what(), driver.shown);
		std::printf("app-error %d %016llx\n", driver.shown, (unsigned long long)driver.steps.size());
	}
	gfx.driver = nullptr;
}

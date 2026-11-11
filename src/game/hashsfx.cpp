// Sound and input part of `--hash-run` (see hashrun.cpp).
//
// Sound: the 16-bit samples made from each TC sound, a mixer driven by random
// adds, stops, volume changes and mixes of random lengths (sounds starting in
// the past and the future, more sounds than channels, looping, repeated and
// null ids, volumes past the mask, a full-scale square wave for clipping and
// sounds one and two samples long), and four bot games whose sounds are mixed
// 630 frames per tick, as at 44.1 kHz and 70 ticks a second.
//
// Input: the SDL scancode to DOS key mapping for every scancode, SDL key and
// joystick events through Gfx::processEvent into the key state and a
// controller, the key tests the menus use, key names, the local controller
// turning keys into worm controls and the frame skip keys.

#include "hashrun.hpp"

#include "common.hpp"
#include "game.hpp"
#include "gfx.hpp"
#include "keys.hpp"
#include "worm.hpp"
#include "mixer/player.hpp"

#include <SDL.h>
#include <cstdio>
#include <cstring>

namespace
{

void print(char const* name, int step, uint64_t h)
{
	std::printf("%s %d %016llx\n", name, step, (unsigned long long)h);
}

void hashSamples(Common& common)
{
	for (std::size_t i = 0; i < common.sounds.size(); ++i)
	{
		SfxSample const& s = common.sounds[i];
		Fnv f;
		f.add(s.valid);
		if (s.valid)
		{
			std::size_t n = s.sample_count;
			f.add(uint32_t(n));
			for (std::size_t j = 0; j < n; ++j)
				f.add(int32_t(s.samples[j]));
		}
		print("sfx-sample", int(i), f.h);
	}
}

void hashBuffer(Fnv& f, int16_t const* buf, int n)
{
	f.add(uint32_t(n));
	for (int i = 0; i < n; ++i)
		f.add(int32_t(buf[i]));
}

void mixerScript(Common& common, uint32_t seed, int steps)
{
	std::vector<TcList<int16_t>> sounds;
	for (SfxSample const& s : common.sounds)
		if (s.valid)
			sounds.push_back(TcList<int16_t>(s.samples, s.sample_count));

	// A full-scale square wave, which clips as soon as two play at once.
	std::vector<int16_t> square(300);
	for (int i = 0; i < 300; ++i)
		square[i] = (i / 25) % 2 ? 32767 : -32768;
	sounds.push_back(TcList<int16_t>(square.data(), square.size()));
	std::vector<int16_t> one { -32768 };
	sounds.push_back(TcList<int16_t>(one.data(), one.size()));
	std::vector<int16_t> two { 12345, -23456 };
	sounds.push_back(TcList<int16_t>(two.data(), two.size()));

	// Ids are only compared, never followed.
	void* ids[] = { 0, (void*)16, (void*)32, (void*)48, (void*)64, (void*)80 };
	int const idCount = sizeof(ids) / sizeof(*ids);

	char name[32];
	std::snprintf(name, sizeof(name), "sfx-mixer-%u", seed);

	LieroMixer* m = liero_mixer_new();
	Rand r(seed);
	static int16_t buf[5000];
	Fnv f;
	for (int step = 1; step <= steps; ++step)
	{
		void* id = ids[r(idCount)];
		uint32_t op = r(10);
		if (op < 4)
		{
			TcList<int16_t> const& snd = sounds[r(uint32_t(sounds.size()))];
			uint32_t time = uint32_t(liero_mixer_now(m) + int32_t(r(6000)) - 3000);
			uint32_t flags = r(4) == 0 ? LIERO_SOUND_LOOP : LIERO_SOUND_NORMAL;
			void* ret = liero_mixer_add(m, snd.items, snd.size(), time, id, flags);
			f.add(ret == id);
			f.add(ret != 0);
		}
		else if (op == 4)
			liero_mixer_stop(m, id);
		else if (op == 5)
			f.add(liero_mixer_is_playing(m, id));
		else if (op == 6)
			liero_mixer_set_volume(m, id, r(2600) / 1000.0);
		else
		{
			int n = 1 + int(r(r(3) == 0 ? 5000 : 700));
			liero_mixer_mix(m, buf, n);
			hashBuffer(f, buf, n);
			f.add(liero_mixer_now(m));
		}

		if (step % 100 == 0)
			print(name, step, f.h);
	}

	liero_mixer_free(m);
}

void soundGame(std::shared_ptr<Common> common, char const* name, uint32_t seed, int gameMode, char const* const* weapons, int ticks)
{
	std::unique_ptr<Game> gamePtr = setUpGame(common, seed, gameMode, weapons);
	Game& game = *gamePtr;
	LieroMixer* m = liero_mixer_new();
	game.setSoundPlayer(std::make_shared<RecordSoundPlayer>(*common, m));

	game.startGame();

	std::string gameName = std::string(name) + "-game";
	static int16_t buf[630];
	Fnv f;
	for (int tick = 1; tick <= ticks; ++tick)
	{
		botTick(game);
		game.processFrame();
		liero_mixer_mix(m, buf, 630);
		hashBuffer(f, buf, 630);

		bool over = game.isGameOver();
		if (over || tick % 100 == 0)
		{
			print(name, tick, f.h);
			// Sounds don't change the game.
			print(gameName.c_str(), tick, hashGame(game));
		}
		if (over)
			break;
	}

	game.setSoundPlayer(std::make_shared<NullSoundPlayer>());
	liero_mixer_free(m);
}

char const* const throwers[] = { "FLAMER", "FAN", "DART", "DIRTBALL", "GREENBALL" };
char const* const loud[] = { "BIG NUKE", "MINIGUN", "CHAINGUN", "LASER", "HELLRAIDER" };

void hashSound(std::shared_ptr<Common> common)
{
	hashSamples(*common);
	for (uint32_t seed = 1; seed <= 4; ++seed)
		mixerScript(*common, seed, 2000);

	soundGame(common, "sfx-kill-1", 1, Settings::GMKillEmAll, 0, 3000);
	soundGame(common, "sfx-throwers-13", 13, Settings::GMKillEmAll, throwers, 3000);
	soundGame(common, "sfx-loud-21", 21, Settings::GMKillEmAll, loud, 3000);
	soundGame(common, "sfx-zone-6", 6, Settings::GMHoldazone, 0, 3000);
}

void hashKeyMap()
{
	// Every scancode SDL has, in blocks, then some it doesn't.
	for (int block = 0; block < SDL_NUM_SCANCODES / 64; ++block)
	{
		Fnv f;
		for (int sc = block * 64; sc < block * 64 + 64; ++sc)
		{
			f.add(SDLToDOSKey(SDL_Scancode(sc)));
			SDL_Keysym ks;
			std::memset(&ks, 0, sizeof(ks));
			ks.scancode = SDL_Scancode(sc);
			f.add(SDLToDOSKey(ks));
		}
		print("keys-map", block, f.h);
	}

	Fnv f;
	int const others[] = { -1, -1000, 512, 513, 1000, 0x7fffffff };
	for (int sc : others)
		f.add(SDLToDOSKey(SDL_Scancode(sc)));
	print("keys-map", SDL_NUM_SCANCODES / 64, f.h);
}

struct KeyRecorder : KeySink
{
	Fnv f;

	void onKey(int key, bool state)
	{
		f.add(key);
		f.add(state);
	}
};

void hashKeyState(Fnv& f)
{
	for (bool k : gfx.dosKeys.down)
		f.add(k);
	for (Joystick& js : gfx.joysticks)
		for (bool b : js.btnState)
			f.add(b);
	f.add(uint32_t(gfx.keyBufPtr - gfx.keyBuf));
}

SDL_Scancode randomScancode(Rand& r)
{
	SDL_Scancode sc;
	do
	{
		uint32_t kind = r(4);
		if (kind == 0)
			sc = SDL_Scancode(r(SDL_NUM_SCANCODES));
		else if (kind == 1)
			sc = SDL_Scancode(SDL_SCANCODE_RIGHT + r(8)); // arrows and the right-hand modifiers
		else
			sc = SDL_Scancode(r(100));
	}
	// F11 toggles fullscreen, which needs a window.
	while (sc == SDL_SCANCODE_F11);
	return sc;
}

int16_t randomAxis(Rand& r)
{
	int16_t const values[] = { -32768, -10001, -10000, -1, 0, 1, 10000, 10001, 32767 };
	if (r(3) == 0)
		return int16_t(int32_t(r(65536)) - 32768);
	return values[r(9)];
}

void hashEvents(uint32_t seed, int steps)
{
	gfx.clearKeys();
	gfx.joysticks.clear();
	gfx.joysticks.resize(2);
	for (Joystick& js : gfx.joysticks)
	{
		js.sdlGameController = 0;
		js.clearState();
	}
	gfx.keyBufPtr = gfx.keyBuf;

	char name[32];
	std::snprintf(name, sizeof(name), "keys-events-%u", seed);

	KeyRecorder rec;
	Rand r(seed);
	Fnv f;
	for (int step = 1; step <= steps; ++step)
	{
		SDL_Event ev;
		std::memset(&ev, 0, sizeof(ev));
		uint32_t op = r(12);
		bool event = true;
		if (op < 4)
		{
			ev.type = r(2) ? SDL_KEYDOWN : SDL_KEYUP;
			ev.key.state = ev.type == SDL_KEYDOWN ? SDL_PRESSED : SDL_RELEASED;
			ev.key.keysym.scancode = randomScancode(r);
		}
		else if (op == 4)
		{
			ev.type = SDL_JOYAXISMOTION;
			ev.jaxis.which = r(2);
			ev.jaxis.axis = r(6);
			ev.jaxis.value = randomAxis(r);
		}
		else if (op == 5)
		{
			ev.type = SDL_JOYHATMOTION;
			ev.jhat.which = r(2);
			ev.jhat.value = r(16);
		}
		else if (op == 6)
		{
			ev.type = r(2) ? SDL_JOYBUTTONDOWN : SDL_JOYBUTTONUP;
			ev.jbutton.which = r(2);
			ev.jbutton.button = r(16);
			ev.jbutton.state = r(2) ? SDL_PRESSED : SDL_RELEASED;
		}
		else
		{
			event = false;
			uint32_t k = r(177);
			SDL_Scancode sc = randomScancode(r);
			switch (op)
			{
			case 7: f.add(gfx.testKeyOnce(k)); break;
			case 8: f.add(gfx.testKey(k)); gfx.toggleKey(k); break;
			case 9: f.add(gfx.testSDLKeyOnce(sc)); break;
			case 10: f.add(gfx.testSDLKey(sc)); gfx.releaseSDLKey(sc); break;
			default: gfx.setKey(k, r(2) != 0); break;
			}
		}

		if (event)
			gfx.processEvent(ev, r(4) ? &rec : 0);

		hashKeyState(f);
		f.add(uint32_t(rec.f.h));
		f.add(uint32_t(rec.f.h >> 32));

		if (step % 40 == 0)
		{
			// The key buffer holds 32 presses until Gfx::process empties it.
			for (SDL_Keysym* k = gfx.keyBuf; k != gfx.keyBufPtr; ++k)
				f.add(int32_t(k->scancode));
			gfx.keyBufPtr = gfx.keyBuf;
		}
		if (step % 100 == 0)
			print(name, step, f.h);
	}

	gfx.clearKeys();
	gfx.joysticks.clear();
	gfx.keyBufPtr = gfx.keyBuf;
}

void hashKeyNames()
{
	Fnv f;
	for (uint32_t k = 0; k < 1200; ++k)
	{
		std::string n = gfx.getKeyName(k);
		f.add(uint32_t(n.size()));
		f.bytes(reinterpret_cast<uint8_t const*>(n.data()), n.size());
		f.add(isExtendedKey(k));
	}
	for (int j = 0; j < 4; ++j)
		for (int b = 0; b < MaxJoyButtons; ++b)
			f.add(joyButtonToExKey(j, b));
	print("keys-names", 0, f.h);
}

void hashControls(std::shared_ptr<Common> common, uint32_t seed, int steps)
{
	std::shared_ptr<Settings> settings(new Settings);
	Rand r(seed);
	// A small pool of keys, so the two worms and their controls share some.
	uint32_t pool[12];
	for (uint32_t& k : pool)
		k = r(4) == 0 ? joyButtonToExKey(r(2), r(32)) : 2 + r(100);
	for (int i = 0; i < 2; ++i)
	{
		WormSettings& ws = *settings->wormSettings[i];
		ws.controller = 0;
		for (uint32_t& c : ws.controls)
			c = pool[r(12)];
		for (uint32_t& c : ws.controlsEx)
			c = pool[r(12)];
	}

	std::unique_ptr<LieroController, void (*)(LieroController*)> lc(
		liero_local_controller_new(settings->rust(), common->tc), liero_controller_free);
	LieroControllerData& d = *liero_controller_data(lc.get());
	LieroGame* game = liero_controller_game(lc.get());
	auto worm = [game](std::size_t i) { return liero_worm_data(liero_game_worm(game, i)); };

	char name[32];
	std::snprintf(name, sizeof(name), "keys-controls-%u", seed);

	Fnv f;
	for (int step = 1; step <= steps; ++step)
	{
		uint32_t op = r(10);
		if (op < 7)
		{
			uint32_t key = r(8) == 0 ? DkEscape : r(8) == 0 ? r(177) : pool[r(12)];
			liero_controller_on_key(lc.get(), int(key), r(2) != 0);
		}
		else if (op == 7)
			d.going_to_menu = false;
		else if (op == 8)
		{
			// The frame skip keys, maybe with ctrl.
			SDL_Scancode const keys[] = {
				SDL_SCANCODE_0, SDL_SCANCODE_1, SDL_SCANCODE_2, SDL_SCANCODE_5, SDL_SCANCODE_9,
				SDL_SCANCODE_LCTRL, SDL_SCANCODE_RCTRL, SDL_SCANCODE_A };
			for (int i = 0; i < 3; ++i)
				gfx.setKey(SDLToDOSKey(keys[r(8)]), r(2) != 0);
			liero_controller_process(&d, &gfx.dosKeys);
		}
		else
		{
			LieroWormData* w = worm(r(2));
			w->control_states = r(128);
		}

		for (std::size_t i = 0; i < liero_game_worm_count(game); ++i)
		{
			f.add(worm(i)->control_states);
			f.add(worm(i)->clean_control_states);
		}
		f.add(d.fade_value);
		f.add(d.going_to_menu);
		f.add(d.frame_skip);
		f.add(d.inverse_frame_skip);
		f.add(d.cycles);
		for (bool k : gfx.dosKeys.down)
			f.add(k);

		if (step % 100 == 0)
			print(name, step, f.h);
	}

	gfx.clearKeys();
}

void hashInput(std::shared_ptr<Common> common)
{
	gfx.setCommon(common);
	hashKeyMap();
	for (uint32_t seed = 1; seed <= 3; ++seed)
		hashEvents(seed, 2000);
	hashKeyNames();
	for (uint32_t seed = 1; seed <= 3; ++seed)
		hashControls(common, seed, 1000);
}

} // namespace

void hashSfxAndInput(std::shared_ptr<Common> common)
{
	hashSound(common);
	hashInput(common);
}

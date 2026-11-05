// Headless determinism check for the Rust port.
//
// `openliero --hash-run [ticks] [interval]` plays bot-vs-bot games from fixed
// seeds on the bundled TC, without a window or sound, and prints a hash of the
// whole game state every `interval` ticks, then draws the split-screen and
// spectator views off-screen and prints a hash of their pixels and palettes
// (the lines ending in "frame"). tools/hashrun/run.ts runs it with
// Bun and compares the output with tools/hashrun/expected.txt, which was made
// from the original C++ game logic. A port that changes any behaviour changes
// a hash.

#include "hashrun.hpp"

#include "game.hpp"
#include "worm.hpp"
#include "weapsel.hpp"
#include "viewport.hpp"
#include "spectatorviewport.hpp"
#include "filesystem.hpp"
#include "gfx.hpp"
#include "math.hpp"
#include "mixer/player.hpp"
#include "gfx/renderer.hpp"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>

namespace
{


// Draws the game as the split-screen and spectator windows would show it,
// and hashes the pixels and palettes.
uint64_t hashFrame(Game& game, Renderer& split, Renderer& spectator)
{
	Fnv f;
	Renderer* renderers[] = { &split, &spectator };
	for (int i = 0; i < 2; ++i)
	{
		Renderer& r = *renderers[i];
		r.clear();
		game.draw(r, StateGame, i == 1);
		for (int y = 0; y < r.bmp.h; ++y)
			f.bytes(&r.bmp.getPixel(0, y), r.bmp.w);
		for (Color const& c : r.pal.entries)
		{
			f.add(uint32_t(c.r));
			f.add(uint32_t(c.g));
			f.add(uint32_t(c.b));
		}
	}
	return f.h;
}

int wormIndex(Worm const* w)
{
	return w ? w->index : -1;
}

void hashWorm(Fnv& f, Worm& w)
{
	f.add(w.pos); f.add(w.vel);
	f.add(w.logicRespawn.x); f.add(w.logicRespawn.y);
	f.add(w.hotspotX); f.add(w.hotspotY);
	f.add(w.aimingAngle); f.add(w.aimingSpeed);
	f.add(w.ableToJump); f.add(w.ableToDig); f.add(w.keyChangePressed);
	f.add(w.movable); f.add(w.animate); f.add(w.visible); f.add(w.ready);
	f.add(w.flag); f.add(w.makeSightGreen);
	f.add(w.health); f.add(w.lives); f.add(w.kills); f.add(w.timer);
	f.add(w.killedTimer); f.add(w.currentFrame); f.add(w.flags);

	Ninjarope& r = w.ninjarope;
	f.add(r.out); f.add(r.attached); f.add(wormIndex(r.anchor));
	f.add(r.pos); f.add(r.vel); f.add(r.length); f.add(r.curLen);

	f.add(w.currentWeapon); f.add(w.lastKilledByIdx);
	f.add(w.fireCone); f.add(w.leaveShellTimer);
	for (int i = 0; i < 4; ++i)
		f.add(w.reacts[i]);

	for (int i = 0; i < Settings::selectableWeapons; ++i)
	{
		WormWeapon& ww = w.weapons[i];
		f.add(ww.ammo); f.add(ww.delayLeft); f.add(ww.loadingLeft);
		f.add(ww.type ? ww.type->id : -1);
	}

	f.add(uint32_t(w.direction));
	f.add(w.controlStates.pack());
	f.add(w.prevControlStates.pack());
}

uint64_t hashGame(Game& game)
{
	Fnv f;

	f.add(game.cycles); f.add(game.rand.x); f.add(game.rand.c);
	f.add(game.screenFlash); f.add(game.lastKilledIdx); f.add(game.gotChanged);

	Holdazone& z = game.holdazone;
	f.add(z.rect.x1); f.add(z.rect.y1); f.add(z.rect.x2); f.add(z.rect.y2);
	f.add(z.holderIdx); f.add(z.contenderIdx); f.add(z.contenderFrames);
	f.add(z.timeoutLeft); f.add(z.zoneWidth); f.add(z.zoneHeight);

	for (Worm* w : game.worms)
		hashWorm(f, *w);

	// Each list in processing order, since the order changes the outcome.
	f.add(uint32_t(game.bonuses.size()));
	auto br = game.bonuses.all();
	for (Bonus* b; (b = br.next()); )
	{
		f.add(b->x); f.add(b->y); f.add(b->velY);
		f.add(b->frame); f.add(b->timer); f.add(b->weapon);
	}

	f.add(uint32_t(game.sobjects.size()));
	auto sr = game.sobjects.all();
	for (SObject* s; (s = sr.next()); )
	{
		f.add(s->x); f.add(s->y); f.add(s->id);
		f.add(s->curFrame); f.add(s->animDelay);
	}

	f.add(uint32_t(game.wobjects.size()));
	auto wr = game.wobjects.all();
	for (WObject* o; (o = wr.next()); )
	{
		f.add(o->pos); f.add(o->vel); f.add(o->type->id);
		f.add(o->ownerIdx); f.add(o->curFrame); f.add(o->timeLeft);
	}

	f.add(uint32_t(game.nobjects.size()));
	auto nr = game.nobjects.all();
	for (NObject* o; (o = nr.next()); )
	{
		f.add(o->pos); f.add(o->vel); f.add(o->type->id);
		f.add(o->ownerIdx); f.add(o->curFrame); f.add(o->timeLeft);
	}

	f.add(uint32_t(game.bobjects.size()));
	for (auto i = game.bobjects.begin(); i != game.bobjects.end(); ++i)
	{
		f.add(i->pos); f.add(i->vel); f.add(i->color);
	}

	f.bytes(game.level.data.data(), game.level.data.size());

	return f.h;
}

struct RunConfig
{
	char const* name;
	uint32_t seed;
	int gameMode;
	// Weapons both worms get, by name, or null for random ones.
	char const* const* weapons;
};

// Fixed loadouts that together cover all 40 weapons of the bundled TC.
char const* const explosives[] = { "GRENADE", "MINE", "CLUSTER BOMB", "BIG NUKE", "BOUNCY MINE" };
char const* const bombs[] = { "GRASSHOPPER", "CHIQUITA BOMB", "NAPALM", "MINI NUKE", "FLOAT MINE" };
char const* const guns[] = { "MINIGUN", "UZI", "HANDGUN", "RIFLE", "WINCHESTER" };
char const* const shotguns[] = { "SHOTGUN", "SUPER SHOTGUN", "CHAINGUN", "GAUSS GUN", "BLASTER" };
char const* const rockets[] = { "BAZOOKA", "MISSILE", "MINI ROCKETS", "DOOMSDAY", "HELLRAIDER" };
char const* const beams[] = { "LASER", "LARPA", "BOUNCY LARPA", "CRACKLER", "ZIMM" };
char const* const throwers[] = { "FLAMER", "FAN", "DART", "DIRTBALL", "GREENBALL" };
char const* const traps[] = { "BOOBY TRAP", "EXPLOSIVES", "SPIKEBALLS", "CANNON", "RB RAMPAGE" };

// Random weapons in each game mode, then each fixed loadout.
RunConfig const configs[] = {
	{ "kill-1", 1, Settings::GMKillEmAll },
	{ "kill-2", 2, Settings::GMKillEmAll },
	{ "kill-3", 3, Settings::GMKillEmAll },
	{ "kill-4", 4, Settings::GMKillEmAll },
	{ "tag-5", 5, Settings::GMGameOfTag },
	{ "zone-6", 6, Settings::GMHoldazone },
	{ "explosives-7", 7, Settings::GMKillEmAll, explosives },
	{ "bombs-8", 8, Settings::GMKillEmAll, bombs },
	{ "guns-9", 9, Settings::GMKillEmAll, guns },
	{ "shotguns-10", 10, Settings::GMKillEmAll, shotguns },
	{ "rockets-11", 11, Settings::GMKillEmAll, rockets },
	{ "beams-12", 12, Settings::GMKillEmAll, beams },
	{ "throwers-13", 13, Settings::GMKillEmAll, throwers },
	{ "traps-14", 14, Settings::GMKillEmAll, traps },
	{ "soj-15", 15, Settings::GMScalesOfJustice },
	{ "soj-guns-16", 16, Settings::GMScalesOfJustice, guns },
	{ "zone-shotguns-17", 17, Settings::GMHoldazone, shotguns },
};

// Sets a worm's weapons to the menu positions of the named weapons.
void setWeapons(Common& common, WormSettings& ws, char const* const* names)
{
	for (int j = 0; j < Settings::selectableWeapons; ++j)
	{
		ws.weapons[j] = 0;
		for (std::size_t k = 0; k < common.weapOrder.size(); ++k)
		{
			if (common.weapons[common.weapOrder[k]].name == names[j])
				ws.weapons[j] = int(k) + 1;
		}
		if (ws.weapons[j] == 0)
		{
			std::fprintf(stderr, "No weapon called %s in the TC\n", names[j]);
			std::exit(1);
		}
	}
}

void runGame(gvl::shared_ptr<Common> common, RunConfig const& config, int ticks, int interval)
{
	gvl::shared_ptr<Settings> settings(new Settings);
	settings->gameMode = config.gameMode;
	for (int i = 0; i < 2; ++i)
	{
		settings->wormSettings[i]->controller = 1; // DumbLieroAI
		if (config.weapons)
			setWeapons(*common, *settings->wormSettings[i], config.weapons);
	}
	// 0 gives bots random weapons; 2 keeps the ones set above. (The
	// default, 1, would keep the menu's default weapons: all bazookas.)
	settings->selectBotWeapons = config.weapons ? 2 : 0;

	// Everything random comes from these seeds. Weapon selection and the
	// level use gfx.rand, as in the menus.
	gfx.rand.seed(config.seed);

	// Game holds its object lists by value, too big for the wasm stack.
	std::unique_ptr<Game> gamePtr(new Game(common, settings, gvl::shared_ptr<SoundPlayer>(new NullSoundPlayer)));
	Game& game = *gamePtr;
	game.rand.seed(config.seed * 7919);

	// The same setup as LocalController.
	for (int i = 0; i < 2; ++i)
	{
		Worm* worm = new Worm();
		worm->settings = settings->wormSettings[i];
		worm->health = worm->settings->health;
		worm->index = i;
		worm->statsX = i == 0 ? 0 : 218;
		// Worm's constructor leaves these to whatever was in memory. The game
		// sets them before using them, but the hash would read them first.
		for (int& r : worm->reacts)
			r = 0;
		worm->ninjarope.length = 0;
		worm->ninjarope.curLen = 0;
		DumbLieroAI* ai = new DumbLieroAI();
		ai->rand.seed(config.seed * 104729 + i);
		worm->ai.reset(ai);
		game.addWorm(worm);
	}

	game.addViewport(new Viewport(gvl::rect(0, 0, 158, 158), 0, 504, 350));
	game.addViewport(new Viewport(gvl::rect(160, 0, 158+160, 158), 1, 504, 350));
	game.addSpectatorViewport(new SpectatorViewport(gvl::rect(0, 0, 504 + 68, 350), 504, 350));

	{
		Level newLevel(*common);
		newLevel.generateFromSettings(*common, *settings, gfx.rand);
		game.level.swap(newLevel);
	}

	{
		WeaponSelection ws(game);
		ws.finalize();
	}

	for (Worm* w : game.worms)
		w->lives = settings->lives;

	game.startGame();

	// Drawing doesn't change the game, so the game hashes don't depend on it.
	Renderer split, spectator;
	split.init(320, 200);
	spectator.init(640, 400);
	for (Renderer* r : { &split, &spectator })
	{
		r->loadPalette(*common);
		game.focus(*r);
	}

	std::printf("%s 0 %016llx\n", config.name, (unsigned long long)hashGame(game));
	std::printf("%s 0 %016llx frame\n", config.name, (unsigned long long)hashFrame(game, split, spectator));

	for (int tick = 1; tick <= ticks; ++tick)
	{
		// The same order as LocalController::process.
		int phase = game.cycles % 2;
		for (std::size_t i = 0; i < game.worms.size(); ++i)
		{
			Worm& worm = *game.worms[(i + phase) % game.worms.size()];
			worm.ai->process(game, worm);
		}

		game.processFrame();

		bool over = game.isGameOver();
		if (over || tick % interval == 0)
		{
			std::printf("%s %d %016llx%s\n", config.name, tick, (unsigned long long)hashGame(game), over ? " end" : "");
			std::printf("%s %d %016llx frame\n", config.name, tick, (unsigned long long)hashFrame(game, split, spectator));
		}
		if (over)
			break;
	}

	std::printf("%s summary: %d ticks, kills %d-%d, lives %d-%d\n", config.name, game.cycles,
		game.worms[0]->kills, game.worms[1]->kills, game.worms[0]->lives, game.worms[1]->lives);
}

} // namespace

int hashRun(int argc, char* argv[])
{
	int ticks = argc > 2 ? std::atoi(argv[2]) : 30000;
	int interval = argc > 3 ? std::atoi(argv[3]) : 100;

	precomputeTables();

	gvl::shared_ptr<Common> common(new Common());
	common->load(FsNode(std::string("TC/openliero")));
	hashTc(*common);

	for (RunConfig const& config : configs)
		runGame(common, config, ticks, interval);

	hashMenus(common);

	return 0;
}

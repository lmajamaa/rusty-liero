// Games between the predictive bots (rust/liero-game/src/ai/predictive.rs), which
// the main hash games don't use: they play the original bot, which is much
// cheaper. Each predictive tick simulates hundreds of frames on copies of the
// game, so these games are short and most turn the AI's search down.
//
// Besides the game state, they hash the AI's own state: its candidate
// plans and their scores, the learnt transition model, the path-finding
// cells and the positions it traced (liero_worm_ai_hash, which hashes
// them in the order the C++ did).

#include "hashrun.hpp"

#include "game.hpp"
#include "worm.hpp"
#include "viewport.hpp"
#include "gfx/renderer.hpp"

#include <cstdio>

uint64_t hashFrame(Game& game, Renderer& split, Renderer& spectator);

namespace
{

struct AiConfig
{
	char const* name;
	uint32_t seed;
	int gameMode;
	char const* const* weapons;
	int aiFrames, aiMutations, aiParallels;
	bool aiTraces;
	int ticks;
};

char const* const rockets[] = { "BAZOOKA", "MISSILE", "MINI ROCKETS", "DOOMSDAY", "HELLRAIDER" };
char const* const mixed[] = { "MISSILE", "SHOTGUN", "LASER", "GRENADE", "NAPALM" };

AiConfig const aiConfigs[] = {
	{ "ai-kill-1", 101, Settings::GMKillEmAll, 0, 30, 1, 3, false, 1500 },
	{ "ai-tag-2", 102, Settings::GMGameOfTag, 0, 30, 1, 3, false, 1500 },
	{ "ai-zone-3", 103, Settings::GMHoldazone, 0, 30, 1, 3, false, 1500 },
	{ "ai-rockets-4", 104, Settings::GMKillEmAll, rockets, 30, 1, 2, false, 1500 },
	{ "ai-mixed-5", 105, Settings::GMKillEmAll, mixed, 20, 0, 4, true, 1000 },
	// The menu's AI settings.
	{ "ai-default-6", 106, Settings::GMKillEmAll, mixed, 70 * 2, 2, 3, true, 120 },
};

void runAiGame(std::shared_ptr<Common> common, AiConfig const& config, int interval)
{
	std::unique_ptr<Game> gamePtr = setUpGame(common, config.seed, config.gameMode, config.weapons);
	Game& game = *gamePtr;
	Settings& settings = *game.settings;
	settings.aiFrames = config.aiFrames;
	settings.aiMutations = config.aiMutations;
	settings.aiParallels = config.aiParallels;
	settings.aiTraces = config.aiTraces;

	// As createAi makes them for controller 2.
	for (int i = 0; i < 2; ++i)
	{
		Worm& worm = *game.worms[i];
		worm.settings->controller = 2;
		worm.setAi(2, settings.aiParallels, worm.index == 0);
		liero_rand_seed(liero_worm_ai_rand(worm.ai()), config.seed * 31 + i);
	}

	game.startGame();

	Renderer split, spectator;
	split.init(320, 200);
	spectator.init(640, 400);
	for (Renderer* r : { &split, &spectator })
	{
		r->loadPalette(*common);
		game.focus(*r);
	}

	// The controls every tick, which the game hashes only sample.
	Fnv controls;

	for (int tick = 1; tick <= config.ticks; ++tick)
	{
		botTick(game);
		for (Worm* w : game.worms)
			controls.add(w->controlStates.pack());
		game.processFrame();

		bool over = game.isGameOver();
		if (over || tick % interval == 0)
		{
			std::printf("%s %d %016llx%s\n", config.name, tick, (unsigned long long)hashGame(game), over ? " end" : "");
			std::printf("%s-controls %d %016llx\n", config.name, tick, (unsigned long long)controls.h);
			for (int i = 0; i < 2; ++i)
				std::printf("%s-ai%d %d %016llx\n", config.name, i, tick, (unsigned long long)liero_worm_ai_hash(game.worms[i]->ai()));
			std::printf("%s %d %016llx frame\n", config.name, tick, (unsigned long long)hashFrame(game, split, spectator));
		}
		if (over)
			break;
	}

	std::printf("%s summary: %d ticks, kills %d-%d, lives %d-%d\n", config.name, game.cycles,
		game.worms[0]->kills, game.worms[1]->kills, game.worms[0]->lives, game.worms[1]->lives);
}

} // namespace

void hashAiGames(std::shared_ptr<Common> common)
{
	for (AiConfig const& config : aiConfigs)
		runAiGame(common, config, config.ticks <= 200 ? 10 : 50);
}

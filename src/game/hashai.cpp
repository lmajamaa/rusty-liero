// Games between the predictive bots (FollowAI, ai/predictive_ai.cpp), which
// the main hash games don't use: they play DumbLieroAI, which is much
// cheaper. Each FollowAI tick simulates hundreds of frames on copies of the
// game, so these games are short and most turn the AI's search down.
//
// Besides the game state, they hash the AI's own state: its candidate
// plans and their scores, the learnt transition model, the path-finding
// cells and the positions it traced.

#include "hashrun.hpp"

#include "game.hpp"
#include "worm.hpp"
#include "viewport.hpp"
#include "spectatorviewport.hpp"
#include "gfx/renderer.hpp"
#include "ai/predictive_ai.hpp"

#include <cstdio>
#include <cstring>

uint64_t hashFrame(Game& game, Renderer& split, Renderer& spectator);

namespace
{

void addDouble(Fnv& f, double d)
{
	uint64_t bits;
	std::memcpy(&bits, &d, sizeof bits);
	f.add(uint32_t(bits));
	f.add(uint32_t(bits >> 32));
}

void hashResult(Fnv& f, EvaluateResult const& r)
{
	f.add(uint32_t(r.scoreOverTime.size()));
	for (double d : r.scoreOverTime)
		addDouble(f, d);
	addDouble(f, r.futureScore);
}

uint64_t hashAi(FollowAI& ai)
{
	Fnv f;
	f.add(ai.frame);
	f.add(ai.rand.x); f.add(ai.rand.c);
	InputContext& c = ai.currentContext;
	f.add(c.currentState.idx); f.add(c.wantedWeapon); f.add(c.hiddenFrames);
	f.add(c.facingEnemy); f.add(c.ninjaropeOut);
	f.add(ai.evaluationBudget);
	f.add(ai.testing);

	f.add(uint32_t(ai.candPlan.size()));
	for (CandPlan& p : ai.candPlan)
	{
		f.add(uint32_t(p.plan.size()));
		for (InputState s : p.plan)
			f.add(s.idx);
		hashResult(f, p.prevResult);
		f.add(p.prevResultAge);
	}
	f.add(ai.best ? int(ai.best - &ai.candPlan[0]) : -1);

	for (auto& row : ai.model.trans)
		for (double d : row)
			addDouble(f, d);

	// Not the presence and damage grid (AiContext::state): only code under
	// #if 0 reads it, and it starts as whatever was in memory.

	// The cells' g is only set once a search reaches them.
	dijkstra_level& d = ai.dlevel;
	for (level_cell& cell : d.cells)
	{
		f.add(cell.cost);
		f.add(cell.state);
		if (cell.state != path_node::none)
		{
			f.add(cell.g);
			f.add(cell.parent ? int(static_cast<level_cell*>(cell.parent) - d.cells) : -1);
		}
	}

	f.add(uint32_t(ai.evaluatePositions.size()));
	for (auto& p : ai.evaluatePositions)
	{
		f.add(std::get<0>(p).x);
		f.add(std::get<0>(p).y);
		f.add(uint32_t(std::get<1>(p)));
	}
	return f.h;
}

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

void runAiGame(gvl::shared_ptr<Common> common, AiConfig const& config, int interval)
{
	std::unique_ptr<Game> gamePtr = setUpGame(common, config.seed, config.gameMode, config.weapons);
	Game& game = *gamePtr;
	Settings& settings = *game.settings;
	settings.aiFrames = config.aiFrames;
	settings.aiMutations = config.aiMutations;
	settings.aiParallels = config.aiParallels;
	settings.aiTraces = config.aiTraces;

	// As createAi makes them for controller 2.
	FollowAI* ais[2];
	for (int i = 0; i < 2; ++i)
	{
		Worm& worm = *game.worms[i];
		worm.settings->controller = 2;
		ais[i] = new FollowAI(Weights(), settings.aiParallels, worm.index == 0);
		ais[i]->rand.seed(config.seed * 31 + i);
		worm.ai.reset(ais[i]);
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
				std::printf("%s-ai%d %d %016llx\n", config.name, i, tick, (unsigned long long)hashAi(*ais[i]));
			std::printf("%s %d %016llx frame\n", config.name, tick, (unsigned long long)hashFrame(game, split, spectator));
		}
		if (over)
			break;
	}

	std::printf("%s summary: %d ticks, kills %d-%d, lives %d-%d\n", config.name, game.cycles,
		game.worms[0]->kills, game.worms[1]->kills, game.worms[0]->lives, game.worms[1]->lives);
}

} // namespace

void hashAiGames(gvl::shared_ptr<Common> common)
{
	for (AiConfig const& config : aiConfigs)
		runAiGame(common, config, config.ticks <= 200 ? 10 : 50);
}

#include "predictive_ai.hpp"

#include "game/game.hpp"
#include "game/gfx/renderer.hpp"

#include <algorithm>

namespace
{

Game& gameOf(void* game)
{
	return *static_cast<Game*>(game);
}

void* cloneGame(void* game)
{
	Game& org = gameOf(game);
	// On the heap, as the search keeps it after this returns.
	Game* copy = new Game(org);
	copy->postClone(org);
	copy->quickSim = true;
	return copy;
}

void freeGame(void* game)
{
	Game* copy = static_cast<Game*>(game);
	// postClone gave the copy its own worms, which ~Game doesn't delete.
	for (Worm* w : copy->worms)
		delete w;
	delete copy;
}

void processFrame(void* game)
{
	gameOf(game).processFrame();
}

void gameInfo(void* game, LieroAiGame* out)
{
	Game& g = gameOf(game);
	Settings& s = *g.settings;
	out->game_mode = s.gameMode;
	out->holder_idx = g.holdazone.holderIdx;
	out->contender_idx = g.holdazone.contenderIdx;
	out->contender_frames = g.holdazone.contenderFrames;
	out->zone_center[0] = g.holdazone.rect.center_x();
	out->zone_center[1] = g.holdazone.rect.center_y();
	out->last_killed_idx = g.lastKilledIdx;
	out->ai_frames = s.aiFrames;
	out->ai_mutations = s.aiMutations;
	out->ai_traces = s.aiTraces;
}

void wormInfo(void* game, int32_t place, LieroAiWorm* out)
{
	Worm& w = *gameOf(game).worms[place];
	out->index = w.index;
	out->pos[0] = w.pos.x;
	out->pos[1] = w.pos.y;
	out->aiming_angle = w.aimingAngle;
	out->direction = w.direction;
	out->visible = w.visible;
	out->ready = w.ready;
	out->health = w.health;
	out->lives = w.lives;
	out->settings_health = w.settings->health;
	out->current_weapon = w.currentWeapon;
	for (int i = 0; i < 5; ++i)
	{
		WormWeapon const& ww = w.weapons[i];
		LieroAiWeapon& o = out->weapons[i];
		o.ammo = ww.ammo;
		o.delay_left = ww.delayLeft;
		o.loading_left = ww.loadingLeft;
		o.loading_time = ww.type->loading_time;
		o.max_ammo = ww.type->ammo;
	}
	out->rope_out = w.ninjarope.out;
	out->steerable_count = w.steerableCount;
	out->steerable_sum[0] = w.steerableSumX;
	out->steerable_sum[1] = w.steerableSumY;
	out->control_states = w.controlStates.pack();
}

void setControls(void* game, int32_t place, uint32_t controls)
{
	gameOf(game).worms[place]->controlStates.istate = controls;
}

void levelInfo(void* game, LieroAiLevel* out)
{
	Game& g = gameOf(game);
	out->data = g.level.data.data();
	out->len = g.level.data.size();
	out->width = g.level.width;
	out->height = g.level.height;
	out->materials = g.common->levelCommon().materials;
}

LieroAiHooks const hooks = {
	cloneGame, freeGame, processFrame, gameInfo, wormInfo, setControls, levelInfo
};

}

void FollowAI::process(Game& game, Worm& worm)
{
	auto place = std::find(game.worms.begin(), game.worms.end(), &worm) - game.worms.begin();
	worm.controlStates.istate = liero_follow_ai_process(ai, &game, int32_t(place), &hooks);
}

void FollowAI::drawDebug(Game& game, Worm const& worm, Renderer& renderer, int offsX, int offsY)
{
	std::size_t count;
	LieroAiTrace const* traces = liero_follow_ai_traces(ai, &count);
	for (std::size_t i = 0; i < count; ++i)
		renderer.bmp.setPixel(ftoi(traces[i].x) + offsX, ftoi(traces[i].y) + offsY, traces[i].color);
}

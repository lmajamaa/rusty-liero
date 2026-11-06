#include "stats_recorder.hpp"

#include "common.hpp"
#include <gvl/system/system.hpp>
#include "game.hpp"
#include "appclock.hpp"
#include "text.hpp"

#include <cassert>
#include <vector>

void StatsRecorder::damagePotential(Worm* byWorm, WormWeapon* weapon, int hp)
{
}

void StatsRecorder::damageDealt(Worm* byWorm, WormWeapon* weapon, Worm* toWorm, int hp, bool hasHit)
{
}

void StatsRecorder::shot(Worm* byWorm, WormWeapon* weapon)
{
}

void StatsRecorder::hit(Worm* byWorm, WormWeapon* weapon, Worm* toWorm)
{
}

void StatsRecorder::afterSpawn(Worm* worm)
{
}

void StatsRecorder::afterDeath(Worm* worm)
{
}

void StatsRecorder::finish(Game& game)
{

}

void StatsRecorder::preTick(Game& game)
{
}

void StatsRecorder::tick(Game& game)
{

}

void StatsRecorder::aiProcessTime(Worm* worm, uint64_t time)
{
}

/*
void StatsRecorder::write(Common& common, gvl::stream_ptr sink)
{
}*/

namespace
{

int wormIndex(Worm const* worm)
{
	return worm ? worm->index : -1;
}

int weaponIndex(WormWeapon const* weapon)
{
	return weapon ? weapon->type->id : -1;
}

LieroStatsWorm statsWorm(Worm const& w)
{
	LieroStatsWorm v;
	v.index = w.index;
	v.x = ftoi(w.pos.x);
	v.y = ftoi(w.pos.y);
	v.visible = w.visible;
	v.health = w.health;
	v.lives = w.lives;
	v.timer = w.timer;
	v.kills = w.kills;
	v.max_health = w.settings->health;
	v.fire = w.controlStates[Worm::Control::Fire];
	v.change = w.controlStates[Worm::Control::Change];
	v.left = w.controlStates[Worm::Control::Left];
	v.right = w.controlStates[Worm::Control::Right];
	v.current_weapon = w.currentWeapon;
	for (int i = 0; i < 5; ++i)
		v.loading_left[i] = w.weapons[i].loadingLeft;
	return v;
}

std::vector<LieroStatsWorm> statsWorms(Game& game)
{
	std::vector<LieroStatsWorm> v;
	for (Worm* w : game.worms)
		v.push_back(statsWorm(*w));
	return v;
}

} // namespace

void NormalStatsRecorder::damagePotential(Worm* byWorm, WormWeapon* weapon, int hp)
{
	liero_stats_damage_potential(s, wormIndex(byWorm), weaponIndex(weapon), hp);
}

void NormalStatsRecorder::damageDealt(Worm* byWorm, WormWeapon* weapon, Worm* toWorm, int hp, bool hasHit)
{
	assert(toWorm);
	liero_stats_damage_dealt(s, wormIndex(byWorm), weaponIndex(weapon), toWorm->index,
		ftoi(toWorm->pos.x), ftoi(toWorm->pos.y), hp, hasHit);
}

void NormalStatsRecorder::shot(Worm* byWorm, WormWeapon* weapon)
{
	liero_stats_shot(s, wormIndex(byWorm), weaponIndex(weapon));
}

void NormalStatsRecorder::hit(Worm* byWorm, WormWeapon* weapon, Worm* toWorm)
{
	assert(toWorm);
	liero_stats_hit(s, wormIndex(byWorm), weaponIndex(weapon), toWorm->index);
}

void NormalStatsRecorder::afterSpawn(Worm* worm)
{
	liero_stats_after_spawn(s, worm->index);
}

void NormalStatsRecorder::afterDeath(Worm* worm)
{
	liero_stats_after_death(s, worm->index);
}

void NormalStatsRecorder::preTick(Game& game)
{
	uint64_t ticks = appHiresTicks();
	std::vector<LieroStatsWorm> worms = statsWorms(game);
	liero_stats_pre_tick(s, ticks, worms.data(), worms.size());
}

void NormalStatsRecorder::tick(Game& game)
{
	uint64_t ticks = appHiresTicks();
	std::vector<LieroStatsWorm> worms = statsWorms(game);
	liero_stats_tick(s, ticks, worms.data(), worms.size());
}

void NormalStatsRecorder::finish(Game& game)
{
	std::vector<LieroStatsWorm> worms = statsWorms(game);
	liero_stats_finish(s, worms.data(), worms.size());
}

void NormalStatsRecorder::aiProcessTime(Worm* worm, uint64_t time)
{
	liero_stats_ai_process_time(s, worm->index, time);
}

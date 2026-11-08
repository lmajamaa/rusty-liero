#ifndef STATS_RECORDER_HPP
#define STATS_RECORDER_HPP

#include "worm.hpp"
#include <liero_game.h>

struct Common;
struct Renderer;

struct StatsRecorder
{
	virtual void damagePotential(Worm* byWorm, WormWeapon* weapon, int hp);
	virtual void damageDealt(Worm* byWorm, WormWeapon* weapon, Worm* toWorm, int hp, bool hasHit);

	virtual void shot(Worm* byWorm, WormWeapon* weapon);
	virtual void hit(Worm* byWorm, WormWeapon* weapon, Worm* toWorm);

	virtual void afterSpawn(Worm* worm);
	virtual void afterDeath(Worm* worm);
	virtual void preTick(Game& game);
	virtual void tick(Game& game);
	virtual void finish(Game& game);

	virtual void aiProcessTime(Worm* worm, uint64_t time);

};

// Records stats in Rust (rust/liero-game/src/stats.rs), which owns them.
struct NormalStatsRecorder : StatsRecorder
{
	NormalStatsRecorder()
	: s(liero_stats_new())
	{
	}

	NormalStatsRecorder(NormalStatsRecorder const& other)
	: StatsRecorder(other)
	, s(liero_stats_clone(other.s))
	{
	}

	NormalStatsRecorder& operator=(NormalStatsRecorder const&) = delete;

	~NormalStatsRecorder()
	{
		liero_stats_free(s);
	}

	// Valid until the stats next change.
	LieroStatsView view() const { return liero_stats_view(s); }

	void damagePotential(Worm* byWorm, WormWeapon* weapon, int hp);
	void damageDealt(Worm* byWorm, WormWeapon* weapon, Worm* toWorm, int hp, bool hasHit);

	void shot(Worm* byWorm, WormWeapon* weapon);
	void hit(Worm* byWorm, WormWeapon* weapon, Worm* toWorm);

	void afterSpawn(Worm* worm);
	void afterDeath(Worm* worm);
	void preTick(Game& game);
	void tick(Game& game);

	void finish(Game& game);
	void aiProcessTime(Worm* worm, uint64_t time);

	LieroStats* s;
};

#endif // STATS_RECORDER_HPP

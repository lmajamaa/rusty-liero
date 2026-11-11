#ifndef STATS_RECORDER_HPP
#define STATS_RECORDER_HPP

#include <liero_game.h>
#include <cstdint>

struct Game;
struct Worm;

// A view of the stats a game records, which the Rust game owns and its tick
// records into (rust/liero-game/src/stats.rs).
struct NormalStatsRecorder
{
	explicit NormalStatsRecorder(LieroStats* s)
	: s(s)
	{
	}

	NormalStatsRecorder(NormalStatsRecorder const&) = delete;
	NormalStatsRecorder& operator=(NormalStatsRecorder const&) = delete;

	// Valid until the stats next change.
	LieroStatsView view() const { return liero_stats_view(s); }

	// The end of the game (in game.cpp).
	void finish(Game& game);
	void aiProcessTime(int wormIndex, uint64_t time)
	{
		liero_stats_ai_process_time(s, wormIndex, time);
	}

	LieroStats* s;
};

typedef NormalStatsRecorder StatsRecorder;

#endif // STATS_RECORDER_HPP

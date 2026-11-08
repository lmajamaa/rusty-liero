#ifndef STATS_RECORDER_HPP
#define STATS_RECORDER_HPP

#include <liero_game.h>
#include <cstdint>

struct Game;
struct Worm;

// The stats a game records, in Rust (rust/liero-game/src/stats.rs), which
// the game tick records into.
struct NormalStatsRecorder
{
	NormalStatsRecorder()
	: s(liero_stats_new())
	{
	}

	NormalStatsRecorder(NormalStatsRecorder const& other)
	: s(liero_stats_clone(other.s))
	{
	}

	NormalStatsRecorder& operator=(NormalStatsRecorder const&) = delete;

	~NormalStatsRecorder()
	{
		liero_stats_free(s);
	}

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

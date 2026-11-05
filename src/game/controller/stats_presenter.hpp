#ifndef LIERO_CONTROLLER_STATS_PRESENTER_HPP
#define LIERO_CONTROLLER_STATS_PRESENTER_HPP

#include "../stats_recorder.hpp"

#include <vector>

struct Renderer;

// The stats screen after a game: the overall pane in the middle and one pane
// per worm on each side, scrolled by `offset` pixels. Laid out and drawn in
// Rust (rust/liero-game/src/stats.rs).
struct StatsScreen
{
	StatsScreen(NormalStatsRecorder& recorder, Game& game, Renderer& renderer);
	~StatsScreen();

	StatsScreen(StatsScreen const&) = delete;
	StatsScreen& operator=(StatsScreen const&) = delete;

	// Draws over the renderer's bitmap at pane -1, 0 or 1 (or between, while
	// moving) and sets its palette.
	void draw(double pane, double offset);

	NormalStatsRecorder& recorder;
	Game& game;
	Renderer& renderer;
	LieroStatsScreen* screen;
	std::vector<LieroStr> weaponNames;
};

void presentStats(NormalStatsRecorder& recorder, Game& game);

#endif // LIERO_CONTROLLER_STATS_PRESENTER_HPP


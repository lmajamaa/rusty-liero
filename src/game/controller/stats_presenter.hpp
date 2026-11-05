#ifndef LIERO_CONTROLLER_STATS_PRESENTER_HPP
#define LIERO_CONTROLLER_STATS_PRESENTER_HPP

#include "../stats_recorder.hpp"

#include <vector>

struct Renderer;

// The stats screen after a game: the overall pane in the middle and one pane
// per worm on each side, scrolled by `offset` pixels.
struct StatsScreen
{
	StatsScreen(NormalStatsRecorder& recorder, Game& game, Renderer& renderer);

	// Draws over the renderer's bitmap at pane -1, 0 or 1 (or between, while
	// moving) and sets its palette.
	void draw(double pane, double offset);

	NormalStatsRecorder& recorder;
	Game& game;
	Renderer& renderer;

	std::vector<WeaponStats> combinedWeaponStats, weaponStats[2];
	std::vector<double> wormDamages[2], wormTotalHpDiff;
};

void presentStats(NormalStatsRecorder& recorder, Game& game);

#endif // LIERO_CONTROLLER_STATS_PRESENTER_HPP


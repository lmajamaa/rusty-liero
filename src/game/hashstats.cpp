// Stats part of `--hash-run` (see hashrun.cpp): what the stats recorder has
// collected during a game, and the stats screen drawn from it.

#include "hashrun.hpp"

#include "game.hpp"
#include "stats_recorder.hpp"
#include "controller/stats_presenter.hpp"
#include "gfx/renderer.hpp"

#include <cstdio>

namespace
{

void addHeatmap(Fnv& f, Heatmap const& hm)
{
	f.add(int32_t(hm.width)); f.add(int32_t(hm.height));
	f.add(int32_t(hm.orgWidth)); f.add(int32_t(hm.orgHeight));
	for (int v : hm.map)
		f.add(int32_t(v));
}

} // namespace

// Everything but the process times, which are measured with the clock.
uint64_t hashStats(Game& game)
{
	NormalStatsRecorder& s = static_cast<NormalStatsRecorder&>(*game.statsRecorder);
	Fnv f;
	f.add(int32_t(s.frame));
	f.add(int32_t(s.gameTime));
	addHeatmap(f, s.presence);

	for (WormStats const& w : s.worms)
	{
		f.add(uint32_t(w.lifeSpans.size()));
		for (auto const& l : w.lifeSpans)
		{
			f.add(int32_t(l.first));
			f.add(int32_t(l.second));
		}

		for (WeaponStats const& ws : w.weapons)
		{
			f.add(int32_t(ws.potentialHp)); f.add(int32_t(ws.actualHp));
			f.add(int32_t(ws.potentialHits)); f.add(int32_t(ws.actualHits));
			f.add(int32_t(ws.totalHp)); f.add(int32_t(ws.index));
		}

		f.add(int32_t(w.damage)); f.add(int32_t(w.damageDealt)); f.add(int32_t(w.selfDamage));
		addHeatmap(f, w.damageHm);
		addHeatmap(f, w.presence);
		f.add(int32_t(w.weaponChangeGood)); f.add(int32_t(w.weaponChangeBad));

		f.add(uint32_t(w.wormFrameStats.size()));
		for (WormFrameStats const& fs : w.wormFrameStats)
		{
			f.add(int32_t(fs.damage));
			f.add(int32_t(fs.totalHp));
		}

		f.add(int32_t(w.spawnTime)); f.add(int32_t(w.index));
		f.add(int32_t(w.lives)); f.add(int32_t(w.timer)); f.add(int32_t(w.kills));
	}
	return f.h;
}

// Ends the game's stats and draws the stats screen over the last frame, at
// each pane and part way between them, scrolled to several depths.
void hashStatsScreen(char const* name, Game& game, Renderer& renderer)
{
	game.statsRecorder->finish(game);
	std::printf("%s-stats-end 0 %016llx\n", name, (unsigned long long)hashStats(game));

	NormalStatsRecorder& recorder = static_cast<NormalStatsRecorder&>(*game.statsRecorder);
	Bitmap bg;
	bg.copy(renderer.bmp);
	StatsScreen screen(recorder, game, renderer);

	double const panes[] = { 0, -1, 1, 0.37, -0.81 };
	double const offsets[] = { 0, -35.5, -140, -400, -1200 };
	int n = 0;
	for (double pane : panes)
	{
		for (double offset : offsets)
		{
			renderer.bmp.copy(bg);
			screen.draw(pane, offset);

			Fnv f;
			for (int y = 0; y < renderer.bmp.h; ++y)
				f.bytes(&renderer.bmp.getPixel(0, y), renderer.bmp.w);
			for (Color const& c : renderer.pal.entries)
			{
				f.add(uint32_t(c.r));
				f.add(uint32_t(c.g));
				f.add(uint32_t(c.b));
			}
			std::printf("%s-statsview %d %016llx\n", name, n++, (unsigned long long)f.h);
		}
	}
}

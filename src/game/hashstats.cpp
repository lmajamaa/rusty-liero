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

void addHeatmap(Fnv& f, LieroHeatmapView const& hm)
{
	f.add(int32_t(hm.width)); f.add(int32_t(hm.height));
	f.add(int32_t(hm.org_width)); f.add(int32_t(hm.org_height));
	for (std::size_t i = 0; i < hm.len; ++i)
		f.add(int32_t(hm.map[i]));
}

} // namespace

// Everything but the process times, which are measured with the clock.
uint64_t hashStats(Game& game)
{
	NormalStatsRecorder& recorder = static_cast<NormalStatsRecorder&>(*game.statsRecorder);
	LieroStatsView s = recorder.view();
	Fnv f;
	f.add(int32_t(s.frame));
	f.add(int32_t(s.game_time));
	addHeatmap(f, s.presence);

	for (LieroWormStatsView const& w : s.worms)
	{
		f.add(uint32_t(w.life_span_count));
		for (std::size_t i = 0; i < w.life_span_count; ++i)
		{
			f.add(int32_t(w.life_spans[i][0]));
			f.add(int32_t(w.life_spans[i][1]));
		}

		for (int i = 0; i < 40; ++i)
		{
			LieroWeaponStats const& ws = w.weapons[i];
			f.add(int32_t(ws.potential_hp)); f.add(int32_t(ws.actual_hp));
			f.add(int32_t(ws.potential_hits)); f.add(int32_t(ws.actual_hits));
			f.add(int32_t(ws.total_hp)); f.add(int32_t(ws.index));
		}

		f.add(int32_t(w.damage)); f.add(int32_t(w.damage_dealt)); f.add(int32_t(w.self_damage));
		addHeatmap(f, w.damage_hm);
		addHeatmap(f, w.presence);
		f.add(int32_t(w.weapon_change_good)); f.add(int32_t(w.weapon_change_bad));

		f.add(uint32_t(w.frame_stat_count));
		for (std::size_t i = 0; i < w.frame_stat_count; ++i)
		{
			f.add(int32_t(w.frame_stats[i].damage));
			f.add(int32_t(w.frame_stats[i].total_hp));
		}

		f.add(int32_t(w.spawn_time)); f.add(int32_t(w.index));
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

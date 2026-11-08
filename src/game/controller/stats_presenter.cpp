#include "stats_presenter.hpp"

#include <algorithm>
#include "ticks.hpp"
#include "game/game.hpp"
#include "game/gfx.hpp"
#include "game/screen.hpp"

StatsScreen::StatsScreen(NormalStatsRecorder& recorder, Game& game, Renderer& renderer)
: recorder(recorder)
, game(game)
, renderer(renderer)
, screen(liero_stats_screen_new(recorder.s, renderer.renderResX))
{
	for (auto const& w : game.common->weapons)
	{
		weaponNames.push_back(w.name);
	}
}

StatsScreen::~StatsScreen()
{
	liero_stats_screen_free(screen);
}

namespace
{

LieroStr str(std::string const& s)
{
	LieroStr r = { reinterpret_cast<uint8_t const*>(s.data()), s.size() };
	return r;
}

} // namespace

LieroStatsScene statsScene(Game& game, Renderer& renderer, std::vector<LieroStr> const& weaponNames)
{
	Common& common = *game.common;

	LieroStatsScene scene;
	scene.font = common.font.view();
	for (int i = 0; i < 2; ++i)
	{
		Sprite spr = common.wormSpriteObj(2, i == 0 ? 1 : 0, i);
		scene.worm_sprites[i].mem = spr.mem;
		scene.worm_sprites[i].width = spr.width;
		scene.worm_sprites[i].height = spr.height;
		scene.worm_sprites[i].pitch = spr.pitch;
		scene.names[i] = game.worms[i]->settings->name.view();
	}
	scene.game_mode = game.settings->gameMode;
	scene.game_mode_name = str(common.texts.gameModes[game.settings->gameMode]);
	scene.weapon_names = weaponNames.data();
	scene.weapon_count = weaponNames.size();
	scene.render_res_x = renderer.renderResX;
	scene.render_res_y = renderer.renderResY;
	scene.ticks_per_sec = hiresTicksPerSec();
	return scene;
}

void StatsScreen::draw(double pane, double offset)
{
	LieroStatsScene scene = statsScene(game, renderer, weaponNames);
	LieroBitmap v = renderer.bmp.view();
	liero_stats_screen_draw(screen, recorder.s, &scene, &v, pane, offset);

	renderer.pal = game.common->exepal; // We don't use gfx.origpal because the colors are unpredictable
}

void presentStats(NormalStatsRecorder& recorder, Game& game)
{
	// The screen copies the scene's texts; the stats, font and sprites
	// outlive it.
	std::vector<LieroStr> weaponNames;
	for (auto const& w : game.common->weapons)
		weaponNames.push_back(w.name);
	LieroStatsScene scene = statsScene(game, gfx.playRenderer, weaponNames);
	gfx.open(fromFfi(liero_stats_presenter_screen(recorder.s, &scene)), [](int) { return -1; });
}

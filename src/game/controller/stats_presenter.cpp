#include "stats_presenter.hpp"

#include <algorithm>
#include <gvl/system/system.hpp>
#include "game/game.hpp"
#include "game/gfx.hpp"

StatsScreen::StatsScreen(NormalStatsRecorder& recorder, Game& game, Renderer& renderer)
: recorder(recorder)
, game(game)
, renderer(renderer)
, screen(liero_stats_screen_new(recorder.s, renderer.renderResX))
{
	for (auto const& w : game.common->weapons)
	{
		LieroStr name = { reinterpret_cast<uint8_t const*>(w.name.data()), w.name.size() };
		weaponNames.push_back(name);
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

void StatsScreen::draw(double pane, double offset)
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
	scene.ticks_per_sec = gvl::hires_ticks_per_sec();

	LieroBitmap v = renderer.bmp.view();
	liero_stats_screen_draw(screen, recorder.s, &scene, &v, pane, offset);

	renderer.pal = common.exepal; // We don't use gfx.origpal because the colors are unpredictable
}

namespace
{

// The stats screen over the last frame of the game, until Enter or Escape.
struct StatsPresenter : Screen
{
	Bitmap bg;
	std::unique_ptr<StatsScreen> screen;
	double offset = 0, destOffset = 0;
	double pane = 0;
	double destPane = 0;
	bool shown = false;

	StatsPresenter(NormalStatsRecorder& recorder, Game& game)
	{
		gfx.clearKeys();
		bg.copy(gfx.playRenderer.bmp);
		screen.reset(new StatsScreen(recorder, game, gfx.playRenderer));
	}

	Tick tick() override
	{
		if (shown)
		{
			gfx.process();

			if (gfx.testSDLKey(SDL_SCANCODE_DOWN))
			{
				destOffset = destOffset - 10;
			}
			else if (gfx.testSDLKey(SDL_SCANCODE_UP))
			{
				destOffset = std::min(destOffset + 10.0, 0.0);
			}
			else if (gfx.testSDLKeyOnce(SDL_SCANCODE_RIGHT))
			{
				destPane = std::min(destPane + 1.0, 1.0);
			}
			else if (gfx.testSDLKeyOnce(SDL_SCANCODE_LEFT))
			{
				destPane = std::max(destPane - 1.0, -1.0);
			}
			else if (gfx.testSDLKey(SDL_SCANCODE_RETURN) ||
			         gfx.testSDLKey(SDL_SCANCODE_ESCAPE))
			{
				fill(gfx.playRenderer.bmp, 0);

				gfx.clearKeys();
				return Tick::close();
			}

			pane = (pane * 0.89 + destPane * 0.11);
			offset = (offset * 0.89 + destOffset * 0.11);

			if (offset > 0)
			{
				offset = 0;
			}
		}

		gfx.playRenderer.bmp.copy(bg);
		screen->draw(pane, offset);
		shown = true;
		return Tick::stay(true);
	}
};

}

void presentStats(NormalStatsRecorder& recorder, Game& game)
{
	gfx.open(std::unique_ptr<Screen>(new StatsPresenter(recorder, game)), [](int) { return -1; });
}

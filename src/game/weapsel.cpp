#include <SDL.h>

#include "weapsel.hpp"
#include "gfx.hpp"
#include "game.hpp"
#include "worm.hpp"
#include "text.hpp"
#include "menu/menu.hpp"
#include "mixer/player.hpp"
#include "viewport.hpp"
#include "filesystem.hpp"
#include "gfx/renderer.hpp"

// Choosing, changing and drawing weapons are implemented in Rust
// (rust/liero-game/src/weapsel.rs).

namespace
{

LieroWeapselEnv weapselEnv(Game& game)
{
	Common& common = *game.common;
	LieroWeapselEnv e;
	e.weap_table = game.settings->weapTable;
	e.weap_order = common.weapOrder.items;
	e.weapon_count = int32_t(common.weapOrder.size());
	e.rand = &gfx.rand;
	e.tc = liero_tc_data(common.tc);
	e.sound = game.soundPlayer->out();
	return e;
}

}

WeaponSelection::WeaponSelection(Game& game)
: game(game)
, box(nullptr)
, cachedBackground(false)
, cachedSpectatorBackground(false)
, focused(true)
{
	Common& common = *game.common;
	static_assert(LIERO_WEAPON_TABLE_SIZE == 40, "Settings::weapTable");
	static_assert(LIERO_SELECTABLE_WEAPONS == Settings::selectableWeapons, "selectable weapons");

	std::vector<LieroWeapselWorm> worms;
	for (std::size_t i = 0; i < game.viewports.size(); ++i)
	{
		Viewport& vp = *game.viewports[i];
		WormSettings& ws = *game.wormByIdx(vp.wormIdx)->settings;
		LieroWeapselWorm w;
		w.worm = game.wormByIdx(vp.wormIdx)->rust();
		w.weapons = ws.weapons;
		w.name = ws.name.view();
		w.color = ws.color;
		w.center_x = vp.rect.center_x();
		w.center_y = vp.rect.center_y();
		w.random = ws.controller != 0 && game.settings->selectBotWeapons == 0;
		w.ready = ws.controller != 0 && game.settings->selectBotWeapons != 1;
		worms.push_back(w);
	}
	std::vector<LieroStr> names;
	for (auto const& w : common.weapons)
		names.push_back(w.name);

	std::string randomize = LS(Randomize);
	std::string const& done = LS(Done);
	LieroWeapselSpec spec;
	spec.worms = worms.data();
	spec.worm_count = int32_t(worms.size());
	spec.weapon_names = names.data();
	spec.weapon_count = int32_t(names.size());
	spec.randomize = { reinterpret_cast<uint8_t const*>(randomize.data()), randomize.size() };
	spec.done = { reinterpret_cast<uint8_t const*>(done.data()), done.size() };

	LieroWeapselEnv env = weapselEnv(game);
	box = liero_weapsel_new(&spec, &env);
}

WeaponSelection::~WeaponSelection()
{
	liero_weapsel_free(box);
}

void WeaponSelection::drawSpectatorViewports(Renderer& renderer, GameState state)
{
	Common& common = *game.common;
	int centerX = renderer.renderResX / 2;
	int centerY = renderer.renderResY / 4;

	if (!cachedSpectatorBackground)
	{
		drawVersusInfo(common, renderer.bmp, *game.settings, gfx.settings->levelFile.str(), centerX, centerY);
		common.font.drawCenteredText(renderer.bmp, "WEAPON SELECTION", centerX, centerY + 48, 7, 2);
		game.level.drawMiniature(renderer.bmp, centerX - 126, renderer.renderResY - 208, 2);

		gfx.frozenSpectatorScreen.copy(renderer.bmp);
		cachedSpectatorBackground = true;
	}

	renderer.bmp.copy(gfx.frozenSpectatorScreen);

	if(!focused)
		return;

	{
		LieroBitmap scr = renderer.bmp.view();
		LieroFont font = common.font.view();
		liero_weapsel_draw_spectator(box, &scr, &font);
	}

	// TODO: This just uses the currently activated palette, which might well be wrong.
	gfx.singleScreenRenderer.pal = gfx.singleScreenRenderer.origpal;
	gfx.singleScreenRenderer.pal.rotateFrom(gfx.singleScreenRenderer.origpal, 168, 174, gfx.menuCycles);
	gfx.singleScreenRenderer.pal.fade(gfx.singleScreenRenderer.fadeValue);
}

void WeaponSelection::drawNormalViewports(Renderer& renderer, GameState state)
{
	Common& common = *game.common;

	if (!cachedBackground)
	{
		game.draw(renderer, state, false);

		if (game.settings->levelFile.empty())
		{
			common.font.drawText(renderer.bmp, LS(LevelRandom), 0, 162, 50);
		}
		else
		{
			auto levelName = getBasename(getLeaf(gfx.settings->levelFile.str()));
			common.font.drawText(renderer.bmp, (LS(LevelIs1) + levelName + LS(LevelIs2)), 0, 162, 50);
		}

		gfx.frozenScreen.copy(renderer.bmp);
		cachedBackground = true;
	}

	renderer.bmp.copy(gfx.frozenScreen);

	if(!focused)
		return;

	{
		LieroBitmap scr = renderer.bmp.view();
		// C++ drew the menus on the play screen, whatever `renderer` was.
		LieroBitmap menuScr = gfx.playRenderer.bmp.view();
		LieroFont font = common.font.view();
		std::string const& title = LS(SelWeap);
		LieroStr t = { reinterpret_cast<uint8_t const*>(title.data()), title.size() };
		liero_weapsel_draw(box, &scr, &menuScr, &font, t);
	}

	// TODO: This just uses the currently activated palette, which might well be wrong.
	gfx.playRenderer.pal = gfx.playRenderer.origpal;
	gfx.playRenderer.pal.rotateFrom(gfx.playRenderer.origpal, 168, 174, gfx.menuCycles);
	gfx.playRenderer.pal.fade(gfx.playRenderer.fadeValue);
}

void WeaponSelection::draw(Renderer& renderer, GameState state, bool useSpectatorViewports)
{
	if (useSpectatorViewports)
	{
		drawSpectatorViewports(renderer, state);
	}
	else
	{
		drawNormalViewports(renderer, state);
	}
}

bool WeaponSelection::processFrame()
{
	LieroWeapselEnv env = weapselEnv(game);
	return liero_weapsel_process(box, &env);
}

// Gives the worms their weapons and releases their controls, in Rust.
void WeaponSelection::finalize()
{
	std::vector<LieroWormData*> worms;
	std::vector<uint32_t const*> weapons;
	for (Worm* w : game.worms)
	{
		worms.push_back(w->rust());
		weapons.push_back(w->settings->weapons);
	}
	liero_weapsel_finalize(worms.data(), weapons.data(), int32_t(worms.size()), liero_tc_data(game.common->tc));
}

void WeaponSelection::focus()
{
	focused = true;
}

void WeaponSelection::unfocus()
{
	focused = false;
}

#if 0
void selectWeapons(Game& game)
{
	WeaponSelection ws(game);

	while(!ws.processFrame())
	{
		ws.draw();
		gfx.flip();
		gfx.process(&game);
	}

	ws.finalize();
	// Important that escape isn't released here
}
#endif

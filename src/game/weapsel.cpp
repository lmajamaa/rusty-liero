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
	return e;
}

Worm& menuWorm(WeaponSelection& ws, int menu)
{
	return *ws.game.wormByIdx(ws.game.viewports[menu]->wormIdx);
}

WeaponSelection& weapselOf(void* ctx)
{
	return *static_cast<WeaponSelection*>(ctx);
}

bool hookPressed(void* ctx, int32_t menu, uint32_t control)
{
	return menuWorm(weapselOf(ctx), menu).pressed(Worm::Control(control));
}

void hookRelease(void* ctx, int32_t menu, uint32_t control)
{
	menuWorm(weapselOf(ctx), menu).release(Worm::Control(control));
}

bool hookPressedOnce(void* ctx, int32_t menu, uint32_t control)
{
	return menuWorm(weapselOf(ctx), menu).pressedOnce(Worm::Control(control));
}

void hookPlaySound(void* ctx, int32_t sound)
{
	weapselOf(ctx).game.soundPlayer->play(sound);
}

void hookSetWeapon(void* ctx, int32_t menu, int32_t slot, int32_t weapon)
{
	WeaponSelection& ws = weapselOf(ctx);
	menuWorm(ws, menu).weapons[slot].type = &ws.game.common->weapons[weapon];
}

void hookInitWeapon(void* ctx, int32_t menu, int32_t slot, int32_t weapon)
{
	WeaponSelection& ws = weapselOf(ctx);
	Worm& worm = menuWorm(ws, menu);
	WormWeapon& ww = worm.weapons[slot];
	ww.ammo = 0;
	ww.type = &ws.game.common->weapons[weapon];
	worm.currentWeapon = 0;
}

LieroWeapselHooks weapselHooks(WeaponSelection& ws)
{
	LieroWeapselHooks hooks = {
		&ws, hookPressed, hookRelease, hookPressedOnce, hookPlaySound, hookSetWeapon, hookInitWeapon
	};
	return hooks;
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
	LieroWeapselHooks hooks = weapselHooks(*this);
	box = liero_weapsel_new(&spec, &env, &hooks);
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
	LieroWeapselHooks hooks = weapselHooks(*this);
	return liero_weapsel_process(box, &env, &hooks);
}

void WeaponSelection::finalize()
{
	for(std::size_t i = 0; i < game.worms.size(); ++i)
	{
		Worm& worm = *game.worms[i];

		worm.initWeapons(game);
		/*
		for(int j = 0; j < 6; ++j)
		{
			gfx.releaseKey(worm.settings->controls[j]);
		}*/
	}
	game.releaseControls();

	// TODO: Make sure the weapon selection is transfered back to Gfx to be saved
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

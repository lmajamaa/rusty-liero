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
	e.weap_order = common.weapOrder.data();
	e.weapon_count = int32_t(common.weapOrder.size());
	e.rand = &gfx.rand;
	return e;
}

// The menus as Rust sees them, written back when done.
struct WeapselMenus
{
	WeapselMenus(WeaponSelection& ws)
	: ws(ws)
	{
		Game& game = ws.game;
		for (std::size_t i = 0; i < ws.menus.size(); ++i)
		{
			Menu& m = ws.menus[i];
			WormSettings& s = *game.wormByIdx(game.viewports[i]->wormIdx)->settings;
			items.push_back(m.itemViews());
			LieroWeapselMenu v;
			v.menu = m.state();
			v.items = items.back().data();
			v.item_count = int32_t(items.back().size());
			v.weapons = s.weapons;
			v.ready = ws.isReady[i];
			v.name = s.name.view();
			v.color = s.color;
			views.push_back(v);
		}
		// `items` doesn't move from here on.
		for (std::size_t i = 0; i < views.size(); ++i)
			views[i].items = items[i].data();
	}

	~WeapselMenus()
	{
		for (std::size_t i = 0; i < views.size(); ++i)
		{
			ws.menus[i].setState(views[i].menu, items[i]);
			ws.isReady[i] = views[i].ready;
		}
	}

	WeaponSelection& ws;
	std::vector<std::vector<LieroMenuItem>> items;
	std::vector<LieroWeapselMenu> views;
};

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

void hookSetItemName(void* ctx, int32_t menu, int32_t slot, int32_t weapon)
{
	WeaponSelection& ws = weapselOf(ctx);
	ws.menus[menu].setItemString(slot + 1, ws.game.common->weapons[weapon].name);
}

void hookSetWeapon(void* ctx, int32_t menu, int32_t slot, int32_t weapon)
{
	WeaponSelection& ws = weapselOf(ctx);
	menuWorm(ws, menu).weapons[slot].type = &ws.game.common->weapons[weapon];
	hookSetItemName(ctx, menu, slot, weapon);
}

}

WeaponSelection::WeaponSelection(Game& game)
: game(game)
, enabledWeaps(0)
, isReady(game.viewports.size())
, menus(game.viewports.size())
, cachedBackground(false)
, cachedSpectatorBackground(false)
, focused(true)
{
	Common& common = *game.common;

	for(int i = 0; i < 40; ++i)
	{
		if(game.settings->weapTable[i] == 0)
			++enabledWeaps;
	}
	static_assert(LIERO_WEAPON_TABLE_SIZE == 40, "Settings::weapTable");
	static_assert(LIERO_SELECTABLE_WEAPONS == Settings::selectableWeapons, "selectable weapons");

	for(std::size_t i = 0; i < menus.size(); ++i)
	{
		Viewport& vp = *game.viewports[i];

		Worm& worm = *game.wormByIdx(vp.wormIdx);
		WormSettings& ws = *worm.settings;

		menus[i].pushItem(MenuItem(57, 57, LS(Randomize)));

		{
			int x = vp.rect.center_x() - 31;
			int y = vp.rect.center_y() - 51;
			menus[i].place(x, y);
		}

		bool randomWeapons = (ws.controller != 0 && game.settings->selectBotWeapons == 0);

		LieroWeapselEnv env = weapselEnv(game);
		int32_t chosen[Settings::selectableWeapons];
		liero_weapsel_choose(ws.weapons, randomWeapons, &env, chosen);

		for(int j = 0; j < Settings::selectableWeapons; ++j)
		{
			int w = chosen[j];

			WormWeapon& ww = worm.weapons[j];

			ww.ammo = 0;
			ww.type = &common.weapons[w];

			menus[i].pushItem(MenuItem(48, 48, common.weapons[w].name));
		}

		menus[i].pushItem(MenuItem(10, 10, LS(Done)));

		worm.currentWeapon = 0;

		menus[i].moveToFirstVisible();
		isReady[i] = (ws.controller != 0 && game.settings->selectBotWeapons != 1);
	}
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
		WeapselMenus m(*this);
		LieroBitmap scr = renderer.bmp.view();
		LieroFont font = common.font.view();
		liero_weapsel_draw_spectator(m.views.data(), int32_t(m.views.size()), &scr, &font);
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
		WeapselMenus m(*this);
		LieroBitmap scr = renderer.bmp.view();
		// C++ drew the menus on the play screen, whatever `renderer` was.
		LieroBitmap menuScr = gfx.playRenderer.bmp.view();
		LieroFont font = common.font.view();
		std::string const& title = LS(SelWeap);
		LieroStr t = { reinterpret_cast<uint8_t const*>(title.data()), title.size() };
		liero_weapsel_draw(m.views.data(), int32_t(m.views.size()), &scr, &menuScr, &font, t);
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
	LieroWeapselHooks hooks = {
		this, hookPressed, hookRelease, hookPressedOnce, hookPlaySound, hookSetWeapon, hookSetItemName
	};
	WeapselMenus m(*this);
	return liero_weapsel_process(m.views.data(), int32_t(m.views.size()), &env, &hooks);
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

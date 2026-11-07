// Menu checks for the Rust port, run after the games by `--hash-run`.
//
// Drives the main, match setup, player, options and weapon availability
// menus and the weapon selection screen with scripted keys from fixed seeds,
// and prints a hash of each menu's state and of what is drawn after every
// step (lines ending in "frame"). The boxes the menus open (text input,
// messages and the spectator window's "A vs B") are drawn and hashed too.
// Only items whose actions don't open a dialog, files or a window are used.

#include "hashrun.hpp"

#include "game.hpp"
#include "gfx.hpp"
#include "weapsel.hpp"
#include "viewport.hpp"
#include "spectatorviewport.hpp"
#include "worm.hpp"
#include "mixer/player.hpp"
#include "menu/arrayEnumBehavior.hpp"

#include <cstdio>
#include <memory>
#include <set>
#include <string>

namespace
{

void hashString(Fnv& f, std::string const& s)
{
	f.add(uint32_t(s.size()));
	f.bytes(reinterpret_cast<uint8_t const*>(s.data()), s.size());
}

void hashMenu(Fnv& f, Menu& m)
{
	f.add(int32_t(m.selection())); f.add(int32_t(m.topItem)); f.add(int32_t(m.bottomItem));
	f.add(int32_t(m.visibleItemCount)); f.add(int32_t(m.height));
	hashString(f, m.searchPrefix);
	for (MenuItem& i : m.items)
	{
		hashString(f, i.string);
		hashString(f, i.value);
		f.add(i.hasValue); f.add(i.visible); f.add(i.selectable);
	}
}

void hashScreen(Fnv& f, Renderer& r)
{
	for (int y = 0; y < r.bmp.h; ++y)
		f.bytes(&r.bmp.getPixel(0, y), r.bmp.w);
	for (Color const& c : r.pal.entries)
	{
		f.add(uint32_t(c.r));
		f.add(uint32_t(c.g));
		f.add(uint32_t(c.b));
	}
}

void print(char const* name, int step, Fnv const& f)
{
	std::printf("%s %d %016llx frame\n", name, step, (unsigned long long)f.h);
}

// Weapon availability, as `Gfx::weaponOptions` shows it.
struct AvailabilityMenu : Menu
{
	AvailabilityMenu(int x, int y)
	: Menu(x, y)
	{
	}

	ItemBehavior* getItemBehavior(Common& common, MenuItem& item)
	{
		int index = common.weapOrder[item.id];
		return new ArrayEnumBehavior(common, gfx.settings->weapTable[index], common.texts.weapStates);
	}
};

// Presses random keys in `menu` for `steps` steps. Items in `noEnter` and
// `noLeftRight` would open a dialog or change the window.
void runMenu(Common& common, char const* name, Menu& menu, std::set<int> const& noEnter,
	std::set<int> const& noLeftRight, bool contains, uint32_t seed, int steps)
{
	Rand r(seed);
	Renderer& rr = gfx.playRenderer;
	Uint32 time = 0;

	menu.updateItems(common);
	menu.moveToFirstVisible();

	for (int step = 0; step < steps; ++step)
	{
		int id = menu.selectedId();
		switch (r(9))
		{
		case 0: menu.movement(-1); break;
		case 1: menu.movement(1); break;
		case 2: menu.movementPage(r(2) ? 1 : -1); break;
		case 3: menu.scroll(int(r(5)) - 2); break;
		case 4:
			if (!noLeftRight.count(id))
				menu.onLeftRight(common, r(2) ? 1 : -1);
			break;
		case 5:
			if (!noEnter.count(id))
				menu.onEnter(common);
			break;
		case 6:
		{
			SDL_Keysym k = {};
			if (r(4) == 0)
			{
				k.scancode = SDL_SCANCODE_TAB;
				k.sym = SDLK_TAB;
			}
			else
			{
				char const* letters = "AEIOSTNRLMBCDGHPWaeios 1";
				k.scancode = SDL_SCANCODE_A;
				k.sym = letters[r(24)];
			}
			time += r(4) == 0 ? 2000 : r(700);
			menu.onKeys(&k, &k + 1, contains, time);
			break;
		}
		case 7:
		{
			int i = int(r(uint32_t(menu.items.size())));
			if (menu.items[i].id >= 0 && menu.indexFromId(menu.items[i].id) == i)
				menu.setVisibility(menu.items[i].id, !menu.items[i].visible);
			break;
		}
		case 8: menu.updateItems(common); break;
		}

		++gfx.menuCycles;

		Fnv f;
		hashMenu(f, menu);
		rr.clear();
		bool disabled = r(5) == 0;
		int x = r(4) == 0 ? int(r(300)) : -1;
		bool showDisabledSelection = r(2) == 1;
		menu.draw(common, rr, disabled, x, showDisabledSelection);
		hashScreen(f, rr);
		print(name, step, f);
	}
}

void runMenus(gvl::shared_ptr<Common> common)
{
	Common& c = *common;
	gfx.settings.reset(new Settings);
	// The menus show the name of the loaded setup.
	gfx.settingsNode = FsNode(std::string("config/LIERO.CFG"));
	for (int i = 0; i < 2; ++i)
	{
		WormSettings& ws = *gfx.settings->wormSettings[i];
		ws.name = i == 0 ? "LEFTY" : "RIGHTY";
		ws.color = i == 0 ? 32 : 41;
		for (int j = 0; j < 7; ++j)
			ws.controls[j] = 2 + j * 5 + i;
	}

	gfx.loadMenus();

	// As the menu loop sets them up before a game.
	gfx.mainMenu.setVisibility(MainMenu::MaResumeGame, false);
	gfx.mainMenu.itemFromId(MainMenu::MaNewGame)->string = "NEW GAME (F1)";

	runMenu(c, "menu-main", gfx.mainMenu, { MainMenu::MaReplays, MainMenu::MaTc }, {}, false, 1, 200);

	runMenu(c, "menu-setup", gfx.settingsMenu,
		{ SettingsMenu::SiLevel, SettingsMenu::SiWeaponOptions, SettingsMenu::LoadOptions, SettingsMenu::SaveOptions,
		  SettingsMenu::SiLoadingTimes, SettingsMenu::SiMaxBonuses, SettingsMenu::SiLives, SettingsMenu::SiFlagsToWin },
		{}, false, 2, 400);

	gfx.playerMenu.ws = gfx.settings->wormSettings[0];
	std::set<int> playerNoEnter;
	for (int id = PlayerMenu::PlName; id <= PlayerMenu::PlLoadProfile; ++id)
		if (id != PlayerMenu::PlController)
			playerNoEnter.insert(id);
	runMenu(c, "menu-player", gfx.playerMenu, playerNoEnter, {}, false, 3, 400);

	runMenu(c, "menu-options", gfx.hiddenMenu,
		{ HiddenMenu::DoubleRes, HiddenMenu::Fullscreen, HiddenMenu::SpectatorWindow,
		  HiddenMenu::AiFrames, HiddenMenu::AiMutations, HiddenMenu::PaletteSelect, HiddenMenu::AiParallels },
		{ HiddenMenu::DoubleRes, HiddenMenu::Fullscreen, HiddenMenu::SpectatorWindow }, false, 4, 300);

	// Long enough to scroll, and searched by any part of the name like the
	// file selectors.
	AvailabilityMenu weapons(179, 28);
	weapons.setHeight(14);
	weapons.valueOffsetX = 89;
	for (int i = 0; i < (int)c.weapons.size(); ++i)
		weapons.addItem(MenuItem(48, 7, c.weapons[c.weapOrder[i]].name, i));
	runMenu(c, "menu-weapons", weapons, {}, {}, true, 5, 400);
}

// The weapon selection screen with both worms' controls pressed at random.
void runWeaponSelection(gvl::shared_ptr<Common> common, char const* name, uint32_t seed,
	uint32_t controller1, uint32_t selectBotWeapons, int steps)
{
	gvl::shared_ptr<Settings> settings(new Settings);
	gfx.settings = settings;
	settings->selectBotWeapons = selectBotWeapons;
	Rand r(seed);
	for (int i = 0; i < 40; ++i)
		settings->weapTable[i] = r(6) == 0 ? 1 + r(2) : 0;
	for (int i = 0; i < 2; ++i)
	{
		WormSettings& ws = *settings->wormSettings[i];
		ws.name = i == 0 ? "LEFTY" : "RIGHTY";
		ws.color = i == 0 ? 32 : 41;
		ws.controller = i == 1 ? controller1 : 0;
		for (int j = 0; j < 5; ++j)
			ws.weapons[j] = 1 + r(40);
	}
	if (seed % 2 == 0)
		settings->levelFile = "levels/SOME.LEV";

	gfx.rand.seed(seed);

	std::unique_ptr<Game> gamePtr(new Game(common, settings, gvl::shared_ptr<SoundPlayer>(new NullSoundPlayer)));
	Game& game = *gamePtr;
	for (int i = 0; i < 2; ++i)
	{
		Worm* worm = new Worm();
		worm->settings = settings->wormSettings[i];
		worm->health = worm->settings->health;
		worm->index = i;
		worm->statsX = i == 0 ? 0 : 218;
		game.addWorm(worm);
	}
	game.addViewport(new Viewport(gvl::rect(0, 0, 158, 158), 0, 504, 350));
	game.addViewport(new Viewport(gvl::rect(160, 0, 158+160, 158), 1, 504, 350));
	game.addSpectatorViewport(new SpectatorViewport(gvl::rect(0, 0, 504 + 68, 350), 504, 350));
	{
		Level newLevel(*common);
		newLevel.generateFromSettings(*common, *settings, gfx.rand);
		game.level.swap(newLevel);
	}
	game.focus(gfx.playRenderer);
	game.focus(gfx.singleScreenRenderer);

	WeaponSelection ws(game);

	for (int step = 0; step < steps; ++step)
	{
		for (Worm* w : game.worms)
		{
			w->setControlState(Worm::Left, r(6) == 0);
			w->setControlState(Worm::Right, r(6) == 0);
			w->setControlState(Worm::Up, r(5) == 0);
			w->setControlState(Worm::Down, r(4) == 0);
			w->setControlState(Worm::Fire, r(40) == 0);
		}

		if (step % 40 == 20)
			ws.unfocus();
		else if (step % 40 == 22)
			ws.focus();

		bool done = ws.processFrame();
		++gfx.menuCycles;

		Fnv f;
		for (std::size_t i = 0; i < ws.menus.size(); ++i)
		{
			f.add(bool(ws.isReady[i]));
			hashMenu(f, ws.menus[i]);
		}
		for (Worm* w : game.worms)
		{
			for (int j = 0; j < 5; ++j)
			{
				f.add(w->settings->weapons[j]);
				f.add(w->weapons[j].type ? w->weapons[j].type->id : -1);
			}
		}

		gfx.playRenderer.clear();
		ws.draw(gfx.playRenderer, StateWeaponSelection, false);
		hashScreen(f, gfx.playRenderer);
		gfx.singleScreenRenderer.clear();
		ws.draw(gfx.singleScreenRenderer, StateWeaponSelection, true);
		hashScreen(f, gfx.singleScreenRenderer);
		print(name, step, f);

		if (done)
			break;
	}
}

// The boxes drawn by the menus' dialogs.
void runBoxes(Common& c)
{
	Rand r(6);
	Bitmap frozen;
	frozen.alloc(320, 200, 320);
	for (int y = 0; y < 200; ++y)
		for (int x = 0; x < 320; ++x)
			frozen.getPixel(x, y) = PalIdx(x ^ y);

	char const* texts[] = { "", "A", "NO WEAPONS", "SAVE SETUP AS...", "LEVELS\\SOME.LEV", "1234567890" };

	for (int step = 0; step < 24; ++step)
	{
		Fnv f;
		Settings settings;
		settings.wormSettings[0]->name = texts[r(6)];
		settings.wormSettings[1]->name = texts[r(6)];
		settings.wormSettings[0]->color = int(r(256));
		settings.wormSettings[1]->color = int(r(256));
		if (r(2))
			settings.levelFile = "levels/SOME.LEV";

		gfx.singleScreenRenderer.clear();
		drawVersusInfo(c, gfx.singleScreenRenderer.bmp, settings, "x/" + std::string(texts[r(6)]) + ".lev", 320, 100);
		hashScreen(f, gfx.singleScreenRenderer);

		gfx.playRenderer.clear();
		drawInfoBox(c, gfx.playRenderer.bmp, texts[r(6)], 20 + int(r(280)), 10 + int(r(180)));
		drawInputBox(c, gfx.playRenderer.bmp, frozen, std::string(texts[r(6)]) + "_", 20 + int(r(200)), 10 + int(r(180)), r(2) == 1);
		hashScreen(f, gfx.playRenderer);
		print("menu-boxes", step, f);
	}

	// A one-character level file still names the level.
	{
		Fnv f;
		Settings settings;
		settings.levelFile = "A";
		gfx.singleScreenRenderer.clear();
		drawVersusInfo(c, gfx.singleScreenRenderer.bmp, settings, "x/A.lev", 320, 100);
		hashScreen(f, gfx.singleScreenRenderer);
		print("menu-boxes-short", 0, f);
	}
}

}

void hashMenus(gvl::shared_ptr<Common> common)
{
	gfx.common = common;
	gfx.playRenderer.init(320, 200);
	gfx.singleScreenRenderer.init(640, 400);
	gfx.playRenderer.loadPalette(*common);
	gfx.singleScreenRenderer.loadPalette(*common);
	gfx.menuCycles = 0;

	runMenus(common);
	runBoxes(*common);

	runWeaponSelection(common, "weapsel-1", 1, 0, 0, 300);
	runWeaponSelection(common, "weapsel-2", 2, 1, 1, 300);
	runWeaponSelection(common, "weapsel-3", 3, 1, 0, 300);
	runWeaponSelection(common, "weapsel-4", 4, 0, 2, 300);
}

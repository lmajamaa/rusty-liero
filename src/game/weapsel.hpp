#ifndef LIERO_WEAPSEL_HPP
#define LIERO_WEAPSEL_HPP

#include "game.hpp"
#include "menu/menu.hpp"
#include "gfx/renderer.hpp"

struct Game;

// Weapon selection before a game. Its menus and which worms are ready are in
// Rust (LieroWeapsel, rust/liero-game/src/weapsel.rs).
struct WeaponSelection
{
	WeaponSelection(Game& game);
	~WeaponSelection();

	WeaponSelection(WeaponSelection const&) = delete;
	WeaponSelection& operator=(WeaponSelection const&) = delete;

	void draw(Renderer& renderer, GameState state, bool useSpectatorViewports);
	void drawNormalViewports(Renderer& renderer, GameState state);
	void drawSpectatorViewports(Renderer& renderer, GameState state);
	bool processFrame();
	void finalize();

	void focus();
	void unfocus();

	int menuCount() const { return liero_weapsel_menu_count(box); }
	LieroMenuBox* menu(int i) { return liero_weapsel_menu(box, i); }
	bool isReady(int i) const { return liero_weapsel_ready(box, i); }

	Game& game;
	LieroWeapsel* box;
	bool cachedBackground, cachedSpectatorBackground;
	bool focused;
};

//void selectWeapons(Game& game);

#endif // LIERO_WEAPSEL_HPP

#ifndef LIERO_WEAPSEL_HPP
#define LIERO_WEAPSEL_HPP

#include "game.hpp"
#include "gfx/renderer.hpp"

// Weapon selection before a game, in Rust (LieroWeapsel,
// rust/liero-game/src/weapsel.rs).
struct WeaponSelection
{
	WeaponSelection(Game& game)
	: box(liero_weapsel_start(game.handle()))
	{
	}

	~WeaponSelection()
	{
		liero_weapsel_free(box);
	}

	WeaponSelection(WeaponSelection const&) = delete;
	WeaponSelection& operator=(WeaponSelection const&) = delete;

	void draw(Renderer& renderer, GameState state, bool useSpectatorViewports)
	{
		liero_weapsel_draw_game(box, renderer.rust(), state, useSpectatorViewports);
	}

	bool processFrame() { return liero_weapsel_process_game(box); }
	// Gives the worms their weapons and releases their controls.
	void finalize() { liero_weapsel_finish(box); }

	void focus() { liero_weapsel_set_focused(box, true); }
	void unfocus() { liero_weapsel_set_focused(box, false); }

	int menuCount() const { return liero_weapsel_menu_count(box); }
	LieroMenuBox* menu(int i) { return liero_weapsel_menu(box, i); }
	bool isReady(int i) const { return liero_weapsel_ready(box, i); }

	LieroWeapsel* box;
};

#endif // LIERO_WEAPSEL_HPP

#ifndef LIERO_MATERIAL_HPP
#define LIERO_MATERIAL_HPP

#include <cstdint>
#include <liero_game.h>

// Material flags of a palette index or level pixel. The flags and the struct
// are defined in Rust (rust/liero-game/src/material.rs), which uses them for
// the level code. The tests stay inline here because the renderer and the AI
// call them for every pixel.
struct Material : LieroMaterial
{
	enum
	{
		Dirt = LIERO_MATERIAL_DIRT,
		Dirt2 = LIERO_MATERIAL_DIRT2,
		Rock = LIERO_MATERIAL_ROCK,
		Background = LIERO_MATERIAL_BACKGROUND,
		SeeShadow = LIERO_MATERIAL_SEE_SHADOW,
		WormM = LIERO_MATERIAL_WORM
	};

	bool dirt() { return (flags & Dirt) != 0; }
	bool dirt2() { return (flags & Dirt2) != 0; }
	bool rock() { return (flags & Rock) != 0; }
	bool background() { return (flags & Background) != 0; }
	bool seeShadow() { return (flags & SeeShadow) != 0; }

	// Constructed
	bool dirtRock() { return (flags & (Dirt | Dirt2 | Rock)) != 0; }
	bool anyDirt() { return (flags & (Dirt | Dirt2)) != 0; }
	bool dirtBack() { return (flags & (Dirt | Dirt2 | Background)) != 0; }
	bool worm() { return (flags & WormM) != 0; }
};

static_assert(sizeof(Material) == sizeof(LieroMaterial), "Level views pass Material arrays to Rust");

#endif // LIERO_MATERIAL_HPP

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

	bool dirt() const { return (flags & Dirt) != 0; }
	bool dirt2() const { return (flags & Dirt2) != 0; }
	bool rock() const { return (flags & Rock) != 0; }
	bool background() const { return (flags & Background) != 0; }
	bool seeShadow() const { return (flags & SeeShadow) != 0; }

	// Constructed
	bool dirtRock() const { return (flags & (Dirt | Dirt2 | Rock)) != 0; }
	bool anyDirt() const { return (flags & (Dirt | Dirt2)) != 0; }
	bool dirtBack() const { return (flags & (Dirt | Dirt2 | Background)) != 0; }
	bool worm() const { return (flags & WormM) != 0; }
};

static_assert(sizeof(Material) == sizeof(LieroMaterial), "Level views pass Material arrays to Rust");

#endif // LIERO_MATERIAL_HPP

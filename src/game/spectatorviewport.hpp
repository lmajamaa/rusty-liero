#ifndef LIERO_SPECTATORVIEWPORT_HPP
#define LIERO_SPECTATORVIEWPORT_HPP

#include "game.hpp"
#include "worm.hpp"
#include "rand.hpp"
#include "viewport.hpp"
#include "geom.hpp"
#include <ctime>

struct Renderer;

struct SpectatorViewport : Viewport
{
	// Shows the whole level. Its kind, in Rust, is what processes and draws
	// it so, even in the split-screen list of a single-screen replay.
	SpectatorViewport(Rect rect, int levwidth, int levheight)
	: Viewport(rect, 0, levwidth, levheight)
	{
		rust()->kind = LIERO_VIEWPORT_SPECTATOR;
	}
};

#endif // LIERO_SPECTATORVIEWPORT_HPP

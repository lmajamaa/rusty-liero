#ifndef LIERO_VIEWPORT_HPP
#define LIERO_VIEWPORT_HPP

#include "game.hpp"
#include "worm.hpp"
#include "rand.hpp"
#include "geom.hpp"
#include <ctime>

struct Renderer;

// A view of one of the game's viewports, which Rust owns, processes and
// draws (LieroViewport, rust/liero-game/src/viewport.rs). A spectator one
// shows the whole level, by its kind.
struct Viewport
{
	explicit Viewport(LieroViewport* d)
	: data(*d)
	, x(d->x), y(d->y)
	, shake(d->shake)
	, maxX(d->max_x), maxY(d->max_y)
	, centerX(d->center_x), centerY(d->center_y)
	, wormIdx(d->worm_idx)
	, bannerY(d->banner_y)
	, rect(reinterpret_cast<Rect&>(d->rect))
	, rand(static_cast<Rand&>(d->rand))
	{
	}

	Viewport(Viewport const&) = delete;
	Viewport& operator=(Viewport const&) = delete;

	LieroViewport* rust() { return &data; }

private:
	LieroViewport& data;

public:
	int& x;
	int& y;
	int& shake;
	int& maxX;
	int& maxY;
	int& centerX;
	int& centerY;
	int& wormIdx;
	int& bannerY;
	Rect& rect;
	Rand& rand;


	void setCenter(int x, int y)
	{
		this->x = x - centerX;
		this->y = y - centerY;
	}

	void scrollTo(int destX, int destY, int iter)
	{
		for(int c = 0; c < iter; c++)
		{
			if(x < destX - centerX) ++x;
			else if(x > destX - centerX) --x;

			if(y < destY - centerY) ++y;
			else if(y > destY - centerY) --y;
		}
	}
};

// Draws one of the game's viewports, by its kind, in Rust.
void drawViewport(Game& game, LieroViewport* vp, Renderer& renderer, GameState state, bool isReplay);

#endif // LIERO_VIEWPORT_HPP

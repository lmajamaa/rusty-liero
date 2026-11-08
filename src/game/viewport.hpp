#ifndef LIERO_VIEWPORT_HPP
#define LIERO_VIEWPORT_HPP

#include "game.hpp"
#include "worm.hpp"
#include "rand.hpp"
#include "geom.hpp"
#include <ctime>

struct Renderer;

// A camera. Rust owns its state (LieroViewport in
// rust/liero-game/src/viewport.rs); the fields here refer to it.
struct Viewport
{
	Viewport(Rect rect, int wormIdx, int levwidth, int levheight)
	: Viewport(liero_viewport_new(LieroRect{ rect.x1, rect.y1, rect.x2, rect.y2 }, wormIdx, levwidth, levheight))
	{
	}

	// Its fields are set after.
	Viewport()
	: Viewport(Rect(0, 0, 0, 0), 0, 0, 0)
	{
	}

	// A copy with state of its own.
	Viewport(Viewport const& other)
	: Viewport(liero_viewport_clone(&other.data))
	{
	}

	Viewport& operator=(Viewport const&) = delete;

	virtual ~Viewport()
	{
		liero_viewport_free(&data);
	}

	// The viewport's state in Rust.
	LieroViewport* rust() { return &data; }

private:
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


	virtual void draw(Game& game, Renderer& renderer, GameState state, bool isReplay);
	virtual void process(Game& game);
};

#endif // LIERO_VIEWPORT_HPP

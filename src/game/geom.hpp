#ifndef LIERO_GEOM_HPP
#define LIERO_GEOM_HPP

// Integer points and rectangles, as gvl's ivec2 and rect were. Rect's
// layout is LieroRect's.

#include <algorithm>

struct IVec2
{
	IVec2()
	: x(0), y(0)
	{
	}

	IVec2(int x, int y)
	: x(x), y(y)
	{
	}

	IVec2& operator+=(IVec2 b) { x += b.x; y += b.y; return *this; }
	IVec2& operator*=(int b) { x *= b; y *= b; return *this; }
	IVec2& operator/=(int b) { x /= b; y /= b; return *this; }

	friend IVec2 operator*(IVec2 a, int b) { return a *= b; }
	friend IVec2 operator/(IVec2 a, int b) { return a /= b; }

	int x;
	int y;
};

// From (x1, y1) up to, but not including, (x2, y2).
struct Rect
{
	Rect()
	: x1(0), y1(0), x2(0), y2(0)
	{
	}

	Rect(int x1, int y1, int x2, int y2)
	: x1(x1), y1(y1), x2(x2), y2(y2)
	{
	}

	int width() const { return x2 - x1; }
	int height() const { return y2 - y1; }
	int center_x() const { return (x1 + x2) / 2; }
	int center_y() const { return (y1 + y2) / 2; }

	bool inside(int x, int y) const
	{
		return x - x1 < width() && x - x1 >= 0
		    && y - y1 < height() && y - y1 >= 0;
	}

	// Shrinks to the part inside `b`; false if nothing is left.
	bool intersect(Rect const& b)
	{
		x1 = std::max(b.x1, x1);
		y1 = std::max(b.y1, y1);
		x2 = std::min(b.x2, x2);
		y2 = std::min(b.y2, y2);
		return x1 <= x2 && y1 <= y2;
	}

	// The smallest rectangle holding both.
	friend Rect operator|(Rect a, Rect const& b)
	{
		a.x1 = std::min(b.x1, a.x1);
		a.y1 = std::min(b.y1, a.y1);
		a.x2 = std::max(b.x2, a.x2);
		a.y2 = std::max(b.y2, a.y2);
		return a;
	}

	int x1;
	int y1;
	int x2;
	int y2;
};

#endif // LIERO_GEOM_HPP

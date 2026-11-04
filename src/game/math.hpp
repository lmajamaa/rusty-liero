#ifndef LIERO_MATH_HPP
#define LIERO_MATH_HPP

#include <gvl/math/vec.hpp>
#include <liero_game.h>

using fixedvec = gvl::ivec2;

typedef int fixed;

inline fixed itof(int v)
{
	return v << 16;
}

inline int ftoi(fixed v)
{
	return v >> 16;
}

inline fixedvec itof(gvl::ivec2 v)
{
	return fixedvec(itof(v.x), itof(v.y));
}

inline gvl::ivec2 ftoi(fixedvec v)
{
	return gvl::ivec2(ftoi(v.x), ftoi(v.y));
}

// fixedvec and Rust's LieroIVec2 are both two ints, so Rust can work on
// fixedvec fields in place.
static_assert(sizeof(fixedvec) == sizeof(LieroIVec2), "fixedvec layout");

inline LieroIVec2 toRust(fixedvec v)
{
	LieroIVec2 r = { v.x, v.y };
	return r;
}

inline LieroIVec2* toRust(fixedvec* v)
{
	return reinterpret_cast<LieroIVec2*>(v);
}

inline fixedvec fromRust(LieroIVec2 v)
{
	return fixedvec(v.x, v.y);
}

extern fixedvec cossinTable[128];

int vectorLength(int x, int y);

inline int distanceTo(int x1, int y1, int x2, int y2)
{
	return vectorLength(x1 - x2, y1 - y2);
}

void precomputeTables();

#endif // LIERO_MATH_HPP

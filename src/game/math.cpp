#include "math.hpp"
#include <liero_game.h>

// Implemented in Rust (rust/liero-game/src/math.rs).

fixedvec cossinTable[LIERO_COS_SIN_ENTRIES];

int vectorLength(int x, int y)
{
	return liero_vector_length(x, y);
}

void precomputeTables()
{
	LieroIVec2 table[LIERO_COS_SIN_ENTRIES];
	liero_cos_sin_table(table);

	for(int i = 0; i < LIERO_COS_SIN_ENTRIES; ++i)
		cossinTable[i] = fixedvec(table[i].x, table[i].y);
}

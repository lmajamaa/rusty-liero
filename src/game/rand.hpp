#ifndef UUID_C1DCDBE1BD8541A14AEC91A1BDF3B9F4
#define UUID_C1DCDBE1BD8541A14AEC91A1BDF3B9F4

#include <gvl/support/cstdint.hpp>
#include <gvl/support/debug.hpp>
#include <liero_game.h>

// The game's random number generator, implemented in Rust
// (rust/liero-game/src/rand.rs). It is the same multiply-with-carry generator
// as gvl::mwc. The state stays in this struct, so copying a Rand copies the
// generator, as the AI does with whole games.
struct Rand : LieroRand
{
	Rand(uint32_t seed = LIERO_RAND_DEFAULT_SEED)
	{
		liero_rand_seed(this, seed);
	}

	void seed(uint32_t seed)
	{
		liero_rand_seed(this, seed);
	}

	uint32_t operator()()
	{
		return liero_rand_next(this);
	}

	// Number in [0, max).
	uint32_t operator()(uint32_t max)
	{
		return liero_rand_below(this, max);
	}

	// Number in [min, max).
	uint32_t operator()(uint32_t min, uint32_t max)
	{
		sassert(min <= max);
		return liero_rand_range(this, min, max);
	}

	// Number in [0, max).
	double get_double(double max)
	{
		return liero_rand_double_below(this, max);
	}

	bool operator==(Rand const& b) const
	{
		return x == b.x && c == b.c;
	}

	bool operator!=(Rand const& b) const
	{
		return !operator==(b);
	}
};

#endif // UUID_C1DCDBE1BD8541A14AEC91A1BDF3B9F4

// Prints the expected values for the tests in rust/liero-game/src/rand.rs and
// math.rs by running the same loops through the original C++ code: gvl::mwc
// and prng_common from src/gvl, and math.cpp's functions as they were before
// the port (copied below, unchanged).
//
// Build it with em++ and run it with Bun, so it goes through the same compiler
// as the game. From the repo root:
//
//   docker compose run --rm web sh -c "em++ -std=c++11 -O2 -I src rust/liero-game/reference/reference_values.cpp src/gvl/support/debug.cpp -o /tmp/reference_values.js && bun /tmp/reference_values.js"
//
// A native g++ gives the same values only with -fwrapv (the C++ code relies on
// signed overflow wrapping around, as clang's wasm output does) and
// -ffp-contract=off (wasm has no fused multiply-add, which g++ uses on arm64).

#include <gvl/math/cmwc.hpp>
#include <cstdint>
#include <cstdio>
#include <cstring>

// ---- From src/game/math.cpp before the Rust port ----

struct ivec2 { int x, y; };
ivec2 cossinTable[128];

uint32_t sqr(uint32_t op)
{
    uint32_t res = 0;
    uint32_t one = 1uL << 30; // The second-to-top bit is set: use 1u << 14 for uint16_t type; use 1uL<<30 for uint32_t type

    // "one" starts at the highest power of four <= than the argument.
    while (one > op)
        one >>= 2;

    while (one != 0)
    {
        if (op >= res + one)
        {
            op -= res + one;
            res += 2 * one;
        }
        res >>= 1;
        one >>= 2;
    }
    return res;
}

int vectorLength(int x, int y)
{
	return int(sqr(x*x + y*y));
}

struct FP
{
	FP(int64_t s, int bits)
	: s(s), bits(bits)
	{
	}

	void reduce(int tobits)
	{
		int64_t lim = (1ll << tobits);

		while (s < (-lim - 1) || s > lim)
		{
			s >>= 1;
			--bits;
		}
	}

	int64_t reducedfrac(int tobits)
	{
		int64_t rs = s;
		int rbits = bits;
		while (rbits > 60)
		{
			rs >>= 1;
			--rbits;
		}

		return rs << (tobits - rbits);
	}

	int64_t s;
	int bits;
};

void precomputeTables()
{
	int scalebits = 28;
	int32_t scale = 13176795; // (2pi / 128) << scalebits

	for(int i = 0; i < 128; ++i)
	{
		int64_t rf = 0;
		int32_t c = -1;
		int32_t xf = i * scale;

		// Simple Taylor series. Performance is not important.
		FP num(xf, scalebits);
		for(int t = 1; t < 26; )
		{
			rf += c * num.reducedfrac(60);

			num.s /= ++t;
			num.reduce(31);
			num.s = num.s * xf;
			num.bits += scalebits;

			num.s /= ++t;
			num.reduce(31);
			num.s = num.s * xf;
			num.bits += scalebits;

			c = -c;
		}

		int shift = 60 - 16;

		rf += (1LL << (shift - 1)); // Correct rounding

		int32_t r = (int32_t)(rf >> shift);

		cossinTable[i].x = r;
		cossinTable[(i + 32) & 0x7f].y = r;
	}
}

// ---- Hashes in the same order as the Rust tests ----

struct Fnv
{
	uint64_t h = 0xcbf29ce484222325ull;
	void add(uint32_t v) { add64(v); }
	void add64(uint64_t v) { h = (h ^ v) * 0x100000001b3ull; }
};

template<typename F>
uint64_t streamHash(uint32_t seed, F f)
{
	gvl::mwc r(seed);
	Fnv h;
	for (uint32_t i = 0; i < 1000000; ++i)
		h.add(f(r, i));
	h.add(r.x);
	h.add(r.c);
	return h.h;
}

int main()
{
	std::printf("EXPECTED_NEXT: u64 = 0x%016llx\n", (unsigned long long)streamHash(0x1337, [](gvl::mwc& r, uint32_t) { return r(); }));
	std::printf("EXPECTED_NEXT_SEED0: u64 = 0x%016llx\n", (unsigned long long)streamHash(0, [](gvl::mwc& r, uint32_t) { return r(); }));
	std::printf("EXPECTED_NEXT_SEEDMAX: u64 = 0x%016llx\n", (unsigned long long)streamHash(0xffffffff, [](gvl::mwc& r, uint32_t) { return r(); }));
	std::printf("EXPECTED_BELOW: u64 = 0x%016llx\n", (unsigned long long)streamHash(12345, [](gvl::mwc& r, uint32_t i) { return r(i % 1000); }));
	std::printf("EXPECTED_RANGE: u64 = 0x%016llx\n", (unsigned long long)streamHash(777, [](gvl::mwc& r, uint32_t i) { return r(i % 50, i % 50 + i % 7); }));

	{
		gvl::mwc r(4242);
		Fnv h;
		for (uint32_t i = 0; i < 100000; ++i)
		{
			double d = r.get_double(1.0 + i * 0.37);
			uint64_t bits;
			std::memcpy(&bits, &d, sizeof bits);
			h.add64(bits);
		}
		std::printf("EXPECTED_DOUBLE: u64 = 0x%016llx\n", (unsigned long long)h.h);
	}

	{
		Fnv h;
		for (int x = -2000; x <= 2000; x += 7)
			for (int y = -2000; y <= 2000; y += 11)
				h.add(uint32_t(vectorLength(x, y)));
		for (int x = 0; x <= 200000; x += 997)
			h.add(uint32_t(vectorLength(x, -x / 3)));
		std::printf("EXPECTED_VECTOR_LENGTH: u64 = 0x%016llx\n", (unsigned long long)h.h);
	}

	{
		precomputeTables();
		Fnv h;
		for (int i = 0; i < 128; ++i)
		{
			h.add(uint32_t(cossinTable[i].x));
			h.add(uint32_t(cossinTable[i].y));
		}
		std::printf("EXPECTED_COS_SIN: u64 = 0x%016llx\n", (unsigned long long)h.h);
		std::printf("cossinTable[0] = (%d, %d), [32] = (%d, %d)\n", cossinTable[0].x, cossinTable[0].y, cossinTable[32].x, cossinTable[32].y);
	}
}

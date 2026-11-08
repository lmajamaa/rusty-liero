#ifndef LIERO_TICKS_HPP
#define LIERO_TICKS_HPP

#include <chrono>
#include <cstdint>

// A monotonic clock in nanoseconds, as gvl's hires ticks were.
inline uint64_t hiresTicks()
{
	return uint64_t(std::chrono::duration_cast<std::chrono::nanoseconds>(
		std::chrono::steady_clock::now().time_since_epoch()).count());
}

inline uint64_t hiresTicksPerSec()
{
	return 1000000000ull;
}

#endif // LIERO_TICKS_HPP

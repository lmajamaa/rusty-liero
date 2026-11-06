#ifndef LIERO_APPCLOCK_HPP
#define LIERO_APPCLOCK_HPP

#include <cstdint>
#include <ctime>

// The clock outside the game logic: milliseconds for the menus' typing
// search, nanoseconds for the stats' timings of ticks and bot thinking, and
// the time of day that seeds new games and names replays. The hash run's
// driver replaces all three (see AppDriver in gfx.hpp).
uint32_t appTicks();
uint64_t appHiresTicks();
std::time_t appTime();

#endif // LIERO_APPCLOCK_HPP

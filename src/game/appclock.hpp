#ifndef LIERO_APPCLOCK_HPP
#define LIERO_APPCLOCK_HPP

#include <cstdint>
#include <ctime>

// The clock outside the game logic: milliseconds for the menus' typing
// search, and the time of day that seeds new games and names replays. The
// hash run's driver replaces both (see AppDriver in gfx.hpp).
uint32_t appTicks();
std::time_t appTime();

#endif // LIERO_APPCLOCK_HPP

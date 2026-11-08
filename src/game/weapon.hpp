#ifndef LIERO_WEAPON_HPP
#define LIERO_WEAPON_HPP

#include "math.hpp"
#include "exactObjectList.hpp"
#include <liero_game.h>

struct Worm;
struct Game;
struct Settings;
struct WormWeapon;

// A weapon of the TC, owned by Rust (LieroTcWeapon in
// rust/liero-game/src/tc.rs).
typedef LieroTcWeapon Weapon;

int computedLoadingTime(Weapon const& type, Settings& settings);


struct WObject : ExactObjectListBase
{

	fixedvec pos, vel;
	//int id;
	Weapon const* type;
	int ownerIdx;
	int curFrame;
	int timeLeft;

	// STATS
	WormWeapon* firedBy;
	bool hasHit;
};


#endif // LIERO_WEAPON_HPP

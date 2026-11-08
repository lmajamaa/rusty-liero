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

void fireWeapon(Game& game, Weapon const& type, int angle, fixedvec vel, int speed, fixedvec pos, int ownerIdx, WormWeapon* ww);
int computedLoadingTime(Weapon const& type, Settings& settings);

// The parts of a Weapon that the Rust code uses.
LieroWeapon toRust(Weapon const& w);

struct WObject : ExactObjectListBase
{
	void blowUpObject(Game& game, int causeIdx);
	void process(Game& game);

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

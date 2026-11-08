#ifndef LIERO_NOBJECT_HPP
#define LIERO_NOBJECT_HPP

#include "math.hpp"
#include "exactObjectList.hpp"
#include <liero_game.h>

struct Worm;
struct Game;
struct WormWeapon;
struct NObject;

// An object type of the TC, owned by Rust (LieroTcNObject in
// rust/liero-game/src/tc.rs).
typedef LieroTcNObject NObjectType;

NObject& createNObject(Game& game, NObjectType const& type, fixedvec vel, fixedvec pos, int color, int ownerIdx, WormWeapon* firedBy);
NObject& createNObject1(Game& game, NObjectType const& type, fixedvec vel, fixedvec pos, int color, int ownerIdx, WormWeapon* firedBy);
void createNObject2(Game& game, NObjectType const& type, int angle, fixedvec vel, fixedvec pos, int color, int ownerIdx, WormWeapon* firedBy);

struct NObject : ExactObjectListBase
{
	void process(Game& game);

	fixedvec pos, vel;
	int timeLeft;
	//int id;
	NObjectType const* type;
	int ownerIdx;
	int curFrame;

	// STATS
	WormWeapon* firedBy;
	bool hasHit;
};

#endif // LIERO_NOBJECT_HPP

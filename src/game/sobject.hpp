#ifndef LIERO_SOBJECT_HPP
#define LIERO_SOBJECT_HPP

#include "math.hpp"
#include "exactObjectList.hpp"
#include <liero_game.h>

struct Worm;
struct Game;
struct WormWeapon;
struct WObject;

// A special object type of the TC, owned by Rust (LieroTcSObject in
// rust/liero-game/src/tc.rs).
typedef LieroTcSObject SObjectType;

void createSObject(Game& game, SObjectType const& type, int x, int y, int ownedIdx, WormWeapon* firedBy, WObject* from = 0);

struct SObject : ExactObjectListBase
{
	void process(Game& game);

	int x, y;
	int id; // type
	int curFrame;
	int animDelay;
};

#endif // LIERO_SOBJECT_HPP

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


struct SObject : ExactObjectListBase
{

	int x, y;
	int id; // type
	int curFrame;
	int animDelay;
};

#endif // LIERO_SOBJECT_HPP

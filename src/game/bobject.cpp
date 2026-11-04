#include "bobject.hpp"

#include "gfx/color.hpp"
#include "game.hpp"
#include "constants.hpp"
#include <cstddef>

void Game::createBObject(fixedvec pos, fixedvec vel)
{
	Common& common = *this->common;

	BObject& obj = *bobjects.newObjectReuse();

	obj.color = rand(LC(NumBloodColours)) + LC(FirstBloodColour);
	obj.pos = pos;
	obj.vel = vel;
}

// Rust works on BObject in place.
static_assert(sizeof(BObject) == sizeof(LieroBObject), "BObject layout");
static_assert(offsetof(BObject, pos) == offsetof(LieroBObject, pos), "BObject layout");
static_assert(offsetof(BObject, vel) == offsetof(LieroBObject, vel), "BObject layout");
static_assert(offsetof(BObject, color) == offsetof(LieroBObject, color), "BObject layout");

bool BObject::process(Game& game)
{
	Common& common = *game.common;
	LieroLevel v = game.level.view();
	LieroLevelCommon c = common.levelCommon();
	return liero_bobject_process(reinterpret_cast<LieroBObject*>(this), &v, &c, &game.rand, LC(BObjGravity));
}

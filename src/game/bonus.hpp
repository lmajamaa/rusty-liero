#ifndef LIERO_BONUS_HPP
#define LIERO_BONUS_HPP

#include "math.hpp"
#include "exactObjectList.hpp"

struct Game;

struct Bonus : ExactObjectListBase
{
	// weapon starts at 0: a health bonus keeps the weapon of the bonus that
	// used its slot before, and the first one used to read whatever was in
	// memory. Nothing reads a health bonus's weapon, but the hash run does.
	Bonus()
	: frame(-1), weapon(0)
	{
	}

	fixed x;
	fixed y;
	fixed velY;
	int frame;
	int timer;
	int weapon;

};

#endif // LIERO_BONUS_HPP

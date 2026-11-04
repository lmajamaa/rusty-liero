#include "bonus.hpp"
#include "game.hpp"
#include "constants.hpp"
#include <cassert>

void Bonus::process(Game& game)
{
	Common& common = *game.common;

	int ix = ftoi(x);
	assert(ix >= 0 && ix < game.level.width);

	LieroLevel v = game.level.view();
	LieroBonusConstants k = { LC(BonusGravity), LC(BonusBounceMul), LC(BonusBounceDiv) };

	if(liero_bonus_process(x, &y, &velY, &timer, &v, &k))
	{
		int iy = ftoi(y);
		common.sobjectTypes[common.bonusSObjects[frame]].create(game, ix, iy, 0, 0);
		if(used)
			game.bonuses.free(this);
	}
}

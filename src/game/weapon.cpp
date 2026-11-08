#include "weapon.hpp"
#include "settings.hpp"

// Weapon objects are in Rust (rust/liero-game/src/weapon.rs and world.rs).

int computedLoadingTime(Weapon const& type, Settings& settings)
{
	int ret = (settings.loadingTime * type.loading_time) / 100;
	if(ret == 0)
		ret = 1;
	return ret;
}

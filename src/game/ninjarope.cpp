#include "worm.hpp"
#include "constants.hpp"
#include "game.hpp"
#include "gfx/color.hpp"
#include "math.hpp"
#include <cstddef>

// Rust works on Ninjarope in place, with the anchor as an opaque pointer.
static_assert(sizeof(Ninjarope) == sizeof(LieroNinjarope), "Ninjarope layout");
static_assert(offsetof(Ninjarope, out) == offsetof(LieroNinjarope, out), "Ninjarope layout");
static_assert(offsetof(Ninjarope, attached) == offsetof(LieroNinjarope, attached), "Ninjarope layout");
static_assert(offsetof(Ninjarope, anchor) == offsetof(LieroNinjarope, anchor), "Ninjarope layout");
static_assert(offsetof(Ninjarope, pos) == offsetof(LieroNinjarope, pos), "Ninjarope layout");
static_assert(offsetof(Ninjarope, vel) == offsetof(LieroNinjarope, vel), "Ninjarope layout");
static_assert(offsetof(Ninjarope, length) == offsetof(LieroNinjarope, length), "Ninjarope layout");
static_assert(offsetof(Ninjarope, curLen) == offsetof(LieroNinjarope, cur_len), "Ninjarope layout");

namespace
{

struct RopeContext
{
	Game& game;
	Worm& owner;
};

void* hookFindAnchor(void* ctx, int32_t x, int32_t y)
{
	auto& c = *static_cast<RopeContext*>(ctx);

	for(std::size_t i = 0; i < c.game.worms.size(); ++i)
	{
		Worm& w = *c.game.worms[i];

		if(&w != &c.owner
		&& checkForSpecWormHit(c.game, x, y, 1, w))
		{
			return &w;
		}
	}

	return 0;
}

LieroWormMotion hookWormMotion(void* worm)
{
	Worm& w = *static_cast<Worm*>(worm);
	LieroWormMotion m = { toRust(&w.pos), toRust(&w.vel) };
	return m;
}

void hookRopeCreateNObject2(void* ctx, int32_t kind, int32_t angle, LieroIVec2 vel, LieroIVec2 pos, int32_t color)
{
	auto& c = *static_cast<RopeContext*>(ctx);
	c.game.common->nobjectTypes[kind].create2(
		c.game, angle, fromRust(vel), fromRust(pos), color, c.owner.index, 0);
}

} // namespace

void Ninjarope::process(Worm& owner, Game& game)
{
	Common& common = *game.common;

	RopeContext context = { game, owner };
	LieroWormMotion ownerMotion = hookWormMotion(&owner);
	LieroLevel level = game.level.view();
	LieroNinjaropeConstants k = {
		LC(NRForceShlX), LC(NRForceDivX),
		LC(NRForceShlY), LC(NRForceDivY),
		LC(NRForceLenShl), LC(NRAttachLength), LC(NinjaropeGravity),
	};
	LieroNinjaropeHooks hooks = { &context, hookFindAnchor, hookWormMotion, hookRopeCreateNObject2 };

	liero_ninjarope_process(reinterpret_cast<LieroNinjarope*>(this), ownerMotion, &level, &game.rand, &k, &hooks);
}

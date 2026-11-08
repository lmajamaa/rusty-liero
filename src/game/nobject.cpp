#include "nobject.hpp"
#include "game.hpp"
#include "gfx/renderer.hpp"
#include "mixer/player.hpp"
#include "bobject.hpp"
#include "worm.hpp"
#include "gfx/blit.hpp"

NObject& createNObject(Game& game, NObjectType const& type, fixedvec vel, fixedvec pos, int color, int ownerIdx, WormWeapon* firedBy)
{
	NObject& obj = *game.nobjects.newObjectReuse();

	obj.type = &type;
	obj.ownerIdx = ownerIdx;
	obj.pos = pos;

	obj.vel = vel;

	// STATS
	obj.firedBy = firedBy;
	obj.hasHit = false;

	Worm* owner = game.wormByIdx(ownerIdx);
	game.statsRecorder->damagePotential(owner, firedBy, type.hit_damage);

	if(type.start_frame > 0)
	{
		obj.curFrame = game.rand(type.num_frames + 1);
	}
	else if(color != 0)
	{
		obj.curFrame = color;
	}
	else
	{
		obj.curFrame = type.color_bullets;
	}

	obj.timeLeft = type.time_to_explo;

	if(type.time_to_explo_v)
		obj.timeLeft -= game.rand(type.time_to_explo_v);

	return obj;
}

NObject& createNObject1(Game& game, NObjectType const& type, fixedvec vel, fixedvec pos, int color, int ownerIdx, WormWeapon* firedBy)
{
	if(type.distribution)
	{
		vel.x += type.distribution - game.rand(type.distribution * 2);
		vel.y += type.distribution - game.rand(type.distribution * 2);
	}

	return createNObject(game, type, vel, pos, color, ownerIdx, firedBy);
}

void createNObject2(Game& game, NObjectType const& type, int angle, fixedvec vel, fixedvec pos, int color, int ownerIdx, WormWeapon* firedBy)
{
	int realSpeed = type.speed - game.rand(type.speed_v);

	vel += cossinTable[angle] * realSpeed / 100;

	// TODO: !REPLAYS Make the distributions use the same code
	if(type.distribution)
	{
		vel.x += game.rand(type.distribution * 2) - type.distribution;
		vel.y += game.rand(type.distribution * 2) - type.distribution;
	}

	auto& obj = createNObject(game, type,
		vel,
		pos, color, ownerIdx, firedBy);

	obj.pos += obj.vel;
}

namespace
{

// What the Rust code in liero_nobject_process calls back for.
struct NObjectContext
{
	Game& game;
	NObject& obj;
};

void hookCreateBObject(void* ctx, LieroIVec2 pos, LieroIVec2 vel)
{
	auto& c = *static_cast<NObjectContext*>(ctx);
	c.game.createBObject(fromRust(pos), fromRust(vel));
}

void hookCreateSObject(void* ctx, int32_t kind, int32_t x, int32_t y)
{
	auto& c = *static_cast<NObjectContext*>(ctx);
	createSObject(c.game, c.game.common->sobjectTypes[kind], x, y, c.obj.ownerIdx, c.obj.firedBy);
}

void hookCreateNObject2(void* ctx, int32_t kind, int32_t angle, LieroIVec2 vel, LieroIVec2 pos, int32_t color)
{
	auto& c = *static_cast<NObjectContext*>(ctx);
	createNObject2(c.game, c.game.common->nobjectTypes[kind], angle, fromRust(vel), fromRust(pos), color, c.obj.ownerIdx, 0);
}

void hookDrawOnMap(void* ctx, int32_t x, int32_t y)
{
	auto& c = *static_cast<NObjectContext*>(ctx);
	Common& common = *c.game.common;
	NObjectType const& t = *c.obj.type;

	blitImageOnMap(
		common,
		c.game.level,
		common.smallSprites.spritePtr(t.start_frame + c.obj.curFrame),
		x,
		y,
		7,
		7);
	// x and y are 3 left of and above the object. This seems like an overly
	// large rectangle.
	if(c.game.settings->shadow)
		correctShadow(common, c.game.level, Rect(x - 5, y - 5, x + 12, y + 12));
}

bool hookHitWorms(void* ctx)
{
	auto& c = *static_cast<NObjectContext*>(ctx);
	Game& game = c.game;
	Common& common = *game.common;
	NObject& o = c.obj;
	NObjectType const& t = *o.type;
	bool doExplode = false;

	for(std::size_t i = 0; i < game.worms.size(); ++i)
	{
		Worm& w = *game.worms[i];

		if(checkForSpecWormHit(game, ftoi(o.pos.x), ftoi(o.pos.y), t.detect_distance, w))
		{
			w.vel += o.vel * t.blow_away / 100;

			game.doDamage(w, t.hit_damage, o.ownerIdx);

			Worm* owner = game.wormByIdx(o.ownerIdx);
			game.statsRecorder->damageDealt(owner, o.firedBy, &w, t.hit_damage, o.hasHit);
			o.hasHit = true;

			if(t.hit_damage > 0
			&& w.health > 0
			&& game.rand(3) == 0)
			{
				int snd = 18 + game.rand(3); // NOTE: MUST be outside the unpredictable branch below
				if(!game.soundPlayer->isPlaying(&w))
				{
					game.soundPlayer->play(snd, &w);
				}
			}

			int blood = t.blood_on_hit * game.settings->blood / 100;

			for(int i = 0; i < blood; ++i)
			{
				int angle = game.rand(128);
				createNObject2(game, common.nobjectTypes[6],
					angle,
					o.vel / 3,
					o.pos,
					0,
					o.ownerIdx,
					0);
			}

			if(t.worm_explode)
				doExplode = true;
			else if (t.worm_destroy && o.used)
				game.nobjects.free(&o);
		}
	}

	return doExplode;
}

void hookFree(void* ctx)
{
	auto& c = *static_cast<NObjectContext*>(ctx);
	if(c.obj.used)
		c.game.nobjects.free(&c.obj);
}

LieroNObjectType toRust(NObjectType const& t)
{
	LieroNObjectType r;
	r.gravity = t.gravity;
	r.bounce = t.bounce;
	r.hit_damage = t.hit_damage;
	r.expl_ground = t.expl_ground;
	r.blood_trail = t.blood_trail;
	r.blood_trail_delay = t.blood_trail_delay;
	r.start_frame = t.start_frame;
	r.num_frames = t.num_frames;
	r.draw_on_map = t.draw_on_map;
	r.leave_obj = t.leave_obj;
	r.leave_obj_delay = t.leave_obj_delay;
	r.time_to_explo = t.time_to_explo;
	r.create_on_exp = t.create_on_exp;
	r.dirt_effect = t.dirt_effect;
	r.splinter_amount = t.splinter_amount;
	r.splinter_colour = t.splinter_colour;
	r.splinter_type = t.splinter_type;
	return r;
}

} // namespace

void NObject::process(Game& game)
{
	Common& common = *game.common;

	NObjectContext context = { game, *this };
	LieroNObjectFields fields = { toRust(&pos), toRust(&vel), &timeLeft, &curFrame };
	LieroNObjectType t = toRust(*type);
	LieroNObjectEnv env = { game.level.view(), common.levelCommon(), &game.rand, game.cycles, game.settings->shadow };
	LieroNObjectHooks hooks = {
		&context,
		hookCreateBObject,
		hookCreateSObject,
		hookCreateNObject2,
		hookDrawOnMap,
		hookHitWorms,
		hookFree,
	};

	liero_nobject_process(&fields, &t, &env, &hooks);
}

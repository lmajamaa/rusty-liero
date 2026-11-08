#include "weapon.hpp"
#include "game.hpp"
#include "mixer/player.hpp"
#include "math.hpp"
#include "gfx/renderer.hpp"
#include "constants.hpp"
#include "worm.hpp"

int computedLoadingTime(Weapon const& type, Settings& settings)
{
	int ret = (settings.loadingTime * type.loading_time) / 100;
	if(ret == 0)
		ret = 1;
	return ret;
}

LieroWeapon toRust(Weapon const& w)
{
	LieroWeapon r;
	r.id = w.id;
	r.detect_distance = w.detect_distance;
	r.affect_by_worm = w.affect_by_worm;
	r.blow_away = w.blow_away;
	r.gravity = w.gravity;
	r.explo_sound = w.explo_sound;
	r.speed = w.speed;
	r.add_speed = w.add_speed;
	r.distribution = w.distribution;
	r.parts = w.parts;
	r.recoil = w.recoil;
	r.mult_speed = w.mult_speed;
	r.delay = w.delay;
	r.loading_time = w.loading_time;
	r.ammo = w.ammo;
	r.create_on_exp = w.create_on_exp;
	r.dirt_effect = w.dirt_effect;
	r.leave_shells = w.leave_shells;
	r.leave_shell_delay = w.leave_shell_delay;
	r.play_reload_sound = w.play_reload_sound;
	r.expl_ground = w.expl_ground;
	r.fire_cone = w.fire_cone;
	r.collide_with_objects = w.collide_with_objects;
	r.bounce = w.bounce;
	r.time_to_explo = w.time_to_explo;
	r.time_to_explo_v = w.time_to_explo_v;
	r.start_frame = w.start_frame;
	r.num_frames = w.num_frames;
	r.loop_anim = w.loop_anim;
	r.shot_type = w.shot_type;
	r.color_bullets = w.color_bullets;
	r.splinter_amount = w.splinter_amount;
	r.splinter_colour = w.splinter_colour;
	r.splinter_type = w.splinter_type;
	r.splinter_scatter = w.splinter_scatter;
	r.obj_trail_type = w.obj_trail_type;
	r.obj_trail_delay = w.obj_trail_delay;
	r.part_trail_type = w.part_trail_type;
	r.part_trail_obj = w.part_trail_obj;
	r.part_trail_delay = w.part_trail_delay;
	r.loop_sound = w.loop_sound;
	r.laser_sight = w.laser_sight;
	return r;
}

void fireWeapon(Game& game, Weapon const& type, int angle, fixedvec vel, int speed, fixedvec pos, int ownerIdx, WormWeapon* ww)
{
	WObject* obj = game.wobjects.newObjectReuse();

	obj->type = &type;
	obj->pos = pos;
	obj->ownerIdx = ownerIdx;

	// STATS
	obj->firedBy = ww;
	obj->hasHit = false;

	Worm* owner = game.wormByIdx(ownerIdx);
	game.statsRecorder->damagePotential(owner, ww, type.hit_damage);
	game.statsRecorder->shot(owner, ww);

	LieroWeapon w = toRust(type);
	LieroLaunch launch = liero_weapon_launch(&w, &game.rand, angle, toRust(vel), speed);
	obj->vel = fromRust(launch.vel);
	obj->curFrame = launch.cur_frame;
	obj->timeLeft = launch.time_left;
}

namespace
{

LieroWObjectEnv wobjectEnv(Game& game)
{
	Common& common = *game.common;
	LieroWObjectEnv env = {
		game.level.view(), common.levelCommon(), &game.rand, game.cycles, game.settings->shadow,
		LC(SplinterLarpaVelDiv), LC(SplinterCracklerVelDiv),
	};
	return env;
}

// What liero_wobject_blow_up calls back for. The object is already freed.
struct BlowUpContext
{
	Game& game;
	WObject& obj;
	int causeIdx;
};

void blowUpCreateSObject(void* ctx, int32_t kind, int32_t x, int32_t y)
{
	auto& c = *static_cast<BlowUpContext*>(ctx);
	createSObject(c.game, c.game.common->sobjectTypes[kind], x, y, c.causeIdx, c.obj.firedBy, &c.obj);
}

void blowUpPlaySound(void* ctx, int32_t sound)
{
	auto& c = *static_cast<BlowUpContext*>(ctx);
	c.game.soundPlayer->play(sound);
}

void blowUpCreateNObject1(void* ctx, int32_t kind, LieroIVec2 vel, LieroIVec2 pos, int32_t color)
{
	auto& c = *static_cast<BlowUpContext*>(ctx);
	createNObject1(c.game, c.game.common->nobjectTypes[kind], fromRust(vel), fromRust(pos), color, c.causeIdx, c.obj.firedBy);
}

void blowUpCreateNObject2(void* ctx, int32_t kind, int32_t angle, LieroIVec2 vel, LieroIVec2 pos, int32_t color)
{
	auto& c = *static_cast<BlowUpContext*>(ctx);
	createNObject2(c.game, c.game.common->nobjectTypes[kind], angle, fromRust(vel), fromRust(pos), color, c.causeIdx, c.obj.firedBy);
}

} // namespace

void WObject::blowUpObject(Game& game, int causeIdx)
{
	LieroIVec2 p = toRust(pos);
	LieroIVec2 v = toRust(vel);

	game.wobjects.free(this);

	LieroWeapon w = toRust(*type);
	LieroWObjectEnv env = wobjectEnv(game);
	BlowUpContext context = { game, *this, causeIdx };
	LieroBlowUpHooks hooks = {
		&context, blowUpCreateSObject, blowUpPlaySound, blowUpCreateNObject1, blowUpCreateNObject2,
	};

	liero_wobject_blow_up(&w, p, v, &env, &hooks);
}

namespace
{

// What liero_wobject_process calls back for.
struct WObjectContext
{
	Game& game;
	WObject& obj;
	Worm* owner;
};

bool hookOwnerSteers(void* ctx)
{
	auto& c = *static_cast<WObjectContext*>(ctx);
	return c.owner->visible && c.owner->pressed(Worm::Up);
}

void hookCreateSObject(void* ctx, int32_t kind, int32_t x, int32_t y)
{
	auto& c = *static_cast<WObjectContext*>(ctx);
	createSObject(c.game, c.game.common->sobjectTypes[kind], x, y, c.obj.ownerIdx, c.obj.firedBy);
}

void hookCreateNObject1(void* ctx, int32_t kind, LieroIVec2 vel, LieroIVec2 pos, int32_t color)
{
	auto& c = *static_cast<WObjectContext*>(ctx);
	createNObject1(c.game, c.game.common->nobjectTypes[kind], fromRust(vel), fromRust(pos), color, c.obj.ownerIdx, c.obj.firedBy);
}

void hookCreateNObject2(void* ctx, int32_t kind, int32_t angle, LieroIVec2 vel, LieroIVec2 pos, int32_t color)
{
	auto& c = *static_cast<WObjectContext*>(ctx);
	createNObject2(c.game, c.game.common->nobjectTypes[kind], angle, fromRust(vel), fromRust(pos), color, c.obj.ownerIdx, c.obj.firedBy);
}

void hookPushObjects(void* ctx, LieroIVec2 impulseRust)
{
	auto& c = *static_cast<WObjectContext*>(ctx);
	Game& game = c.game;
	WObject& o = c.obj;
	fixedvec impulse = fromRust(impulseRust);
	fixedvec pos = o.pos;

	auto wr = game.wobjects.all();
	for (WObject* i; (i = wr.next()); )
	{
		if(i->type != o.type
		|| i->ownerIdx != o.ownerIdx)
		{
			if(pos.x >= i->pos.x - itof(2)
			&& pos.x <= i->pos.x + itof(2)
			&& pos.y >= i->pos.y - itof(2)
			&& pos.y <= i->pos.y + itof(2))
			{
				i->vel += impulse;
			}
		}
	}

	auto nr = game.nobjects.all();
	for (NObject* i; (i = nr.next()); )
	{
		if(pos.x >= i->pos.x - itof(2)
		&& pos.x <= i->pos.x + itof(2)
		&& pos.y >= i->pos.y - itof(2)
		&& pos.y <= i->pos.y + itof(2))
		{
			i->vel += impulse;
		}
	}
}

uint32_t hookHitWorms(void* ctx)
{
	auto& c = *static_cast<WObjectContext*>(ctx);
	Game& game = c.game;
	Common& common = *game.common;
	WObject& o = c.obj;
	Weapon const& w = *o.type;
	uint32_t result = 0;

	for(std::size_t i = 0; i < game.worms.size(); ++i)
	{
		Worm& worm = *game.worms[i];

		if((w.hit_damage || w.blow_away || w.blood_on_hit || w.worm_collide)
		&& checkForSpecWormHit(game, ftoi(o.pos.x), ftoi(o.pos.y), w.detect_distance, worm))
		{
			worm.vel += o.vel * w.blow_away / 100;

			game.doDamage(worm, w.hit_damage, o.ownerIdx);
			game.statsRecorder->damageDealt(c.owner, o.firedBy, &worm, w.hit_damage, o.hasHit);
			if (!o.hasHit)
				game.statsRecorder->hit(c.owner, o.firedBy, &worm);
			o.hasHit = true;

			int bloodAmount = w.blood_on_hit * game.settings->blood / 100;

			for(int i = 0; i < bloodAmount; ++i)
			{
				int angle = game.rand(128);
				createNObject2(game, common.nobjectTypes[6], angle, o.vel / 3, o.pos, 0, worm.index, o.firedBy);
			}

			if(w.hit_damage > 0
			&& worm.health > 0
			&& game.rand(3) == 0)
			{
				int snd = game.rand(3) + 18; // NOTE: MUST be outside the unpredictable branch below
				if(!game.soundPlayer->isPlaying(&worm))
				{
					game.soundPlayer->play(snd, &worm);
				}
			}

			if(w.worm_collide)
			{
				if(game.rand(w.worm_collide) == 0)
				{
					if(w.worm_explode)
						result |= LIERO_HIT_EXPLODE;

					result |= LIERO_HIT_REMOVE;
				}
			}
		}
	}

	return result;
}

void hookBlowUp(void* ctx)
{
	auto& c = *static_cast<WObjectContext*>(ctx);
	c.obj.blowUpObject(c.game, c.obj.ownerIdx);
}

void hookFree(void* ctx)
{
	auto& c = *static_cast<WObjectContext*>(ctx);
	c.game.wobjects.free(&c.obj);
}

bool hookUsed(void* ctx)
{
	auto& c = *static_cast<WObjectContext*>(ctx);
	return c.obj.used;
}

} // namespace

void WObject::process(Game& game)
{
	Common& common = *game.common;
	Worm* owner = game.wormByIdx(ownerIdx);

	// As liero would do this while rendering, we try to do it as early as possible
	if(common.H[HRemExp]
	&& type - &common.weapons[0] == LC(RemExpObject) - 1)
	{
		if(owner->pressed(Worm::Change)
		&& owner->pressed(Worm::Fire))
		{
			timeLeft = 0;
		}
	}

	WObjectContext context = { game, *this, owner };
	LieroWObjectFields fields = { toRust(&pos), toRust(&vel), &curFrame, &timeLeft };
	LieroWeapon w = toRust(*type);
	LieroWObjectEnv env = wobjectEnv(game);
	LieroWObjectHooks hooks = {
		&context,
		hookOwnerSteers,
		hookCreateSObject,
		hookCreateNObject1,
		hookCreateNObject2,
		hookPushObjects,
		hookHitWorms,
		hookBlowUp,
		hookFree,
		hookUsed,
	};

	liero_wobject_process(&fields, &w, &env, &hooks);
}

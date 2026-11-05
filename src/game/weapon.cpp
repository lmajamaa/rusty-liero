#include "weapon.hpp"
#include "game.hpp"
#include "mixer/player.hpp"
#include "math.hpp"
#include "gfx/renderer.hpp"
#include "constants.hpp"
#include "worm.hpp"

int Weapon::computedLoadingTime(Settings& settings) const
{
	int ret = (settings.loadingTime * loadingTime) / 100;
	if(ret == 0)
		ret = 1;
	return ret;
}

LieroWeapon toRust(Weapon const& w)
{
	LieroWeapon r;
	r.id = w.id;
	r.detect_distance = w.detectDistance;
	r.affect_by_worm = w.affectByWorm;
	r.blow_away = w.blowAway;
	r.gravity = w.gravity;
	r.explo_sound = w.exploSound;
	r.speed = w.speed;
	r.add_speed = w.addSpeed;
	r.distribution = w.distribution;
	r.parts = w.parts;
	r.recoil = w.recoil;
	r.mult_speed = w.multSpeed;
	r.delay = w.delay;
	r.loading_time = w.loadingTime;
	r.ammo = w.ammo;
	r.create_on_exp = w.createOnExp;
	r.dirt_effect = w.dirtEffect;
	r.leave_shells = w.leaveShells;
	r.leave_shell_delay = w.leaveShellDelay;
	r.play_reload_sound = w.playReloadSound;
	r.expl_ground = w.explGround;
	r.fire_cone = w.fireCone;
	r.collide_with_objects = w.collideWithObjects;
	r.bounce = w.bounce;
	r.time_to_explo = w.timeToExplo;
	r.time_to_explo_v = w.timeToExploV;
	r.start_frame = w.startFrame;
	r.num_frames = w.numFrames;
	r.loop_anim = w.loopAnim;
	r.shot_type = w.shotType;
	r.color_bullets = w.colorBullets;
	r.splinter_amount = w.splinterAmount;
	r.splinter_colour = w.splinterColour;
	r.splinter_type = w.splinterType;
	r.splinter_scatter = w.splinterScatter;
	r.obj_trail_type = w.objTrailType;
	r.obj_trail_delay = w.objTrailDelay;
	r.part_trail_type = w.partTrailType;
	r.part_trail_obj = w.partTrailObj;
	r.part_trail_delay = w.partTrailDelay;
	r.loop_sound = w.loopSound;
	r.laser_sight = w.laserSight;
	return r;
}

void Weapon::fire(Game& game, int angle, fixedvec vel, int speed, fixedvec pos, int ownerIdx, WormWeapon* ww) const
{
	WObject* obj = game.wobjects.newObjectReuse();

	obj->type = this;
	obj->pos = pos;
	obj->ownerIdx = ownerIdx;

	// STATS
	obj->firedBy = ww;
	obj->hasHit = false;

	Worm* owner = game.wormByIdx(ownerIdx);
	game.statsRecorder->damagePotential(owner, ww, hitDamage);
	game.statsRecorder->shot(owner, ww);

	LieroWeapon w = toRust(*this);
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
	c.game.common->sobjectTypes[kind].create(c.game, x, y, c.causeIdx, c.obj.firedBy, &c.obj);
}

void blowUpPlaySound(void* ctx, int32_t sound)
{
	auto& c = *static_cast<BlowUpContext*>(ctx);
	c.game.soundPlayer->play(sound);
}

void blowUpCreateNObject1(void* ctx, int32_t kind, LieroIVec2 vel, LieroIVec2 pos, int32_t color)
{
	auto& c = *static_cast<BlowUpContext*>(ctx);
	c.game.common->nobjectTypes[kind].create1(c.game, fromRust(vel), fromRust(pos), color, c.causeIdx, c.obj.firedBy);
}

void blowUpCreateNObject2(void* ctx, int32_t kind, int32_t angle, LieroIVec2 vel, LieroIVec2 pos, int32_t color)
{
	auto& c = *static_cast<BlowUpContext*>(ctx);
	c.game.common->nobjectTypes[kind].create2(c.game, angle, fromRust(vel), fromRust(pos), color, c.causeIdx, c.obj.firedBy);
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
	c.game.common->sobjectTypes[kind].create(c.game, x, y, c.obj.ownerIdx, c.obj.firedBy);
}

void hookCreateNObject1(void* ctx, int32_t kind, LieroIVec2 vel, LieroIVec2 pos, int32_t color)
{
	auto& c = *static_cast<WObjectContext*>(ctx);
	c.game.common->nobjectTypes[kind].create1(c.game, fromRust(vel), fromRust(pos), color, c.obj.ownerIdx, c.obj.firedBy);
}

void hookCreateNObject2(void* ctx, int32_t kind, int32_t angle, LieroIVec2 vel, LieroIVec2 pos, int32_t color)
{
	auto& c = *static_cast<WObjectContext*>(ctx);
	c.game.common->nobjectTypes[kind].create2(c.game, angle, fromRust(vel), fromRust(pos), color, c.obj.ownerIdx, c.obj.firedBy);
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

		if((w.hitDamage || w.blowAway || w.bloodOnHit || w.wormCollide)
		&& checkForSpecWormHit(game, ftoi(o.pos.x), ftoi(o.pos.y), w.detectDistance, worm))
		{
			worm.vel += o.vel * w.blowAway / 100;

			game.doDamage(worm, w.hitDamage, o.ownerIdx);
			game.statsRecorder->damageDealt(c.owner, o.firedBy, &worm, w.hitDamage, o.hasHit);
			if (!o.hasHit)
				game.statsRecorder->hit(c.owner, o.firedBy, &worm);
			o.hasHit = true;

			int bloodAmount = w.bloodOnHit * game.settings->blood / 100;

			for(int i = 0; i < bloodAmount; ++i)
			{
				int angle = game.rand(128);
				common.nobjectTypes[6].create2(game, angle, o.vel / 3, o.pos, 0, worm.index, o.firedBy);
			}

			if(w.hitDamage > 0
			&& worm.health > 0
			&& game.rand(3) == 0)
			{
				int snd = game.rand(3) + 18; // NOTE: MUST be outside the unpredictable branch below
				if(!game.soundPlayer->isPlaying(&worm))
				{
					game.soundPlayer->play(snd, &worm);
				}
			}

			if(w.wormCollide)
			{
				if(game.rand(w.wormCollide) == 0)
				{
					if(w.wormExplode)
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

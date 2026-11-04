#include "nobject.hpp"
#include "game.hpp"
#include "gfx/renderer.hpp"
#include "mixer/player.hpp"
#include "bobject.hpp"
#include "worm.hpp"
#include "gfx/blit.hpp"

NObject& NObjectType::create(Game& game, fixedvec vel, fixedvec pos, int color, int ownerIdx, WormWeapon* firedBy)
{
	NObject& obj = *game.nobjects.newObjectReuse();

	obj.type = this;
	obj.ownerIdx = ownerIdx;
	obj.pos = pos;

	obj.vel = vel;

	// STATS
	obj.firedBy = firedBy;
	obj.hasHit = false;

	Worm* owner = game.wormByIdx(ownerIdx);
	game.statsRecorder->damagePotential(owner, firedBy, hitDamage);

	if(startFrame > 0)
	{
		obj.curFrame = game.rand(numFrames + 1);
	}
	else if(color != 0)
	{
		obj.curFrame = color;
	}
	else
	{
		obj.curFrame = colorBullets;
	}

	obj.timeLeft = timeToExplo;

	if(timeToExploV)
		obj.timeLeft -= game.rand(timeToExploV);

	return obj;
}

NObject& NObjectType::create1(Game& game, fixedvec vel, fixedvec pos, int color, int ownerIdx, WormWeapon* firedBy)
{
	if(distribution)
	{
		vel.x += distribution - game.rand(distribution * 2);
		vel.y += distribution - game.rand(distribution * 2);
	}

	return create(game, vel, pos, color, ownerIdx, firedBy);
}

void NObjectType::create2(Game& game, int angle, fixedvec vel, fixedvec pos, int color, int ownerIdx, WormWeapon* firedBy)
{
	int realSpeed = speed - game.rand(speedV);

	vel += cossinTable[angle] * realSpeed / 100;

	// TODO: !REPLAYS Make the distributions use the same code
	if(distribution)
	{
		vel.x += game.rand(distribution * 2) - distribution;
		vel.y += game.rand(distribution * 2) - distribution;
	}

	auto& obj = create(game,
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
	c.game.common->sobjectTypes[kind].create(c.game, x, y, c.obj.ownerIdx, c.obj.firedBy);
}

void hookCreateNObject2(void* ctx, int32_t kind, int32_t angle, LieroIVec2 vel, LieroIVec2 pos, int32_t color)
{
	auto& c = *static_cast<NObjectContext*>(ctx);
	c.game.common->nobjectTypes[kind].create2(
		c.game, angle, fromRust(vel), fromRust(pos), color, c.obj.ownerIdx, 0);
}

void hookDrawOnMap(void* ctx, int32_t x, int32_t y)
{
	auto& c = *static_cast<NObjectContext*>(ctx);
	Common& common = *c.game.common;
	NObjectType const& t = *c.obj.type;

	blitImageOnMap(
		common,
		c.game.level,
		common.smallSprites.spritePtr(t.startFrame + c.obj.curFrame),
		x,
		y,
		7,
		7);
	// x and y are 3 left of and above the object. This seems like an overly
	// large rectangle.
	if(c.game.settings->shadow)
		correctShadow(common, c.game.level, gvl::rect(x - 5, y - 5, x + 12, y + 12));
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

		if(checkForSpecWormHit(game, ftoi(o.pos.x), ftoi(o.pos.y), t.detectDistance, w))
		{
			w.vel += o.vel * t.blowAway / 100;

			game.doDamage(w, t.hitDamage, o.ownerIdx);

			Worm* owner = game.wormByIdx(o.ownerIdx);
			game.statsRecorder->damageDealt(owner, o.firedBy, &w, t.hitDamage, o.hasHit);
			o.hasHit = true;

			if(t.hitDamage > 0
			&& w.health > 0
			&& game.rand(3) == 0)
			{
				int snd = 18 + game.rand(3); // NOTE: MUST be outside the unpredictable branch below
				if(!game.soundPlayer->isPlaying(&w))
				{
					game.soundPlayer->play(snd, &w);
				}
			}

			int blood = t.bloodOnHit * game.settings->blood / 100;

			for(int i = 0; i < blood; ++i)
			{
				int angle = game.rand(128);
				common.nobjectTypes[6].create2(
					game,
					angle,
					o.vel / 3,
					o.pos,
					0,
					o.ownerIdx,
					0);
			}

			if(t.wormExplode)
				doExplode = true;
			else if (t.wormDestroy && o.used)
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
	r.hit_damage = t.hitDamage;
	r.expl_ground = t.explGround;
	r.blood_trail = t.bloodTrail;
	r.blood_trail_delay = t.bloodTrailDelay;
	r.start_frame = t.startFrame;
	r.num_frames = t.numFrames;
	r.draw_on_map = t.drawOnMap;
	r.leave_obj = t.leaveObj;
	r.leave_obj_delay = t.leaveObjDelay;
	r.time_to_explo = t.timeToExplo;
	r.create_on_exp = t.createOnExp;
	r.dirt_effect = t.dirtEffect;
	r.splinter_amount = t.splinterAmount;
	r.splinter_colour = t.splinterColour;
	r.splinter_type = t.splinterType;
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

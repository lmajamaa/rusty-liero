#include "sobject.hpp"

#include "worm.hpp"
#include "game.hpp"
#include "viewport.hpp"
#include "gfx/blit.hpp"
#include "gfx/renderer.hpp"
#include "mixer/player.hpp"
#include "console.hpp"
#include "text.hpp"
#include <cmath>
#include <cstdlib>
#include <cassert>

void createSObject(Game& game, SObjectType const& type, int x, int y, int ownerIdx, WormWeapon* firedBy, WObject* from)
{
	Common& common = *game.common;
	SObject& obj = *game.sobjects.newObjectReuse();


	assert(type.num_sounds < 10);

	if(type.start_sound >= 0)
		game.soundPlayer->play(game.rand(type.num_sounds) + type.start_sound);

	for(std::size_t i = 0; i < game.viewports.size(); ++i)
	{
		Viewport& v = *game.viewports[i];

		if(x > v.x
		&& x < v.x + v.rect.width()
		&& y > v.y
		&& y < v.y + v.rect.height())
		{
			if(itof(type.shake) > v.shake)
				v.shake = itof(type.shake);
		}
	}

	obj.id = type.id;
	obj.x = x - 8;
	obj.y = y - 8;
	obj.curFrame = 0;
	obj.animDelay = type.anim_delay;

	if(type.flash > game.screenFlash)
	{
		game.screenFlash = type.flash;
	}

	Worm* owner = game.wormByIdx(ownerIdx);

	game.statsRecorder->damagePotential(owner, firedBy, type.damage);

	if(type.damage > 0)
	{
		for(std::size_t i = 0; i < game.worms.size(); ++i)
		{
			Worm& w = *game.worms[i];

			int powerSum;
			if(liero_explosion_push_worm(toRust(w.pos), toRust(&w.vel), x, y, type.detect_range, type.blow_away, &powerSum))
			{
				int z = liero_explosion_damage(type.damage, powerSum, type.detect_range);

				if (from && !from->hasHit)
				{
					game.statsRecorder->hit(owner, firedBy, &w);
					from->hasHit = true;
				}

				if(w.health > 0)
				{
					game.doDamage(w, z, ownerIdx);
					game.statsRecorder->damageDealt(owner, firedBy, &w, z, false);

					int bloodAmount = game.settings->blood * powerSum / 100;

					if(bloodAmount > 0)
					{
						for(int i = 0; i < bloodAmount; ++i)
						{
							int angle = game.rand(128);
							createNObject2(game, common.nobjectTypes[6],
								angle,
								w.vel / 3,
								w.pos,
								0,
								w.index,
								firedBy);
						}
					}

					if(game.rand(3) == 0)
					{
						int snd = 18 + game.rand(3); // NOTE: MUST be outside the unpredictable branch below
						if(!game.soundPlayer->isPlaying(&w))
						{
							game.soundPlayer->play(snd, &w);
						}
					}
				}
			}
		} // for( ... worms ...

		int objBlowAway = type.blow_away / 3; // TODO: Read from EXE

		auto wr = game.wobjects.all();
		for (WObject* i; (i = wr.next()); )
		{
			Weapon const& weapon = *i->type;

			if(weapon.affect_by_explosions
			&& liero_explosion_push_object(toRust(i->pos), toRust(&i->vel), x, y, type.detect_range, objBlowAway)
			&& weapon.chain_explosion)
			{
				i->blowUpObject(game, ownerIdx);
			} // if( ... affectByExplosions ...
		} // for( ... wobjects ...

		auto nr = game.nobjects.all();
		for (NObject* i; (i = nr.next()); )
		{
			NObjectType const& t = *i->type;

			if(t.affect_by_explosions)
				liero_explosion_push_object(toRust(i->pos), toRust(&i->vel), x, y, type.detect_range, objBlowAway);
		}

		{
			int width = type.detect_range / 2;

			Rect rect(x - width, y - width, x + width + 1, y + width + 1);

			rect.intersect(game.level.rect());

			for(int y = rect.y1; y < rect.y2; ++y)
			for(int x = rect.x1; x < rect.x2; ++x)
			{
				if(game.level.mat(x, y).anyDirt()
				&& game.rand(8) == 0)
				{
					PalIdx pix = game.level.pixel(x, y);
					int angle = game.rand(128);
					createNObject2(game, common.nobjectTypes[2],
						angle,
						fixedvec(),
						itof(IVec2(x, y)),
						pix, ownerIdx, firedBy);
				}
			}
		}

	} // if(damage ...

	if(type.dirt_effect >= 0)
	{
		drawDirtEffect(common, game.rand, game.level, type.dirt_effect, x - 7, y - 7);

		if(game.settings->shadow)
			correctShadow(common, game.level, Rect(x - 10, y - 10, x + 11, y + 11));
	}

	auto br = game.bonuses.all();
	for (Bonus* i; (i = br.next()); )
	{
		int ix = ftoi(i->x), iy = ftoi(i->y);

		if(ix > x - type.detect_range
		&& ix < x + type.detect_range
		&& iy > y - type.detect_range
		&& iy < y + type.detect_range)
		{
			game.bonuses.free(br);
			createSObject(game, common.sobjectTypes[0], ix, iy, ownerIdx, firedBy);
		}
	} // for( ... bonuses ...
}

void SObject::process(Game& game)
{
	Common& common = *game.common;
	SObjectType const& t = common.sobjectTypes[id];

	if(liero_sobject_animate(&curFrame, &animDelay, t.anim_delay, t.num_frames))
		game.sobjects.free(this);
}

// The TC as TOML, field by field, for the hash run's writer (hashtc.cpp).
#include "common.hpp"

template<typename T>
struct ObjectResolver
{
	ObjectResolver(Common& common, TcList<T> const& vec)
	: common(common)
	, vec(vec)
	{
	}

	template<typename Archive>
	void v2r(Archive& ar, int v)
	{
		if (v < 0)
			ar.null(0);
		else
			ar.str(0, vec[v].id_str);
	}

	Common& common;
	TcList<T> const& vec;
};

template<typename Archive>
void archive_text(Common& common, NObjectType const& nobject, Archive& ar)
{
	ar.obj(0, [&] {
		#define I(k, f) ar.i32(#k, nobject.f);
		#define B(k, f) ar.b(#k, nobject.f);
		#define S(k, f) ar.str(#k, nobject.f);
		#define NObj(k, f) ar.ref(#k, nobject.f, ObjectResolver<NObjectType>(common, common.nobjectTypes));
		#define SObj(k, f) ar.ref(#k, nobject.f, ObjectResolver<SObjectType>(common, common.sobjectTypes));

		B(wormExplode, worm_explode)
		B(explGround, expl_ground)
		B(wormDestroy, worm_destroy)
		B(drawOnMap, draw_on_map)
		B(affectByExplosions, affect_by_explosions)
		B(bloodTrail, blood_trail)

		I(detectDistance, detect_distance)
		I(gravity, gravity)
		I(speed, speed)
		I(speedV, speed_v)
		I(distribution, distribution)
		I(blowAway, blow_away)
		I(bounce, bounce)
		I(hitDamage, hit_damage)
		I(bloodOnHit, blood_on_hit)
		I(startFrame, start_frame)
		I(numFrames, num_frames)
		I(colorBullets, color_bullets)
		SObj(createOnExp, create_on_exp)
		I(dirtEffect, dirt_effect)
		I(splinterAmount, splinter_amount)
		I(splinterColour, splinter_colour)
		NObj(splinterType, splinter_type)
		I(bloodTrailDelay, blood_trail_delay)
		SObj(leaveObj, leave_obj)
		I(leaveObjDelay, leave_obj_delay)
		I(timeToExplo, time_to_explo)
		I(timeToExploV, time_to_explo_v)

		#undef I
		#undef B
		#undef S
		#undef NObj
		#undef SObj
	});
}

template<typename Archive>
void archive_text(Common& common, SObjectType const& sobject, Archive& ar)
{
	ar.obj(0, [&] {
		#define I(k, f) ar.i32(#k, sobject.f);
		#define B(k, f) ar.b(#k, sobject.f);
		#define S(k, f) ar.str(#k, sobject.f);

		B(shadow, shadow)
		I(startSound, start_sound)
		I(numSounds, num_sounds)
		I(animDelay, anim_delay)
		I(startFrame, start_frame)
		I(numFrames, num_frames)
		I(detectRange, detect_range)
		I(damage, damage)
		I(blowAway, blow_away)
		I(shake, shake)
		I(flash, flash)
		I(dirtEffect, dirt_effect)

		#undef I
		#undef B
		#undef S
	});
}

template<typename Archive>
void archive_text(Common& common, Weapon const& weapon, Archive& ar)
{
	ar.obj(0, [&] {
		#define I(k, f) ar.i32(#k, weapon.f);
		#define B(k, f) ar.b(#k, weapon.f);
		#define S(k, f) ar.str(#k, weapon.f);
		#define NObj(k, f) ar.ref(#k, weapon.f, ObjectResolver<NObjectType>(common, common.nobjectTypes));
		#define SObj(k, f) ar.ref(#k, weapon.f, ObjectResolver<SObjectType>(common, common.sobjectTypes));

		S(name, name)

		B(affectByWorm, affect_by_worm)
		B(shadow, shadow)
		B(laserSight, laser_sight)
		B(playReloadSound, play_reload_sound)
		B(wormExplode, worm_explode)
		B(explGround, expl_ground)
		B(wormCollide, worm_collide)
		B(collideWithObjects, collide_with_objects)
		B(affectByExplosions, affect_by_explosions)
		B(loopAnim, loop_anim)
		I(detectDistance, detect_distance)
		I(blowAway, blow_away)
		I(gravity, gravity)
		I(launchSound, launch_sound)
		I(loopSound, loop_sound)
		I(exploSound, explo_sound)
		I(speed, speed)
		I(addSpeed, add_speed)
		I(distribution, distribution)
		I(parts, parts)
		I(recoil, recoil)
		I(multSpeed, mult_speed)
		I(delay, delay)
		I(loadingTime, loading_time)
		I(ammo, ammo)
		I(dirtEffect, dirt_effect)
		I(leaveShells, leave_shells)
		I(leaveShellDelay, leave_shell_delay)
		I(fireCone, fire_cone)
		I(bounce, bounce)
		I(timeToExplo, time_to_explo)
		I(timeToExploV, time_to_explo_v)
		I(hitDamage, hit_damage)
		I(bloodOnHit, blood_on_hit)
		I(startFrame, start_frame)
		I(numFrames, num_frames)
		I(shotType, shot_type)
		I(colorBullets, color_bullets)
		I(splinterAmount, splinter_amount)
		I(splinterColour, splinter_colour)
		NObj(splinterType, splinter_type)
		I(splinterScatter, splinter_scatter)
		SObj(objTrailType, obj_trail_type)
		I(objTrailDelay, obj_trail_delay)
		I(partTrailType, part_trail_type)
		NObj(partTrailObj, part_trail_obj)
		I(partTrailDelay, part_trail_delay)
		SObj(createOnExp, create_on_exp)
		B(chainExplosion, chain_explosion)

		#undef I
		#undef B
		#undef S
		#undef NObj
		#undef SObj
	});
}

template<typename Archive>
void archive_text(Common& common, Archive& ar)
{
	ar.obj("types", [&] {

		ar.arr("sounds", common.sounds, [&] (SfxSample& s) {
			ar.str(0, s.name);
		});

		ar.arr("weapons", common.weapons, [&] (Weapon const& w) {
			ar.str(0, w.id_str);
		});

		ar.arr("nobjects", common.nobjectTypes, [&] (NObjectType const& n) {
			ar.str(0, n.id_str);
		});

		ar.arr("sobjects", common.sobjectTypes, [&] (SObjectType const& s) {
			ar.str(0, s.id_str);
		});

	});

	ar.obj("constants", [&] {
		int bonusIndexes[2] = {0, 1};

		ar.array_obj("bonuses", bonusIndexes, [&] (int idx) {
			ar.i32("timer", common.bonusRandTimer[idx][0]);
			ar.i32("timerV", common.bonusRandTimer[idx][1]);
			ar.i32("frame", common.bonusFrames[idx]);
			ar.ref("sobj", common.bonusSObjects[idx], ObjectResolver<SObjectType>(common, common.sobjectTypes));
		});

		ar.array_obj("textures", common.textures, [&] (Texture& tx) {
			ar.i32("mframe", tx.m_frame);
			ar.i32("rframe", tx.r_frame);
			ar.i32("sframe", tx.s_frame);
			ar.b("ndrawback", tx.n_draw_back);
		});

		ar.array_obj("colorAnim", common.colorAnim, [&] (ColourAnim& ca) {
			ar.i32("from", ca.from);
			ar.i32("to", ca.to);
		});

		ar.arr("materials", common.materials, [&] (Material& m) {
			int f = m.flags;
			ar.i32(0, f);
			m.flags = (uint8_t)(f & 0xff);
		});

		char const* names[7] = {
			"up", "down", "left", "right",
			"fire", "change", "jump"
		};

		ar.obj("aiparams", [&] () {
			for (int i = 0; i < 7; ++i)
			{
				ar.obj(names[i], [&] () {
					ar.i32("on", common.aiParams.k[1][i]);
					ar.i32("off", common.aiParams.k[0][i]);
				});
			}
		});


		#define A(n) ar.i32(#n, common.C[C##n]);
		LIERO_CDEFS(A)
		#undef A
	});

	ar.obj("texts", [&] {
		#define A(n) ar.str(#n, common.S[S##n]);
		LIERO_SDEFS(A)
		#undef A
	});

	ar.obj("hacks", [&] {
		#define A(n) ar.b(#n, common.H[H##n]);
		LIERO_HDEFS(A)
		#undef A
	});
}
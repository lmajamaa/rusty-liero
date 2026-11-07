#include <cstddef>
#include "worm.hpp"

#include <array>
#include <vector>
#include "game.hpp"
#include "mixer/player.hpp"
#include "gfx/renderer.hpp"
#include "constants.hpp"
#include "console.hpp"
#include "filesystem.hpp" // For joinPath
#include <cstdlib>

#include <gvl/serialization/context.hpp>
#include <gvl/serialization/archive.hpp>
#include "replay.hpp"

#include <gvl/crypt/gash.hpp>
#include <gvl/io2/fstream.hpp>


gvl::gash::value_type& WormSettings::updateHash()
{
	LieroWormSettings view = wormSettingsView(*this);
	SettingsResult r(liero_worm_settings_hash(&view));
	setWormSettings(*this, r.worm());
	hash = r.hash();
	return hash;
}

void WormSettings::saveProfile(FsNode node)
{
	try
	{
		//auto const& fullPath = path + ".lpf";
		//create_directories(fullPath);
		//gvl::sink str(new gvl::file_bucket_pipe(fullPath.c_str(), "wb"));

		//gvl::octet_writer writer(str);

		auto writer = node.toOctetWriter();

		//profilePath = path;
		profileNode = node;
		GameSerializationContext context;
		archive(gvl::out_archive<gvl::octet_writer, GameSerializationContext>(writer, context), *this);
	}
	catch(gvl::stream_error& e)
	{
		Console::writeWarning(std::string("Stream error saving profile: ") + e.what());
	}
}

void WormSettings::loadProfile(FsNode node)
{
	int oldColor = color;
	try
	{
		//gvl::source str(gvl::to_source(new gvl::file_bucket_pipe(path.c_str(), "rb")));

		//gvl::octet_reader reader(str);

		auto reader = node.toOctetReader();

		profileNode = node;
		GameSerializationContext context;
		archive(gvl::in_archive<gvl::octet_reader, GameSerializationContext>(reader, context), *this);
	}
	catch(gvl::stream_error& e)
	{
		Console::writeWarning(std::string("Stream error loading profile: ") + e.what());
		Console::writeWarning("The profile may just be old, in which case there is nothing to worry about");
	}

	color = oldColor;  // We preserve the color
}

namespace
{

// Rust works on the weapon slots, the rope and the respawn camera in place.
static_assert(sizeof(WormWeapon) == sizeof(LieroWormWeapon), "WormWeapon layout");
static_assert(offsetof(WormWeapon, type) == offsetof(LieroWormWeapon, kind), "WormWeapon layout");
static_assert(offsetof(WormWeapon, ammo) == offsetof(LieroWormWeapon, ammo), "WormWeapon layout");
static_assert(offsetof(WormWeapon, delayLeft) == offsetof(LieroWormWeapon, delay_left), "WormWeapon layout");
static_assert(offsetof(WormWeapon, loadingLeft) == offsetof(LieroWormWeapon, loading_left), "WormWeapon layout");
static_assert(Settings::selectableWeapons == LIERO_SELECTABLE_WEAPONS, "weapon count");
static_assert(sizeof(Worm::ControlState) == sizeof(uint32_t), "ControlState layout");
static_assert(Worm::Up == LIERO_CONTROL_UP && Worm::Down == LIERO_CONTROL_DOWN
	&& Worm::Left == LIERO_CONTROL_LEFT && Worm::Right == LIERO_CONTROL_RIGHT
	&& Worm::Fire == LIERO_CONTROL_FIRE && Worm::Change == LIERO_CONTROL_CHANGE
	&& Worm::Jump == LIERO_CONTROL_JUMP, "control bits");
static_assert(Settings::GMKillEmAll == LIERO_GAME_KILL_EM_ALL
	&& Settings::GMGameOfTag == LIERO_GAME_GAME_OF_TAG
	&& Settings::GMHoldazone == LIERO_GAME_HOLDAZONE
	&& Settings::GMScalesOfJustice == LIERO_GAME_SCALES_OF_JUSTICE, "game modes");

// What liero_worm_process, liero_worm_fire and liero_worm_process_weapons
// call back for.
struct WormContext
{
	WormContext(Game& game, Worm& worm)
	: game(game), worm(worm)
	, bonuses(game.bonuses.all())
	, wobjects(game.wobjects.all())
	{
	}

	Game& game;
	Worm& worm;
	Game::BonusList::range bonuses;
	Game::WObjectList::range wobjects;
};

WormContext& context(void* ctx)
{
	return *static_cast<WormContext*>(ctx);
}

void hookLaunchSound(void* ctx)
{
	auto& c = context(ctx);
	Worm& worm = c.worm;
	Weapon const& w = *worm.weapons[worm.currentWeapon].type;

	if(w.launchSound >= 0)
	{
		if(w.loopSound)
		{
			if(!c.game.soundPlayer->isPlaying(&worm.weapons[worm.currentWeapon]))
			{
				c.game.soundPlayer->play(w.launchSound, &worm.weapons[worm.currentWeapon], -1);
			}
		}
		else
		{
			c.game.soundPlayer->play(w.launchSound);
		}
	}
}

void hookFirePart(void* ctx, int32_t angle, LieroIVec2 vel, int32_t speed, LieroIVec2 pos)
{
	auto& c = context(ctx);
	Worm& worm = c.worm;
	WormWeapon& ww = worm.weapons[worm.currentWeapon];
	ww.type->fire(c.game, angle, fromRust(vel), speed, fromRust(pos), worm.index, &ww);
}

void hookPlaySound(void* ctx, int32_t sound)
{
	context(ctx).game.soundPlayer->play(sound);
}

void hookCreateNObject1(void* ctx, int32_t kind, LieroIVec2 vel, LieroIVec2 pos, int32_t color)
{
	auto& c = context(ctx);
	c.game.common->nobjectTypes[kind].create1(c.game, fromRust(vel), fromRust(pos), color, c.worm.index, 0);
}

LieroWeapon hookWeapon(void*, void const* kind)
{
	return toRust(*static_cast<Weapon const*>(kind));
}

void hookGiveWeapon(void* ctx, int32_t slot, int32_t weapon)
{
	auto& c = context(ctx);
	WormWeapon& ww = c.worm.weapons[slot];
	ww.type = &c.game.common->weapons[weapon];
	ww.ammo = ww.type->ammo;
}

void hookBonusesBegin(void* ctx)
{
	auto& c = context(ctx);
	c.bonuses = c.game.bonuses.all();
}

LieroBonusRef hookBonusNext(void* ctx)
{
	LieroBonusRef r = {};
	if(Bonus* b = context(ctx).bonuses.next())
	{
		r.found = true;
		r.x = b->x;
		r.y = b->y;
		r.frame = b->frame;
		r.weapon = b->weapon;
	}
	return r;
}

void hookBonusFree(void* ctx)
{
	auto& c = context(ctx);
	c.game.bonuses.free(c.bonuses);
}

void hookHeal(void* ctx, int32_t amount)
{
	auto& c = context(ctx);
	c.game.doHealing(c.worm, amount);
}

void hookCreateSObject(void* ctx, int32_t kind, int32_t x, int32_t y)
{
	auto& c = context(ctx);
	c.game.common->sobjectTypes[kind].create(c.game, x, y, c.worm.index, 0);
}

void hookCreateNObject2(void* ctx, int32_t kind, int32_t angle, LieroIVec2 vel, LieroIVec2 pos, int32_t color)
{
	auto& c = context(ctx);
	c.game.common->nobjectTypes[kind].create2(c.game, angle, fromRust(vel), fromRust(pos), color, c.worm.index, 0);
}

void hookPlayWormSound(void* ctx, int32_t sound)
{
	auto& c = context(ctx);
	c.game.soundPlayer->play(sound, &c.worm);
}

bool hookWormSoundPlaying(void* ctx)
{
	auto& c = context(ctx);
	return c.game.soundPlayer->isPlaying(&c.worm);
}

void hookStopLoopSound(void* ctx)
{
	auto& c = context(ctx);
	c.game.soundPlayer->stop(&c.worm.weapons[c.worm.currentWeapon]);
}

bool hookWormHit(void* ctx, int32_t x, int32_t y)
{
	auto& c = context(ctx);
	return checkForWormHit(c.game, x, y, 0, &c.worm);
}

void hookSteerablesBegin(void* ctx)
{
	auto& c = context(ctx);
	c.wobjects = c.game.wobjects.all();
}

LieroSteerable hookSteerableNext(void* ctx)
{
	auto& c = context(ctx);
	Weapon const* type = c.worm.weapons[c.worm.currentWeapon].type;
	LieroSteerable r = {};
	while(WObject* i = c.wobjects.next())
	{
		if(i->type == type && i->ownerIdx == c.worm.index)
		{
			r.cur_frame = &i->curFrame;
			r.pos = toRust(i->pos);
			break;
		}
	}
	return r;
}

bool hookEnemyPos(void* ctx, LieroIVec2* pos)
{
	auto& c = context(ctx);
	if(c.game.worms.size() != 2)
		return false;
	*pos = toRust(ftoi(c.game.worms[c.worm.index ^ 1]->pos));
	return true;
}

void hookAddKill(void* ctx, int32_t idx)
{
	++context(ctx).game.wormByIdx(idx)->kills;
}

void hookAfterDeath(void* ctx)
{
	auto& c = context(ctx);
	c.game.statsRecorder->afterDeath(&c.worm);
}

void hookAfterSpawn(void* ctx)
{
	auto& c = context(ctx);
	c.game.statsRecorder->afterSpawn(&c.worm);
}

LieroWormConstants wormConstants(Common& common)
{
	LieroWormConstants k;
	k.min_bounce_up = LC(MinBounceUp);
	k.min_bounce_down = LC(MinBounceDown);
	k.min_bounce_left = LC(MinBounceLeft);
	k.min_bounce_right = LC(MinBounceRight);
	k.worm_gravity = LC(WormGravity);
	k.walk_vel_left = LC(WalkVelLeft);
	k.max_vel_left = LC(MaxVelLeft);
	k.walk_vel_right = LC(WalkVelRight);
	k.max_vel_right = LC(MaxVelRight);
	k.jump_force = LC(JumpForce);
	k.max_aim_vel_left = LC(MaxAimVelLeft);
	k.aim_acc_left = LC(AimAccLeft);
	k.max_aim_vel_right = LC(MaxAimVelRight);
	k.aim_acc_right = LC(AimAccRight);
	k.nr_initial_length = LC(NRInitialLength);
	k.nr_min_length = LC(NRMinLength);
	k.nr_max_length = LC(NRMaxLength);
	k.nr_throw_vel_x = LC(NRThrowVelX);
	k.nr_throw_vel_y = LC(NRThrowVelY);
	k.nr_pull_vel = LC(NRPullVel);
	k.nr_release_vel = LC(NRReleaseVel);
	k.worm_fric_mult = LC(WormFricMult);
	k.worm_fric_div = LC(WormFricDiv);
	k.worm_min_spawn_dist_last = LC(WormMinSpawnDistLast);
	k.worm_min_spawn_dist_enemy = LC(WormMinSpawnDistEnemy);
	k.worm_spawn_rect_x = LC(WormSpawnRectX);
	k.worm_spawn_rect_y = LC(WormSpawnRectY);
	k.worm_spawn_rect_w = LC(WormSpawnRectW);
	k.worm_spawn_rect_h = LC(WormSpawnRectH);
	k.aim_fric_mult = LC(AimFricMult);
	k.aim_fric_div = LC(AimFricDiv);
	k.aim_max_right = LC(AimMaxRight);
	k.aim_min_right = LC(AimMinRight);
	k.aim_max_left = LC(AimMaxLeft);
	k.aim_min_left = LC(AimMinLeft);
	k.bonus_explode_risk = LC(BonusExplodeRisk);
	k.bonus_health_var = LC(BonusHealthVar);
	k.bonus_min_health = LC(BonusMinHealth);
	k.laser_weapon = LC(LaserWeapon);
	k.fall_damage_right = LC(FallDamageRight);
	k.fall_damage_down = LC(FallDamageDown);
	k.worm_float_level = LC(WormFloatLevel);
	k.worm_float_power = LC(WormFloatPower);
	k.fall_damage = common.H[HFallDamage];
	k.worm_float = common.H[HWormFloat];
	k.bonus_reload_only = common.H[HBonusReloadOnly];
	k.air_jump = common.H[HAirJump];
	k.multi_jump = common.H[HMultiJump];
	k.signed_recoil = common.H[HSignedRecoil];
	return k;
}

} // namespace

// The Rust types the fields refer to are laid out like the C++ ones.
static_assert(sizeof(Ninjarope) == sizeof(LieroNinjarope), "Ninjarope layout");
static_assert(sizeof(WormWeapon) == sizeof(LieroWormWeapon), "WormWeapon layout");
static_assert(sizeof(Worm::ControlState) == sizeof(uint32_t), "ControlState layout");

Worm::Worm(LieroWormData* d)
: data(*d)
, pos(reinterpret_cast<fixedvec&>(d->pos))
, vel(reinterpret_cast<fixedvec&>(d->vel))
, logicRespawn(reinterpret_cast<gvl::ivec2&>(d->logic_respawn))
, hotspotX(d->hotspot_x), hotspotY(d->hotspot_y)
, aimingAngle(d->aiming_angle), aimingSpeed(d->aiming_speed)
, ableToJump(d->able_to_jump), ableToDig(d->able_to_dig)
, keyChangePressed(d->key_change_pressed)
, movable(d->movable)
, animate(d->animate)
, visible(d->visible)
, ready(d->ready)
, flag(d->flag)
, makeSightGreen(d->make_sight_green)
, health(d->health)
, lives(d->lives)
, kills(d->kills)
, timer(d->timer)
, killedTimer(d->killed_timer)
, currentFrame(d->current_frame)
, flags(d->flags)
, ninjarope(reinterpret_cast<Ninjarope&>(d->ninjarope))
, currentWeapon(d->current_weapon)
, lastKilledByIdx(d->last_killed_by_idx)
, fireCone(d->fire_cone)
, leaveShellTimer(d->leave_shell_timer)
, index(d->index)
, reacts(d->reacts)
, weapons(reinterpret_cast<WormWeapon(&)[5]>(d->weapons))
, direction(d->direction)
, controlStates(reinterpret_cast<ControlState&>(d->control_states))
, prevControlStates(reinterpret_cast<ControlState&>(d->prev_control_states))
, steerableSumX(d->steerable_sum_x), steerableSumY(d->steerable_sum_y)
, steerableCount(d->steerable_count)
, statsX(d->stats_x)
, cleanControlStates(reinterpret_cast<ControlState&>(d->clean_control_states))
{
}

void Worm::process(Game& game)
{
	Common& common = *game.common;
	WormContext context(game, *this);

	LieroWormConstants k = wormConstants(common);

	LieroWormEnv env = {
		game.level.view(), common.levelCommon(), game.rust(), int32_t(game.cycles),
		int32_t(game.settings->gameMode), game.settings->blood, game.settings->loadingTime,
		game.settings->loadChange, game.settings->shadow, game.quickSim,
		{ Settings::wormAnimTab[0], Settings::wormAnimTab[1], Settings::wormAnimTab[2], Settings::wormAnimTab[3] },
	};

	LieroWormFiringHooks firing = { &context, hookLaunchSound, hookFirePart, hookPlaySound, hookCreateNObject1 };

	LieroWormHooks hooks = {
		&context, &firing, hookWeapon, hookGiveWeapon,
		hookBonusesBegin, hookBonusNext, hookBonusFree, hookHeal,
		hookCreateSObject, hookCreateNObject2,
		hookPlayWormSound, hookWormSoundPlaying, hookStopLoopSound, hookWormHit,
		hookSteerablesBegin, hookSteerableNext, hookEnemyPos, hookAddKill,
		hookAfterDeath, hookAfterSpawn,
	};

	liero_worm_process(&data, settings->health, &k, &env, &hooks);
}

int Worm::angleFrame() const
{
	return liero_worm_angle_frame(aimingAngle, direction);
}

int sqrVectorLength(int x, int y)
{
	return x*x + y*y;
}

// In Rust (rust/liero-game/src/ai/dumb.rs).
void DumbLieroAI::process(Game& game, Worm& worm)
{
	Common& common = *game.common;
	Weapon const& w = *worm.weapons[worm.currentWeapon].type;

	LieroDumbAiWorm me;
	me.pos[0] = worm.pos.x;
	me.pos[1] = worm.pos.y;
	me.visible = worm.visible;
	me.aiming_angle = worm.aimingAngle;
	me.direction = worm.direction;
	for (int i = 0; i < 4; ++i)
		me.reacts[i] = worm.reacts[i];
	me.rope_out = worm.ninjarope.out;
	me.rope_attached = worm.ninjarope.attached;
	me.control_states = worm.controlStates.pack();
	me.time_to_explo = w.timeToExplo;
	me.time_to_explo_v = w.timeToExploV;
	me.speed = w.speed;
	me.gravity = w.gravity;

	std::vector<std::array<int32_t, 2>> others;
	for (Worm* o : game.worms)
	{
		if (o != &worm)
			others.push_back({{ o->pos.x, o->pos.y }});
	}

	worm.controlStates.istate = liero_dumb_ai_process(
		&rand, &me, reinterpret_cast<int32_t const (*)[2]>(others.data()), others.size(),
		reinterpret_cast<int32_t const (*)[2][7]>(&common.aiParams.k));
}

void Worm::initWeapons(Game& game)
{
	Common& common = *game.common;
	currentWeapon = 0; // It was 1 in OpenLiero A1

	for(int j = 0; j < Settings::selectableWeapons; ++j)
	{
		WormWeapon& ww = weapons[j];
		ww.type = &common.weapons[common.weapOrder[settings->weapons[j] - 1]];
		ww.ammo = ww.type->ammo;
		ww.delayLeft = 0;
		ww.loadingLeft = 0;
	}
}

bool checkForWormHit(Game& game, int x, int y, int dist, Worm* ownWorm)
{
	for(std::size_t i = 0; i < game.worms.size(); ++i)
	{
		Worm& w = *game.worms[i];

		if(&w != ownWorm)
		{
			return checkForSpecWormHit(game, x, y, dist, w);
		}
	}

	return false;
}

bool checkForSpecWormHit(Game& game, int x, int y, int dist, Worm& w)
{
	Common& common = *game.common;

	if(!w.visible)
		return false;

	PalIdx* wormSprite = common.wormSprite(w.currentFrame, w.direction, 0);

	int deltaX = x - ftoi(w.pos.x) + 7;
	int deltaY = y - ftoi(w.pos.y) + 5;

	gvl::rect r(deltaX - dist, deltaY - dist, deltaX + dist + 1, deltaY + dist + 1);

	r.intersect(gvl::rect(0, 0, 16, 16));

	for(int cy = r.y1; cy < r.y2; ++cy)
	for(int cx = r.x1; cx < r.x2; ++cx)
	{
		assert(cy*16 + cx < 16*16);
		if(common.materials[wormSprite[cy*16 + cx]].worm())
			return true;
	}

	return false;
}

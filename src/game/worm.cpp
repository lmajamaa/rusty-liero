#include <cstddef>
#include "worm.hpp"

#include <algorithm>
#include <array>
#include <vector>
#include "game.hpp"
#include "mixer/player.hpp"
#include "gfx/renderer.hpp"
#include "constants.hpp"
#include "console.hpp"
#include "filesystem.hpp" // For joinPath
#include <cstdlib>

#include "replay.hpp"



SettingsHash& WormSettings::updateHash()
{
	LieroWormSettings view = wormSettingsView(*this);
	SettingsResult r(liero_worm_settings_hash(&view));
	setWormSettings(*this, r.worm());
	hash = r.hash();
	return hash;
}

void WormSettings::saveProfile(FsNode node)
{
	// Profiles use the archive of the current replay version.
	LieroWormSettings view = wormSettingsView(*this);
	LieroSettingsResult* saved = node.rust()
		? liero_worm_settings_save_profile(&view, GameSerializationContext().replayVersion, node.rust())
		: nullptr;
	if (!saved)
		throw std::runtime_error("Could not write " + node.fullPath());
	SettingsResult r(saved);
	profileNode = node;
	setWormSettings(*this, r.worm());
}

void WormSettings::loadProfile(FsNode node)
{
	int oldColor = color;
	LieroWormSettings view = wormSettingsView(*this);
	LieroSettingsResult* loaded = node.rust()
		? liero_worm_settings_load_profile(&view, GameSerializationContext().replayVersion, node.rust())
		: nullptr;
	if (!loaded)
		throw std::runtime_error("Could not read " + node.fullPath());
	SettingsResult r(loaded);
	profileNode = node;
	setWormSettings(*this, r.worm());
	// A profile that ends early, or fails its checkpoint, throws before the
	// colour is put back. Nothing catches it: the warnings the C++ had for
	// stream errors were never reached.
	throwArchiveError(r.status(), nullptr);

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

} // namespace

// The Rust types the fields refer to are laid out like the C++ ones.
static_assert(sizeof(Ninjarope) == sizeof(LieroNinjarope), "Ninjarope layout");
static_assert(sizeof(WormWeapon) == sizeof(LieroWormWeapon), "WormWeapon layout");
static_assert(sizeof(Worm::ControlState) == sizeof(uint32_t), "ControlState layout");

Worm::Worm(LieroWormData* d)
: data(*d)
, pos(reinterpret_cast<fixedvec&>(d->pos))
, vel(reinterpret_cast<fixedvec&>(d->vel))
, logicRespawn(reinterpret_cast<IVec2&>(d->logic_respawn))
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

int Worm::angleFrame() const
{
	return liero_worm_angle_frame(aimingAngle, direction);
}

int sqrVectorLength(int x, int y)
{
	return x*x + y*y;
}

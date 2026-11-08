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



SettingsHash WormSettings::updateHash()
{
	LieroWormSettings view = wormSettingsView(*this);
	SettingsResult r(liero_worm_settings_hash(&view));
	setWormSettings(*this, r.worm());
	return r.hash();
}

// In Rust, which remembers the profile in the settings' box.
void WormSettings::saveProfile(FsNode node)
{
	if (!liero_worm_settings_box_save_profile(box, node.rust()))
		throw std::runtime_error("Could not write " + node.fullPath());
}

void WormSettings::loadProfile(FsNode node)
{
	LieroSettingsStatus status = LieroSettingsStatus_Ok;
	if (!liero_worm_settings_box_load_profile(box, node.rust(), &status))
		throw std::runtime_error("Could not read " + node.fullPath());
	// A profile that ends early, or fails its checkpoint, throws with the
	// profile's colour. Nothing catches it: the warnings the C++ had for
	// stream errors were never reached.
	throwArchiveError(status, nullptr);
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

Worm::Worm(LieroWorm* w)
: Worm(w, liero_worm_data(w))
{
}

Worm::Worm(LieroWorm* w, LieroWormData* d)
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
, handle(w)
{
	if (LieroWormSettingsBox* b = liero_worm_settings(w))
		settings = WormSettings::share(b);
}

int Worm::angleFrame() const
{
	return liero_worm_angle_frame(aimingAngle, direction);
}

int sqrVectorLength(int x, int y)
{
	return x*x + y*y;
}

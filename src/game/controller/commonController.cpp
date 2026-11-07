
#include "commonController.hpp"

#include "../gfx.hpp"

static_assert(sizeof(GameState) == sizeof(int32_t), "GameState layout");
static_assert(LIERO_STATE_INITIAL == StateInitial, "game states");
static_assert(LIERO_STATE_WEAPON_SELECTION == StateWeaponSelection, "game states");
static_assert(LIERO_STATE_GAME == StateGame, "game states");
static_assert(LIERO_STATE_GAME_ENDED == StateGameEnded, "game states");

CommonController::CommonController()
: data(*liero_controller_data_new())
, state(reinterpret_cast<GameState&>(data.state))
, fadeValue(data.fade_value)
, goingToMenu(data.going_to_menu)
, frameSkip(data.frame_skip)
, inverseFrameSkip(data.inverse_frame_skip)
, cycles(data.cycles)
{
}

CommonController::~CommonController()
{
	liero_controller_data_free(&data);
}

bool CommonController::process()
{
	// 1 to 0 choose the frame skip, with ctrl inverted, in Rust.
	liero_controller_process(&data, &gfx.dosKeys);
	return true;
}

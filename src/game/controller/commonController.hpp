#ifndef LIERO_CONTROLLER_COMMON_CONTROLLER_HPP
#define LIERO_CONTROLLER_COMMON_CONTROLLER_HPP

#include "controller.hpp"
#include "../game.hpp"

// What the controllers share. Rust owns its state (LieroControllerData in
// rust/liero-game/src/controller.rs); the fields here refer to it.
struct CommonController : Controller
{
	CommonController();
	CommonController(CommonController const&) = delete;
	CommonController& operator=(CommonController const&) = delete;
	~CommonController();

	bool process();

	// The controller's state in Rust.
	LieroControllerData* rust() { return &data; }

private:
	LieroControllerData& data;

public:
	GameState& state;
	int& fadeValue;
	bool& goingToMenu;
	int& frameSkip;
	bool& inverseFrameSkip;
	int& cycles;
};

#endif // LIERO_CONTROLLER_COMMON_CONTROLLER_HPP


#include "commonController.hpp"

#include "../gfx.hpp"

CommonController::CommonController()
: frameSkip(1)
, inverseFrameSkip(false)
, cycles(0)
{
}

bool CommonController::process()
{
	// 1 to 0 choose the frame skip, with ctrl inverted (in Rust).
	liero_keys_frame_skip(&gfx.dosKeys, &frameSkip, &inverseFrameSkip);

	++cycles;

	return true;
}

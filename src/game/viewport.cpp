#include "viewport.hpp"
#include "game.hpp"
#include "gfx/renderer.hpp"

// Viewports are processed and drawn in Rust (rust/liero-game/src/viewport.rs),
// which builds what they draw from the game (scene.rs).

static_assert(LIERO_GAME_KILL_EM_ALL == Settings::GMKillEmAll, "game modes");
static_assert(LIERO_GAME_GAME_OF_TAG == Settings::GMGameOfTag, "game modes");
static_assert(LIERO_GAME_HOLDAZONE == Settings::GMHoldazone, "game modes");
static_assert(LIERO_GAME_SCALES_OF_JUSTICE == Settings::GMScalesOfJustice, "game modes");

#include "viewport.hpp"
#include "spectatorviewport.hpp"
#include "game.hpp"
#include "gfx/renderer.hpp"

// Viewports are processed and drawn in Rust (rust/liero-game/src/viewport.rs),
// which builds what they draw from the game (scene.rs).

namespace
{

void draw(Viewport& vp, int kind, Game& game, Renderer& renderer, GameState gameState, bool isReplay)
{
	Settings& s = *game.settings;
	GameView view(game);
	LieroDrawSettings d;
	d.names_on_bonuses = s.namesOnBonuses;
	d.allow_viewing_spawn_point = s.allowViewingSpawnPoint;
	d.map = s.map;
	d.render_res_x = renderer.renderResX;
	d.render_res_y = renderer.renderResY;
	d.state_game = gameState == StateGame;
	d.is_replay = isReplay;
	LieroBitmap scr = renderer.bmp.view();
	liero_game_draw_viewport(&view.fields, &view.settings, &d, vp.rust(), kind, &scr);
}

}

static_assert(LIERO_GAME_KILL_EM_ALL == Settings::GMKillEmAll, "game modes");
static_assert(LIERO_GAME_GAME_OF_TAG == Settings::GMGameOfTag, "game modes");
static_assert(LIERO_GAME_HOLDAZONE == Settings::GMHoldazone, "game modes");
static_assert(LIERO_GAME_SCALES_OF_JUSTICE == Settings::GMScalesOfJustice, "game modes");

void Viewport::draw(Game& game, Renderer& renderer, GameState state, bool isReplay)
{
	::draw(*this, LIERO_VIEWPORT_WORM, game, renderer, state, isReplay);
}

void SpectatorViewport::draw(Game& game, Renderer& renderer, GameState state, bool isReplay)
{
	::draw(*this, LIERO_VIEWPORT_SPECTATOR, game, renderer, state, isReplay);
}

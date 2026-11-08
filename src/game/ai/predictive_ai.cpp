#include "predictive_ai.hpp"

#include "game/game.hpp"
#include "game/gfx/renderer.hpp"

#include <algorithm>

void FollowAI::process(Game& game, Worm& worm)
{
	auto place = std::find(game.worms.begin(), game.worms.end(), &worm) - game.worms.begin();
	Settings const& s = *game.settings;
	LieroAiSettings aiSettings = { s.aiFrames, s.aiMutations, s.aiTraces };
	GameView view(game);
	worm.controlStates.istate = liero_follow_ai_process(
		ai, &view.fields, &view.settings, &aiSettings, int32_t(place));
}

void FollowAI::drawDebug(Game& game, Worm const& worm, Renderer& renderer, int offsX, int offsY)
{
	std::size_t count;
	LieroAiTrace const* traces = liero_follow_ai_traces(ai, &count);
	for (std::size_t i = 0; i < count; ++i)
		renderer.bmp.setPixel(ftoi(traces[i].x) + offsX, ftoi(traces[i].y) + offsY, traces[i].color);
}

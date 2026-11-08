#ifndef LIERO_PREDICTIVE_AI_HPP
#define LIERO_PREDICTIVE_AI_HPP

#include "../worm.hpp"
#include <liero_game.h>

// The predictive bot, in Rust (rust/liero-game/src/ai/predictive.rs). It
// plays plans out on copies of the game, which Rust makes and runs too.
struct FollowAI : WormAI
{
	// `candidates` plans, as many as the aiParallels setting. `testing` also
	// tries changing to loading weapons; the game gives it to the first
	// worm's AI.
	FollowAI(int candidates, bool testing)
	: ai(liero_follow_ai_new(candidates, testing))
	{
	}

	~FollowAI()
	{
		liero_follow_ai_free(ai);
	}

	FollowAI(FollowAI const&) = delete;
	FollowAI& operator=(FollowAI const&) = delete;

	void process(Game& game, Worm& worm);

	// Draws the positions the plans passed through, with aiTraces on.
	void drawDebug(Game& game, Worm const& worm, Renderer& renderer, int offsX, int offsY);

	LieroFollowAi* ai;
};

#endif // LIERO_PREDICTIVE_AI_HPP

#include "replayController.hpp"

#include "../game.hpp"
#include "stats_presenter.hpp"
#include "../viewport.hpp"
#include "../sfx.hpp"

ReplayController::ReplayController(
	std::shared_ptr<Common> common, FsNode const& node)
: replay(new ReplayReader(node))
, common(common)
{
}

void ReplayController::onKey(int key, bool keyState)
{
	if(key == DkEscape && !goingToMenu)
	{
		fadeValue = 31;
		goingToMenu = true;
	}
}

// Called when the controller loses focus. When not focused, it will not receive key events among other things.
void ReplayController::unfocus()
{
}

// Called when the controller gets focus.
void ReplayController::focus()
{
	if(state == StateGameEnded)
	{
		goingToMenu = true;
		fadeValue = 0;
		return;
	}
	if(state == StateInitial)
	{
		try
		{
			game = replay->beginPlayback(common, std::shared_ptr<SoundPlayer>(new DefaultSoundPlayer(*common)));
		}
		catch(std::runtime_error& e)
		{
			gfx.infoBox(std::string("Error starting replay playback: ") + e.what());
			goingToMenu = true;
			fadeValue = 0;
			return;
		}
		// Changing state first when game is available
		changeState(StateGame);
	}
	game->focus(gfx.playRenderer);
	game->focus(gfx.singleScreenRenderer);
	goingToMenu = false;
	fadeValue = 0;
}

bool ReplayController::process()
{
	if(liero_controller_in_game(rust()))
	{
		if(gfx.testSDLKeyOnce(SDL_SCANCODE_R))
		{
			game->assign(*initialGame);
			replay->setPos(initialPos);
		}

		int ticks = liero_controller_ticks(rust());
		for(int i = 0; i < ticks && liero_controller_in_game(rust()); ++i)
		{
			if(replay.get())
			{
				try
				{
					if (!replay->playbackFrame(*gfx.primaryRenderer))
					{
						// End of replay
						replay.reset();
					}
				}
				catch(ArchiveCheckError& e)
				{
					gfx.infoBox(std::string("Archive error in replay: ") + e.what());
					changeState(StateGameEnded);
					replay.reset();
				}
			}
			game->processFrame();

			if(goingToMenu)
			{
				if(fadeValue > 0)
					fadeValue -= 1;
				else
					break;
			}
			else if(fadeValue < 33)
			{
				fadeValue += 1;
			}
		}

		if(game->isGameOver())
		{
			changeState(StateGameEnded);
		}
	}

	CommonController::process();

	if (goingToMenu && fadeValue <= 0)
	{
		if (state == StateGameEnded)
		{
			game->statsRecorder->finish(*game);
			presentStats(static_cast<NormalStatsRecorder&>(*game->statsRecorder), *game);
		}
		return false;
	}

	if (!replay.get() && state == StateGame)
	{
		game->statsRecorder->finish(*game);
		presentStats(static_cast<NormalStatsRecorder&>(*game->statsRecorder), *game);
		return false;
	}

	return true;
}

void ReplayController::draw(Renderer& renderer, bool useSpectatorViewports)
{
	if(state == StateGame || state == StateGameEnded)
	{
		game->draw(renderer, state, useSpectatorViewports, true);
	}
	renderer.fadeValue = fadeValue;
}

void ReplayController::changeState(GameState newState)
{
	if(state == newState)
		return;

	if(newState == StateGame)
	{
		// FIXME: the viewports are changed based on the replay for some
		// reason, so we need to restore them here. Probably makes more sense
		// to not save the viewports at all. But that probably breaks save
		// format compatibility?
		game->clearViewports();

		// for backwards compatibility reasons, this is not stored within the
		// replay. Yet.
		game->worms[0]->statsX = 0;
		game->worms[1]->statsX = 218;

		// spectator viewport is always full size
		// +68 on x to align the viewport in the middle
		game->addSpectatorViewport(Rect(0, 0, 504 + 68, 350), 504, 350);
		if (gfx.settings->singleScreenReplay)
		{
			// on single screen replay, use the spectator viewport for the
			// main screen as well
			game->addViewport(Rect(0, 0, 504 + 68, 350), 0, 504, 350, LIERO_VIEWPORT_SPECTATOR);
		}
		else
		{
			game->addViewport(Rect(0, 0, 158, 158), game->worms[0]->index, 504, 350);
			game->addViewport(Rect(160, 0, 158+160, 158), game->worms[1]->index, 504, 350);
		}
		game->startGame();
		initialGame = game->clone();
		initialPos = replay->pos();
	}
	else if(newState == StateGameEnded)
	{
		if(!goingToMenu)
		{
			fadeValue = 180;
			goingToMenu = true;
		}
	}

	state = newState;
}

void ReplayController::swapLevel(Level& newLevel)
{
	currentLevel()->swap(newLevel);
}

Level* ReplayController::currentLevel()
{
	if(game.get() && replay.get())
		return &game->level;
	return nullptr;
}

Game* ReplayController::currentGame()
{
	return game.get();
}


bool ReplayController::running()
{
	return state != StateGameEnded && state != StateInitial;
}

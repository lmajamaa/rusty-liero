
#include "localController.hpp"

#include "ticks.hpp"
#include "stats_presenter.hpp"
#include "../keys.hpp"
#include "../gfx.hpp"
#include "../sfx.hpp"
#include "../filesystem.hpp"

#include "../worm.hpp"
#include "../spectatorviewport.hpp"
#include "../viewport.hpp"

#include <cctype>

LocalController::LocalController(std::shared_ptr<Common> common, std::shared_ptr<Settings> settings)
: game(common, settings, std::shared_ptr<SoundPlayer>(new DefaultSoundPlayer(*common)))
{
	Worm* worm1 = new Worm();
	worm1->settings = settings->wormSettings[0];
	worm1->health = worm1->settings->health;
	worm1->index = 0;
	worm1->statsX = 0;
	worm1->ai = createWormAi(worm1->settings->controller, settings->aiParallels, worm1->index == 0);

	Worm* worm2 = new Worm();
	worm2->settings = settings->wormSettings[1];
	worm2->health = worm2->settings->health;
	worm2->index = 1;
	worm2->statsX = 218;
	worm2->ai = createWormAi(worm2->settings->controller, settings->aiParallels, worm2->index == 0);

#if 0
	for(int i = 0; i < 10; ++i)
	{
		Worm* worm2 = new Worm(*this);
		worm2->settings = settings->wormSettings[1];
		worm2->health = worm2->settings->health;
		worm2->index = 1;
		if(worm2->settings->controller == 1)
			worm2->ai.reset(new DumbLieroAI(*worm2));

		addWorm(worm2);
	}
#endif

	game.addViewport(new Viewport(Rect(0, 0, 158, 158), worm1->index, 504, 350));
	game.addViewport(new Viewport(Rect(160, 0, 158+160, 158), worm2->index, 504, 350));

	game.addWorm(worm1);
	game.addWorm(worm2);

	// +68 on x to align the viewport in the middle
	game.addSpectatorViewport(new SpectatorViewport(Rect(0, 0, 504 + 68, 350), 504, 350));
}

LocalController::~LocalController()
{
	endRecord();
}

void LocalController::onKey(int key, bool keyState)
{
	// Which worm's control the key is, and what it does, is in Rust
	// (liero_local_controller_key).
	std::vector<LieroWormKeys> worms;
	for (Worm* w : game.worms)
	{
		LieroWormKeys k;
		k.controls = game.settings->extensions ? w->settings->controlsEx : w->settings->controls;
		k.states = &w->controlStates.istate;
		k.clean = &w->cleanControlStates.istate;
		worms.push_back(k);
	}
	std::size_t controlCount = game.settings->extensions ? WormSettings::MaxControlEx : WormSettings::MaxControl;
	bool escape = liero_local_controller_key(worms.data(), worms.size(), controlCount, key, keyState);

	if(escape && !goingToMenu)
	{
		fadeValue = 31;
		goingToMenu = true;
	}
}

// Called when the controller loses focus. When not focused, it will not receive key events among other things.
void LocalController::unfocus()
{
	if(replay.get())
		replay->unfocus();
	if(state == StateWeaponSelection)
		ws->unfocus();
}

// Called when the controller gets focus.
void LocalController::focus()
{
	if(state == StateGameEnded)
	{
		goingToMenu = true;
		fadeValue = 0;
		return;
	}
	if(state == StateWeaponSelection)
		ws->focus();
	if(replay.get())
		replay->focus();
	if(state == StateInitial)
		changeState(StateWeaponSelection);
	game.focus(gfx.playRenderer);
	// FIXME rewrite the focus function to avoid nonsense like this?
	game.focus(gfx.singleScreenRenderer);
	goingToMenu = false;
	fadeValue = 0;
}

bool LocalController::process()
{
	if(state == StateWeaponSelection)
	{
		if(ws->processFrame())
			changeState(StateGame);
	}
	else if(liero_controller_in_game(rust()))
	{
		int ticks = liero_controller_ticks(rust());
		for(int i = 0; i < ticks && liero_controller_in_game(rust()); ++i)
		{
			game.processAis(true);
			if(replay.get())
			{
				try
				{
					replay->recordFrame();
				}
				catch(std::runtime_error& e)
				{
					Console::writeWarning(std::string("Error recording replay frame: ") + e.what());
					Console::writeWarning("Replay recording aborted");
					replay.reset();
				}
			}
			game.processFrame();

			if(game.isGameOver())
			{
				changeState(StateGameEnded);
			}
		}
	}

	//CommonController::process();

	if(goingToMenu)
	{
		if(fadeValue > 0)
			fadeValue -= 1;
		else
		{
			if(state == StateGameEnded)
			{
				endRecord();
				game.statsRecorder->finish(game);
				// TODO: Get rid of cast.
				presentStats(static_cast<NormalStatsRecorder&>(*game.statsRecorder), game);
			}
			return false;
		}
	}
	else
	{
		if(fadeValue < 33)
		{
			fadeValue += 1;
		}
	}

	return true;
}

void LocalController::draw(Renderer& renderer, bool useSpectatorViewports)
{
	if(state == StateWeaponSelection)
	{
		ws->draw(renderer, state, useSpectatorViewports);
	}
	else if(state == StateGame || state == StateGameEnded || state == StateInitial)
	{
		game.draw(renderer, state, useSpectatorViewports);
	}
	renderer.fadeValue = fadeValue;
}

void LocalController::changeState(GameState newState)
{
	if(state == newState)
		return;

	// NOTE: We prepare new state before destroying the old.
	// e.g. weapon selection is destroyed first after we successfully
	// started recording.

	// NOTE: Must do this here before starting recording!
	if(state == StateWeaponSelection)
	{
		ws->finalize();
	}

	if(newState == StateWeaponSelection)
	{
		ws.reset(new WeaponSelection(game));
	}
	else if(newState == StateGame)
	{
		// NOTE: This must be done before the replay recording starts below
		for(std::size_t i = 0; i < game.worms.size(); ++i)
		{
			Worm& worm = *game.worms[i];
			worm.lives = game.settings->lives;
		}

		if(game.settings->extensions && game.settings->recordReplays)
		{
			try
			{
				std::time_t ticks = appTime();
				// The hash run names replays the same in every time zone.
				std::tm* now = gfx.driver ? std::gmtime(&ticks) : std::localtime(&ticks);

				char buf[512];
				std::strftime(buf, sizeof(buf), "%Y-%m-%d %H.%M.%S", now);

				std::string playerNames = " ";
				for(std::size_t i = 0; i < 2; ++i)
				{
					Worm& worm = *game.worms[i];
					std::string const name = worm.settings->name.str();
					int chars = 0;

					if(i > 0)
						playerNames.push_back('-');
					for(std::size_t c = 0; c < name.size() && chars < 4; ++c, ++chars)
					{
						unsigned char ch = (unsigned char)name[c];
						if(std::isalnum(ch))
							playerNames.push_back(ch);
					}
				}
				//std::string path = joinPath(joinPath(configRoot, "Replays"), prefix + buf);
				//create_directories(path);

				auto node = gfx.getConfigNode() / "Replays" / (buf + playerNames + ".lrp");

				replay.reset(new ReplayWriter(node));

				replay->beginRecord(game);
			}
			catch(std::runtime_error& e)
			{
				gfx.infoBox(std::string("Error starting replay recording: ") + e.what());
				goingToMenu = true;
				fadeValue = 0;
				return;
			}
		}

		game.startGame();
	}
	else if(newState == StateGameEnded)
	{
		if(!goingToMenu)
		{
			fadeValue = 180;
			goingToMenu = true;
		}
	}

	if(state == StateWeaponSelection)
	{
		fadeValue = 33;
		ws.reset();
	}

	state = newState;
}

void LocalController::endRecord()
{
	if(replay.get())
	{
		replay.reset();
	}
}

void LocalController::swapLevel(Level& newLevel)
{
	currentLevel()->swap(newLevel);
}

Level* LocalController::currentLevel()
{
	return &game.level;
}

Game* LocalController::currentGame()
{
	return &game;
}

bool LocalController::running()
{
	return state != StateGameEnded && state != StateInitial;
}

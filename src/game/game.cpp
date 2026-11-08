#include <SDL.h>
#include <cstdlib>
#include <ctime>

#include "game.hpp"

#include <utility>
#include "appclock.hpp"
#include "viewport.hpp"
#include "spectatorviewport.hpp"
#include "worm.hpp"
#include "filesystem.hpp"
#include "gfx/renderer.hpp"
#include "weapsel.hpp"
#include "constants.hpp"
#include <cstddef>
#include <cassert>

GameMembers::GameMembers(
	std::shared_ptr<Common> common,
	std::shared_ptr<Settings> settings,
	std::shared_ptr<SoundPlayer> soundPlayer)
: common(common)
, soundPlayer(soundPlayer)
, settings(settings)
, statsRecorder(new NormalStatsRecorder)
{
}

// The Rust types the fields refer to are laid out like the C++ ones.
static_assert(sizeof(Rand) == sizeof(LieroRand), "Rand layout");
static_assert(sizeof(Holdazone) == sizeof(LieroHoldazone), "Holdazone layout");
static_assert(sizeof(Bonus) == sizeof(LieroBonus), "Bonus layout");
static_assert(sizeof(WObject) == sizeof(LieroWObject), "WObject layout");
static_assert(sizeof(SObject) == sizeof(LieroSObject), "SObject layout");
static_assert(sizeof(NObject) == sizeof(LieroNObject), "NObject layout");
static_assert(sizeof(BObject) == sizeof(LieroBObject), "BObject layout");
static_assert(sizeof(Game::BonusList) == sizeof(LieroBonusList), "bonus list layout");
static_assert(sizeof(Game::WObjectList) == sizeof(LieroWObjectList), "wobject list layout");
static_assert(sizeof(Game::SObjectList) == sizeof(LieroSObjectList), "sobject list layout");
static_assert(sizeof(Game::NObjectList) == sizeof(LieroNObjectList), "nobject list layout");
static_assert(sizeof(Game::BObjectList) == sizeof(LieroBObjectList), "blood list layout");

template<typename... Members>
Game::Game(LieroGameData* d, Members&&... members)
: GameMembers(std::forward<Members>(members)...)
, data(*d)
, screenFlash(d->screen_flash)
, gotChanged(d->got_changed)
, lastKilledIdx(d->last_killed_idx)
, paused(d->paused)
, cycles(d->cycles)
, rand(static_cast<Rand&>(d->rand))
, holdazone(reinterpret_cast<Holdazone&>(d->holdazone))
, quickSim(d->quick_sim)
, bonuses(reinterpret_cast<BonusList&>(d->bonuses))
, wobjects(reinterpret_cast<WObjectList&>(d->wobjects))
, sobjects(reinterpret_cast<SObjectList&>(d->sobjects))
, nobjects(reinterpret_cast<NObjectList&>(d->nobjects))
, bobjects(reinterpret_cast<BObjectList&>(d->bobjects))
{
}

Game::Game(
	std::shared_ptr<Common> common,
	std::shared_ptr<Settings> settingsInit,
	std::shared_ptr<SoundPlayer> soundPlayer)
: Game(liero_game_data_new(), common, settingsInit, soundPlayer)
{
	rand.seed(uint32_t(appTime()));
}

Game::Game(Game const& other)
: Game(liero_game_data_clone(&other.data), static_cast<GameMembers const&>(other))
{
}

Game& Game::operator=(Game const& other)
{
	GameMembers::operator=(other);
	liero_game_data_assign(&data, &other.data);
	return *this;
}

Game::~Game()
{
	clearViewports();
	clearWorms();
	liero_game_data_free(&data);
}

void Game::onKey(uint32_t key, bool state)
{
	for(std::size_t i = 0; i < worms.size(); ++i)
	{
		Worm& w = *worms[i];

		for(std::size_t control = 0; control < WormSettings::MaxControl; ++control)
		{
			if(w.settings->controls[control] == key)
			{
				w.setControlState(static_cast<Worm::Control>(control), state);
			}
		}
	}
}

void Game::releaseControls()
{
	for(std::size_t i = 0; i < worms.size(); ++i)
	{
		Worm& w = *worms[i];

		for(std::size_t control = 0; control < WormSettings::MaxControl; ++control)
		{
			w.release(static_cast<Worm::Control>(control));
		}
	}
}

void Game::clearViewports()
{
	viewports.clear();
	spectatorViewports.clear();
}

void Game::addViewport(Viewport* vp)
{
	//vp->worm->viewport = vp;
	viewports.push_back(vp);
}

void Game::addSpectatorViewport(SpectatorViewport* vp)
{
	spectatorViewports.push_back(vp);
}

void Game::processViewports()
{
	for(std::size_t i = 0; i < viewports.size(); ++i)
	{
		viewports[i]->process(*this);
	}
	for(std::size_t i = 0; i < spectatorViewports.size(); ++i)
	{
		spectatorViewports[i]->process(*this);
	}

}

void Game::drawViewports(Renderer& renderer, GameState state, bool isReplay)
{
	for(std::size_t i = 0; i < viewports.size(); ++i)
	{
		viewports[i]->draw(*this, renderer, state, isReplay);
	}
}

void Game::drawSpectatorViewports(Renderer& renderer, GameState state, bool isReplay)
{
	for(std::size_t i = 0; i < spectatorViewports.size(); ++i)
	{
		spectatorViewports[i]->draw(*this, renderer, state, isReplay);
	}
}


void Game::clearWorms()
{
	worms.clear();
}

void Game::resetWorms()
{
	for(std::size_t i = 0; i < worms.size(); ++i)
	{
		Worm& w = *worms[i];
		w.health = w.settings->health;
		w.lives = settings->lives; // Not in the original!
		w.kills = 0;
		w.visible = false;
		w.killedTimer = 150;

		w.currentWeapon = 1;
	}
}

void Game::addWorm(Worm* worm)
{
	worms.push_back(worm);
}

void Game::draw(Renderer& renderer, GameState state, bool useSpectatorViewports, bool isReplay)
{
	if (useSpectatorViewports)
	{
		drawSpectatorViewports(renderer, state, isReplay);
	}
	else
	{
		drawViewports(renderer, state, isReplay);
	}

	//common->font.drawText(toString(cycles / 70), 10, 10, 7);

	renderer.pal = renderer.origpal;

	for(int w = 0; w < 4; ++w)
		renderer.pal.rotateFrom(renderer.origpal, common->colorAnim[w].from, common->colorAnim[w].to, cycles >> 3);

	renderer.pal.fade(renderer.fadeValue);

	if(screenFlash > 0)
	{
		renderer.pal.lightUp(screenFlash);
	}
}


GameView::GameView(Game& game)
{
	Common& common = *game.common;
	Settings& s = *game.settings;

	for (Worm* w : game.worms)
	{
		LieroGameWorm v = { w->rust(), w->settings->health, w->ai.get() };
		worms.push_back(v);
	}

	fields.worms = worms.data();
	fields.worm_count = int32_t(worms.size());
	fields.game = game.rust();
	fields.level = game.level.view();
	fields.tc = common.tc ? liero_tc_data(common.tc) : nullptr;
	for (Viewport* v : game.viewports)
		viewports.push_back(v->rust());
	for (SpectatorViewport* v : game.spectatorViewports)
		spectatorViewports.push_back(v->rust());
	fields.viewports = viewports.data();
	fields.viewport_count = int32_t(viewports.size());
	fields.spectator_viewports = spectatorViewports.data();
	fields.spectator_viewport_count = int32_t(spectatorViewports.size());
	fields.stats = game.statsRecorder ? game.statsRecorder->s : nullptr;
	fields.sound = game.soundPlayer->out();

	settings.game_mode = int32_t(s.gameMode);
	settings.max_bonuses = s.maxBonuses;
	settings.time_to_lose = s.timeToLose;
	settings.zone_timeout = s.zoneTimeout;
	settings.zone_capture_time = Settings::zoneCaptureTime;
	settings.weap_table = s.weapTable;
	settings.weapon_count = int32_t(common.weapons.size());
	for (int f = 0; f < 2; ++f)
		for (int i = 0; i < 2; ++i)
			settings.bonus_rand_timer[f][i] = common.bonusRandTimer[f][i];
	settings.bonus_spawn_rect_x = LC(BonusSpawnRectX);
	settings.bonus_spawn_rect_y = LC(BonusSpawnRectY);
	settings.bonus_spawn_rect_w = LC(BonusSpawnRectW);
	settings.bonus_spawn_rect_h = LC(BonusSpawnRectH);
	settings.bonus_drop_chance = LC(BonusDropChance);
	settings.bonus_spawn_rect = common.H[HBonusSpawnRect];
	settings.bonus_only_health = common.H[HBonusOnlyHealth];
	settings.bonus_only_weapon = common.H[HBonusOnlyWeapon];
	settings.bonus_disable = common.H[HBonusDisable];
	settings.blood = s.blood;
	settings.shadow = s.shadow;
	settings.loading_time = s.loadingTime;
	settings.load_change = s.loadChange;
}

namespace
{

// Rust works on these in place.
static_assert(sizeof(Rect) == sizeof(LieroRect), "rect layout");
static_assert(offsetof(Rect, x1) == offsetof(LieroRect, x1) && offsetof(Rect, y1) == offsetof(LieroRect, y1)
	&& offsetof(Rect, x2) == offsetof(LieroRect, x2) && offsetof(Rect, y2) == offsetof(LieroRect, y2), "rect layout");
static_assert(sizeof(Holdazone) == sizeof(LieroHoldazone), "Holdazone layout");
static_assert(sizeof(Bonus) == sizeof(LieroBonus), "Bonus layout");
static_assert(sizeof(WObject) == sizeof(LieroWObject), "WObject layout");
static_assert(sizeof(SObject) == sizeof(LieroSObject), "SObject layout");
static_assert(sizeof(NObject) == sizeof(LieroNObject), "NObject layout");
static_assert(sizeof(BObject) == sizeof(LieroBObject), "BObject layout");
static_assert(sizeof(Game::BonusList) == sizeof(LieroBonusList), "bonus list layout");
static_assert(sizeof(Game::WObjectList) == sizeof(LieroWObjectList), "wobject list layout");
static_assert(sizeof(Game::SObjectList) == sizeof(LieroSObjectList), "sobject list layout");
static_assert(sizeof(Game::NObjectList) == sizeof(LieroNObjectList), "nobject list layout");
static_assert(sizeof(Game::BObjectList) == sizeof(LieroBObjectList), "blood list layout");
static_assert(offsetof(Holdazone, rect) == offsetof(LieroHoldazone, rect), "Holdazone layout");
static_assert(offsetof(Holdazone, holderIdx) == offsetof(LieroHoldazone, holder_idx), "Holdazone layout");
static_assert(offsetof(Holdazone, contenderIdx) == offsetof(LieroHoldazone, contender_idx), "Holdazone layout");
static_assert(offsetof(Holdazone, contenderFrames) == offsetof(LieroHoldazone, contender_frames), "Holdazone layout");
static_assert(offsetof(Holdazone, timeoutLeft) == offsetof(LieroHoldazone, timeout_left), "Holdazone layout");
static_assert(offsetof(Holdazone, zoneWidth) == offsetof(LieroHoldazone, zone_width), "Holdazone layout");
static_assert(offsetof(Holdazone, zoneHeight) == offsetof(LieroHoldazone, zone_height), "Holdazone layout");
static_assert(sizeof(Settings::weapTable[0]) == sizeof(uint32_t), "weapTable type");

Game& gameOf(void* ctx)
{
	return *static_cast<Game*>(ctx);
}

uint64_t hookHiresTicks(void*)
{
	return appHiresTicks();
}

void hookEndTick(void* ctx)
{
	Game& game = gameOf(ctx);
	game.processViewports();
}

} // namespace

void Game::processFrame()
{
	GameView view(*this);
	LieroGameHooks hooks = {
		this, hookHiresTicks, hookEndTick,
	};
	liero_game_process_frame(&view.fields, &view.settings, &hooks);
}

void Game::processAis(bool timed)
{
	GameView view(*this);
	LieroGameHooks hooks = {
		this, hookHiresTicks, hookEndTick,
	};
	LieroAiSettings ai = { settings->aiFrames, settings->aiMutations, settings->aiTraces };
	liero_game_process_ais(&view.fields, &view.settings, &ai, timed ? &hooks : nullptr);
}

void Game::focus(Renderer& renderer)
{
	updateSettings(renderer);
}

void Game::updateSettings(Renderer& renderer)
{
	renderer.origpal = level.origpal; // Activate the Level palette

	for(std::size_t i = 0; i < worms.size(); ++i)
	{
		Worm& worm = *worms[i];
		if(worm.index >= 0 && worm.index < 2)
			renderer.origpal.setWormColour(worm.index, *worm.settings);
	}
}

void Game::spawnZone()
{
	GameView view(*this);
	liero_game_spawn_zone(&view.fields, &view.settings);
}

void Game::startGame()
{
	soundPlayer->play(22);
	bobjects.resize(settings->bloodParticleMax);

	if (settings->gameMode == Settings::GMHoldazone)
	{
		spawnZone();
	}
}

void NormalStatsRecorder::finish(Game& game)
{
	GameView view(game);
	liero_game_stats_finish(&view.fields);
}

bool Game::isGameOver()
{
	GameView view(*this);
	return liero_game_is_over(&view.fields, &view.settings);
}

void Game::postClone(Game& original, bool complete)
{
	if (!complete)
	{
		statsRecorder.reset();
		soundPlayer.reset(new NullSoundPlayer);
		viewports.clear();
	}
	else
	{
		if (statsRecorder)
			statsRecorder.reset(new NormalStatsRecorder(*statsRecorder));

		for (auto& vp : viewports)
		{
			vp = new Viewport(*vp);
		}
	}

	for (auto& w : worms)
	{
		w = new Worm(*w);
	}

}

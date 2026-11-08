#include <SDL.h>
#include <cstdlib>
#include <ctime>

#include "game.hpp"

#include <utility>
#include "appclock.hpp"
#include "viewport.hpp"
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
Game::Game(LieroGame* g, Members&&... members)
: Game(g, liero_game_data(g), std::forward<Members>(members)...)
{
}

template<typename... Members>
Game::Game(LieroGame* g, LieroGameData* d, Members&&... members)
: GameMembers(std::forward<Members>(members)...)
, g(g)
, data(*d)
, statsRecorder(new NormalStatsRecorder(liero_game_stats(g)))
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
: Game(liero_game_new(), common, settingsInit, soundPlayer)
{
	rand.seed(uint32_t(appTime()));
}

Game::~Game()
{
	liero_game_free(g);
}

std::unique_ptr<Game> Game::clone() const
{
	std::unique_ptr<Game> c(new Game(liero_game_clone(g), static_cast<GameMembers const&>(*this)));
	c->syncViews();
	return c;
}

void Game::assign(Game const& other)
{
	GameMembers::operator=(other);
	liero_game_assign(g, other.g);
	syncViews();
}

void Game::syncViews()
{
	wormViews.clear();
	worms.clear();
	for (std::size_t i = 0; i < liero_game_worm_count(g); ++i)
	{
		LieroWorm* w = liero_game_worm(g, i);
		if (w)
		{
			wormViews.emplace_back(new Worm(w));
			worms.push_back(wormViews.back().get());
		}
		else
			worms.push_back(nullptr);
	}

	viewportViews.clear();
	viewports.clear();
	for (std::size_t i = 0; i < liero_game_viewport_count(g, false); ++i)
	{
		viewportViews.emplace_back(new Viewport(liero_game_viewport(g, false, i)));
		viewports.push_back(viewportViews.back().get());
	}
}

void Game::clearViewports()
{
	liero_game_clear_viewports(g);
	syncViews();
}

Viewport* Game::addViewport(Rect rect, int wormIdx, int levwidth, int levheight, int kind)
{
	LieroRect r = { rect.x1, rect.y1, rect.x2, rect.y2 };
	viewportViews.emplace_back(new Viewport(liero_game_add_viewport(g, r, wormIdx, levwidth, levheight, kind, false)));
	viewports.push_back(viewportViews.back().get());
	return viewports.back();
}

void Game::addSpectatorViewport(Rect rect, int levwidth, int levheight)
{
	LieroRect r = { rect.x1, rect.y1, rect.x2, rect.y2 };
	liero_game_add_viewport(g, r, 0, levwidth, levheight, LIERO_VIEWPORT_SPECTATOR, true);
}

void Game::drawViewports(Renderer& renderer, GameState state, bool spectators, bool isReplay)
{
	for (std::size_t i = 0; i < liero_game_viewport_count(g, spectators); ++i)
		drawViewport(*this, liero_game_viewport(g, spectators, i), renderer, state, isReplay);
}

Worm* Game::addWorm(std::shared_ptr<WormSettings> settings)
{
	LieroWorm* w = newWorm();
	listWorm(w);
	Worm* worm = worms.back();
	worm->setSettings(settings);
	return worm;
}

LieroWorm* Game::newWorm()
{
	return liero_game_new_worm(g, nullptr);
}

void Game::listWorm(LieroWorm* w)
{
	liero_game_list_worm(g, w);
	if (w)
	{
		wormViews.emplace_back(new Worm(w));
		worms.push_back(wormViews.back().get());
	}
	else
		worms.push_back(nullptr);
}

void Game::draw(Renderer& renderer, GameState state, bool useSpectatorViewports, bool isReplay)
{
	drawViewports(renderer, state, useSpectatorViewports, isReplay);
	liero_game_palette(
		reinterpret_cast<LieroColor*>(renderer.pal.entries), reinterpret_cast<LieroColor const*>(renderer.origpal.entries),
		&liero_tc_data(common->tc)->color_anim, cycles, renderer.fadeValue, screenFlash);
}


GameView::GameView(Game& game)
{
	Common& common = *game.common;
	Settings& s = *game.settings;

	LieroGameExtras extras = {
		game.level.view(),
		common.tc ? liero_tc_data(common.tc) : nullptr,
		game.soundPlayer->out(),
	};
	view = liero_game_view_new(game.handle(), &extras);
	fields = *liero_game_view_fields(view);

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

GameView::~GameView()
{
	liero_game_view_free(view);
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

uint64_t hookHiresTicks(void*)
{
	return appHiresTicks();
}

} // namespace

void Game::processFrame()
{
	GameView view(*this);
	LieroGameHooks hooks = {
		this, hookHiresTicks, true,
	};
	liero_game_process_frame(&view.fields, &view.settings, &hooks);
}

void Game::processAis(bool timed)
{
	GameView view(*this);
	LieroGameHooks hooks = {
		this, hookHiresTicks, true,
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
	liero_game_origpal(g, reinterpret_cast<LieroColor*>(renderer.origpal.entries), reinterpret_cast<LieroColor const*>(level.origpal.entries));
}

void Game::startGame()
{
	GameView view(*this);
	liero_game_start(&view.fields, &view.settings, settings->bloodParticleMax);
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

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
	gvl::shared_ptr<Common> common,
	gvl::shared_ptr<Settings> settings,
	gvl::shared_ptr<SoundPlayer> soundPlayer)
: common(common)
, soundPlayer(soundPlayer)
, settings(settings)
, statsRecorder(new NormalStatsRecorder)
, level(*common)
{
}

// The Rust types the fields refer to are laid out like the C++ ones.
static_assert(sizeof(Rand) == sizeof(LieroRand), "Rand layout");
static_assert(sizeof(Holdazone) == sizeof(LieroHoldazone), "Holdazone layout");

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
{
}

Game::Game(
	gvl::shared_ptr<Common> common,
	gvl::shared_ptr<Settings> settingsInit,
	gvl::shared_ptr<SoundPlayer> soundPlayer)
: Game(liero_game_data_new(), common, settingsInit, soundPlayer)
{
#if ENABLE_TRACING
	rand.seed(1);
#else
	rand.seed(uint32_t(appTime()));
#endif
}

Game::Game(Game const& other)
: Game(liero_game_data_clone(&other.data), static_cast<GameMembers const&>(other))
{
}

Game& Game::operator=(Game const& other)
{
	GameMembers::operator=(other);
	data = other.data;
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

#if ENABLE_TRACING

void checkMap(Game& game) {
	Common& common = *game.common;
	uint32 h = 1;
	for (std::size_t i = 0; i < 504*350; ++i) {
		h = h * 33 ^ game.level.data[i];
	}
	LTRACE(maph, 0, pixl, h);
	h = 1;
	for (std::size_t i = 0; i < 504*350; ++i) {
		h = h * 33 ^ game.level.materials[i].flags;
	}
	LTRACE(maph, 0, matr, h);
}

#endif

namespace
{

// Rust works on these in place.
static_assert(sizeof(gvl::rect) == sizeof(LieroRect), "rect layout");
static_assert(offsetof(gvl::rect, x1) == offsetof(LieroRect, x1) && offsetof(gvl::rect, y1) == offsetof(LieroRect, y1)
	&& offsetof(gvl::rect, x2) == offsetof(LieroRect, x2) && offsetof(gvl::rect, y2) == offsetof(LieroRect, y2), "rect layout");
static_assert(sizeof(Holdazone) == sizeof(LieroHoldazone), "Holdazone layout");
static_assert(offsetof(Holdazone, rect) == offsetof(LieroHoldazone, rect), "Holdazone layout");
static_assert(offsetof(Holdazone, holderIdx) == offsetof(LieroHoldazone, holder_idx), "Holdazone layout");
static_assert(offsetof(Holdazone, contenderIdx) == offsetof(LieroHoldazone, contender_idx), "Holdazone layout");
static_assert(offsetof(Holdazone, contenderFrames) == offsetof(LieroHoldazone, contender_frames), "Holdazone layout");
static_assert(offsetof(Holdazone, timeoutLeft) == offsetof(LieroHoldazone, timeout_left), "Holdazone layout");
static_assert(offsetof(Holdazone, zoneWidth) == offsetof(LieroHoldazone, zone_width), "Holdazone layout");
static_assert(offsetof(Holdazone, zoneHeight) == offsetof(LieroHoldazone, zone_height), "Holdazone layout");
// The game as the Rust game rules see it. Pointers into the game, so it
// must not outlive the call it is made for.
struct GameView
{
	GameView(Game& game)
	{
		Common& common = *game.common;
		Settings& s = *game.settings;

		for (Worm* w : game.worms)
		{
			LieroGameWorm v = { w->rust(), w->settings->health };
			worms.push_back(v);
		}

		fields.worms = worms.data();
		fields.worm_count = int32_t(worms.size());
		fields.game = game.rust();
		fields.level = game.level.view();

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
	}

	std::vector<LieroGameWorm> worms;
	LieroGameFields fields;
	LieroGameSettings settings;
};

static_assert(sizeof(Settings::weapTable[0]) == sizeof(uint32_t), "weapTable type");

int wormPosition(Game& game, Worm& w)
{
	for (std::size_t i = 0; i < game.worms.size(); ++i)
	{
		if (game.worms[i] == &w)
			return int(i);
	}
	assert(false);
	return -1;
}

Game& gameOf(void* ctx)
{
	return *static_cast<Game*>(ctx);
}

void hookBeginTick(void* ctx)
{
	Game& game = gameOf(ctx);
	game.statsRecorder->preTick(game);

	if(game.screenFlash > 0)
		--game.screenFlash;

	for(std::size_t i = 0; i < game.viewports.size(); ++i)
	{
		if(game.viewports[i]->shake > 0)
			game.viewports[i]->shake -= 4000; // TODO: Read 4000 from exe?
	}

	for(std::size_t i = 0; i < game.spectatorViewports.size(); ++i)
	{
		if(game.spectatorViewports[i]->shake > 0)
			game.spectatorViewports[i]->shake -= 4000; // TODO: Read 4000 from exe?
	}
}

void hookProcessBonuses(void* ctx)
{
	Game& game = gameOf(ctx);
	auto br = game.bonuses.all();
	for (Bonus* i; (i = br.next()); )
	{
		i->process(game);
	}
}

void moveBanner(int& bannerY, bool down)
{
	if(down)
	{
		if(bannerY < 2)
			++bannerY;
	}
	else
	{
		if(bannerY > -8)
			--bannerY;
	}
}

void hookUpdateBanners(void* ctx)
{
	Game& game = gameOf(ctx);
	for(std::size_t i = 0; i < game.viewports.size(); ++i)
	{
		Viewport& v = *game.viewports[i];
		moveBanner(v.bannerY, game.wormByIdx(v.wormIdx)->killedTimer > 16);
	}

	for(std::size_t i = 0; i < game.spectatorViewports.size(); ++i)
	{
		SpectatorViewport& v = *game.spectatorViewports[i];
		moveBanner(v.bannerY, game.wormByIdx(0)->killedTimer > 16 || game.wormByIdx(1)->killedTimer > 16);
	}
}

void hookProcessSObjects(void* ctx)
{
	Game& game = gameOf(ctx);
	auto sr = game.sobjects.all();
	for (SObject* i; (i = sr.next()); )
	{
		i->process(game);
	}
}

void hookProcessWObjects(void* ctx)
{
	Game& game = gameOf(ctx);
	auto wr = game.wobjects.all();
	for (WObject* i; (i = wr.next()); )
	{
		i->process(game);
	}
}

void hookProcessNObjects(void* ctx)
{
	Game& game = gameOf(ctx);
	auto nr = game.nobjects.all();
	for (NObject* i; (i = nr.next()); )
	{
		i->process(game);
	}
}

void hookProcessBObjects(void* ctx)
{
	Game& game = gameOf(ctx);
	for(Game::BObjectList::iterator i = game.bobjects.begin(); i != game.bobjects.end(); )
	{
		if(i->process(game))
			++i;
		else
			game.bobjects.free(i);
	}
}

int32_t hookBonusCount(void* ctx)
{
	return int32_t(gameOf(ctx).bonuses.size());
}

void* hookNewBonus(void* ctx)
{
	return gameOf(ctx).bonuses.newObject();
}

void hookSetBonus(void*, void* bonus, LieroBonusState const* state)
{
	Bonus& b = *static_cast<Bonus*>(bonus);
	b.x = state->x;
	b.y = state->y;
	b.velY = state->vel_y;
	b.frame = state->frame;
	b.timer = state->timer;
	// Health bonuses keep the weapon of the bonus that used the slot before.
	if (state->frame == 0)
		b.weapon = state->weapon;
}

void hookCreateSObject(void* ctx, int32_t kind, int32_t x, int32_t y)
{
	Game& game = gameOf(ctx);
	game.common->sobjectTypes[kind].create(game, x, y, 0, 0);
}

void hookProcessWorm(void* ctx, int32_t i)
{
	Game& game = gameOf(ctx);
	game.worms[i]->process(game);
}

void hookProcessRope(void* ctx, int32_t i)
{
	Game& game = gameOf(ctx);
	game.worms[i]->ninjarope.process(*game.worms[i], game);
}

void hookEndTick(void* ctx)
{
	Game& game = gameOf(ctx);
	game.processViewports();

	// Store old control states so we can see what changes (mainly for replays)
	for(std::size_t i = 0; i < game.worms.size(); ++i)
	{
		game.worms[i]->prevControlStates = game.worms[i]->controlStates;
	}

	game.statsRecorder->tick(game);
}

} // namespace

void Game::processFrame()
{
	GameView view(*this);
	LieroGameHooks hooks = {
		this, hookBeginTick, hookProcessBonuses, hookUpdateBanners,
		hookProcessSObjects, hookProcessWObjects, hookProcessNObjects, hookProcessBObjects,
		hookBonusCount, hookNewBonus, hookSetBonus, hookCreateSObject,
		hookProcessWorm, hookProcessRope, hookEndTick,
	};
	liero_game_process_frame(&view.fields, &view.settings, &hooks);
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

bool Game::isGameOver()
{
	GameView view(*this);
	return liero_game_is_over(&view.fields, &view.settings);
}

void Game::doDamage(Worm& w, int amount, int byIdx)
{
	GameView view(*this);
	liero_game_damage(&view.fields, &view.settings, wormPosition(*this, w), amount, byIdx);
}

void Game::doHealing(Worm& w, int amount)
{
	GameView view(*this);
	liero_game_heal(&view.fields, &view.settings, wormPosition(*this, w), amount);
}

void Game::postClone(Game& original, bool complete)
{
	if (!complete)
	{
		statsRecorder.reset(new StatsRecorder);
		soundPlayer.reset(new NullSoundPlayer);
		viewports.clear();
	}
	else
	{
		statsRecorder.reset(new NormalStatsRecorder(static_cast<NormalStatsRecorder&>(*statsRecorder)));

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

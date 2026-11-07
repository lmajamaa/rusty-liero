#include "viewport.hpp"
#include "spectatorviewport.hpp"
#include "game.hpp"
#include "constants.hpp"
#include "gfx/bitmap.hpp"
#include "gfx/renderer.hpp"
#include <vector>

// Viewports are processed and drawn in Rust (rust/liero-game/src/viewport.rs).
// For each draw, C++ copies what is drawn into a LieroScene.

namespace
{

LieroStr str(std::string const& s)
{
	LieroStr r = { reinterpret_cast<uint8_t const*>(s.data()), s.size() };
	return r;
}

LieroSpriteSet spriteSet(SpriteSet const& s)
{
	LieroSpriteSet r = { s.data.data(), s.width, s.height, s.count };
	return r;
}

int weaponIndex(Common& common, Weapon const* w)
{
	return w ? int(w - &common.weapons[0]) : -1;
}

LieroViewWorm viewWorm(Common& common, Worm const& w)
{
	LieroViewWorm r = {};
	r.pos = { w.pos.x, w.pos.y };
	r.visible = w.visible;
	r.ready = w.ready;
	r.health = w.health;
	r.max_health = w.settings->health;
	r.killed_timer = w.killedTimer;
	r.stats_x = w.statsX;
	r.kills = w.kills;
	r.lives = w.lives;
	r.timer = w.timer;
	r.index = w.index;
	r.name = w.settings->name.view();
	r.color = w.settings->color;
	r.pressed_change = w.pressed(Worm::Change);
	r.pressed_fire = w.pressed(Worm::Fire);
	r.current_frame = w.currentFrame;
	r.direction = w.direction;
	r.hotspot_x = w.hotspotX;
	r.hotspot_y = w.hotspotY;
	r.aiming_angle = w.aimingAngle;
	r.make_sight_green = w.makeSightGreen;
	r.rope_out = w.ninjarope.out;
	r.rope_pos = { w.ninjarope.pos.x, w.ninjarope.pos.y };
	r.fire_cone = w.fireCone;
	r.last_killed_by_idx = w.lastKilledByIdx;
	r.current_weapon = w.currentWeapon;
	for (int i = 0; i < 5; ++i)
	{
		WormWeapon const& ww = w.weapons[i];
		r.weapons[i] = { weaponIndex(common, ww.type), ww.ammo, ww.loadingLeft };
	}
	r.steerable_count = w.steerableCount;
	r.steerable_sum_x = w.steerableSumX;
	r.steerable_sum_y = w.steerableSumY;
	r.has_ai = bool(w.ai);
	return r;
}

std::vector<LieroViewWorm> viewWorms(Game& game)
{
	std::vector<LieroViewWorm> worms;
	for (Worm* w : game.worms)
		worms.push_back(viewWorm(*game.common, *w));
	return worms;
}

static_assert(sizeof(gvl::rect) == sizeof(LieroRect), "gvl::rect layout");

struct AiDebugContext
{
	Game& game;
	Renderer& renderer;
};

void hookDrawAiDebug(void* ctx, int32_t worm, int32_t offsX, int32_t offsY, LieroRect clip)
{
	AiDebugContext& c = *static_cast<AiDebugContext*>(ctx);
	Worm const& w = *c.game.worms[worm];
	Bitmap& bmp = c.renderer.bmp;
	gvl::rect org = bmp.clip_rect;
	bmp.clip_rect = gvl::rect(clip.x1, clip.y1, clip.x2, clip.y2);
	w.ai->drawDebug(c.game, w, c.renderer, offsX, offsY);
	bmp.clip_rect = org;
}

// Copies of everything drawn, kept alive while Rust draws.
struct SceneData
{
	std::vector<LieroViewWorm> worms;
	std::vector<LieroViewWeapon> weapons;
	std::vector<LieroViewBonus> bonuses;
	std::vector<LieroViewSObject> sobjects;
	std::vector<LieroViewWObject> wobjects;
	std::vector<LieroViewNObject> nobjects;
	std::vector<LieroViewBObject> bobjects;
	std::vector<LieroViewportBanner> viewports;
};

template<typename T>
int32_t count(std::vector<T> const& v)
{
	return int32_t(v.size());
}

void draw(Viewport& vp, int kind, Game& game, Renderer& renderer, GameState gameState, bool isReplay)
{
	Common& common = *game.common;
	Settings& settings = *game.settings;
	SceneData d;

	d.worms = viewWorms(game);

	for (Weapon const& w : common.weapons)
	{
		LieroViewWeapon r;
		r.name = str(w.name);
		r.ammo = w.ammo;
		r.loading_time = w.loadingTime;
		r.computed_loading_time = w.computedLoadingTime(settings);
		r.laser_sight = w.laserSight;
		r.color_bullets = w.colorBullets;
		r.fire_cone = w.fireCone;
		r.start_frame = w.startFrame;
		r.shot_type = w.shotType;
		r.shadow = w.shadow;
		d.weapons.push_back(r);
	}

	auto br = game.bonuses.all();
	for (Bonus* i; (i = br.next()); )
		d.bonuses.push_back({ i->x, i->y, i->timer, i->frame, i->weapon });

	auto sr = game.sobjects.all();
	for (SObject* i; (i = sr.next()); )
		d.sobjects.push_back({ i->x, i->y, i->curFrame, common.sobjectTypes[i->id].startFrame });

	auto wr = game.wobjects.all();
	for (WObject* i; (i = wr.next()); )
		d.wobjects.push_back({ { i->pos.x, i->pos.y }, weaponIndex(common, i->type), i->curFrame, int32_t(i - game.wobjects.arr) });

	auto nr = game.nobjects.all();
	for (NObject* i; (i = nr.next()); )
		d.nobjects.push_back({ { i->pos.x, i->pos.y }, i->curFrame, i->type->startFrame });

	for (Game::BObjectList::iterator i = game.bobjects.begin(); i != game.bobjects.end(); ++i)
		d.bobjects.push_back({ { i->pos.x, i->pos.y }, i->color });

	int selfIndex = -1;
	for (std::size_t i = 0; i < game.viewports.size(); ++i)
	{
		Viewport* v = game.viewports[i];
		if (v == &vp)
			selfIndex = int(i);
		d.viewports.push_back({ v->wormIdx, v->bannerY });
	}

	AiDebugContext aiCtx = { game, renderer };

	LieroScene s = {};
	s.worms = d.worms.data();
	s.worm_count = count(d.worms);
	s.weapons = d.weapons.data();
	s.weapon_count = count(d.weapons);
	s.bonuses = d.bonuses.data();
	s.bonus_count = count(d.bonuses);
	s.sobjects = d.sobjects.data();
	s.sobject_count = count(d.sobjects);
	s.wobjects = d.wobjects.data();
	s.wobject_count = count(d.wobjects);
	s.nobjects = d.nobjects.data();
	s.nobject_count = count(d.nobjects);
	s.bobjects = d.bobjects.data();
	s.bobject_count = count(d.bobjects);
	s.viewports = d.viewports.data();
	s.viewport_count = count(d.viewports);

	s.small_sprites = spriteSet(common.smallSprites);
	s.large_sprites = spriteSet(common.largeSprites);
	s.worm_sprites = spriteSet(common.wormSprites);
	s.fire_cone_sprites = spriteSet(common.fireConeSprites);
	s.text_sprites = spriteSet(common.textSprites);
	s.font = reinterpret_cast<LieroFontChar const*>(common.font.chars.data());
	s.font_count = common.font.chars.size();
	s.materials = reinterpret_cast<LieroMaterial const*>(common.materials);
	s.level = game.level.view();
	s.bonus_frames[0] = common.bonusFrames[0];
	s.bonus_frames[1] = common.bonusFrames[1];

	s.reloading = str(LS(Reloading));
	s.kills = str(LS(Kills));
	s.lives = str(LS(Lives));
	s.press_fire = str(LS(PressFire));
	s.youre_it = str(LS(YoureIt));
	s.killed_msg = str(LS(KilledMsg));
	s.committed_suicide_msg = str(LS(CommittedSuicideMsg));

	s.cycles = game.cycles;
	s.game_mode = settings.gameMode;
	s.holdazone = reinterpret_cast<LieroHoldazone const&>(game.holdazone);
	s.zone_capture_time = settings.zoneCaptureTime;
	s.last_killed_idx = game.lastKilledIdx;
	s.got_changed = game.gotChanged;
	s.shadow = settings.shadow;
	s.names_on_bonuses = settings.namesOnBonuses;
	s.allow_viewing_spawn_point = settings.allowViewingSpawnPoint;
	s.map = settings.map;
	s.rem_exp = common.H[HRemExp];
	s.laser_weapon = LC(LaserWeapon);
	s.bonus_flicker_time = LC(BonusFlickerTime);
	s.nr_colour_begin = LC(NRColourBegin);
	s.nr_colour_end = LC(NRColourEnd);

	s.render_res_x = renderer.renderResX;
	s.render_res_y = renderer.renderResY;
	s.state_game = gameState == StateGame;
	s.is_replay = isReplay;

	s.hooks.ctx = &aiCtx;
	s.hooks.draw_ai_debug = hookDrawAiDebug;

	LieroBitmap scr = renderer.bmp.view();
	liero_viewport_draw(vp.rust(), kind, selfIndex, &s, &scr);
}

void process(Viewport& vp, int kind, Game& game)
{
	std::vector<LieroViewWorm> worms = viewWorms(game);
	liero_viewport_process(vp.rust(), kind, worms.data(), count(worms));
}

}

static_assert(LIERO_GAME_KILL_EM_ALL == Settings::GMKillEmAll, "game modes");
static_assert(LIERO_GAME_GAME_OF_TAG == Settings::GMGameOfTag, "game modes");
static_assert(LIERO_GAME_HOLDAZONE == Settings::GMHoldazone, "game modes");
static_assert(LIERO_GAME_SCALES_OF_JUSTICE == Settings::GMScalesOfJustice, "game modes");

void Viewport::process(Game& game)
{
	::process(*this, LIERO_VIEWPORT_WORM, game);
}

void Viewport::draw(Game& game, Renderer& renderer, GameState state, bool isReplay)
{
	::draw(*this, LIERO_VIEWPORT_WORM, game, renderer, state, isReplay);
}

void SpectatorViewport::process(Game& game)
{
	::process(*this, LIERO_VIEWPORT_SPECTATOR, game);
}

void SpectatorViewport::draw(Game& game, Renderer& renderer, GameState state, bool isReplay)
{
	::draw(*this, LIERO_VIEWPORT_SPECTATOR, game, renderer, state, isReplay);
}

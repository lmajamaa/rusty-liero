#include "replay.hpp"

#include "game.hpp"
#include "worm.hpp"
#include "viewport.hpp"
#include "settingsffi.hpp"
#include <gvl/serialization/except.hpp>
#include <gvl/io2/deflate_filter.hpp>

#include <algorithm>
#include <stdexcept>
#include <string>
#include <vector>

// The replay format is in Rust (rust/liero-game/src/replay.rs). C++ keeps the
// deflate streams, makes the Game, Worms and Viewports a replay starts with,
// and copies settings back after each call, since reading and writing
// replays both change them.

namespace
{

// Owns a LieroReplayResult.
struct ReplayResult
{
	explicit ReplayResult(LieroReplayResult* r)
	: r(r)
	{
	}

	~ReplayResult()
	{
		liero_replay_result_free(r);
	}

	ReplayResult(ReplayResult const&) = delete;
	ReplayResult& operator=(ReplayResult const&) = delete;

	LieroReplayStatus status() const { return liero_replay_result_status(r); }
	LieroSettings const& settings() const { return *liero_replay_result_settings(r); }
	LieroWormSettings const& worm(std::size_t i) const { return *liero_replay_result_worm_settings(r, i); }
	bool touched(std::size_t i) const { return liero_replay_result_touched(r, i); }

	void writeTo(gvl::octet_writer& writer) const
	{
		LieroStr b = liero_replay_result_bytes(r);
		if (b.len)
			writer.put(b.ptr, b.len);
	}

	// Throws what the C++ replay code threw for the status.
	void rethrow(std::exception_ptr const& readerError) const
	{
		switch (status())
		{
		case LieroReplayStatus_ReadError:
			if (readerError)
				std::rethrow_exception(readerError);
			throw std::runtime_error("Read error in get()");
		case LieroReplayStatus_CheckError:
		{
			LieroStr m = liero_replay_result_message(r);
			throw gvl::archive_check_error(std::string(reinterpret_cast<char const*>(m.ptr), m.len));
		}
		case LieroReplayStatus_OutOfRange:
			// What std::vector::at throws in libc++.
			throw std::out_of_range("vector");
		default:
			return;
		}
	}

	LieroReplayResult* r;
};

typedef ArchiveByteSource<gvl::octet_reader> ReplaySource;

LieroReplayWorm wormFields(Worm const& worm, Game& game)
{
	LieroReplayWorm f;
	f.pos[0] = worm.pos.x; f.pos[1] = worm.pos.y;
	f.vel[0] = worm.vel.x; f.vel[1] = worm.vel.y;
	f.logic_respawn[0] = worm.logicRespawn.x; f.logic_respawn[1] = worm.logicRespawn.y;
	f.hotspot[0] = worm.hotspotX; f.hotspot[1] = worm.hotspotY;
	f.aiming_angle = worm.aimingAngle;
	f.aiming_speed = worm.aimingSpeed;
	f.able_to_jump = worm.ableToJump;
	f.able_to_dig = worm.ableToDig;
	f.key_change_pressed = worm.keyChangePressed;
	f.movable = worm.movable;
	f.animate = worm.animate;
	f.visible = worm.visible;
	f.ready = worm.ready;
	f.flag = worm.flag;
	f.make_sight_green = worm.makeSightGreen;
	f.health = worm.health;
	f.lives = worm.lives;
	f.kills = worm.kills;
	f.timer = worm.timer;
	f.killed_timer = worm.killedTimer;
	f.current_frame = worm.currentFrame;
	f.flags = worm.flags;

	// The anchor by its place among the game's worms. The game's ropes only
	// attach to its own worms.
	auto anchor = std::find(game.worms.begin(), game.worms.end(), worm.ninjarope.anchor);
	f.rope_anchor = worm.ninjarope.anchor && anchor != game.worms.end() ? int(anchor - game.worms.begin()) : -1;
	f.rope_attached = worm.ninjarope.attached;
	f.rope_cur_len = worm.ninjarope.curLen;
	f.rope_length = worm.ninjarope.length;
	f.rope_out = worm.ninjarope.out;
	f.rope_vel[0] = worm.ninjarope.vel.x; f.rope_vel[1] = worm.ninjarope.vel.y;
	f.rope_pos[0] = worm.ninjarope.pos.x; f.rope_pos[1] = worm.ninjarope.pos.y;
	f.current_weapon = worm.currentWeapon;
	f.last_killed_by = worm.lastKilledByIdx;
	f.fire_cone = worm.fireCone;
	f.leave_shell_timer = worm.leaveShellTimer;
	f.index = worm.index;
	for (int i = 0; i < 4; ++i)
		f.reacts[i] = worm.reacts[i];
	for (int i = 0; i < 5; ++i)
	{
		WormWeapon const& w = worm.weapons[i];
		f.weapons[i].ammo = w.ammo;
		f.weapons[i].delay_left = w.delayLeft;
		f.weapons[i].weapon = int(w.type - &game.common->weapons[0]);
		f.weapons[i].loading_left = w.loadingLeft;
	}
	f.direction = worm.direction;
	f.control_states = worm.controlStates.pack();
	f.prev_control_states = worm.prevControlStates.pack();
	return f;
}

void setWormFields(Worm& worm, LieroReplayWorm const& f, std::vector<Worm*> const& all, Common& common)
{
	worm.pos.x = f.pos[0]; worm.pos.y = f.pos[1];
	worm.vel.x = f.vel[0]; worm.vel.y = f.vel[1];
	worm.logicRespawn.x = f.logic_respawn[0]; worm.logicRespawn.y = f.logic_respawn[1];
	worm.hotspotX = f.hotspot[0]; worm.hotspotY = f.hotspot[1];
	worm.aimingAngle = f.aiming_angle;
	worm.aimingSpeed = f.aiming_speed;
	worm.ableToJump = f.able_to_jump;
	worm.ableToDig = f.able_to_dig;
	worm.keyChangePressed = f.key_change_pressed;
	worm.movable = f.movable;
	worm.animate = f.animate;
	worm.visible = f.visible;
	worm.ready = f.ready;
	worm.flag = f.flag;
	worm.makeSightGreen = f.make_sight_green;
	worm.health = f.health;
	worm.lives = f.lives;
	worm.kills = f.kills;
	worm.timer = f.timer;
	worm.killedTimer = f.killed_timer;
	worm.currentFrame = f.current_frame;
	worm.flags = f.flags;

	worm.ninjarope.anchor = f.rope_anchor < 0 ? 0 : all[f.rope_anchor];
	worm.ninjarope.attached = f.rope_attached;
	worm.ninjarope.curLen = f.rope_cur_len;
	worm.ninjarope.length = f.rope_length;
	worm.ninjarope.out = f.rope_out;
	worm.ninjarope.vel.x = f.rope_vel[0]; worm.ninjarope.vel.y = f.rope_vel[1];
	worm.ninjarope.pos.x = f.rope_pos[0]; worm.ninjarope.pos.y = f.rope_pos[1];
	worm.currentWeapon = f.current_weapon;
	worm.lastKilledByIdx = f.last_killed_by;
	worm.fireCone = f.fire_cone;
	worm.leaveShellTimer = f.leave_shell_timer;
	worm.index = f.index;
	for (int i = 0; i < 4; ++i)
		worm.reacts[i] = f.reacts[i];
	for (int i = 0; i < 5; ++i)
	{
		WormWeapon& w = worm.weapons[i];
		w.ammo = f.weapons[i].ammo;
		w.delayLeft = f.weapons[i].delay_left;
		// Unchecked, as before.
		w.type = &common.weapons[0] + f.weapons[i].weapon;
		w.loadingLeft = f.weapons[i].loading_left;
	}
	worm.direction = f.direction;
	worm.controlStates.istate = f.control_states;
	worm.prevControlStates.istate = f.prev_control_states;
}

LieroReplayViewport viewportFields(Viewport const& vp)
{
	LieroReplayViewport f;
	f.x = vp.x;
	f.y = vp.y;
	f.shake = vp.shake;
	f.max_x = vp.maxX;
	f.max_y = vp.maxY;
	f.center_x = vp.centerX;
	f.center_y = vp.centerY;
	f.worm_idx = vp.wormIdx;
	f.banner_y = vp.bannerY;
	f.rect[0] = vp.rect.x1;
	f.rect[1] = vp.rect.y1;
	f.rect[2] = vp.rect.x2;
	f.rect[3] = vp.rect.y2;
	return f;
}

void setViewportFields(Viewport& vp, LieroReplayViewport const& f)
{
	vp.x = f.x;
	vp.y = f.y;
	vp.shake = f.shake;
	vp.maxX = f.max_x;
	vp.maxY = f.max_y;
	vp.centerX = f.center_x;
	vp.centerY = f.center_y;
	vp.wormIdx = f.worm_idx;
	vp.bannerY = f.banner_y;
	vp.rect.x1 = f.rect[0];
	vp.rect.y1 = f.rect[1];
	vp.rect.x2 = f.rect[2];
	vp.rect.y2 = f.rect[3];
}

// What frames read and write of the game's worms.
std::vector<LieroReplayFrameWorm> frameWorms(Game& game, GameSerializationContext& context, bool withSettings)
{
	std::vector<LieroReplayFrameWorm> worms(game.worms.size());
	for (std::size_t i = 0; i < worms.size(); ++i)
	{
		Worm& worm = *game.worms[i];
		LieroReplayFrameWorm& f = worms[i];
		f.index = worm.index;
		f.control_states = worm.controlStates.pack();
		f.prev_control_states = worm.prevControlStates.pack();
		f.settings_expired = withSettings && context.wormData[&worm].settingsExpired;
		f.settings = wormSettingsView(*worm.settings);
	}
	return worms;
}

// Copies back the settings a frame read or wrote. The game's settings go
// first: their worms can be the same as the game's worms' settings, which
// the frame may have changed after.
void copyFrameSettings(ReplayResult const& r, Game& game)
{
	if (r.touched(0))
		setSettings(*game.settings, r.settings());
	for (std::size_t i = 0; i < game.worms.size(); ++i)
	{
		if (r.touched(1 + i))
			setWormSettings(*game.worms[i]->settings, r.worm(i));
	}
}

}

uint32_t fastGameChecksum(Game& game)
{
	std::vector<int32_t> worms(game.worms.size() * 4);
	for (std::size_t i = 0; i < game.worms.size(); ++i)
	{
		Worm& worm = *game.worms[i];
		worms[i * 4 + 0] = worm.pos.x;
		worms[i * 4 + 1] = worm.pos.y;
		worms[i * 4 + 2] = worm.vel.x;
		worms[i * 4 + 3] = worm.vel.y;
	}
	return liero_replay_checksum(game.rand.x, reinterpret_cast<int32_t const (*)[4]>(worms.data()), game.worms.size());
}

ReplayWriter::ReplayWriter(gvl::sink str_init)
: settingsExpired(true)
{
	gvl::deflate_source* ds(new gvl::deflate_source(gvl::source(), true, false));

	ds->sink = str_init;

	writer.attach(gvl::sink(ds));
}

ReplayWriter::~ReplayWriter()
{
	endRecord();
}

ReplayReader::ReplayReader(gvl::source str_init)
{
	reader.attach(gvl::to_source(new gvl::deflate_source(str_init, false)));
}

std::unique_ptr<Game> ReplayReader::beginPlayback(gvl::shared_ptr<Common> common, gvl::shared_ptr<SoundPlayer> soundPlayer)
{
	ReplaySource source(reader);
	ReplayResult r(liero_replay_read_start(&source.hooks));
	r.rethrow(source.error);
	LieroReplayGameOut const& g = *liero_replay_result_game(r.r);
	context.replayVersion = g.version;

	gvl::shared_ptr<Settings> settings(new Settings);
	setSettings(*settings, g.settings);

	std::unique_ptr<Game> game(new Game(common, settings, soundPlayer));
	context.game = game.get();

	game->cycles = g.fields.cycles;
	game->gotChanged = g.fields.got_changed;
	game->lastKilledIdx = g.fields.last_killed;
	game->screenFlash = g.fields.screen_flash;
	game->rand.x = g.fields.rand_x;
	game->rand.c = g.fields.rand_c;

	// Every worm read, including any only referred to, which the game
	// doesn't own, as before.
	std::vector<Worm*> all(g.worm_count);
	for (auto& w : all)
		w = new Worm();
	for (std::size_t i = 0; i < g.worm_count; ++i)
	{
		setWormFields(*all[i], g.worms[i], all, *common);
		all[i]->settings.reset(new WormSettings);
		setWormSettings(*all[i]->settings, g.worm_settings[i]);
	}
	for (std::size_t i = 0; i < g.list_len; ++i)
		game->addWorm(g.list[i] < 0 ? 0 : all[g.list[i]]);

	for (std::size_t i = 0; i < g.viewport_count; ++i)
	{
		Viewport* vp = new Viewport();
		setViewportFields(*vp, g.viewports[i]);
		game->addViewport(vp);
	}

	Level& level = game->level;
	level.resize(g.level_width, g.level_height);
	for (unsigned int y = 0; y < g.level_height; ++y)
	for (unsigned int x = 0; x < g.level_width; ++x)
		level.setPixel(x, y, g.level_pixels[y * g.level_width + x], *common);
	for (int i = 0; i < 256; ++i)
	{
		level.origpal.entries[i].r = g.level_palette[i * 3 + 0];
		level.origpal.entries[i].g = g.level_palette[i * 3 + 1];
		level.origpal.entries[i].b = g.level_palette[i * 3 + 2];
	}

	return game;
}

void ReplayWriter::beginRecord(Game& game)
{
	context.game = &game;

	std::vector<LieroReplayWorm> worms;
	std::vector<LieroWormSettings> wormSettings;
	for (auto* worm : game.worms)
	{
		worms.push_back(wormFields(*worm, game));
		wormSettings.push_back(wormSettingsView(*worm->settings));
	}
	std::vector<LieroReplayViewport> viewports;
	for (auto* vp : game.viewports)
		viewports.push_back(viewportFields(*vp));
	uint8_t palette[768];
	for (int i = 0; i < 256; ++i)
	{
		palette[i * 3 + 0] = game.level.origpal.entries[i].r;
		palette[i * 3 + 1] = game.level.origpal.entries[i].g;
		palette[i * 3 + 2] = game.level.origpal.entries[i].b;
	}

	LieroReplayGameIn g;
	g.settings = settingsView(*game.settings);
	g.fields.cycles = game.cycles;
	g.fields.got_changed = game.gotChanged;
	g.fields.last_killed = game.lastKilledIdx;
	g.fields.screen_flash = game.screenFlash;
	g.fields.rand_x = game.rand.x;
	g.fields.rand_c = game.rand.c;
	g.worms = worms.data();
	g.worm_settings = wormSettings.data();
	g.worm_count = worms.size();
	g.viewports = viewports.data();
	g.viewport_count = viewports.size();
	g.level_width = game.level.width;
	g.level_height = game.level.height;
	g.level_pixels = game.level.data.data();
	g.level_palette = palette;

	ReplayResult r(liero_replay_write_start(&g, context.replayVersion));
	setSettings(*game.settings, r.settings());
	for (std::size_t i = 0; i < game.worms.size(); ++i)
		setWormSettings(*game.worms[i]->settings, r.worm(i));
	r.writeTo(writer);
	r.rethrow(std::exception_ptr());

	// They were just written, so they're up to date.
	for (auto* worm : game.worms)
		context.wormData[worm].settingsExpired = false;
	settingsExpired = false;
}

void ReplayWriter::endRecord()
{
	ReplayResult r(liero_replay_write_end());
	r.writeTo(writer);
}

bool ReplayReader::playbackFrame(Renderer& renderer)
{
	Game& game = *context.game;

	std::vector<LieroReplayFrameWorm> worms = frameWorms(game, context, false);
	LieroSettings settings = settingsView(*game.settings);
	ReplaySource source(reader);
	ReplayResult r(liero_replay_read_frame(&source.hooks, context.replayVersion, &settings, worms.data(), worms.size()));

	// What was read stays, even when the frame broke off.
	copyFrameSettings(r, game);
	for (std::size_t i = 0; i < game.worms.size(); ++i)
		game.worms[i]->controlStates.istate = liero_replay_result_frame_state(r.r, i);
	r.rethrow(source.error);

	if (liero_replay_result_end(r.r))
		return false;

	if (liero_replay_result_settings_changed(r.r))
		game.updateSettings(renderer);

	ReplayResult c(liero_replay_read_checksum(&source.hooks, game.cycles, fastGameChecksum(game)));
	c.rethrow(source.error);
	return true;
}

void ReplayWriter::recordFrame()
{
	Game& game = *context.game;

	// Worms' settings are only written with more than three worms.
	std::vector<LieroReplayFrameWorm> worms = frameWorms(game, context, game.worms.size() > 3);
	LieroSettings settings = settingsView(*game.settings);
	ReplayResult r(liero_replay_write_frame(
		context.replayVersion, &settings, settingsExpired,
		worms.data(), worms.size(), game.cycles, fastGameChecksum(game)));

	copyFrameSettings(r, game);
	if (r.touched(0))
		settingsExpired = false;
	for (std::size_t i = 0; i < game.worms.size(); ++i)
	{
		if (r.touched(1 + i))
			context.wormData[game.worms[i]].settingsExpired = false;
	}
	r.writeTo(writer);
}

void ReplayWriter::unfocus()
{
	for(GameSerializationContext::WormDataMap::iterator i = context.wormData.begin();
		i != context.wormData.end();
		++i)
	{
		i->second.lastSettingsHash = i->first->settings->updateHash();
	}

	lastSettingsHash = context.game->settings->updateHash();
}

void ReplayWriter::focus()
{
	for(GameSerializationContext::WormDataMap::iterator i = context.wormData.begin();
		i != context.wormData.end();
		++i)
	{
		if(i->second.lastSettingsHash != i->first->settings->updateHash())
		{
			i->second.settingsExpired = true;
		}
	}

	if(lastSettingsHash != context.game->settings->updateHash())
		settingsExpired = true;
}

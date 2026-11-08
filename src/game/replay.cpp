#include "replay.hpp"

#include "game.hpp"
#include "worm.hpp"
#include "viewport.hpp"
#include "settingsffi.hpp"

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

	template<typename Writer>
	void writeTo(Writer& writer) const
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
			throw ArchiveCheckError(std::string(reinterpret_cast<char const*>(m.ptr), m.len));
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

typedef ArchiveByteSource<ByteReader> ReplaySource;

// The worms' and viewports' fields are copied in Rust.
std::vector<LieroWormData*> rustWorms(std::vector<Worm*> const& worms)
{
	std::vector<LieroWormData*> r;
	for (Worm* w : worms)
		r.push_back(w ? w->rust() : nullptr);
	return r;
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
	GameView view(game);
	return liero_replay_game_checksum(&view.fields);
}

void ReplayWriter::Writer::put(uint8_t const* p, std::size_t n)
{
	LieroStr s = { p, n };
	liero_deflate_out_write(out, s);
}

ReplayWriter::ReplayWriter(FsNode const& node)
: settingsExpired(true)
{
	writer.out = node.rust() ? liero_deflate_out_new(node.rust()) : nullptr;
	if (!writer.out)
		throw std::runtime_error("Could not write " + node.fullPath());
}

ReplayWriter::~ReplayWriter()
{
	endRecord();
	// Finishes the file.
	liero_deflate_out_free(writer.out);
}

ReplayReader::ReplayReader(FsNode const& node)
{
	std::vector<uint8_t> data;
	bool error = false;
	std::pair<std::vector<uint8_t>*, bool*> out(&data, &error);
	bool read = node.rust() && liero_inflate_node(node.rust(), &out, [](void* ctx, LieroStr d, bool e)
	{
		auto& o = *static_cast<std::pair<std::vector<uint8_t>*, bool*>*>(ctx);
		o.first->assign(d.ptr, d.ptr + d.len);
		*o.second = e;
	});
	if (!read)
		throw std::runtime_error("Could not read " + node.fullPath());
	// Past what inflated, gvl's reader threw what inflating it had.
	reader = ByteReader(std::move(data), error ? "Error while deflating" : "Read error in get()");
}

std::unique_ptr<Game> ReplayReader::beginPlayback(std::shared_ptr<Common> common, std::shared_ptr<SoundPlayer> soundPlayer)
{
	ReplaySource source(reader);
	ReplayResult r(liero_replay_read_start(&source.hooks));
	r.rethrow(source.error);
	LieroReplayGameOut const& g = *liero_replay_result_game(r.r);
	context.replayVersion = g.version;

	std::shared_ptr<Settings> settings(new Settings);
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
	std::vector<LieroWormData*> allData = rustWorms(all);
	LieroTcData const* tc = liero_tc_data(common->tc);
	for (std::size_t i = 0; i < g.worm_count; ++i)
	{
		liero_replay_worm_set(all[i]->rust(), &g.worms[i], allData.data(), allData.size(), tc);
		all[i]->settings.reset(new WormSettings);
		setWormSettings(*all[i]->settings, g.worm_settings[i]);
	}
	for (std::size_t i = 0; i < g.list_len; ++i)
		game->addWorm(g.list[i] < 0 ? 0 : all[g.list[i]]);

	for (std::size_t i = 0; i < g.viewport_count; ++i)
	{
		Viewport* vp = new Viewport();
		liero_replay_viewport_set(vp->rust(), &g.viewports[i]);
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
	std::vector<LieroWormData*> gameWorms = rustWorms(game.worms);
	LieroTcData const* tc = liero_tc_data(game.common->tc);
	for (auto* worm : game.worms)
	{
		worms.push_back(liero_replay_worm_of(worm->rust(), gameWorms.data(), gameWorms.size(), tc));
		wormSettings.push_back(wormSettingsView(*worm->settings));
	}
	std::vector<LieroReplayViewport> viewports;
	for (auto* vp : game.viewports)
		viewports.push_back(liero_replay_viewport_of(vp->rust()));
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

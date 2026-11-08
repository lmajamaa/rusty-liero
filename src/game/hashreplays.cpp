// Replay part of `--hash-run` (see hashrun.cpp): replays of the hash games
// played back, and short replays recorded in every version, started part way
// through a game, with settings changed while recording, and broken in ways
// a damaged or hand-made file could be.

#include "hashrun.hpp"

#include "game.hpp"
#include "replay.hpp"
#include "settings.hpp"
#include "viewport.hpp"
#include "spectatorviewport.hpp"
#include "filesystem.hpp"
#include "mixer/player.hpp"
#include "gfx/renderer.hpp"
#include <gvl/serialization/archive.hpp>

#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

namespace
{

typedef std::vector<uint8_t> Bytes;

FsNode tmpNode(std::string const& name)
{
	return FsNode("/tmp/" + name);
}

Bytes readAll(gvl::octet_reader r)
{
	Bytes data;
	uint8_t b;
	while (r.try_get(b))
		data.push_back(b);
	return data;
}

// What a replay file inflates to, and whether reading on failed.
struct Inflated
{
	Bytes data;
	bool error = false;
};

Inflated inflatedFile(FsNode const& node)
{
	Inflated r;
	if (!liero_inflate_node(node.rust(), &r, [](void* ctx, LieroStr d, bool e)
	{
		Inflated& o = *static_cast<Inflated*>(ctx);
		o.data.assign(d.ptr, d.ptr + d.len);
		o.error = e;
	}))
		throw std::runtime_error("Could not read " + node.fullPath());
	return r;
}

// A replay file's bytes before compression.
Bytes readReplay(FsNode const& node)
{
	Inflated r = inflatedFile(node);
	if (r.error)
		throw std::runtime_error("Error while deflating");
	return r.data;
}

// Compresses `data` into a replay file, as ReplayWriter does.
void writeReplay(FsNode const& node, Bytes const& data)
{
	LieroDeflateOut* out = liero_deflate_out_new(node.rust());
	if (!out)
		throw std::runtime_error("Could not write " + node.fullPath());
	LieroStr s = { data.data(), data.size() };
	liero_deflate_out_write(out, s);
	liero_deflate_out_free(out);
}

void addHash(Fnv& f, gvl::gash::value_type const& h)
{
	for (int i = 0; i < gvl::gash::value_type::size; ++i)
	{
		f.add(uint32_t(h.value[i]));
		f.add(uint32_t(h.value[i] >> 32));
	}
}

void add64(Fnv& f, uint64_t v)
{
	f.add(uint32_t(v));
	f.add(uint32_t(v >> 32));
}

// The settings, worm settings and indexes, viewports and level palette and
// materials, which a replay stores but hashGame leaves out.
void hashLoaded(Fnv& f, Game& game)
{
	addHash(f, game.settings->updateHash());
	f.add(uint32_t(game.worms.size()));
	for (Worm* w : game.worms)
	{
		f.add(w->index);
		addHash(f, w->settings->updateHash());
	}

	f.add(uint32_t(game.viewports.size()));
	for (Viewport* vp : game.viewports)
	{
		f.add(vp->x); f.add(vp->y); f.add(vp->shake);
		f.add(vp->maxX); f.add(vp->maxY); f.add(vp->centerX); f.add(vp->centerY);
		f.add(vp->wormIdx); f.add(vp->bannerY);
		f.add(vp->rect.x1); f.add(vp->rect.y1); f.add(vp->rect.x2); f.add(vp->rect.y2);
	}

	Level& level = game.level;
	f.add(level.width); f.add(level.height);
	for (Color const& c : level.origpal.entries)
	{
		f.add(uint32_t(c.r)); f.add(uint32_t(c.g)); f.add(uint32_t(c.b));
	}
	for (Material const& m : level.materials)
		f.add(uint32_t(m.flags));
}

// Starts a bot game as hashrun.cpp does and runs it `ticks` ticks.
std::unique_ptr<Game> botGame(gvl::shared_ptr<Common> common, uint32_t seed, int gameMode, int ticks)
{
	std::unique_ptr<Game> game = setUpGame(common, seed, gameMode, nullptr);
	game->startGame();
	for (int i = 0; i < ticks; ++i)
	{
		botTick(*game);
		game->processFrame();
	}
	return game;
}

// A change to make to the settings while recording.
struct SettingsChange
{
	int tick;
	int loadingTime, maxBonuses, blood;
	char const* wormName;
};

// Records `ticks` ticks of a bot game into `node`. With `startTick` 0 the
// recording starts as LocalController starts it, before startGame;
// otherwise after that many ticks.
void recordGame(gvl::shared_ptr<Common> common, FsNode const& node, uint32_t seed, int gameMode,
	int startTick, int ticks, int version,
	std::vector<SettingsChange> const& changes = std::vector<SettingsChange>())
{
	std::unique_ptr<Game> game;
	std::unique_ptr<ReplayWriter> replay(new ReplayWriter(node));
	replay->context.replayVersion = version;
	if (startTick == 0)
	{
		game = setUpGame(common, seed, gameMode, nullptr);
		replay->beginRecord(*game);
		game->startGame();
	}
	else
	{
		game = botGame(common, seed, gameMode, startTick);
		replay->beginRecord(*game);
	}

	for (int tick = 0; tick < ticks; ++tick)
	{
		for (SettingsChange const& c : changes)
		{
			if (c.tick != tick)
				continue;
			// As when the options menu is opened during a game.
			replay->unfocus();
			Settings& s = *game->settings;
			s.loadingTime = c.loadingTime;
			s.maxBonuses = c.maxBonuses;
			s.blood = c.blood;
			if (c.wormName)
				game->worms[1]->settings->name = c.wormName;
			replay->focus();
		}
		botTick(*game);
		replay->recordFrame();
		game->processFrame();
	}
}

Bytes settingsArchive(Settings const& settings)
{
	FsNode node = tmpNode("settings.bin");
	{
		gvl::octet_writer w(node.toOctetWriter());
		GameSerializationContext context;
		// Writing sets the settings too, so a copy is written.
		std::unique_ptr<Settings> copy(settings.clone());
		archive(gvl::out_archive<gvl::octet_writer, GameSerializationContext>(w, context), *copy);
	}
	return readAll(node.toOctetReader());
}

Bytes wormSettingsArchive(WormSettings const& ws)
{
	FsNode node = tmpNode("worm.bin");
	{
		gvl::octet_writer w(node.toOctetWriter());
		GameSerializationContext context;
		// Writing sets the settings too, so a copy is written.
		std::unique_ptr<WormSettings> copy(ws.clone());
		archive(gvl::out_archive<gvl::octet_writer, GameSerializationContext>(w, context), *copy);
	}
	return readAll(node.toOctetReader());
}

void putU32(Bytes& b, uint32_t v)
{
	for (int i = 3; i >= 0; --i)
		b.push_back(uint8_t(v >> (i * 8)));
}

Bytes replace(Bytes data, std::size_t pos, uint8_t v)
{
	data[pos] = v;
	return data;
}

Bytes insert(Bytes data, std::size_t pos, Bytes const& what)
{
	data.insert(data.begin() + pos, what.begin(), what.end());
	return data;
}

Bytes truncate(Bytes data, std::size_t len)
{
	data.resize(len);
	return data;
}

// Where frame `k` of a recording from the start of a game begins: each
// frame has a byte per worm, and frames on a multiple of 1050 a checksum.
std::size_t frameOffset(std::size_t firstFrame, int k)
{
	return firstFrame + 2 * k + 4 * ((k + 1049) / 1050);
}

} // namespace

// Plays a replay back as ReplayController does, for at most `maxTicks`
// ticks. Prints the replay file, the game it starts with, the game state
// every `interval` ticks and how it ended: the end of the replay, or the
// error that stopped it, with the game and settings then.
// With rewindAt, the replay is also rewound there, as R does in a replay.
void hashReplay(gvl::shared_ptr<Common> common, char const* name, FsNode const& node, int interval, int maxTicks,
	int rewindAt)
{
	{
		// What the file inflates to, not its compressed bytes, which
		// depend on the compressor.
		Fnv f;
		Inflated file = inflatedFile(node);
		f.add(uint32_t(file.data.size()));
		f.bytes(file.data.data(), file.data.size());
		f.add(file.error);
		std::printf("%s-replay-file 0 %016llx\n", name, (unsigned long long)f.h);
	}

	Renderer renderer;
	renderer.init(320, 200);
	renderer.loadPalette(*common);

	ReplayReader reader(node);
	std::unique_ptr<Game> game;
	std::string outcome = "replay ended";
	int ticks = 0;
	try
	{
		game = reader.beginPlayback(common, gvl::shared_ptr<SoundPlayer>(new NullSoundPlayer));
		{
			Fnv f;
			hashLoaded(f, *game);
			add64(f, hashGame(*game));
			std::printf("%s-replay-load 0 %016llx\n", name, (unsigned long long)f.h);
		}

		// As ReplayController::changeState(StateGame), without the single
		// screen option.
		game->clearViewports();
		game->worms[0]->statsX = 0;
		game->worms[1]->statsX = 218;
		game->addSpectatorViewport(new SpectatorViewport(gvl::rect(0, 0, 504 + 68, 350), 504, 350));
		game->addViewport(new Viewport(gvl::rect(0, 0, 158, 158), game->worms[0]->index, 504, 350));
		game->addViewport(new Viewport(gvl::rect(160, 0, 158+160, 158), game->worms[1]->index, 504, 350));
		game->startGame();
		game->focus(renderer);

		// What R rewinds to in ReplayController.
		std::unique_ptr<Game> initialGame;
		gvl::octet_reader initialReader = reader.reader;
		if (rewindAt > 0)
		{
			initialGame.reset(new Game(*game));
			initialGame->postClone(*game, true);
		}

		while (ticks < maxTicks)
		{
			if (!reader.playbackFrame(renderer))
				break;
			game->processFrame();
			++ticks;
			if (ticks % interval == 0)
				std::printf("%s-replay %d %016llx\n", name, ticks, (unsigned long long)hashGame(*game));

			if (initialGame && ticks == rewindAt)
			{
				// As R in ReplayController::process: back to the start, then
				// play to here again, which has to give the same game.
				*game = *initialGame;
				game->postClone(*initialGame, true);
				reader.reader = initialReader;
				int t = 0;
				while (t < rewindAt && reader.playbackFrame(renderer))
				{
					game->processFrame();
					++t;
				}
				std::printf("%s-replay-rewind %d %016llx\n", name, t, (unsigned long long)hashGame(*game));
				// The run's heap is small.
				initialGame.reset();
			}
		}
		if (ticks == maxTicks)
			outcome = "stopped";
	}
	catch (gvl::stream_error& e)
	{
		outcome = std::string("stream error: ") + e.what();
	}
	catch (gvl::archive_check_error& e)
	{
		outcome = std::string("archive error: ") + e.what();
	}
	catch (std::exception& e)
	{
		outcome = std::string("exception: ") + e.what();
	}

	Fnv f;
	f.bytes(reinterpret_cast<uint8_t const*>(outcome.data()), outcome.size());
	f.add(ticks);
	f.add(game ? 1 : 0);
	if (game)
	{
		add64(f, hashGame(*game));
		addHash(f, game->settings->updateHash());
		for (Worm* w : game->worms)
			addHash(f, w->settings->updateHash());
	}
	std::printf("%s-replay-end %d %016llx\n", name, ticks, (unsigned long long)f.h);
	std::fprintf(stderr, "%s: %s after %d ticks\n", name, outcome.c_str(), ticks);

	// The run has a fixed 16 MB heap, which holds the in-memory files too.
	std::remove(node.fullPath().c_str());
}

void hashReplays(gvl::shared_ptr<Common> common)
{
	char name[64];

	// Every version, from the start of a game past the second checksum.
	for (int version = 0; version <= myReplayVersion; ++version)
	{
		std::snprintf(name, sizeof name, "replay-v%d", version);
		FsNode node = tmpNode(std::string(name) + ".lrp");
		recordGame(common, node, 21, Settings::GMKillEmAll, 0, 2200, version);
		hashReplay(common, name, node, 200, 2200, 600);
	}

	// Started part way through games, when ropes, kills and the tag can
	// point at worms.
	int const starts[] = { 150, 900, 2400, 5000 };
	int const modes[] = { Settings::GMKillEmAll, Settings::GMGameOfTag, Settings::GMHoldazone };
	for (int mode : modes)
	{
		for (int start : starts)
		{
			std::snprintf(name, sizeof name, "replay-mode%d-from%d", mode, start);
			FsNode node = tmpNode(std::string(name) + ".lrp");
			recordGame(common, node, 22 + mode, mode, start, 600, myReplayVersion);
			hashReplay(common, name, node, 200, 600, 0);
		}
	}

	// Settings changed while recording, also on a checksum frame and in an
	// old version. Changing only a worm's name isn't recorded with two worms.
	{
		std::vector<SettingsChange> changes;
		changes.push_back(SettingsChange{ 100, 50, 2, 300, nullptr });
		changes.push_back(SettingsChange{ 101, 50, 2, 300, "Changed" });
		changes.push_back(SettingsChange{ 400, 200, 6, 0, nullptr });
		changes.push_back(SettingsChange{ 1050, 100, 4, 100, nullptr });
		for (int version : { myReplayVersion, 1 })
		{
			std::snprintf(name, sizeof name, "replay-settings-v%d", version);
			FsNode node = tmpNode(std::string(name) + ".lrp");
			recordGame(common, node, 24, Settings::GMKillEmAll, 0, 1300, version, changes);
			hashReplay(common, name, node, 100, 1300);
		}
	}

	// Broken replays, made from one recorded from the start of a game. Its
	// 1200 frames end with the end marker.
	FsNode baseNode = tmpNode("replay-base.lrp");
	recordGame(common, baseNode, 25, Settings::GMKillEmAll, 0, 1200, myReplayVersion);
	Bytes base = readReplay(baseNode);
	std::remove(baseNode.fullPath().c_str());
	std::size_t const firstFrame = base.size() - 1 - frameOffset(0, 1200);

	// The run's 16 MB heap can't hold all the broken copies at once, so
	// each is played as it's made.
	int n = 0;
	auto play = [&](Bytes const& data) {
		std::snprintf(name, sizeof name, "replay-broken-%d", n++);
		FsNode node = tmpNode(std::string(name) + ".lrp");
		writeReplay(node, data);
		hashReplay(common, name, node, 300, 1200);
	};

	play(replace(base, 0, base[0] ^ 1)); // Magic
	play(replace(base, 4, 5)); // Version too recent
	play(replace(base, 4, 0xff));
	for (int i = 0; i < 40; ++i)
		play(truncate(base, firstFrame * i / 40));
	for (int k : { 0, 1, 2, 3, 5, 6, 7 })
		play(truncate(base, firstFrame + k));
	play(truncate(base, frameOffset(firstFrame, 500) + 1));
	play(truncate(base, base.size() - 1)); // No end marker

	// The first worm's checkpoint.
	for (std::size_t i = 0; i + 4 <= firstFrame; ++i)
	{
		if (base[i] == 0x12 && base[i + 1] == 0x34 && base[i + 2] == 0x56 && base[i + 3] == 0x78)
		{
			play(replace(base, i + 3, 0x79));
			break;
		}
	}

	std::size_t const frame10 = frameOffset(firstFrame, 10);
	for (uint8_t header : { 0x80, 0x83, 0x84, 0xff })
		play(replace(base, frame10, header));
	play(replace(base, firstFrame + 2, base[firstFrame + 2] ^ 1)); // Checksum at frame 0
	std::size_t const checksum1050 = frameOffset(firstFrame, 1050) + 2;
	play(replace(base, checksum1050, base[checksum1050] ^ 0x40));

	// Settings for the game and for each worm in the middle of the frames,
	// for worms that exist and one that doesn't.
	Settings changed;
	changed.loadingTime = 30;
	changed.lives = 3;
	changed.blood = 500;
	Bytes settingsBytes(1, 0x81);
	Bytes archived = settingsArchive(changed);
	settingsBytes.insert(settingsBytes.end(), archived.begin(), archived.end());
	play(insert(base, frame10, settingsBytes));
	play(insert(base, frame10, truncate(settingsBytes, settingsBytes.size() / 2)));
	play(truncate(insert(base, frame10, settingsBytes), frame10 + settingsBytes.size() / 2));

	WormSettings ws;
	ws.name = "Inserted";
	ws.color = 40;
	ws.health = 50;
	Bytes wormArchive = wormSettingsArchive(ws);
	for (uint32_t id : { 0u, 1u })
	{
		Bytes wormBytes(1, 0x82);
		putU32(wormBytes, id);
		wormBytes.insert(wormBytes.end(), wormArchive.begin(), wormArchive.end());
		play(insert(base, frame10, wormBytes));
		play(truncate(insert(base, frame10, wormBytes), frame10 + wormBytes.size() - 3));
	}
	{
		Bytes unknown(1, 0x82);
		putU32(unknown, 0xffffffffu);
		play(insert(base, frame10, unknown));
	}
	{
		// Two settings in one frame, then the frame.
		Bytes both = settingsBytes;
		Bytes worm(1, 0x82);
		putU32(worm, 0);
		worm.insert(worm.end(), wormArchive.begin(), wormArchive.end());
		both.insert(both.end(), worm.begin(), worm.end());
		play(insert(base, frameOffset(firstFrame, 1050), both));
	}
}

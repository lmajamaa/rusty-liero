#ifndef UUID_4CF92C398C724F883A02E8A68FE1584F
#define UUID_4CF92C398C724F883A02E8A68FE1584F

#include "mixer/player.hpp"
#include <cstring>
#include <map>
#include <memory>
#include "worm.hpp"
#include "common.hpp"
#include "version.hpp"
#include "filesystem.hpp"
#include "bytes.hpp"

struct Game;

struct GameSerializationContext
{
	GameSerializationContext()
	: game(0)
	, replayVersion(myReplayVersion)
	{
	}

	struct WormData
	{
		WormData()
		: settingsExpired(true)
		{
		}

		SettingsHash lastSettingsHash;
		bool settingsExpired;
	};

	int version()
	{
		return replayVersion;
	}

	typedef std::map<Worm*, WormData> WormDataMap;

	Game* game;
	WormDataMap wormData;
	int replayVersion;
};

struct Replay
{
	Replay()
	{
	}

	GameSerializationContext context;

};

struct ReplayWriter : Replay
{
	// Starts a replay file at `node`, or throws if it can't be written.
	ReplayWriter(FsNode const& node);
	~ReplayWriter();

	void unfocus();
	void focus();

	// The file being written, compressed in Rust.
	struct Writer
	{
		void put(uint8_t const* p, std::size_t n);
		LieroDeflateOut* out;
	} writer;
	SettingsHash lastSettingsHash;
	bool settingsExpired;

	void beginRecord(Game& game);
	void recordFrame();
private:
	void endRecord();
};

struct Renderer;

struct ReplayReader : Replay
{
	// Reads the replay file at `node`, or throws if it can't be read.
	ReplayReader(FsNode const& node);

	void unfocus()
	{
		// Nothing
	}

	void focus()
	{
		// Nothing
	}

	std::unique_ptr<Game> beginPlayback(std::shared_ptr<Common> common, std::shared_ptr<SoundPlayer> soundPlayer);
	bool playbackFrame(Renderer& renderer);

	// The inflated file, and how far playback has read it.
	ByteReader reader;
};

#endif // UUID_4CF92C398C724F883A02E8A68FE1584F

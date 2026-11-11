#ifndef UUID_4CF92C398C724F883A02E8A68FE1584F
#define UUID_4CF92C398C724F883A02E8A68FE1584F

#include "mixer/player.hpp"
#include <memory>
#include "common.hpp"
#include "version.hpp"
#include "filesystem.hpp"

struct Game;
struct Renderer;

// Replay files, recorded and played in Rust (rust/liero-game/src/
// replay_file.rs) on the game itself. These throw what the C++ replay code
// threw: std::runtime_error, ArchiveCheckError for a broken replay and
// std::out_of_range for a worm index past the worms.

struct ReplayWriter
{
	// Starts a replay file at `node`, or throws if it can't be written.
	explicit ReplayWriter(FsNode const& node);
	// Ends the replay and closes the file.
	~ReplayWriter();
	ReplayWriter(ReplayWriter const&) = delete;
	ReplayWriter& operator=(ReplayWriter const&) = delete;

	// The version to write, before beginRecord (myReplayVersion otherwise).
	void setVersion(int version);

	// Remember the settings when the menu opens, and write those changed
	// when the game has focus again.
	void unfocus();
	void focus();

	// Writes the game as it starts. The game must outlive the writer.
	void beginRecord(Game& game);
	void recordFrame();

	LieroReplayWriter* w;
};

struct ReplayReader
{
	// Reads the replay file at `node`, or throws if it can't be read.
	explicit ReplayReader(FsNode const& node);
	~ReplayReader();
	ReplayReader(ReplayReader const&) = delete;
	ReplayReader& operator=(ReplayReader const&) = delete;

	// The game the replay starts with, which the reader then plays.
	std::unique_ptr<Game> beginPlayback(std::shared_ptr<Common> common, std::shared_ptr<SoundPlayer> soundPlayer);
	// One tick's controls and settings changes. False at the end.
	bool playbackFrame(Renderer& renderer);

	// How far playback has read, for R to rewind to.
	std::size_t pos() const { return liero_replay_reader_pos(r); }
	void setPos(std::size_t pos) { liero_replay_reader_set_pos(r, pos); }

	LieroReplayReader* r;
	Game* game;
};

#endif // UUID_4CF92C398C724F883A02E8A68FE1584F

#include "replay.hpp"

#include "game.hpp"
#include "settingsffi.hpp"

#include <stdexcept>
#include <string>

namespace
{

// Throws what the C++ replay code threw for the outcome.
void rethrow(LieroReplayOutcome const& o)
{
	std::string message(reinterpret_cast<char const*>(o.message.ptr), o.message.len);
	switch (o.status)
	{
	case LieroReplayStatus_ReadError:
		throw std::runtime_error(message);
	case LieroReplayStatus_CheckError:
		throw ArchiveCheckError(message);
	case LieroReplayStatus_OutOfRange:
		// What std::vector::at throws in libc++.
		throw std::out_of_range("vector");
	default:
		return;
	}
}

}

ReplayWriter::ReplayWriter(FsNode const& node)
: w(node.rust() ? liero_replay_writer_new(node.rust()) : nullptr)
{
	if (!w)
		throw std::runtime_error("Could not write " + node.fullPath());
}

ReplayWriter::~ReplayWriter()
{
	liero_replay_writer_free(w);
}

void ReplayWriter::setVersion(int version)
{
	liero_replay_writer_set_version(w, version);
}

void ReplayWriter::beginRecord(Game& game)
{
	rethrow(liero_replay_writer_begin(w, game.handle()));
}

void ReplayWriter::recordFrame()
{
	liero_replay_writer_frame(w);
}

void ReplayWriter::unfocus()
{
	liero_replay_writer_unfocus(w);
}

void ReplayWriter::focus()
{
	liero_replay_writer_focus(w);
}

ReplayReader::ReplayReader(FsNode const& node)
: r(node.rust() ? liero_replay_reader_new(node.rust()) : nullptr)
, game(nullptr)
{
	if (!r)
		throw std::runtime_error("Could not read " + node.fullPath());
}

ReplayReader::~ReplayReader()
{
	liero_replay_reader_free(r);
}

std::unique_ptr<Game> ReplayReader::beginPlayback(std::shared_ptr<Common> common, std::shared_ptr<SoundPlayer> soundPlayer)
{
	std::shared_ptr<Settings> settings(new Settings);
	std::unique_ptr<Game> g(new Game(common, settings, soundPlayer));
	rethrow(liero_replay_reader_begin(r, g->handle()));
	g->syncViews();
	game = g.get();
	return g;
}

bool ReplayReader::playbackFrame(Renderer& renderer)
{
	LieroReplayOutcome o = liero_replay_reader_frame(r);
	if (o.end)
		return false;
	if (o.settings_changed)
		game->updateSettings(renderer);
	rethrow(o);
	return true;
}

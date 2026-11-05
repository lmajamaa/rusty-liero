#ifndef LIERO_SETTINGS_HPP
#define LIERO_SETTINGS_HPP

#include "worm.hpp"
#include <string>
#include <cstring>
#include <gvl/resman/shared_ptr.hpp>
#include <gvl/support/cstdint.hpp>
#include <gvl/crypt/gash.hpp>
#include <gvl/serialization/archive.hpp>
#include <stdexcept>
#include "settingsffi.hpp"

// The settings the original LIERO.DAT didn't have. The fields and their
// defaults match LieroSettings and liero_data::settings::Settings.
struct Extensions
{
	static bool const extensions = true;

	// Extensions
	bool recordReplays;
	bool loadPowerlevelPalette;
	int32_t bloodParticleMax;

	int aiFrames, aiMutations;
	bool aiTraces;
	int32_t aiParallels;

	bool fullscreen;

	int32_t zoneTimeout;
	uint32_t selectBotWeapons;

	bool allowViewingSpawnPoint;
	bool singleScreenReplay;
	bool spectatorWindow;
	std::string tc;
};

struct Rand;

struct Settings : gvl::shared, Extensions
{
	enum
	{
		GMKillEmAll,
		GMGameOfTag,
		GMHoldazone,
		GMScalesOfJustice
	};

	static int const selectableWeapons = 5;
	static int const zoneCaptureTime = 70;

	static int const wormAnimTab[];

	Settings();

	// liero.cfg. Returns false if it can't be read or parsed, keeping the
	// settings read before the error. Like the C++ reader before it, throws
	// std::out_of_range if a setting is missing.
	bool load(FsNode node, Rand& rand);
	// The original game's LIERO.DAT. Returns false if it ends early.
	bool loadLegacy(FsNode node, Rand& rand);
	void save(FsNode node, Rand& rand);
	// The gash of the settings archive (see archive below). Writing the archive
	// sets the TC name to the default, since the archive has none.
	gvl::gash::value_type& updateHash();

	static void generateName(WormSettings& ws, Rand& rand);

	uint32_t weapTable[40];
	int32_t maxBonuses;
	int32_t blood;
	int32_t timeToLose;
	int32_t flagsToWin;
	uint32_t gameMode;
	bool shadow;
	bool loadChange;
	bool namesOnBonuses;
	bool regenerateLevel;
	int32_t lives;
	int32_t loadingTime;
	bool randomLevel;
	std::string levelFile;
	bool map;
	bool screenSync;

	gvl::shared_ptr<WormSettings> wormSettings[2];

	gvl::gash::value_type hash;
};

// The settings in a replay, without the worms, which replays store with each
// worm. The format is in liero_data::settings::read_archive and
// write_archive.
template<typename Reader, typename Context>
void archive(gvl::in_archive<Reader, Context> ar, Settings& settings)
{
	ArchiveByteSource<Reader> source(ar.reader);
	LieroSettings view = settingsView(settings);
	SettingsResult r(liero_settings_read_archive(&view, &source.hooks));
	setSettings(settings, r.settings());
	source.rethrow(r.status());
}

template<typename Writer, typename Context, bool Reread>
void archive(gvl::out_archive<Writer, Context, Reread> ar, Settings& settings)
{
	// Writing resets the TC name as a reread of the archive would.
	static_assert(Reread, "Settings are only written by rereading archives");
	LieroSettings view = settingsView(settings);
	SettingsResult r(liero_settings_write_archive(&view));
	setSettings(settings, r.settings());
	r.writeTo(ar.writer);
}

#endif // LIERO_SETTINGS_HPP

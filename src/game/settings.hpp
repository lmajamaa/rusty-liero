#ifndef LIERO_SETTINGS_HPP
#define LIERO_SETTINGS_HPP

#include "worm.hpp"
#include <memory>
#include <string>
#include <cstring>
#include <gvl/resman/shared_ptr.hpp>
#include <gvl/support/cstdint.hpp>
#include <gvl/crypt/gash.hpp>
#include <stdexcept>
#include "settingsffi.hpp"

struct Rand;

// The game's settings. Rust owns them (LieroSettingsBox in
// rust/liero-game/src/settings.rs); the fields here refer to its. The fields
// and their defaults match liero_data::settings::Settings. The worms'
// settings are apart, since each worm holds on to its own.
struct Settings : gvl::shared
{
	static bool const extensions = true;

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
	Settings(Settings const&) = delete;
	Settings& operator=(Settings const&) = delete;
	// A copy with settings of its own, sharing the worms' settings. The
	// game shares one Settings; only the hash run copies them.
	std::unique_ptr<Settings> clone() const;
	~Settings();

	// The settings in Rust, for Rust code to work on.
	LieroSettingsBox* rust() const { return box; }

	// liero.cfg. Returns false if it can't be read or parsed, keeping the
	// settings read before the error. Like the C++ reader before it, throws
	// std::out_of_range if a setting is missing.
	bool load(FsNode node, Rand& rand);
	// The original game's LIERO.DAT. Returns false if it ends early.
	bool loadLegacy(FsNode node, Rand& rand);
	void save(FsNode node, Rand& rand);
	// What save writes.
	std::string text();
	// The gash of the settings archive (see archive below). Writing the archive
	// sets the TC name to the default, since the archive has none.
	gvl::gash::value_type& updateHash();

	static void generateName(WormSettings& ws, Rand& rand);

private:
	explicit Settings(LieroSettingsBox* box);

	static LieroStr getTc(void const* b) { return liero_settings_box_tc(static_cast<LieroSettingsBox const*>(b)); }
	static void setTc(void* b, LieroStr s) { liero_settings_box_set_tc(static_cast<LieroSettingsBox*>(b), s); }
	static LieroStr getLevelFile(void const* b) { return liero_settings_box_level_file(static_cast<LieroSettingsBox const*>(b)); }
	static void setLevelFile(void* b, LieroStr s) { liero_settings_box_set_level_file(static_cast<LieroSettingsBox*>(b), s); }

	LieroSettingsBox* box;
	LieroSettingsData& data;

public:
	// Extensions: what the original LIERO.DAT didn't have.
	bool& recordReplays;
	bool& loadPowerlevelPalette;
	int32_t& bloodParticleMax;

	int32_t& aiFrames;
	int32_t& aiMutations;
	bool& aiTraces;
	int32_t& aiParallels;

	bool& fullscreen;

	int32_t& zoneTimeout;
	uint32_t& selectBotWeapons;

	bool& allowViewingSpawnPoint;
	bool& singleScreenReplay;
	bool& spectatorWindow;
	SettingsText tc;

	uint32_t (&weapTable)[40];
	int32_t& maxBonuses;
	int32_t& blood;
	int32_t& timeToLose;
	int32_t& flagsToWin;
	uint32_t& gameMode;
	bool& shadow;
	bool& loadChange;
	bool& namesOnBonuses;
	bool& regenerateLevel;
	int32_t& lives;
	int32_t& loadingTime;
	bool& randomLevel;
	SettingsText levelFile;
	bool& map;
	bool& screenSync;

	gvl::shared_ptr<WormSettings> wormSettings[2];

	gvl::gash::value_type hash;
};

// The settings in a replay, without the worms, which replays store with each
// worm. The format is in liero_data::settings::read_archive and
// write_archive.
template<typename Reader>
void readSettingsArchive(Reader& reader, Settings& settings)
{
	ArchiveByteSource<Reader> source(reader);
	LieroSettings view = settingsView(settings);
	SettingsResult r(liero_settings_read_archive(&view, &source.hooks));
	setSettings(settings, r.settings());
	source.rethrow(r.status());
}

// Writing resets the TC name, as a reread of the archive would.
inline std::vector<uint8_t> writeSettingsArchive(Settings& settings)
{
	LieroSettings view = settingsView(settings);
	SettingsResult r(liero_settings_write_archive(&view));
	setSettings(settings, r.settings());
	return r.bytes();
}

#endif // LIERO_SETTINGS_HPP

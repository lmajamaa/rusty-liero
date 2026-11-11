#include "settings.hpp"

#include "keys.hpp"
#include "gfx.hpp"
#include "filesystem.hpp"


#include <stdexcept>
#include <vector>

int const Settings::wormAnimTab[] =
{
	0,
	7,
	0,
	14
};

Settings::Settings(LieroSettingsBox* box)
: box(box)
, data(*liero_settings_box_data(box))
, recordReplays(data.record_replays)
, loadPowerlevelPalette(data.load_powerlevel_palette)
, bloodParticleMax(data.blood_particle_max)
, aiFrames(data.ai_frames)
, aiMutations(data.ai_mutations)
, aiTraces(data.ai_traces)
, aiParallels(data.ai_parallels)
, fullscreen(data.fullscreen)
, zoneTimeout(data.zone_timeout)
, selectBotWeapons(data.select_bot_weapons)
, allowViewingSpawnPoint(data.allow_viewing_spawn_point)
, singleScreenReplay(data.single_screen_replay)
, spectatorWindow(data.spectator_window)
, tc(box, &getTc, &setTc)
, weapTable(data.weap_table)
, maxBonuses(data.max_bonuses)
, blood(data.blood)
, timeToLose(data.time_to_lose)
, flagsToWin(data.flags_to_win)
, gameMode(data.game_mode)
, shadow(data.shadow)
, loadChange(data.load_change)
, namesOnBonuses(data.names_on_bonuses)
, regenerateLevel(data.regenerate_level)
, lives(data.lives)
, loadingTime(data.loading_time)
, randomLevel(data.random_level)
, levelFile(box, &getLevelFile, &setLevelFile)
, map(data.map)
, screenSync(data.screen_sync)
{
	for (int i = 0; i < 2; ++i)
		wormSettings[i] = WormSettings::share(liero_settings_box_worm(box, i));
}

Settings::Settings()
: Settings(liero_settings_box_new())
{
	SettingsResult r(liero_settings_default());
	setSettings(*this, r.settings());
}

std::unique_ptr<Settings> Settings::clone() const
{
	return std::unique_ptr<Settings>(new Settings(liero_settings_box_clone(box)));
}

Settings::~Settings()
{
	liero_settings_box_free(box);
}

namespace
{

void setStr(SettingsText& s, LieroStr v)
{
	s = std::string(reinterpret_cast<char const*>(v.ptr), v.len);
}

LieroStr str(SettingsText const& s)
{
	return s.view();
}

template<typename T, typename U, std::size_t N>
void copyArray(T (&dest)[N], U const (&src)[N])
{
	for (std::size_t i = 0; i < N; ++i)
		dest[i] = src[i];
}


}

LieroWormSettings wormSettingsView(WormSettings const& ws)
{
	LieroWormSettings v;
	v.health = ws.health;
	v.controller = ws.controller;
	copyArray(v.controls, ws.controls);
	copyArray(v.weapons, ws.weapons);
	v.name = str(ws.name);
	copyArray(v.rgb, ws.rgb);
	v.random_name = ws.randomName;
	v.color = ws.color;
	copyArray(v.controls_ex, ws.controlsEx);
	return v;
}

void setWormSettings(WormSettings& ws, LieroWormSettings const& v)
{
	ws.health = v.health;
	ws.controller = v.controller;
	copyArray(ws.controls, v.controls);
	copyArray(ws.weapons, v.weapons);
	setStr(ws.name, v.name);
	copyArray(ws.rgb, v.rgb);
	ws.randomName = v.random_name;
	ws.color = v.color;
	copyArray(ws.controlsEx, v.controls_ex);
}

LieroSettings settingsView(Settings const& s)
{
	LieroSettings v;
	v.record_replays = s.recordReplays;
	v.load_powerlevel_palette = s.loadPowerlevelPalette;
	v.blood_particle_max = s.bloodParticleMax;
	v.ai_frames = s.aiFrames;
	v.ai_mutations = s.aiMutations;
	v.ai_traces = s.aiTraces;
	v.ai_parallels = s.aiParallels;
	v.fullscreen = s.fullscreen;
	v.zone_timeout = s.zoneTimeout;
	v.select_bot_weapons = s.selectBotWeapons;
	v.allow_viewing_spawn_point = s.allowViewingSpawnPoint;
	v.single_screen_replay = s.singleScreenReplay;
	v.spectator_window = s.spectatorWindow;
	v.tc = str(s.tc);

	copyArray(v.weap_table, s.weapTable);
	v.max_bonuses = s.maxBonuses;
	v.blood = s.blood;
	v.time_to_lose = s.timeToLose;
	v.flags_to_win = s.flagsToWin;
	v.game_mode = s.gameMode;
	v.shadow = s.shadow;
	v.load_change = s.loadChange;
	v.names_on_bonuses = s.namesOnBonuses;
	v.regenerate_level = s.regenerateLevel;
	v.lives = s.lives;
	v.loading_time = s.loadingTime;
	v.random_level = s.randomLevel;
	v.level_file = str(s.levelFile);
	v.map = s.map;
	v.screen_sync = s.screenSync;

	for (int i = 0; i < 2; ++i)
		v.worms[i] = wormSettingsView(*s.wormSettings[i]);
	return v;
}

void setSettings(Settings& s, LieroSettings const& v)
{
	s.recordReplays = v.record_replays;
	s.loadPowerlevelPalette = v.load_powerlevel_palette;
	s.bloodParticleMax = v.blood_particle_max;
	s.aiFrames = v.ai_frames;
	s.aiMutations = v.ai_mutations;
	s.aiTraces = v.ai_traces;
	s.aiParallels = v.ai_parallels;
	s.fullscreen = v.fullscreen;
	s.zoneTimeout = v.zone_timeout;
	s.selectBotWeapons = v.select_bot_weapons;
	s.allowViewingSpawnPoint = v.allow_viewing_spawn_point;
	s.singleScreenReplay = v.single_screen_replay;
	s.spectatorWindow = v.spectator_window;
	setStr(s.tc, v.tc);

	copyArray(s.weapTable, v.weap_table);
	s.maxBonuses = v.max_bonuses;
	s.blood = v.blood;
	s.timeToLose = v.time_to_lose;
	s.flagsToWin = v.flags_to_win;
	s.gameMode = v.game_mode;
	s.shadow = v.shadow;
	s.loadChange = v.load_change;
	s.namesOnBonuses = v.names_on_bonuses;
	s.regenerateLevel = v.regenerate_level;
	s.lives = v.lives;
	s.loadingTime = v.loading_time;
	s.randomLevel = v.random_level;
	setStr(s.levelFile, v.level_file);
	s.map = v.map;
	s.screenSync = v.screen_sync;

	for (int i = 0; i < 2; ++i)
		setWormSettings(*s.wormSettings[i], v.worms[i]);
}

void throwArchiveError(LieroSettingsStatus status, std::exception_ptr const& readerError)
{
	switch (status)
	{
	case LieroSettingsStatus_ReadError:
		if (readerError)
			std::rethrow_exception(readerError);
		throw std::runtime_error("Read error in get()");
	case LieroSettingsStatus_CheckError:
		throw ArchiveCheckError("Expected checkpoint here");
	default:
		return;
	}
}

bool Settings::load(FsNode node, Rand& rand)
{
	try
	{
		LieroSettings view = settingsView(*this);
		LieroSettingsResult* read = node.rust() ? liero_settings_load(&view, node.rust(), false) : nullptr;
		if (!read)
			return false;
		SettingsResult r(read);
		setSettings(*this, r.settings());

		switch (r.status())
		{
		case LieroSettingsStatus_Ok:
			return true;
		case LieroSettingsStatus_MissingKey:
			// What std::map::at threw in the C++ reader. Nothing catches it.
			throw std::out_of_range("Missing setting in liero.cfg");
		default:
			return false;
		}
	}
	catch (std::runtime_error&)
	{
		return false;
	}
}

bool Settings::loadLegacy(FsNode node, Rand& rand)
{
	try
	{
		LieroSettings view = settingsView(*this);
		LieroSettingsResult* read = node.rust() ? liero_settings_load(&view, node.rust(), true) : nullptr;
		if (!read)
			return false;
		SettingsResult r(read);
		setSettings(*this, r.settings());
		return r.status() == LieroSettingsStatus_Ok;
	}
	catch (std::runtime_error&)
	{
		return false;
	}
}

SettingsHash Settings::updateHash()
{
	LieroSettings view = settingsView(*this);
	SettingsResult r(liero_settings_hash(&view));
	setSettings(*this, r.settings());
	return r.hash();
}

void Settings::save(FsNode node, Rand& rand)
{
	LieroSettings view = settingsView(*this);
	if (!node.rust() || !liero_settings_save(&view, node.rust()))
		throw std::runtime_error("Could not write " + node.fullPath());
}

std::string Settings::text()
{
	LieroSettings view = settingsView(*this);
	SettingsResult r(liero_settings_write_text(&view));
	LieroStr b = liero_settings_result_bytes(r.r);
	return std::string(reinterpret_cast<char const*>(b.ptr), b.len);
}

void Settings::generateName(WormSettings& ws, Rand& rand)
{
}

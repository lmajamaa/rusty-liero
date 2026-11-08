#ifndef LIERO_MENU_MENUIDS_HPP
#define LIERO_MENU_MENUIDS_HPP

// The menus' item ids. The menus and what their items do are in Rust
// (rust/liero-game/src/menus.rs), which has the same numbers.

struct MainMenu
{
	enum
	{
		MaResumeGame,
		MaNewGame,
		MaSettings,
		MaPlayer1Settings,
		MaPlayer2Settings,
		MaAdvanced,
		MaQuit,
		MaReplays,
		MaReplay,
		MaTc
	};
};

struct SettingsMenu
{
	enum
	{
		SiGameMode,
		SiLives,
		SiTimeToLose,
		SiTimeToWin,
		SiZoneTimeout,
		SiFlagsToWin,
		SiLoadingTimes,
		SiMaxBonuses,
		SiNamesOnBonuses,
		SiMap,
		SiAmountOfBlood,
		SiLevel,
		SiRegenerateLevel,
		SiWeaponOptions,
		LoadOptions,
		SaveOptions,
		LoadChange,
	};
};

struct PlayerMenu
{
	enum
	{
		PlName,
		PlHealth,
		PlRed, PlGreen, PlBlue,
		PlUp,
		PlDown,
		PlLeft,
		PlRight,
		PlFire,
		PlChange,
		PlJump,
		PlDig,
		PlWeap0,
		PlController = PlWeap0 + 5,
		PlSaveProfile,
		PlSaveProfileAs,
		PlLoadProfile,
		PlLoadedProfile,
	};
};

struct HiddenMenu
{
	enum
	{
		RecordReplays,
		LoadPowerLevels,
		DoubleRes,
		Fullscreen,
		AiFrames,
		AiMutations,
		PaletteSelect,
		Shadows,
		ScreenSync,
		SelectBotWeapons,
		AiTraces,
		AiParallels,
		AllowViewingSpawnPoint,
		SingleScreenReplay,
	};
};

#endif // LIERO_MENU_MENUIDS_HPP

#ifndef LIERO_HASHRUN_HPP
#define LIERO_HASHRUN_HPP

// Plays fixed-seed bot games headlessly and prints game-state hashes.
// See hashrun.cpp.
int hashRun(int argc, char* argv[]);

#include <memory>
#include <cstddef>
#include <cstdint>
#include <memory>
#include "math.hpp"

struct Common;
struct FsNode;
struct Game;
struct Renderer;

// The hash of the whole game state that the hash run prints.
uint64_t hashGame(Game& game);
// A bot-vs-bot game as the hash run sets it up, before startGame, with
// random weapons when `weapons` is null.
std::unique_ptr<Game> setUpGame(std::shared_ptr<Common> common, uint32_t seed, int gameMode, char const* const* weapons);
// The bots' controls for the next tick.
void botTick(Game& game);

// Drives the menus and weapon selection with scripted keys and prints hashes
// of what they show. See hashmenus.cpp.
void hashMenus(std::shared_ptr<Common> common);
void hashTc(Common& common);
void hashRandomTc();
// Hashes settings written, read and broken in every format. See
// hashsettings.cpp.
void hashSettings();
// Hashes the stats recorder and the stats screen. See hashstats.cpp.
uint64_t hashStats(Game& game);
void hashStatsScreen(char const* name, Game& game, Renderer& renderer);
// Plays back and hashes a replay file, and replays recorded, changed and
// broken in other ways. See hashreplays.cpp.
void hashReplay(std::shared_ptr<Common> common, char const* name, FsNode const& node, int interval, int maxTicks,
	int rewindAt = 0);
void hashReplays(std::shared_ptr<Common> common);
// Games between the predictive bots, hashing their state too. See
// hashai.cpp.
void hashAiGames(std::shared_ptr<Common> common);
// The sound mixer and the key handling. See hashsfx.cpp.
void hashSfxAndInput(std::shared_ptr<Common> common);
// The whole app, from the main menu through a game and back, driven by
// scripted keys. See hashapp.cpp.
void hashApp(std::shared_ptr<Common> common);

// 64-bit FNV-1a over 32-bit words, the same as the Rust tests use.
struct Fnv
{
	uint64_t h = 0xcbf29ce484222325ull;

	void add(uint32_t v)
	{
		for (int i = 0; i < 4; ++i)
		{
			h ^= (v >> (i * 8)) & 0xff;
			h *= 0x100000001b3ull;
		}
	}

	void add(int32_t v) { add(uint32_t(v)); }
	void add(bool v) { add(uint32_t(v ? 1 : 0)); }
	void add(fixedvec v) { add(v.x); add(v.y); }

	void bytes(uint8_t const* p, std::size_t n)
	{
		for (std::size_t i = 0; i < n; ++i)
		{
			h ^= p[i];
			h *= 0x100000001b3ull;
		}
	}
};


#endif // LIERO_HASHRUN_HPP

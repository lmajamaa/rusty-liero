// Headless determinism check for the Rust port.
//
// `openliero --hash-run [ticks] [interval]` plays bot-vs-bot games from fixed
// seeds on the bundled TC, without a window or sound, and prints a hash of the
// whole game state every `interval` ticks, then draws the split-screen and
// spectator views off-screen and prints a hash of their pixels and palettes
// (the lines ending in "frame"). tools/hashrun/run.ts runs it with
// Bun and compares the output with tools/hashrun/expected.txt, which was made
// from the original C++ game logic. A port that changes any behaviour changes
// a hash.

#include "hashrun.hpp"

#include "math.hpp"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <stdexcept>
#include <string>

// A section of the hash run that is in Rust (rust/liero-game/src/hashrun).
void rustSection(char const* name, int ticks, int interval)
{
	LieroStr n = { reinterpret_cast<uint8_t const*>(name), std::strlen(name) };
	bool known = liero_hash_section(n, ticks, interval, nullptr, [](void*, LieroStr line)
	{
		std::printf("%.*s\n", int(line.len), reinterpret_cast<char const*>(line.ptr));
	});
	if (!known)
		throw std::runtime_error(std::string("No hash section ") + name);
}

// `--hash-run [ticks] [interval] [sections]`: `sections`, a comma-separated
// list of the names below, runs only those (tools/hashrun/run.ts --only=).
int hashRun(int argc, char* argv[])
{
	int ticks = argc > 2 ? std::atoi(argv[2]) : 30000;
	int interval = argc > 3 ? std::atoi(argv[3]) : 100;
	std::string only = argc > 4 ? argv[4] : "";
	auto run = [&only](char const* name)
	{
		bool on = only.empty() || ("," + only + ",").find(std::string(",") + name + ",") != std::string::npos;
		// Where its lines start, for run.ts to match them on their own.
		if (on)
			std::printf("hash-section %s\n", name);
		return on;
	};

	precomputeTables();

	if (run("tc"))
		rustSection("tc", ticks, interval);

	if (run("games"))
		rustSection("games", ticks, interval);

	if (run("menus"))
		rustSection("menus", ticks, interval);
	if (run("settings"))
		rustSection("settings", ticks, interval);
	if (run("replays"))
		rustSection("replays", ticks, interval);
	if (run("ai"))
		rustSection("ai", ticks, interval);
	if (run("input"))
		rustSection("input", ticks, interval);
	if (run("app"))
		rustSection("app", ticks, interval);
	if (run("levels"))
		rustSection("levels", ticks, interval);
	if (run("randomtc"))
		rustSection("randomtc", ticks, interval);

	return 0;
}

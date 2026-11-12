#ifndef UUID_DC1D9513CDD34960AB8A648004DA149D
#define UUID_DC1D9513CDD34960AB8A648004DA149D

#include <SDL.h>

#include "gfx.hpp"
#include "screen.hpp"
#include "sfx.hpp"

#include <cstdio>
#include <cstring>
#include <ctime>
#include <exception>
#include <stdexcept>
#include <string>
#include <liero_game.h>

namespace
{

LieroStr lstr(std::string const& s)
{
	return LieroStr{ reinterpret_cast<uint8_t const*>(s.data()), s.size() };
}

std::string shellError()
{
	LieroStr e = liero_shell_error();
	return std::string(reinterpret_cast<char const*>(e.ptr), e.len);
}

}

int gameEntry(int argc, char* argv[])
try
{
	std::printf("%s\n", liero_game_version());

	// TODO: Better PRNG seeding
	liero_rand_seed(liero_app_rand(gfx.app), uint32_t(std::time(0)));

	std::string tcName; // Default to the settings' TC
	std::string configPath; // Default to current dir
	std::string tcPath; // Default to TC in the config folder

	for(int i = 1; i < argc; ++i)
	{
		if(argv[i][0] == '-')
		{
			switch(argv[i][1])
			{
			case '-':
				if (std::strcmp(argv[i] + 2, "config-root") == 0 && i + 1 < argc)
				{
					++i;
					configPath = argv[i];
				}
				else if (std::strcmp(argv[i] + 2, "tc-root") == 0 && i + 1 < argc)
				{
					++i;
					tcPath = argv[i];
				}
				break;
			}
		}
		else
		{
			tcName = argv[i];
		}
	}

	SDL_Init(SDL_INIT_VIDEO | SDL_INIT_GAMECONTROLLER);
#ifdef __EMSCRIPTEN__
	// SDL starts with text input on, and then lets the browser handle keys
	// that type characters, so Space scrolls and / opens Firefox's quick find.
	// With it off, SDL calls preventDefault on all keys. The text box turns it
	// on while a name is typed.
	SDL_StopTextInput();
#endif

	liero_menus_load(gfx.menus);

	gfx.init();

	// The settings and the TC (rust/liero-game/src/shell.rs).
	if (!liero_shell_start(gfx.app, lstr(configPath), lstr(tcPath), lstr(tcName)))
		throw std::runtime_error(shellError());

	gfx.setVideoMode();
	sfx.init();

	startScreens(mainScreen(), []() {
		if (!liero_shell_save_settings(gfx.app))
			throw std::runtime_error(shellError());

		sfx.deinit();
		SDL_Quit();
	});


	return 0;
}
catch(std::exception&)
{
	SDL_Quit();
	throw;
}

#endif // UUID_DC1D9513CDD34960AB8A648004DA149D

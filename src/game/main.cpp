#ifndef UUID_DC1D9513CDD34960AB8A648004DA149D
#define UUID_DC1D9513CDD34960AB8A648004DA149D

#include <SDL.h>

#include "gfx.hpp"
#include "screen.hpp"
#include "sfx.hpp"
#include "sys.hpp"
#include "game.hpp"
#include "viewport.hpp"
#include "worm.hpp"
#include "reader.hpp"
#include "filesystem.hpp"
#include "text.hpp"
#include "keys.hpp"
#include "constants.hpp"
#include "math.hpp"
#include "console.hpp"
#include <gvl/support/platform.hpp>

#include <cstdio>
#include <ctime>
#include <exception>
#include <gvl/math/cmwc.hpp>
#include <liero_game.h>
#include "hashrun.hpp"

int gameEntry(int argc, char* argv[])
try
{
	if (argc > 1 && std::strcmp(argv[1], "--hash-run") == 0)
		return hashRun(argc, argv);

	std::printf("%s\n", liero_game_version());

	// TODO: Better PRNG seeding
	gfx.rand.seed(Uint32(std::time(0)));

#if 0
	std::string const& path = "D:\\cpp\\liero\\tc";
	FsNode node(path);

	for (DirectoryListing dir = node.iter(); dir; ++dir)
	{
		auto d = *dir;

		try
		{
			auto zipNode = node / d;

			bool found = false;

			for (DirectoryListing zipDir = zipNode.iter(); zipDir; ++zipDir)
			{
				auto e = *zipDir;
				auto upper = e;
				toUpperCase(upper);

				if (upper.find(".EXE") != std::string::npos && upper != "LEVEDIT.EXE")
				{
					found = true;
					Common common(zipNode, e);

					printf("N: %s\n", common.guessName().c_str());
					//printf("N: %s ||| %s\n", common.S[SCopyright2].c_str(), common.S[SCopyright].c_str() + 4);
					break;
				}
			}

			if (!found)
				printf("** No exe found for %s\n", d.c_str());
		}
		catch (std::runtime_error& e)
		{
			//printf("Failed to load: %s\n", d.c_str());
			//printf("%s\n", e.what());
		}
	}

	return 0;
#endif

	bool tcSet = false;

	std::string tcName;
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
			tcSet = true;
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

	precomputeTables();

	gfx.loadMenus();

	gfx.init();
	gfx.setConfigPath(configPath);

	FsNode configNode(gfx.getConfigNode());
	if (tcPath.empty())
		gfx.tcNode = configNode / "TC";
	else
		gfx.setTcPath(tcPath);

	if (!gfx.loadSettings(configNode / "liero.cfg"))
	{
		if (!gfx.loadSettingsLegacy(configNode / "LIERO.DAT"))
		{
			gfx.settings.reset(new Settings);
			gfx.saveSettings(configNode / "liero.cfg");
		}
	}

	if (tcSet)
		gfx.settings->tc = tcName;
#ifdef __EMSCRIPTEN__
	// What liero.cfg holds now, so the menu only writes it on a change.
	gfx.savedSettingsText = gfx.settings->text();
#endif

	// TC loading
	FsNode lieroRoot(gfx.getTcNode() / gfx.settings->tc);
	gvl::shared_ptr<Common> common(new Common());
	common->load(std::move(lieroRoot));
	gfx.common = common;
	gfx.playRenderer.loadPalette(*common); // This gets the palette from common

	gfx.setVideoMode();
	sfx.init();

	startScreens(mainScreen(), [configNode]() {
		gfx.settings->save(configNode / "liero.cfg", gfx.rand);

		sfx.deinit();
		SDL_Quit();
	});

	//gvl::present_profile(std::cout);

	return 0;
}
catch(std::exception&)
{
	SDL_Quit();
	throw;
}

#endif // UUID_DC1D9513CDD34960AB8A648004DA149D

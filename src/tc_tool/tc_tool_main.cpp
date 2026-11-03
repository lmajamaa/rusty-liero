#include "tc_convert.hpp"
#include "game/filesystem.hpp"
#include <cstdio>
#include <cstring>
#include <exception>

static void printUsage(FILE* out)
{
	fprintf(out,
		"Usage: tctool [options] <path-to-tc>\n"
		"\n"
		"Converts a Liero installation (LIERO.EXE, LIERO.CHR, LIERO.SND) into a TC folder.\n"
		"\n"
		"Options:\n"
		"  --tc-name NAME      Name of the TC folder to write (default: name of <path-to-tc>)\n"
		"  --config-root DIR   Folder that holds the TC folder (default: current folder)\n"
		"  -h, --help          Show this help\n");
}

int main(int argc, char *argv[])
{
	std::string configPath; // Default to current dir
	std::string exePath, tcName;

	for(int i = 1; i < argc; ++i)
	{
		char const* arg = argv[i];

		if (std::strcmp(arg, "-h") == 0 || std::strcmp(arg, "--help") == 0)
		{
			printUsage(stdout);
			return 0;
		}
		else if (std::strcmp(arg, "--config-root") == 0 || std::strcmp(arg, "--tc-name") == 0)
		{
			if (i + 1 >= argc)
			{
				fprintf(stderr, "tctool: %s needs a value\n", arg);
				return 2;
			}

			++i;
			if (std::strcmp(arg, "--config-root") == 0)
				configPath = argv[i];
			else
				tcName = argv[i];
		}
		else if (arg[0] == '-')
		{
			fprintf(stderr, "tctool: unknown option %s\n\n", arg);
			printUsage(stderr);
			return 2;
		}
		else if (!exePath.empty())
		{
			fprintf(stderr, "tctool: more than one path given\n");
			return 2;
		}
		else
		{
			exePath = arg;
		}
	}

	if (exePath.empty())
	{
		printUsage(stderr);
		return 2;
	}

	if (tcName.empty())
		tcName = getLeaf(exePath);

	if (tcName.empty())
	{
		fprintf(stderr, "tctool: can't work out a TC name from %s, use --tc-name\n", exePath.c_str());
		return 2;
	}

	auto writePath = joinPath(joinPath(configPath, "TC"), tcName);

	try
	{
		convertTc(exePath, writePath, [](std::string const& msg) {
			printf("%s\n", msg.c_str());
		});
	}
	catch (std::exception const& e)
	{
		fprintf(stderr, "tctool: %s\n", e.what());
		return 1;
	}

	return 0;
}

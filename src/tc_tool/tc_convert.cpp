#include "tc_convert.hpp"
#include "common_writer.hpp"
#include "common_exereader.hpp"
#include "game/filesystem.hpp"
#include "game/reader.hpp"
#include <stdexcept>

void convertTc(std::string const& sourceDir, std::string const& destDir,
               std::function<void(std::string const&)> const& log)
{
	FsNode path(sourceDir);

	if (!path.exists())
		throw std::runtime_error("Directory not found: " + sourceDir);

	for (auto const& name : path.iter())
	{
		if (toUpperCase(name.name).find(".EXE") == std::string::npos)
			continue;

		ReaderFile exe((path / name.name).toSource());

		if (exe.len < 135000 || exe.len > 137000)
			continue;

		// TODO: Some TCs change the name of the .SND or .CHR for some reason.
		// We could read that name from the exe to make them work.
		for (char const* required : { "LIERO.CHR", "LIERO.SND" })
		{
			if (!(path / required).exists())
				throw std::runtime_error(std::string("Missing ") + required + " in " + sourceDir);
		}

		log("Converting " + name.name + "...");

		ReaderFile gfx((path / "LIERO.CHR").toSource());
		ReaderFile snd((path / "LIERO.SND").toSource());

		Common common;
		loadFromExe(common, exe, gfx, snd);

		log("Writing to " + destDir + "...");

		commonSave(common, destDir);
		return;
	}

	throw std::runtime_error("Could not find a suitable LIERO.EXE in " + sourceDir);
}

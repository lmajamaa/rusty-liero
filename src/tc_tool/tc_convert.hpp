#ifndef LIERO_TC_CONVERT_HPP
#define LIERO_TC_CONVERT_HPP

#include <functional>
#include <string>

// Finds the Liero executable in sourceDir, reads it together with LIERO.CHR and
// LIERO.SND, and writes the converted TC to destDir. Progress messages are passed
// to log. Throws std::runtime_error if the input is missing or can't be read.
void convertTc(std::string const& sourceDir, std::string const& destDir,
               std::function<void(std::string const&)> const& log);

#endif // LIERO_TC_CONVERT_HPP

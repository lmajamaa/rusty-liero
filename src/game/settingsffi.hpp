#ifndef LIERO_SETTINGSFFI_HPP
#define LIERO_SETTINGSFFI_HPP

// Glue for the settings formats, which are in Rust
// (rust/liero-data/src/settings.rs, called through
// rust/liero-game/src/settings.rs). C++ passes a view of its settings in and
// copies the settings in the result back.

#include <liero_game.h>
#include <gvl/crypt/gash.hpp>
#include <cstddef>
#include <exception>

struct Settings;
struct WormSettings;

// Views that point into the settings' strings, valid while they are unchanged.
LieroSettings settingsView(Settings const& s);
LieroWormSettings wormSettingsView(WormSettings const& ws);

void setSettings(Settings& s, LieroSettings const& v);
void setWormSettings(WormSettings& ws, LieroWormSettings const& v);

// Throws what a failed archive read should: the reader's own exception for
// ReadError, gvl::archive_check_error for CheckError.
void throwArchiveError(LieroSettingsStatus status, std::exception_ptr const& readerError);

// Owns a LieroSettingsResult.
struct SettingsResult
{
	explicit SettingsResult(LieroSettingsResult* r)
	: r(r)
	{
	}

	~SettingsResult()
	{
		liero_settings_result_free(r);
	}

	SettingsResult(SettingsResult const&) = delete;
	SettingsResult& operator=(SettingsResult const&) = delete;

	LieroSettingsStatus status() const { return liero_settings_result_status(r); }
	LieroSettings const& settings() const { return *liero_settings_result_settings(r); }
	LieroWormSettings const& worm() const { return *liero_settings_result_worm(r); }

	gvl::gash::value_type hash() const
	{
		LieroHash h = liero_settings_result_hash(r);
		gvl::gash::value_type ret;
		for (int i = 0; i < gvl::gash::value_type::size; ++i)
			ret.value[i] = h.value[i];
		return ret;
	}

	// Puts the bytes written into `writer`, an octet_writer or a hash
	// accumulator.
	template<typename Writer>
	void writeTo(Writer& writer) const
	{
		LieroStr b = liero_settings_result_bytes(r);
		for (std::size_t i = 0; i < b.len; ++i)
			writer.put(b.ptr[i]);
	}

	LieroSettingsResult* r;
};

// Lets Rust read bytes from an archive's reader. The reader throws at the end
// of its data, and an exception must not unwind through Rust, so get()
// catches it and keeps it for rethrow().
template<typename Reader>
struct ArchiveByteSource
{
	explicit ArchiveByteSource(Reader& reader)
	: reader(reader)
	{
		hooks.ctx = this;
		hooks.get = &get;
	}

	ArchiveByteSource(ArchiveByteSource const&) = delete;
	ArchiveByteSource& operator=(ArchiveByteSource const&) = delete;

	static int32_t get(void* ctx)
	{
		ArchiveByteSource& self = *static_cast<ArchiveByteSource*>(ctx);
		try
		{
			return self.reader.get();
		}
		catch (...)
		{
			self.error = std::current_exception();
			return -1;
		}
	}

	void rethrow(LieroSettingsStatus status) const
	{
		throwArchiveError(status, error);
	}

	Reader& reader;
	std::exception_ptr error;
	LieroByteSource hooks;
};

#endif // LIERO_SETTINGSFFI_HPP

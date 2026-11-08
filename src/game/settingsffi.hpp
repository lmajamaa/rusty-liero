#ifndef LIERO_SETTINGSFFI_HPP
#define LIERO_SETTINGSFFI_HPP

// Glue for the settings formats, which are in Rust
// (rust/liero-data/src/settings.rs, called through
// rust/liero-game/src/settings.rs). C++ passes a view of its settings in and
// copies the settings in the result back.

#include <liero_game.h>
#include <array>
#include <cstddef>
#include <cstdint>
#include <exception>
#include <stdexcept>
#include <string>
#include <vector>

struct Settings;
struct WormSettings;

// A hash of settings, from Rust, as gvl::gash's values were.
struct SettingsHash
{
	static int const size = 4;
	std::array<uint64_t, size> value{};

	bool operator==(SettingsHash const& b) const { return value == b.value; }
	bool operator!=(SettingsHash const& b) const { return value != b.value; }
};

// A text field of settings Rust owns (the TC's name, the level file, a
// worm's name), used much like a std::string. Assigning one to another
// copies the text, not which field it is.
struct SettingsText
{
	typedef LieroStr (*Get)(void const* owner);
	typedef void (*Set)(void* owner, LieroStr text);

	SettingsText(void* owner, Get get, Set set)
	: owner(owner), get_(get), set_(set)
	{
	}

	SettingsText(SettingsText const&) = delete;

	LieroStr view() const { return get_(owner); }

	std::string str() const
	{
		LieroStr s = view();
		return std::string(reinterpret_cast<char const*>(s.ptr), s.len);
	}

	explicit operator std::string() const { return str(); }

	bool empty() const { return view().len == 0; }
	std::size_t size() const { return view().len; }
	void clear() { *this = std::string(); }

	SettingsText& operator=(std::string const& s)
	{
		LieroStr v = { reinterpret_cast<uint8_t const*>(s.data()), s.size() };
		set_(owner, v);
		return *this;
	}

	SettingsText& operator=(char const* s) { return *this = std::string(s); }
	SettingsText& operator=(SettingsText const& other) { return *this = other.str(); }

	friend bool operator==(SettingsText const& a, SettingsText const& b) { return a.str() == b.str(); }
	friend bool operator==(SettingsText const& a, std::string const& b) { return a.str() == b; }
	friend bool operator==(std::string const& a, SettingsText const& b) { return a == b.str(); }
	friend bool operator!=(SettingsText const& a, std::string const& b) { return !(a == b); }
	friend bool operator!=(std::string const& a, SettingsText const& b) { return !(a == b); }

private:
	void* owner;
	Get get_;
	Set set_;
};

// Views that point into the settings' strings, valid while they are unchanged.
LieroSettings settingsView(Settings const& s);
LieroWormSettings wormSettingsView(WormSettings const& ws);

void setSettings(Settings& s, LieroSettings const& v);
void setWormSettings(WormSettings& ws, LieroWormSettings const& v);

// A replay or profile whose data isn't what was expected, as
// gvl::archive_check_error was.
struct ArchiveCheckError : std::runtime_error
{
	explicit ArchiveCheckError(std::string const& msg)
	: std::runtime_error(msg)
	{
	}
};

// Throws what a failed archive read should: the reader's own exception for
// ReadError, ArchiveCheckError for CheckError.
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

	SettingsHash hash() const
	{
		LieroHash h = liero_settings_result_hash(r);
		SettingsHash ret;
		for (int i = 0; i < SettingsHash::size; ++i)
			ret.value[i] = h.value[i];
		return ret;
	}

	// The bytes written.
	std::vector<uint8_t> bytes() const
	{
		LieroStr b = liero_settings_result_bytes(r);
		return std::vector<uint8_t>(b.ptr, b.ptr + b.len);
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

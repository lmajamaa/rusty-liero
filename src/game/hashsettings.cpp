// Settings section of `--hash-run` (see hashrun.cpp).
//
// Makes random settings from fixed seeds and runs them through every way the
// game reads and writes settings: liero.cfg (TOML) saving and loading, the
// LIERO.DAT loader, the binary archive in replays and its gash hash, and the
// worm settings archive used by replays and .lpf profiles. Inputs are also
// built by hand, truncated and corrupted, so the error paths are covered too.
// Each line hashes what was written, the settings afterwards and which
// exception, if any, came out.

#include "hashrun.hpp"

#include "settings.hpp"
#include "worm.hpp"
#include "replay.hpp"
#include "rand.hpp"
#include "filesystem.hpp"

#include <gvl/io2/stream.hpp>
#include <gvl/serialization/archive.hpp>
#include <gvl/serialization/context.hpp>
#include <gvl/serialization/except.hpp>

#include <cstdio>
#include <stdexcept>
#include <string>
#include <vector>

namespace
{

struct Xorshift
{
	uint64_t s;

	explicit Xorshift(uint64_t seed) : s(seed * 0x9e3779b97f4a7c15ull + 1) {}

	uint32_t next()
	{
		s ^= s << 13;
		s ^= s >> 7;
		s ^= s << 17;
		return uint32_t(s >> 16);
	}

	uint32_t below(uint32_t n) { return next() % n; }
	bool chance(uint32_t n) { return below(n) == 0; }

	// Mostly small numbers, with the edges of each width now and then.
	uint32_t value()
	{
		switch (below(8))
		{
		case 0: return below(4);
		case 1: return below(64);
		case 2: return below(256);
		case 3: return below(0x10000);
		case 4: return uint32_t(-int32_t(below(300)));
		case 5: return 0x7fffffffu - below(3);
		case 6: return 0x80000000u + below(3);
		default: return next();
		}
	}

	bool flag() { return (next() & 1) != 0; }

	std::string str(uint32_t maxLen)
	{
		std::string s;
		uint32_t len = below(maxLen + 1);
		for (uint32_t i = 0; i < len; ++i)
		{
			switch (below(4))
			{
			case 0: s.push_back(char(below(256))); break;
			case 1: s.push_back("\"\\ \r\n\t[]=.,u0"[below(13)]); break;
			default: s.push_back(char('a' + below(26))); break;
			}
		}
		return s;
	}
};

typedef std::vector<uint8_t> Bytes;

FsNode tmpNode(char const* name)
{
	return FsNode(std::string("/tmp/") + name);
}

Bytes readAll(FsNode const& node)
{
	gvl::octet_reader r(node.toOctetReader());
	Bytes data;
	uint8_t b;
	while (r.try_get(b))
		data.push_back(b);
	return data;
}

void writeAll(FsNode const& node, Bytes const& data)
{
	gvl::octet_writer w(node.toOctetWriter());
	w.put(data.data(), data.size());
}

void addStr(Fnv& f, std::string const& s)
{
	f.add(uint32_t(s.size()));
	f.bytes(reinterpret_cast<uint8_t const*>(s.data()), s.size());
}

void addBytes(Fnv& f, Bytes const& b)
{
	f.add(uint32_t(b.size()));
	f.bytes(b.data(), b.size());
}

void addHash(Fnv& f, gvl::gash::value_type const& h)
{
	for (uint64_t v : h.value)
	{
		f.add(uint32_t(v));
		f.add(uint32_t(v >> 32));
	}
}

void addWorm(Fnv& f, WormSettings const& ws)
{
	f.add(int32_t(ws.health));
	f.add(ws.controller);
	for (uint32_t c : ws.controls) f.add(c);
	for (uint32_t w : ws.weapons) f.add(w);
	addStr(f, ws.name.str());
	for (int c : ws.rgb) f.add(int32_t(c));
	f.add(ws.randomName);
	f.add(int32_t(ws.color));
	for (uint32_t c : ws.controlsEx) f.add(c);
}

void addSettings(Fnv& f, Settings const& s)
{
	f.add(s.recordReplays);
	f.add(s.loadPowerlevelPalette);
	f.add(s.bloodParticleMax);
	f.add(int32_t(s.aiFrames));
	f.add(int32_t(s.aiMutations));
	f.add(s.aiTraces);
	f.add(s.aiParallels);
	f.add(s.fullscreen);
	f.add(s.zoneTimeout);
	f.add(s.selectBotWeapons);
	f.add(s.allowViewingSpawnPoint);
	f.add(s.singleScreenReplay);
	f.add(s.spectatorWindow);
	addStr(f, s.tc.str());

	for (uint32_t w : s.weapTable) f.add(w);
	f.add(s.maxBonuses);
	f.add(s.blood);
	f.add(s.timeToLose);
	f.add(s.flagsToWin);
	f.add(s.gameMode);
	f.add(s.shadow);
	f.add(s.loadChange);
	f.add(s.namesOnBonuses);
	f.add(s.regenerateLevel);
	f.add(s.lives);
	f.add(s.loadingTime);
	f.add(s.randomLevel);
	addStr(f, s.levelFile.str());
	f.add(s.map);
	f.add(s.screenSync);

	for (auto const& ws : s.wormSettings)
		addWorm(f, *ws);
}

// Runs `f` and adds which exception came out of it, if any, and its message.
template<typename F>
void attempt(Fnv& h, F f)
{
	int kind = 0;
	std::string what;
	try
	{
		f();
	}
	catch (gvl::archive_check_error& e) { kind = 1; what = e.what(); }
	catch (gvl::stream_error& e) { kind = 2; what = e.what(); }
	catch (std::runtime_error& e) { kind = 3; what = e.what(); }
	catch (std::out_of_range&) { kind = 4; }
	catch (std::exception& e) { kind = 5; what = e.what(); }
	h.add(int32_t(kind));
	addStr(h, what);
}

void randomWorm(Xorshift& r, WormSettings& ws)
{
	ws.health = int(r.value());
	ws.controller = r.value();
	for (uint32_t& c : ws.controls) c = r.value();
	for (uint32_t& w : ws.weapons) w = r.value();
	ws.name = r.str(25);
	for (int& c : ws.rgb) c = int(r.value());
	ws.randomName = r.flag();
	ws.color = int(r.value());
	for (uint32_t& c : ws.controlsEx) c = r.value();
}

void randomSettings(Xorshift& r, Settings& s)
{
	s.recordReplays = r.flag();
	s.loadPowerlevelPalette = r.flag();
	s.bloodParticleMax = int32_t(r.value());
	s.aiFrames = int(r.value());
	s.aiMutations = int(r.value());
	s.aiTraces = r.flag();
	s.aiParallels = int32_t(r.value());
	s.fullscreen = r.flag();
	s.zoneTimeout = int32_t(r.value());
	s.selectBotWeapons = r.value();
	s.allowViewingSpawnPoint = r.flag();
	s.singleScreenReplay = r.flag();
	s.spectatorWindow = r.flag();
	s.tc = r.str(12);

	for (uint32_t& w : s.weapTable) w = r.chance(2) ? r.below(4) : r.value();
	s.maxBonuses = int32_t(r.value());
	s.blood = int32_t(r.value());
	s.timeToLose = int32_t(r.value());
	s.flagsToWin = int32_t(r.value());
	s.gameMode = r.value();
	s.shadow = r.flag();
	s.loadChange = r.flag();
	s.namesOnBonuses = r.flag();
	s.regenerateLevel = r.flag();
	s.lives = int32_t(r.value());
	s.loadingTime = int32_t(r.value());
	s.randomLevel = r.flag();
	s.levelFile = r.str(12);
	s.map = r.flag();
	s.screenSync = r.flag();

	for (auto& ws : s.wormSettings)
		randomWorm(r, *ws);
}

// The binary archives assert that 8- and 16-bit fields fit, and the game
// aborts when they don't, so archived settings are cut down to fit first.
void fitWorm(WormSettings& ws)
{
	ws.controller &= 0xffff;
	for (uint32_t& c : ws.controls) c &= 0xffff;
	for (uint32_t& w : ws.weapons) w &= 0xffff;
	for (int& c : ws.rgb) c &= 0xffff;
}

void fitSettings(Settings& s)
{
	for (uint32_t& w : s.weapTable) w &= 0xff;
	s.maxBonuses &= 0xffff;
	s.blood &= 0xffff;
	s.timeToLose &= 0xffff;
	s.flagsToWin &= 0xffff;
	s.gameMode &= 0xffff;
	s.lives &= 0xffff;
	s.loadingTime &= 0xffff;
	s.zoneTimeout &= 0xffff;
	for (auto& ws : s.wormSettings)
		fitWorm(*ws);
}

// Some settings start from the defaults and some from random values, so that
// a load that stops part way shows which fields it had set.
void startSettings(Xorshift& r, Settings& s)
{
	if (r.flag())
		randomSettings(r, s);
}

// Breaks a liero.cfg in one to three places.
Bytes corruptText(Xorshift& r, Bytes b)
{
	int ops = 1 + r.below(3);
	for (int i = 0; i < ops; ++i)
	{
		std::size_t n = b.size();
		switch (r.below(7))
		{
		case 0: // Change a byte
			if (n)
			{
				static char const interesting[] = " \r\n\t[]=\"\\u0123456789-,.tfnx";
				b[r.below(uint32_t(n))] = r.chance(3)
					? uint8_t(r.below(256))
					: uint8_t(interesting[r.below(sizeof(interesting) - 1)]);
			}
			break;
		case 1: // Delete a few bytes
			if (n)
			{
				std::size_t at = r.below(uint32_t(n));
				std::size_t len = std::min<std::size_t>(1 + r.below(10), n - at);
				b.erase(b.begin() + at, b.begin() + at + len);
			}
			break;
		case 2: // Insert a byte
			b.insert(b.begin() + r.below(uint32_t(n + 1)), uint8_t(r.chance(2) ? '\n' : r.below(256)));
			break;
		case 3: // Cut the end off
			b.resize(r.below(uint32_t(n + 1)));
			break;
		case 4: // Repeat a line
		case 5: // Drop a line
			if (n)
			{
				std::size_t at = r.below(uint32_t(n));
				std::size_t beg = at, end = at;
				while (beg > 0 && b[beg - 1] != '\n') --beg;
				while (end < n && b[end] != '\n') ++end;
				if (end < n) ++end;
				Bytes line(b.begin() + beg, b.begin() + end);
				if (r.flag())
					b.insert(b.begin() + r.below(uint32_t(n + 1)), line.begin(), line.end());
				else
					b.erase(b.begin() + beg, b.begin() + end);
			}
			break;
		case 6: // Add an element to an array
		{
			std::vector<std::size_t> ends;
			for (std::size_t j = 0; j < n; ++j)
				if (b[j] == ']' && j > 0 && b[j - 1] != ']')
					ends.push_back(j);
			if (!ends.empty())
			{
				static uint8_t const extra[] = { ',', ' ', '1' };
				std::size_t at = ends[r.below(uint32_t(ends.size()))];
				b.insert(b.begin() + at, extra, extra + 3);
			}
			break;
		}
		}
	}
	return b;
}

// Whether the C++ TOML reader would reach the end of `b` inside a string,
// where it loops until it runs out of memory. Every quote it meets outside a
// string either starts one or is a parse error, so counting quotes and
// escapes from the start finds the strings it reads.
bool endsInString(Bytes const& b)
{
	bool in = false;
	for (std::size_t i = 0; i < b.size(); ++i)
	{
		if (!in)
		{
			if (b[i] == '"')
				in = true;
		}
		else if (b[i] == '"')
			in = false;
		else if (b[i] == '\\')
		{
			if (i + 1 >= b.size())
				return true;
			uint8_t e = b[i + 1];
			if (e == '\\' || e == '/' || e == '"')
				i += 1;
			else if (e == 'u')
				i += 5;
			else
				return false; // A parse error
		}
	}
	return in;
}

// Breaks a liero.cfg, but not in the one way that hangs the C++ reader.
Bytes brokenText(Xorshift& r, Bytes const& text)
{
	Bytes b = corruptText(r, text);
	// An escape at the end can swallow the first quotes.
	while (endsInString(b))
		b.push_back('"');
	return b;
}

struct Builder
{
	Xorshift& r;
	Bytes b;

	explicit Builder(Xorshift& r) : r(r) {}

	void u8(uint32_t v) { b.push_back(uint8_t(v)); }
	void u8() { u8(r.below(256)); }
	void boolean() { u8(r.chance(4) ? r.below(256) : r.below(2)); }
	void u16le(uint32_t v) { u8(v); u8(v >> 8); }
	void u16le() { u16le(r.value()); }
	void u16(uint32_t v) { u8(v >> 8); u8(v); }
	void u16() { u16(r.value()); }
	void u32(uint32_t v) { u16(v >> 16); u16(v); }
	void u32() { u32(r.value()); }

	void str()
	{
		if (r.chance(16))
		{
			u32(); // Likely longer than what's left
			return;
		}
		std::string s = r.str(10);
		u32(uint32_t(s.size()));
		b.insert(b.end(), s.begin(), s.end());
	}

	void pascal(int fieldLen)
	{
		// Lengths up to 255, with the bytes after the field read as text too.
		u8(r.chance(4) ? r.below(256) : r.below(fieldLen + 2));
		for (int i = 1; i < fieldLen; ++i)
			u8(r.chance(2) ? 'a' + r.below(26) : r.below(256));
	}

	void maybeTruncate()
	{
		if (r.chance(3))
			b.resize(r.below(uint32_t(b.size() + 1)));
	}
};

// A LIERO.DAT, laid out as archive_liero reads it, with random values and an
// extension block of a random version.
Bytes legacyDat(Xorshift& r)
{
	Builder w(r);
	w.u8();
	for (int i = 0; i < 4; ++i) w.u16le();
	w.boolean(); w.boolean();
	w.u8(); w.u8();
	w.boolean();
	w.u16le();
	w.u8();
	w.boolean(); w.boolean(); w.boolean();
	for (int i = 0; i < 40; ++i) w.u8(r.chance(2) ? r.below(4) : r.below(256));
	for (int i = 0; i < 6; ++i) w.u8();
	for (int i = 0; i < 10; ++i) w.u8();
	w.u16le(); w.u16le();
	w.pascal(21); w.pascal(21);
	w.boolean();
	for (int i = 0; i < 9; ++i) w.u8("\x05LIERO\0\0"[i]);
	for (int i = 0; i < 14; ++i) w.u8(r.chance(2) ? r.below(180) : r.below(256));
	w.pascal(9);

	if (!r.chance(8))
	{
		int v = r.chance(2) ? 10 : int(r.below(13));
		w.u8(v);
		w.boolean(); w.boolean(); w.boolean();
		w.u8(); w.u16(); w.u16();
		if (v >= 4) w.str();
		if (v >= 2) w.boolean();
		if (v >= 2) for (int i = 0; i < 28; ++i) w.u8();
		if (v >= 3) for (int i = 0; i < 16; ++i) w.u32();
		if (v >= 4) { w.u16(); w.u16(); w.u8(); w.u16(); }
		if (v >= 5) { w.boolean(); w.u16(); }
		if (v >= 6) w.boolean();
		if (v >= 7) w.boolean();
		if (v >= 8) w.boolean();
		if (v >= 9) w.boolean();
		if (v >= 10) w.str();
	}

	w.maybeTruncate();
	return w.b;
}

// A settings archive, laid out as `archive` reads it, with an extension
// version other than the one the game writes.
Bytes binarySettings(Xorshift& r)
{
	Builder w(r);
	for (int i = 0; i < 40; ++i) w.u8();
	for (int i = 0; i < 5; ++i) w.u16();
	for (int i = 0; i < 4; ++i) w.boolean();
	w.u16(); w.u16();
	for (int i = 0; i < 3; ++i) w.boolean();
	w.str();
	int v = int(r.below(13));
	w.u8(v);
	w.boolean(); w.boolean(); w.boolean();
	w.u8(); w.u16(); w.u16();
	if (v >= 2) w.boolean();
	if (v >= 4) w.u16();
	if (v >= 6) w.boolean();
	if (v >= 7) w.boolean();
	if (v >= 8) w.boolean();
	if (v >= 9) w.boolean();
	if (v >= 10) w.str();
	w.u32(r.chance(4) ? r.value() : 0x12345678u);
	w.maybeTruncate();
	return w.b;
}

// A worm settings archive with a random version.
Bytes binaryWorm(Xorshift& r)
{
	Builder w(r);
	w.u32(); w.u32(); w.u16();
	for (int i = 0; i < 15; ++i) w.u16();
	w.boolean();
	w.str();
	int v = int(r.below(13));
	w.u8(v);
	if (v >= 2) for (int i = 0; i < 14; ++i) w.u8();
	if (v >= 3) for (int i = 0; i < 8; ++i) w.u32();
	w.maybeTruncate();
	return w.b;
}

typedef gvl::in_archive<gvl::octet_reader> settings_in;
typedef gvl::out_archive<gvl::octet_writer> settings_out;
typedef gvl::in_archive<gvl::octet_reader, GameSerializationContext> worm_in;
typedef gvl::out_archive<gvl::octet_writer, GameSerializationContext> worm_out;

void readSettings(Fnv& h, Bytes const& data, Settings& s)
{
	FsNode node = tmpNode("settings.bin");
	writeAll(node, data);
	attempt(h, [&] {
		gvl::octet_reader reader(node.toOctetReader());
		gvl::default_serialization_context context;
		archive(settings_in(reader, context), s);
	});
	addSettings(h, s);
}

Bytes writeSettings(Fnv& h, Settings& s)
{
	FsNode node = tmpNode("settings.bin");
	{
		gvl::octet_writer writer(node.toOctetWriter());
		gvl::default_serialization_context context;
		archive(settings_out(writer, context), s);
	}
	Bytes b = readAll(node);
	addBytes(h, b);
	addSettings(h, s);
	return b;
}

void readWorm(Fnv& h, Bytes const& data, WormSettings& ws, int replayVersion)
{
	FsNode node = tmpNode("worm.bin");
	writeAll(node, data);
	attempt(h, [&] {
		gvl::octet_reader reader(node.toOctetReader());
		GameSerializationContext context;
		context.replayVersion = replayVersion;
		archive(worm_in(reader, context), ws);
	});
	addWorm(h, ws);
}

Bytes writeWorm(Fnv& h, WormSettings& ws, int replayVersion)
{
	FsNode node = tmpNode("worm.bin");
	{
		gvl::octet_writer writer(node.toOctetWriter());
		GameSerializationContext context;
		context.replayVersion = replayVersion;
		archive(worm_out(writer, context), ws);
	}
	Bytes b = readAll(node);
	addBytes(h, b);
	addWorm(h, ws);
	return b;
}

void print(char const* what, int i, Fnv const& h)
{
	std::printf("settings-%s %d %016llx\n", what, i, (unsigned long long)h.h);
}

int const replayVersions[] = { 0, 1, 2, 3, 4 };

} // namespace

void hashSettings()
{
	Rand rand;

	{
		Fnv h;
		Settings s;
		addSettings(h, s);
		print("default", 0, h);
	}

	for (int i = 0; i < 64; ++i)
	{
		Xorshift r(i);
		Settings s;
		if (i > 0)
			randomSettings(r, s);
		FsNode cfg = tmpNode("liero.cfg");

		// liero.cfg written and read back.
		Bytes text;
		{
			Fnv h;
			s.save(cfg, rand);
			text = readAll(cfg);
			addBytes(h, text);
			addSettings(h, s);
			print("save", i, h);
		}
		{
			Fnv h;
			Settings t;
			startSettings(r, t);
			attempt(h, [&] { h.add(t.load(cfg, rand)); });
			addSettings(h, t);
			print("load", i, h);
		}

		// Broken liero.cfg files.
		{
			Fnv h;
			for (int j = 0; j < 24; ++j)
			{
				writeAll(cfg, brokenText(r, text));
				Settings t;
				startSettings(r, t);
				attempt(h, [&] { h.add(t.load(cfg, rand)); });
				addSettings(h, t);
			}
			print("load-broken", i, h);
		}

		// The settings archive and its hash.
		fitSettings(s);
		{
			Fnv h;
			std::unique_ptr<Settings> copy(s.clone());
			copy->wormSettings[0].reset(s.wormSettings[0]->clone().release());
			copy->wormSettings[1].reset(s.wormSettings[1]->clone().release());
			addHash(h, copy->updateHash());
			addSettings(h, *copy);
			print("hash", i, h);
		}
		Bytes bin;
		{
			Fnv h;
			bin = writeSettings(h, s);
			Settings t;
			startSettings(r, t);
			readSettings(h, bin, t);
			print("archive", i, h);
		}
		{
			Fnv h;
			for (int j = 0; j < 12; ++j)
			{
				Bytes b = j < 4 ? bin : binarySettings(r);
				if (j < 4)
					b.resize(r.below(uint32_t(b.size() + 1)));
				Settings t;
				startSettings(r, t);
				fitSettings(t);
				readSettings(h, b, t);
				// Write what was read, which normalises the version.
				writeSettings(h, t);
			}
			print("archive-broken", i, h);
		}

		// LIERO.DAT files.
		{
			Fnv h;
			FsNode dat = tmpNode("LIERO.DAT");
			for (int j = 0; j < 12; ++j)
			{
				writeAll(dat, legacyDat(r));
				Settings t;
				startSettings(r, t);
				attempt(h, [&] { h.add(t.loadLegacy(dat, rand)); });
				addSettings(h, t);
			}
			print("legacy", i, h);
		}

		// Worm settings: replay archives of every version, their hash and
		// profiles.
		for (int w = 0; w < 2; ++w)
		{
			WormSettings& ws = *s.wormSettings[w];
			int n = i * 2 + w;
			{
				Fnv h;
				for (int rv : replayVersions)
				{
					std::unique_ptr<WormSettings> copy(ws.clone());
					Bytes b = writeWorm(h, *copy, rv);
					WormSettings t;
					if (r.flag()) randomWorm(r, t);
					readWorm(h, b, t, rv);
				}
				std::unique_ptr<WormSettings> copy(ws.clone());
				addHash(h, copy->updateHash());
				addWorm(h, *copy);
				print("worm", n, h);
			}
			{
				Fnv h;
				for (int j = 0; j < 12; ++j)
				{
					int rv = replayVersions[r.below(5)];
					Bytes b;
					if (j < 4)
					{
						std::unique_ptr<WormSettings> owned(ws.clone());
						WormSettings& copy = *owned;
						Fnv ignore;
						b = writeWorm(ignore, copy, rv);
						b.resize(r.below(uint32_t(b.size() + 1)));
					}
					else
						b = binaryWorm(r);
					WormSettings t;
					if (r.flag()) randomWorm(r, t);
					readWorm(h, b, t, rv);
				}
				print("worm-broken", n, h);
			}
			{
				Fnv h;
				FsNode lpf = tmpNode("worm.lpf");
				std::unique_ptr<WormSettings> owned(ws.clone());
				WormSettings& copy = *owned;
				attempt(h, [&] { copy.saveProfile(lpf); });
				Bytes b = readAll(lpf);
				addBytes(h, b);
				for (int j = 0; j < 4; ++j)
				{
					if (j > 0)
					{
						Bytes c = j < 3 ? b : binaryWorm(r);
						if (j < 3) c.resize(r.below(uint32_t(c.size() + 1)));
						writeAll(lpf, c);
					}
					WormSettings t;
					if (r.flag()) randomWorm(r, t);
					attempt(h, [&] { t.loadProfile(lpf); });
					addWorm(h, t);
				}
				print("profile", n, h);
			}
		}
	}
}

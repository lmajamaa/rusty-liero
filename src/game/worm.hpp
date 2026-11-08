#ifndef LIERO_WORM_HPP
#define LIERO_WORM_HPP

#include "math.hpp"
#include "rand.hpp"
#include <string>
#include <cstring>
#include <memory>
#include <numeric>
#include <functional>
#include <memory>
#include "version.hpp"
#include "filesystem.hpp"
#include "settingsffi.hpp"

struct Worm;
struct Game;
#include "weapon.hpp"

struct Ninjarope
{
	Ninjarope()
	: out(false)
	, attached(false)
	, anchor(0)
	, length(0)
	, curLen(0)
	{
	}

	bool out;            //Is the ninjarope out?
	bool attached;
	LieroWormData* anchor;	// If non-zero, the worm the ninjarope is attached to
	fixedvec pos, vel; //Ninjarope props
	// Not needed as far as I can tell: fixed forceX, forceY;
	int length, curLen;

};

/*
struct Controls
{
	bool up, down, left, right;
	bool fire, change, jump;
};*/

struct WormWeapon
{
	WormWeapon()
	: ammo(0)
	, delayLeft(0)
	, loadingLeft(0)
	{
	}

	bool available() const
	{
		return loadingLeft == 0;
	}

	//int id;
	Weapon const* type;
	int ammo;
	int delayLeft;
	int loadingLeft;
};

struct WormSettingsExtensions
{
	enum Control
	{
		Up, Down, Left, Right,
		Fire, Change, Jump,
		Dig,
		MaxControl = Dig,
		MaxControlEx
	};
};

// A worm's settings. Rust owns them (LieroWormSettingsBox in
// rust/liero-game/src/settings.rs); the fields here refer to its. The fields
// and their defaults match liero_data::settings::WormSettings.
struct WormSettings : WormSettingsExtensions
{
	WormSettings()
	: WormSettings(liero_worm_settings_box_new())
	{
		SettingsResult r(liero_worm_settings_default());
		setWormSettings(*this, r.worm());
	}

	WormSettings(WormSettings const&) = delete;
	WormSettings& operator=(WormSettings const&) = delete;

	// A copy of the settings, without the profile they came from. The game
	// shares a worm's settings; only the hash run copies them.
	std::unique_ptr<WormSettings> clone() const
	{
		return std::unique_ptr<WormSettings>(new WormSettings(liero_worm_settings_box_clone(box)));
	}

	~WormSettings()
	{
		liero_worm_settings_box_free(box);
	}

	SettingsHash& updateHash();

	void saveProfile(FsNode node);
	void loadProfile(FsNode node);

	// The settings in Rust, for Rust code to work on.
	LieroWormSettingsBox* rust() const { return box; }

private:
	explicit WormSettings(LieroWormSettingsBox* box)
	: box(box)
	, data(*liero_worm_settings_box_data(box))
	, controlsEx(data.controls_ex)
	, health(data.health)
	, controller(data.controller)
	, controls(data.controls)
	, weapons(data.weapons)
	, name(box, &getName, &setName)
	, rgb(data.rgb)
	, randomName(data.random_name)
	, color(data.color)
	{
	}

	static LieroStr getName(void const* b) { return liero_worm_settings_box_name(static_cast<LieroWormSettingsBox const*>(b)); }
	static void setName(void* b, LieroStr s) { liero_worm_settings_box_set_name(static_cast<LieroWormSettingsBox*>(b), s); }

	LieroWormSettingsBox* box;
	LieroWormSettingsData& data;

public:
	uint32_t (&controlsEx)[MaxControlEx];
	int32_t& health;
	uint32_t& controller; // CPU / Human
	uint32_t (&controls)[MaxControl];
	uint32_t (&weapons)[5]; // TODO: Adjustable
	SettingsText name;
	int32_t (&rgb)[3];
	bool& randomName;

	int32_t& color;

	FsNode profileNode;

	SettingsHash hash;
};

// A worm's settings in a replay of `replayVersion`, or in a profile.
// The format is in liero_data::settings::read_worm and write_worm.
template<typename Reader>
void readWormArchive(Reader& reader, WormSettings& ws, int replayVersion)
{
	ArchiveByteSource<Reader> source(reader);
	LieroWormSettings view = wormSettingsView(ws);
	SettingsResult r(liero_worm_settings_read_archive(&view, replayVersion, &source.hooks));
	setWormSettings(ws, r.worm());
	source.rethrow(r.status());
}

inline std::vector<uint8_t> writeWormArchive(WormSettings& ws, int replayVersion)
{
	LieroWormSettings view = wormSettingsView(ws);
	SettingsResult r(liero_worm_settings_write_archive(&view, replayVersion));
	setWormSettings(ws, r.worm());
	return r.bytes();
}

struct Viewport;
struct Renderer;

// A worm's bot, in Rust (LieroWormAi in rust/liero-game/src/ai/mod.rs):
// the one `controller` picks, or none for a human.
inline std::shared_ptr<LieroWormAi> createWormAi(uint32_t controller, int candidates, bool testing)
{
	LieroWormAi* ai = liero_worm_ai_new(controller, candidates, testing);
	if (!ai)
		return std::shared_ptr<LieroWormAi>();
	return std::shared_ptr<LieroWormAi>(ai, liero_worm_ai_free);
}

struct Worm
{
	enum
	{
		RFDown,
		RFLeft,
		RFUp,
		RFRight
	};

	enum Control
	{
		Up = WormSettings::Up,
		Down = WormSettings::Down,
		Left = WormSettings::Left,
		Right = WormSettings::Right,
		Fire = WormSettings::Fire,
		Change = WormSettings::Change,
		Jump = WormSettings::Jump,
		MaxControl
		//Dig = WormSettings::Dig
	};
	//static const unsigned int MaxControl = Dig;


	struct ControlState
	{
		ControlState()
		: istate(0)
		{
		}

		bool operator==(ControlState const& b) const
		{
			return istate == b.istate;
		}

		uint32_t pack() const
		{
			return istate; // & ((1 << MaxControl)-1);
		}

		void unpack(uint32_t state)
		{
			istate = state & 0x7f;
		}

		bool operator!=(ControlState const& b) const
		{
			return !operator==(b);
		}

		bool operator[](std::size_t n) const
		{
			return ((istate >> n) & 1) != 0;
		}

		void set(std::size_t n, bool v)
		{
			if(v)
				istate |= 1 << n;
			else
				istate &= ~(uint32_t(1u) << n);
		}

		void toggle(std::size_t n)
		{
			istate ^= 1 << n;
		}

		uint32_t istate;
	};

	Worm()
	: Worm(liero_worm_data_new())
	{
	}

	// A copy with state of its own, sharing the settings and AI.
	Worm(Worm const& other)
	: Worm(liero_worm_data_clone(&other.data))
	{
		settings = other.settings;
		ai = other.ai;
	}

	Worm& operator=(Worm const&) = delete;

	~Worm()
	{
		liero_worm_data_free(&data);
	}

	bool pressed(Control control) const
	{
		return controlStates[control];
	}

	bool pressedOnce(Control control)
	{
		bool state = controlStates[control];
		controlStates.set(control, false);
		return state;
	}

	void release(Control control)
	{
		controlStates.set(control, false);
	}

	void press(Control control)
	{
		controlStates.set(control, true);
	}

	void setControlState(Control control, bool state)
	{
		controlStates.set(control, state);
	}

	void toggleControlState(Control control)
	{
		controlStates.set(control, !controlStates[control]);
	}

	int minimapColor() const
	{
		return 129 + index * 4;
	}

	int angleFrame() const;

	// The worm's state in Rust.
	LieroWormData* rust() { return &data; }

private:
	explicit Worm(LieroWormData* d);

	LieroWormData& data;

public:
	// Rust owns these (LieroWormData in rust/liero-game/src/worm.rs).
	fixedvec& pos;
	fixedvec& vel;

	IVec2& logicRespawn;

	int& hotspotX;               //Hotspots for laser, laser sight, etc.
	int& hotspotY;
	fixed& aimingAngle;
	fixed& aimingSpeed;

	bool& ableToJump;            //The previous state of some keys
	bool& ableToDig;
	bool& keyChangePressed;
	bool& movable;

	bool& animate;               //Should the worm be animated?
	bool& visible;               //Is the worm visible?
	bool& ready;                 //Is the worm ready to play?
	bool& flag;                  //Does the worm have a flag?
	bool& makeSightGreen;        //Changes the sight color
	int& health;                 //Health left
	int& lives;                  //lives left
	int& kills;                  //Kills made

	int& timer;                  //Timer for GOT
	int& killedTimer;            //Time until worm respawns
	int& currentFrame;

	int& flags;                  //How many flags does this worm have?

	Ninjarope& ninjarope;

	int& currentWeapon;          //The selected weapon
	int& lastKilledByIdx;        // What worm that last killed this worm
	int& fireCone;               //How much is left of the firecone
	int& leaveShellTimer;        //Time until next shell drop

	int& index; // 0 or 1

	int (&reacts)[4];
	WormWeapon (&weapons)[5];
	int& direction;
	ControlState& controlStates;
	ControlState& prevControlStates;

	// Temporary state for steerables
	int& steerableSumX;
	int& steerableSumY;
	int& steerableCount;

	// which X coordinate to display stats at for this worm
	int& statsX;

	// Data for LocalController
	ControlState& cleanControlStates; // This contains the real state of real and extended controls

	std::shared_ptr<WormSettings> settings; // !CLONING

	std::shared_ptr<LieroWormAi> ai;
};

int sqrVectorLength(int x, int y);


#endif // LIERO_WORM_HPP

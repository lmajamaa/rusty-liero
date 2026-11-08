#ifndef LIERO_GAME_HPP
#define LIERO_GAME_HPP

#include <vector>
#include "level.hpp"
#include "settings.hpp"
#include "weapon.hpp"
#include "sobject.hpp"
#include "nobject.hpp"
#include "bobject.hpp"
#include "fastObjectList.hpp"
#include "rand.hpp"
#include "mixer/player.hpp"
#include "bonus.hpp"
#include "constants.hpp"
#include <string>
#include <memory>
#include "common.hpp"
#include "stats_recorder.hpp"

struct SpectatorViewport;
struct Viewport;
struct Worm;
struct Renderer;

typedef enum {
	StateInitial,
	StateWeaponSelection,
	StateGame,
	StateGameEnded,
} GameState;

struct Holdazone
{
	Holdazone()
	: holderIdx(-1)
	, contenderIdx(-1)
	, contenderFrames(0)
	, timeoutLeft(0)
	, zoneWidth(50), zoneHeight(34)
	{
	}

	Rect rect;
	int holderIdx;

	int contenderIdx, contenderFrames;

	int timeoutLeft;

	int zoneWidth, zoneHeight;
};

// What C++ owns of a Game; copying a Game copies these.
struct GameMembers
{
	GameMembers(std::shared_ptr<Common> common, std::shared_ptr<Settings> settings, std::shared_ptr<SoundPlayer> soundPlayer);

	std::shared_ptr<Common> common;
	std::shared_ptr<SoundPlayer> soundPlayer;
	std::shared_ptr<Settings> settings;
	std::shared_ptr<StatsRecorder> statsRecorder;

	Level level;

	std::vector<Viewport*> viewports;
	std::vector<SpectatorViewport*> spectatorViewports;
	std::vector<Worm*> worms;
};

struct Game : GameMembers
{
	Game(std::shared_ptr<Common> common, std::shared_ptr<Settings> settings, std::shared_ptr<SoundPlayer> soundPlayer);
	// A copy with state of its own. It shares the worms and viewports until
	// postClone.
	Game(Game const& other);
	Game& operator=(Game const& other);
	~Game();

	void onKey(uint32_t key, bool state);
	void releaseControls();
	// Runs the game for one tick, in Rust (rust/liero-game/src/game.rs).
	void processFrame();
	void focus(Renderer& renderer);
	void updateSettings(Renderer& renderer);

	void createBObject(fixedvec pos, fixedvec vel);

	void clearViewports();
	void addViewport(Viewport*);
	void addSpectatorViewport(SpectatorViewport*);
	void processViewports();
	void drawViewports(Renderer& renderer, GameState state, bool isReplay = false);
	void drawSpectatorViewports(Renderer& renderer, GameState state, bool isReplay = false);
	void clearWorms();
	void addWorm(Worm*);
	void resetWorms();
	void draw(Renderer& renderer, GameState state, bool useSpectatorViewports, bool isReplay = false);
	void startGame();
	bool isGameOver();
	void doDamage(Worm& w, int amount, int byIdx);
	void doHealing(Worm& w, int amount);
	void postClone(Game& original, bool complete = false);

	void spawnZone();

	Material pixelMat(int x, int y)
	{
		return common->materials[level.pixel(x, y)];
	}

	Worm* wormByIdx(int idx)
	{
		if (idx < 0) return 0;
		return worms[idx];
	}

	// The game's state in Rust.
	LieroGameData* rust() { return &data; }

private:
	// Builds the members from `members`, in place, without a temporary
	// copy of the level.
	template<typename... Members>
	Game(LieroGameData* d, Members&&... members);

	LieroGameData& data;

public:
	// Rust owns these (LieroGameData in rust/liero-game/src/game.rs).
	int& screenFlash;
	bool& gotChanged;
	int& lastKilledIdx;
	bool& paused;
	int& cycles;
	Rand& rand;

	Holdazone& holdazone;

	bool& quickSim;

	typedef ExactObjectList<Bonus, LIERO_MAX_BONUSES> BonusList;
	typedef ExactObjectList<WObject, LIERO_MAX_WOBJECTS> WObjectList;
	typedef ExactObjectList<SObject, LIERO_MAX_SOBJECTS> SObjectList;
	typedef ExactObjectList<NObject, LIERO_MAX_NOBJECTS> NObjectList;
	typedef BloodList BObjectList;
	BonusList& bonuses;
	WObjectList& wobjects;
	SObjectList& sobjects;
	NObjectList& nobjects;
	BObjectList& bobjects;
};


#endif // LIERO_GAME_HPP


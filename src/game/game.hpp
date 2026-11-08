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

struct Viewport;
struct Worm;
struct WormSettings;
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

	Level level;
};

struct Game;

// The game as the Rust game rules see it. Pointers into the game, so it
// must not outlive the call it is made for.
struct GameView
{
	GameView(Game& game);
	~GameView();
	GameView(GameView const&) = delete;
	GameView& operator=(GameView const&) = delete;

	LieroGameView* view;
	LieroGameFields fields;
	LieroGameSettings settings;
};

// Rust owns the game's state, worms and viewports (LieroGame in
// rust/liero-game/src/game_box.rs); `worms` and the viewport lists are
// views of them.
struct Game : GameMembers
{
	Game(std::shared_ptr<Common> common, std::shared_ptr<Settings> settings, std::shared_ptr<SoundPlayer> soundPlayer);
	Game(Game const&) = delete;
	Game& operator=(Game const&) = delete;
	~Game();

	// A copy of the game, its worms and viewports, as a replay keeps to
	// rewind to. It shares the worms' settings and bots, and the spectator
	// viewports.
	std::unique_ptr<Game> clone() const;
	// Makes this game a copy of `other`, as `clone` would.
	void assign(Game const& other);

	void onKey(uint32_t key, bool state);
	void releaseControls();
	// Runs the game for one tick, in Rust (rust/liero-game/src/game.rs).
	void processFrame();
	// Runs the worms' bots for the tick, in Rust (rust/liero-game/src/ai),
	// timing them into the stats when `timed`.
	void processAis(bool timed);
	void focus(Renderer& renderer);
	void updateSettings(Renderer& renderer);

	void clearViewports();
	// A viewport following the worm at `wormIdx`, or showing the whole
	// level, by `kind`.
	Viewport* addViewport(Rect rect, int wormIdx, int levwidth, int levheight, int kind = LIERO_VIEWPORT_WORM);
	// One of the spectator viewports, which copies of the game share.
	void addSpectatorViewport(Rect rect, int levwidth, int levheight);
	// Draws the split-screen viewports, or the spectator ones.
	void drawViewports(Renderer& renderer, GameState state, bool spectators, bool isReplay = false);
	// A new worm with `settings`, at the end of the list.
	Worm* addWorm(std::shared_ptr<WormSettings> settings);
	// A new worm the game keeps without listing it, for listWorm.
	LieroWorm* newWorm();
	// Lists one of newWorm's worms, or an empty place for null.
	void listWorm(LieroWorm* w);
	void resetWorms();
	void draw(Renderer& renderer, GameState state, bool useSpectatorViewports, bool isReplay = false);
	void startGame();
	bool isGameOver();

	void spawnZone();

	Worm* wormByIdx(int idx)
	{
		if (idx < 0) return 0;
		return worms[idx];
	}

	// The game in Rust.
	LieroGame* handle() const { return g; }
	// The game's state in Rust.
	LieroGameData* rust() { return &data; }

private:
	// Builds the members from `members`, in place, without a temporary
	// copy of the level.
	template<typename... Members>
	Game(LieroGame* g, Members&&... members);
	template<typename... Members>
	Game(LieroGame* g, LieroGameData* d, Members&&... members);

	// Remakes the views of the worms and viewports.
	void syncViews();

	LieroGame* g;
	LieroGameData& data;
	std::vector<std::unique_ptr<Worm>> wormViews;
	std::vector<std::unique_ptr<Viewport>> viewportViews;

public:
	// The game's worms, with null for a replay's empty places.
	std::vector<Worm*> worms;
	// The split-screen viewports.
	std::vector<Viewport*> viewports;
	// The stats the game records.
	std::unique_ptr<StatsRecorder> statsRecorder;

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


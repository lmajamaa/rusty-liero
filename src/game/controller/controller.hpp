#ifndef UUID_9CD8C22BC14D4832AE2A859530FE6339
#define UUID_9CD8C22BC14D4832AE2A859530FE6339

#include "liero_game.h"

#include <string>

struct Renderer;
struct Level;

// Where key events go (Gfx::processEvent).
struct KeySink
{
	virtual ~KeySink() {}
	virtual void onKey(int key, bool state) = 0;
};

// A local match or a replay. Rust runs it (rust/liero-game/src/controllers.rs);
// this holds it.
struct Controller : KeySink
{
	explicit Controller(LieroController* c) : c(c) {}
	~Controller() { liero_controller_free(c); }
	Controller(Controller const&) = delete;
	Controller& operator=(Controller const&) = delete;

	// A local match with the settings' players.
	static Controller* local(LieroSettingsBox* settings, LieroTc* tc);
	// A replay of `node`; throws if it can't be read.
	static Controller* replay(LieroFsNode const* node, std::string const& path, LieroTc* tc);

	bool isReplay() { return liero_controller_is_replay(c); }
	void onKey(int key, bool state) override { liero_controller_on_key(c, key, state); }
	void unfocus() { liero_controller_unfocus(c); }
	void focus() { liero_controller_focus(c); }
	bool process() { return liero_controller_run(c); }
	void draw(Renderer& renderer, bool useSpectatorViewports);
	bool running() { return liero_controller_running(c); }
	// The level behind the menu, or null.
	LieroLevelBox* currentLevel() { return liero_controller_level(c); }
	// The game, or null before a replay starts.
	LieroGame* currentGame() { return liero_controller_game(c); }
	void swapLevel(Level& newLevel);
	// The screen the controller opened over the game, if any.
	bool takeOpened(LieroScreen* out) { return liero_controller_take_opened(c, out); }
	LieroControllerData& data() { return *liero_controller_data(c); }

	LieroController* c;
};

#endif // UUID_9CD8C22BC14D4832AE2A859530FE6339

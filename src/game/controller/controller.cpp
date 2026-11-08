#include "controller.hpp"

#include "../gfx/renderer.hpp"
#include "../level.hpp"

#include <stdexcept>
#include <string>

Controller* Controller::local(LieroSettingsBox* settings, LieroTc* tc)
{
	return new Controller(liero_local_controller_new(settings, tc));
}

Controller* Controller::replay(LieroFsNode const* node, std::string const& path, LieroTc* tc)
{
	LieroController* c = node ? liero_replay_controller_new(node, tc) : nullptr;
	if (!c)
		throw std::runtime_error("Could not read " + path);
	return new Controller(c);
}

void Controller::draw(Renderer& renderer, bool useSpectatorViewports)
{
	liero_controller_draw(c, renderer.rust(), useSpectatorViewports);
}

void Controller::swapLevel(Level& newLevel)
{
	Level current = Level::view(currentLevel());
	current.swap(newLevel);
}

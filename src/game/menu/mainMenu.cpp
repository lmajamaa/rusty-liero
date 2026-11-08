#include "mainMenu.hpp"

#include "game/sfx.hpp"
#include "game/gfx.hpp"

struct ReplaySelectBehavior : ItemBehavior
{
	ReplaySelectBehavior(Common& common)
	: common(common)
	{
	}

	int onEnter(Menu& menu, int item)
	{
		sfx.play(common, 27);
		Common& common = this->common;
		gfx.open(selectReplayScreen(), [&common](int ret)
		{
			sfx.play(common, 27);
			return ret;
		});
		return -1;
	}

	Common& common;
};

struct TcSelectBehavior : ItemBehavior
{
	TcSelectBehavior(Common& common)
	: common(common)
	{
	}

	int onEnter(Menu& menu, int item)
	{
		sfx.play(common, 27);
		std::shared_ptr<std::unique_ptr<Common>> chosen(new std::unique_ptr<Common>());
		gfx.open(selectTcScreen(chosen), [chosen](int)
		{
			if (*chosen)
			{
				// TODO: mixer may still be using sounds from the old common
				gfx.common.reset(chosen->release());
				return int(MainMenu::MaTc);
			}
			return -1;
		});
		return -1;
	}

	Common& common;
};

ItemBehavior* MainMenu::getItemBehavior(Common& common, int item)
{
	switch(itemId(item))
	{
		case MaReplays:
			return new ReplaySelectBehavior(common);

		case MaTc:
			return new TcSelectBehavior(common);

		default:
			return Menu::getItemBehavior(common, item);
	}
}

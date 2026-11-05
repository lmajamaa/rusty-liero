
#include "integerBehavior.hpp"

#include "menu.hpp"
#include "menuItem.hpp"
#include "../sfx.hpp"
#include "../gfx.hpp"
#include "../common.hpp"
#include "../text.hpp"

bool IntegerBehavior::onLeftRight(Menu& menu, MenuItem& item, int dir)
{
	int newV = liero_menu_integer_left_right(v, min, max, step, dir, gfx.menuCycles, scrollInterval);

	if(newV != v)
	{
		v = newV;
		onUpdate(menu, item);
	}

	return true;
}

int IntegerBehavior::onEnter(Menu& menu, MenuItem& item)
{
	sfx.play(common, 27);

	if(!allowEntry)
		return -1; // Not allowed

	int x, y;
	if(menu.itemPosition(item, x, y))
	{
		x += menu.valueOffsetX;
		int digits = 1 + int(std::floor(std::log10(double(max))));
		gfx.inputInteger(v, min, max, digits, x + 2, y);
		onUpdate(menu, item);
	}
	return -1;
}

void IntegerBehavior::onUpdate(Menu& menu, MenuItem& item)
{
	item.value = valueText(v, percentage ? LIERO_MENU_VALUE_PERCENTAGE : LIERO_MENU_VALUE_INTEGER);
	item.hasValue = true;
}

std::string valueText(int v, int kind)
{
	char buf[32];
	std::size_t len = liero_menu_value_text(v, kind, reinterpret_cast<uint8_t*>(buf), sizeof(buf));
	return std::string(buf, len);
}

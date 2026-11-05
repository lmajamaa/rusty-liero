
#include "timeBehavior.hpp"

#include "menu.hpp"
#include "menuItem.hpp"
#include "../common.hpp"
#include "../text.hpp"

void TimeBehavior::onUpdate(Menu& menu, MenuItem& item)
{
	item.value = valueText(v, frames ? LIERO_MENU_VALUE_FRAMES : LIERO_MENU_VALUE_TIME);
	item.hasValue = true;
}

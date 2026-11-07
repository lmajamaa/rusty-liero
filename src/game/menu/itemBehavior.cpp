#include "itemBehavior.hpp"

#include "menu.hpp"
#include "menuItem.hpp"
#include "../gfx.hpp"

RustItemBehavior::RustItemBehavior(Common& common, LieroItemKind kind)
: common(common)
{
	data.kind = kind;
	data.flag = nullptr;
	data.set_flag = nullptr;
	data.value = nullptr;
	data.min = 0;
	data.max = 0;
	data.step = 1;
	data.scroll_interval = 5;
	data.value_kind = LIERO_MENU_VALUE_INTEGER;
	data.allow_entry = true;
	data.names = nullptr;
	data.name_count = 0;
}

LieroItemBehavior RustItemBehavior::view() const
{
	LieroItemBehavior v = data;
	v.names = names.data();
	v.name_count = names.size();
	return v;
}

bool RustItemBehavior::onLeftRight(Menu& menu, MenuItem& item, int dir)
{
	LieroItemBehavior v = view();
	LieroItemAction a = liero_item_left_right(&v, dir, gfx.menuCycles);
	apply(menu, item, a);
	return a.keep_going;
}

int RustItemBehavior::onEnter(Menu& menu, MenuItem& item)
{
	LieroItemBehavior v = view();
	apply(menu, item, liero_item_enter(&v));
	return -1;
}

void RustItemBehavior::apply(Menu& menu, MenuItem& item, LieroItemAction a)
{
	if (a.update_item)
		onUpdate(menu, item);
	if (a.update_menu)
		menu.updateItems(common);

	int x, y;
	if (a.enter_digits > 0 && menu.itemPosition(item, x, y))
	{
		x += menu.valueOffsetX;
		RustItemBehavior self = *this;
		gfx.openInputInteger(*data.value, data.min, data.max, a.enter_digits, x + 2, y, [self, &menu, &item]() mutable
		{
			self.onUpdate(menu, item);
		});
	}
}

void RustItemBehavior::onUpdate(Menu& menu, MenuItem& item)
{
	LieroItemBehavior v = view();
	char buf[64];
	std::size_t len = liero_item_value_text(&v, reinterpret_cast<uint8_t*>(buf), sizeof(buf));
	item.value.assign(buf, std::min(len, sizeof(buf)));
	item.hasValue = true;
}

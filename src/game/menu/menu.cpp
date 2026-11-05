#include "menu.hpp"

#include "../common.hpp"
#include "../gfx/bitmap.hpp"
#include <gvl/support/debug.hpp>

// Selection, scrolling, searching and drawing are implemented in Rust
// (rust/liero-game/src/menu.rs). Menus keep their items and fields in C++
// and copy them in and out for each call.

LieroMenu Menu::state() const
{
	LieroMenu s;
	s.x = x;
	s.y = y;
	s.height = height;
	s.item_height = itemHeight;
	s.value_offset_x = valueOffsetX;
	s.top_item = topItem;
	s.bottom_item = bottomItem;
	s.visible_item_count = visibleItemCount;
	s.selection = selection_;
	s.search_time = searchTime;
	s.centered = centered;
	return s;
}

std::vector<LieroMenuItem> Menu::itemViews()
{
	std::vector<LieroMenuItem> views;
	views.reserve(items.size());
	for (MenuItem const& i : items)
	{
		LieroMenuItem v;
		v.string = { reinterpret_cast<uint8_t const*>(i.string.data()), i.string.size() };
		v.value = { reinterpret_cast<uint8_t const*>(i.value.data()), i.value.size() };
		v.color = i.color;
		v.dis_colour = i.disColour;
		v.has_value = i.hasValue;
		v.visible = i.visible;
		v.selectable = i.selectable;
		views.push_back(v);
	}
	return views;
}

void Menu::setState(LieroMenu const& s, std::vector<LieroMenuItem> const& views)
{
	topItem = s.top_item;
	bottomItem = s.bottom_item;
	visibleItemCount = s.visible_item_count;
	selection_ = s.selection;
	searchTime = s.search_time;
	for (std::size_t i = 0; i < items.size() && i < views.size(); ++i)
		items[i].visible = views[i].visible;
}

namespace
{

// Runs `f(state, items, count)` on the menu's Rust view and copies the
// result back.
template<typename F>
auto withView(Menu& menu, F f) -> decltype(f(std::declval<LieroMenu*>(), std::declval<LieroMenuItem*>(), 0))
{
	LieroMenu s = menu.state();
	std::vector<LieroMenuItem> views = menu.itemViews();
	struct WriteBack
	{
		Menu& menu;
		LieroMenu& s;
		std::vector<LieroMenuItem>& views;
		~WriteBack() { menu.setState(s, views); }
	} writeBack = { menu, s, views };
	return f(&s, views.data(), int32_t(views.size()));
}

struct OverlayContext
{
	Menu& menu;
	Common& common;
};

void hookDrawOverlay(void* ctx, int32_t item, int32_t x, int32_t y, bool selected, bool disabled)
{
	OverlayContext& c = *static_cast<OverlayContext*>(ctx);
	c.menu.drawItemOverlay(c.common, c.menu.items[item], x, y, selected, disabled);
}

}

void Menu::onKeys(SDL_Keysym* begin, SDL_Keysym* end, bool contains, Uint32 time)
{
	for (; begin != end; ++begin)
	{
		bool isTab = begin->scancode == SDL_SCANCODE_TAB;
		if ((begin->sym >= 32 && begin->sym <= 127) // x >= SDLK_SPACE && x <= SDLK_DELETE
		  || isTab)
		{
			char ch = char(begin->sym);
			LieroMenuSearch r = withView(*this, [&](LieroMenu* s, LieroMenuItem* items, int32_t count) {
				return liero_menu_search(s, items, count, reinterpret_cast<uint8_t const*>(searchPrefix.data()),
					searchPrefix.size(), isTab ? -1 : int32_t((unsigned char)ch), contains, time);
			});
			searchPrefix.resize(r.keep);
			if (r.add)
				searchPrefix += ch;
		}
	}
}

void Menu::draw(Common& common, Renderer& renderer, bool disabled, int x, bool showDisabledSelection)
{
	OverlayContext ctx = { *this, common };
	LieroMenuHooks hooks = { &ctx, hookDrawOverlay };
	LieroBitmap scr = renderer.bmp.view();
	LieroFont font = common.font.view();
	withView(*this, [&](LieroMenu* s, LieroMenuItem* items, int32_t count) {
		liero_menu_draw(s, items, count, &scr, &font, &hooks, disabled, x, showDisabledSelection);
	});
}

void Menu::moveToId(int id)
{
	moveTo(indexFromId(id));
}

void Menu::moveTo(int newSelection)
{
	withView(*this, [&](LieroMenu* s, LieroMenuItem* items, int32_t count) {
		liero_menu_move_to(s, items, count, newSelection);
	});
}

bool Menu::itemPosition(MenuItem& item, int& x, int& y)
{
	int index = int(&item - &items[0]);
	return withView(*this, [&](LieroMenu* s, LieroMenuItem* items, int32_t count) {
		return liero_menu_item_position(s, items, count, index, &x, &y);
	});
}

void Menu::setTop(int newTopVisIdx)
{
	withView(*this, [&](LieroMenu* s, LieroMenuItem* items, int32_t count) {
		liero_menu_set_top(s, items, count, newTopVisIdx);
	});
}

void Menu::setVisibility(int id, bool state)
{
	int item = indexFromId(id);
	if (item < 0)
	{
		sassert(false);
		return;
	}

	withView(*this, [&](LieroMenu* s, LieroMenuItem* items, int32_t count) {
		liero_menu_set_visibility(s, items, count, item, state);
	});
}

void Menu::scroll(int dir)
{
	withView(*this, [&](LieroMenu* s, LieroMenuItem* items, int32_t count) {
		liero_menu_scroll(s, items, count, dir);
	});
}

void Menu::movementPage(int direction)
{
	withView(*this, [&](LieroMenu* s, LieroMenuItem* items, int32_t count) {
		liero_menu_movement_page(s, items, count, direction);
	});
}

void Menu::movement(int direction)
{
	withView(*this, [&](LieroMenu* s, LieroMenuItem* items, int32_t count) {
		liero_menu_movement(s, items, count, direction);
	});
}

int Menu::addItem(MenuItem item)
{
	int idx = (int)items.size();
	items.push_back(item);
	if(item.visible)
		++visibleItemCount;
	return idx;
}

void Menu::clear()
{
	items.clear();
	visibleItemCount = 0;
	setTop(0);
}

int Menu::addItem(MenuItem item, int pos)
{
	int idx = (int)items.size();
	items.insert(items.begin() + pos, item);
	if(item.visible)
		++visibleItemCount;
	return idx;
}

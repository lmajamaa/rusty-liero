#ifndef UUID_3DC24B15AD67494EEAB541B4AE253D0F
#define UUID_3DC24B15AD67494EEAB541B4AE253D0F

#include <SDL.h>
#include "../appclock.hpp"
#include <cstddef>
#include <string>
#include <cstdio>
#include <vector>
#include <memory>
#include "../gfx/color.hpp"
#include "../gfx/renderer.hpp"

#include <gvl/resman/shared_ptr.hpp>
#include <gvl/support/cstdint.hpp>

#include "menuItem.hpp"
#include "itemBehavior.hpp"

#include "integerBehavior.hpp"
#include "booleanSwitchBehavior.hpp"
#include "timeBehavior.hpp"
#include "enumBehavior.hpp"

struct Common;
struct Gfx;

// A menu: its state and items are in Rust (LieroMenuBox,
// rust/liero-game/src/menu.rs), along with selection, scrolling, searching
// and drawing. Items are referred to by index. What each item does is the
// ItemBehavior the menu gives for it.
struct Menu
{
	Menu(bool centered = false)
	: Menu(0, 0, centered)
	{
	}

	Menu(int x, int y, bool centered = false)
	: Menu(liero_menu_box_new(x, y, centered))
	{
	}

	Menu(Menu const& other)
	: Menu(liero_menu_box_clone(other.box))
	{
	}

	Menu& operator=(Menu const& other)
	{
		liero_menu_box_assign(box, other.box);
		return *this;
	}

	virtual ~Menu()
	{
		liero_menu_box_free(box);
	}

	void draw(Common& common, Renderer& renderer, bool disabled, int x = -1, bool showDisabledSelection = false);

	virtual void drawItemOverlay(Common& common, int item, int x, int y, bool selected, bool disabled)
	{
		// Nothing by default
	}

	virtual ItemBehavior* getItemBehavior(Common& common, int item)
	{
		// Dummy item behavior by default
		return new ItemBehavior;
	}

	virtual void onUpdate()
	{
		// Nothing by default
	}

	bool onLeftRight(Common& common, int dir)
	{
		int s = selection();
		if (!isSelectionValid()) return false;
		std::unique_ptr<ItemBehavior> b(getItemBehavior(common, s));
		return b->onLeftRight(*this, s, dir);
	}

	int onEnter(Common& common)
	{
		int s = selection();
		if (!isSelectionValid()) return false;
		std::unique_ptr<ItemBehavior> b(getItemBehavior(common, s));
		return b->onEnter(*this, s);
	}

	void onKeys(SDL_Keysym* begin, SDL_Keysym* end, bool contains = false)
	{
		onKeys(begin, end, contains, appTicks());
	}

	// `time` is in milliseconds; typing resumes the search within 1.5 s.
	void onKeys(SDL_Keysym* begin, SDL_Keysym* end, bool contains, Uint32 time)
	{
		for (; begin != end; ++begin)
			liero_menu_box_on_key(box, begin->sym, begin->scancode, contains, time);
	}

	void updateItems(Common& common)
	{
		for(int i = 0; i < itemCount(); ++i)
		{
			std::unique_ptr<ItemBehavior> b(getItemBehavior(common, i));

			b->onUpdate(*this, i);
		}

		onUpdate();
	}

	void place(int newX, int newY)
	{
		x = newX;
		y = newY;
	}

	bool isSelectionValid()
	{
		return selection() >= 0 && selection() < itemCount();
	}

	void moveToFirstVisible()
	{
		moveTo(0);
	}

	void movement(int direction) { liero_menu_box_movement(box, direction); }
	void movementPage(int direction) { liero_menu_box_movement_page(box, direction); }

	// Adds `item` at the end, or at `pos`, and returns how many items there
	// were before.
	int addItem(MenuItem const& item) { return add(item, -1, true); }
	int addItem(MenuItem const& item, int pos) { return add(item, pos, true); }
	// Adds `item` at the end without counting it as visible.
	void pushItem(MenuItem const& item) { add(item, -1, false); }

	bool itemPosition(int item, int& x, int& y) { return liero_menu_box_item_position(box, item, &x, &y); }

	void setHeight(int newHeight)
	{
		height = newHeight;
		setTop(topItem);
	}

	int selection()
	{
		return s.selection;
	}

	int selectedId()
	{
		return isSelectionValid() ? itemId(selection()) : -1;
	}

	int indexFromId(int id)
	{
		for (int i = 0; i < itemCount(); ++i)
		{
			if (itemId(i) == id)
				return i;
		}

		return -1;
	}

	// The items.
	int itemCount() const { return liero_menu_box_count(box); }
	int itemId(int i) const { return liero_menu_box_id(box, i); }
	LieroMenuItem item(int i) const { return liero_menu_box_item(box, i); }
	std::string itemString(int i) const { return text(item(i).string); }
	std::string itemValue(int i) const { return text(item(i).value); }
	bool itemVisible(int i) const { return item(i).visible; }
	void setItemString(int i, std::string const& v) { liero_menu_box_set_string(box, i, view(v)); }
	// Sets the value and shows it.
	void setItemValue(int i, std::string const& v) { liero_menu_box_set_value(box, i, view(v)); }
	// Sets the flag alone, as `item.visible =` did; setVisibility also
	// counts it.
	void setItemVisibleFlag(int i, bool v) { liero_menu_box_set_visible_flag(box, i, v); }
	std::string searchPrefix() const { return text(liero_menu_box_search_text(box)); }

	void setVisibility(int id, bool state);
	void moveTo(int newSelection) { liero_menu_box_move_to(box, newSelection); }
	void moveToId(int id) { moveTo(indexFromId(id)); }
	void setTop(int newTopVisIdx) { liero_menu_box_set_top(box, newTopVisIdx); }
	void scroll(int amount) { liero_menu_box_scroll(box, amount); }

	LieroMenuBox* box;
	LieroMenu& s;

	int& itemHeight;
	int& valueOffsetX;

	int& x;
	int& y;
	int& height;

	int& topItem; // Visible index
	int& bottomItem; // Visible index

	int& visibleItemCount;

	bool& centered;

private:
	explicit Menu(LieroMenuBox* box)
	: box(box)
	, s(*liero_menu_box_state(box))
	, itemHeight(s.item_height)
	, valueOffsetX(s.value_offset_x)
	, x(s.x)
	, y(s.y)
	, height(s.height)
	, topItem(s.top_item)
	, bottomItem(s.bottom_item)
	, visibleItemCount(s.visible_item_count)
	, centered(s.centered)
	{
	}

	int add(MenuItem const& item, int pos, bool count);

	static std::string text(LieroStr v)
	{
		return std::string(reinterpret_cast<char const*>(v.ptr), v.len);
	}

	static LieroStr view(std::string const& v)
	{
		return { reinterpret_cast<uint8_t const*>(v.data()), v.size() };
	}
};

#endif // UUID_3DC24B15AD67494EEAB541B4AE253D0F

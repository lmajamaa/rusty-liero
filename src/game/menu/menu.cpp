#include "menu.hpp"

#include "../common.hpp"
#include "../gfx/bitmap.hpp"
#include <cassert>

// Selection, scrolling, searching and drawing are implemented in Rust
// (rust/liero-game/src/menu.rs), which owns the menu.

int Menu::add(MenuItem const& item, int pos, bool count)
{
	LieroMenuItemSpec spec;
	spec.string = view(item.string);
	spec.value = view(item.value);
	spec.color = item.color;
	spec.dis_colour = item.disColour;
	spec.has_value = item.hasValue;
	spec.visible = item.visible;
	spec.selectable = item.selectable;
	spec.id = item.id;
	return liero_menu_box_add(box, &spec, pos, count);
}

static void noOverlay(void*, int32_t, int32_t, int32_t, bool, bool)
{
}

void Menu::draw(Common& common, Renderer& renderer, bool disabled, int x, bool showDisabledSelection)
{
	LieroMenuHooks hooks = { nullptr, noOverlay };
	LieroBitmap scr = renderer.bmp.view();
	LieroFont font = common.font.view();
	liero_menu_box_draw(box, &scr, &font, &hooks, disabled, x, showDisabledSelection);
}

void Menu::setVisibility(int id, bool state)
{
	int item = indexFromId(id);
	if (item < 0)
	{
		assert(false);
		return;
	}

	liero_menu_box_set_visibility(box, item, state);
}

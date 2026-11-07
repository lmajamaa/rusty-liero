#include "menu.hpp"

#include "../common.hpp"
#include "../gfx/bitmap.hpp"
#include <gvl/support/debug.hpp>

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

namespace
{

struct OverlayContext
{
	Menu& menu;
	Common& common;
};

void hookDrawOverlay(void* ctx, int32_t item, int32_t x, int32_t y, bool selected, bool disabled)
{
	OverlayContext& c = *static_cast<OverlayContext*>(ctx);
	c.menu.drawItemOverlay(c.common, item, x, y, selected, disabled);
}

}

void Menu::draw(Common& common, Renderer& renderer, bool disabled, int x, bool showDisabledSelection)
{
	OverlayContext ctx = { *this, common };
	LieroMenuHooks hooks = { &ctx, hookDrawOverlay };
	LieroBitmap scr = renderer.bmp.view();
	LieroFont font = common.font.view();
	liero_menu_box_draw(box, &scr, &font, &hooks, disabled, x, showDisabledSelection);
}

void Menu::setVisibility(int id, bool state)
{
	int item = indexFromId(id);
	if (item < 0)
	{
		sassert(false);
		return;
	}

	liero_menu_box_set_visibility(box, item, state);
}

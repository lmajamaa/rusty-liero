#ifndef UUID_C5F17B0F3E6B43526CD95D90435B7596
#define UUID_C5F17B0F3E6B43526CD95D90435B7596

#include "itemBehavior.hpp"

// A number from `min` to `max`. Left and right step it, repeating while held
// (`data.scroll_interval`); Enter types one in, unless `data.allow_entry` is
// off.
struct IntegerBehavior : RustItemBehavior
{
	IntegerBehavior(Common& common, int& v, int min, int max, int step = 1, bool percentage = false)
	: RustItemBehavior(common, LieroItemKind_Integer)
	{
		data.value = &v;
		data.min = min;
		data.max = max;
		data.step = step;
		data.value_kind = percentage ? LIERO_MENU_VALUE_PERCENTAGE : LIERO_MENU_VALUE_INTEGER;
	}
};

#endif // UUID_C5F17B0F3E6B43526CD95D90435B7596

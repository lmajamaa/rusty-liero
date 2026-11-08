#ifndef UUID_3B614A27F8FE4D5D7E0254BE877C9E5F
#define UUID_3B614A27F8FE4D5D7E0254BE877C9E5F

#include "integerBehavior.hpp"

// Seconds, or frames shown as seconds; can't be typed in.
struct TimeBehavior : IntegerBehavior
{
	TimeBehavior(Common& common, int& v, int min, int max, int step = 1, bool frames = false)
	: IntegerBehavior(common, v, min, max, step, false)
	{
		data.value_kind = frames ? LIERO_MENU_VALUE_FRAMES : LIERO_MENU_VALUE_TIME;
		data.allow_entry = false;
	}
};

#endif // UUID_3B614A27F8FE4D5D7E0254BE877C9E5F

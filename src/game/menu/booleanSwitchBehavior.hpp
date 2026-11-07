#ifndef UUID_0F083A4D564C4D79CA6387B1D0F1901E
#define UUID_0F083A4D564C4D79CA6387B1D0F1901E

#include "itemBehavior.hpp"
#include "../common.hpp"

// On or off. `set`, when given, makes the change instead of writing `v`.
struct BooleanSwitchBehavior : RustItemBehavior
{
	BooleanSwitchBehavior(Common& common, bool& v, void (*set)(bool) = nullptr)
	: RustItemBehavior(common, LieroItemKind_Switch)
	{
		data.flag = &v;
		data.set_flag = set;
		setNames(common.texts.onoff);
	}
};

#endif // UUID_0F083A4D564C4D79CA6387B1D0F1901E

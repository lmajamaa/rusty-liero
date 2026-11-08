#ifndef UUID_171F520E38DF4A036E1AA3901EEA4801
#define UUID_171F520E38DF4A036E1AA3901EEA4801

#include "itemBehavior.hpp"

#include <cstdint>

// One of `min` to `max`, wrapping round; Enter moves to the next.
struct EnumBehavior : RustItemBehavior
{
	EnumBehavior(Common& common, uint32_t& v, uint32_t min, uint32_t max)
	: RustItemBehavior(common, LieroItemKind_Enum)
	, v(v), min(min), max(max)
	{
		static_assert(sizeof(uint32_t) == sizeof(int32_t), "enums are read as int32_t");
		data.value = reinterpret_cast<int32_t*>(&v);
		data.min = int32_t(min);
		data.max = int32_t(max);
	}

	uint32_t& v;
	uint32_t min, max;
};

#endif // UUID_171F520E38DF4A036E1AA3901EEA4801

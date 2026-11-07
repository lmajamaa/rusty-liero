#ifndef UUID_CCCDF474CB704A62DC6F0AB0CBBD2EFD
#define UUID_CCCDF474CB704A62DC6F0AB0CBBD2EFD

#include "enumBehavior.hpp"

#include <string>

// An enum shown with the names in `arr`.
struct ArrayEnumBehavior : EnumBehavior
{
	template<int N>
	ArrayEnumBehavior(Common& common, uint32_t& v, std::string const (&arr)[N])
	: EnumBehavior(common, v, 0, N-1)
	{
		setNames(arr);
	}
};

#endif // UUID_CCCDF474CB704A62DC6F0AB0CBBD2EFD

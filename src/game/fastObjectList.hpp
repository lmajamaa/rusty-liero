#ifndef UUID_A25E9863C68345C3B8E281AB7DFE5401
#define UUID_A25E9863C68345C3B8E281AB7DFE5401

#include <cstddef>
#include <cassert>
#include "bobject.hpp"
#include <liero_game.h>

// The blood particles. Rust owns them (LieroBObjectList in
// rust/liero-game/src/objects.rs), and Game::bobjects refers to them: `arr`
// holds `limit` particles, and the first `count` are in use.
struct BloodList
{
	typedef BObject* iterator;

	BloodList(BloodList const&) = delete;
	BloodList& operator=(BloodList const&) = delete;

	BObject* getFreeObject()
	{
		assert(count < limit);
		return &arr[count++];
	}

	BObject* newObjectReuse()
	{
		if(count == limit)
			return &arr[limit - 1];
		return getFreeObject();
	}

	BObject* newObject()
	{
		if(count == limit)
			return 0;
		return getFreeObject();
	}

	iterator begin() { return arr; }
	iterator end() { return arr + count; }

	// Moves the last particle in place of `ptr`.
	void free(BObject* ptr)
	{
		assert(ptr < arr + count && ptr >= arr);
		*ptr = arr[--count];
	}

	void clear()
	{
		count = 0;
	}

	// Keeps the first particles. Pointers into the list are invalid after.
	void resize(std::size_t newLimit)
	{
		liero_bobject_list_resize(reinterpret_cast<LieroBObjectList*>(this), newLimit);
	}

	std::size_t size() const
	{
		return count;
	}

	BObject* arr;
	std::size_t limit;
	std::size_t count;
};

#endif // UUID_A25E9863C68345C3B8E281AB7DFE5401

#ifndef LIERO_TCLIST_HPP
#define LIERO_TCLIST_HPP

#include <cstddef>
#include <string>
#include <liero_game.h>

// A list of the TC's, in Rust.
template<typename T>
struct TcList
{
	TcList()
	{
	}

	TcList(T const* items, std::size_t count)
	: items(items), count(count)
	{
	}

	T const* items = nullptr;
	std::size_t count = 0;

	std::size_t size() const { return count; }
	bool empty() const { return count == 0; }
	T const& operator[](std::size_t i) const { return items[i]; }
	T const* begin() const { return items; }
	T const* end() const { return items + count; }
};

// A TC's name or text as a std::string.
inline std::string tcString(LieroStr s)
{
	return std::string(reinterpret_cast<char const*>(s.ptr), s.len);
}

#endif // LIERO_TCLIST_HPP

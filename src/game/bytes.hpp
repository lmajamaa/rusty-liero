#ifndef LIERO_BYTES_HPP
#define LIERO_BYTES_HPP

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

// Reads bytes in order, as gvl's octet_reader did: reading past the end
// throws `endError`, after copying what there was.
struct ByteReader
{
	ByteReader()
	: data(std::make_shared<std::vector<uint8_t>>())
	{
	}

	explicit ByteReader(std::vector<uint8_t> bytes, std::string endError = "Read error in get()")
	: data(std::make_shared<std::vector<uint8_t>>(std::move(bytes)))
	, endError(std::move(endError))
	{
	}

	uint8_t get()
	{
		if (pos >= data->size())
			throw std::runtime_error(endError);
		return (*data)[pos++];
	}

	void get(uint8_t* dest, std::size_t len)
	{
		if (tryGet(dest, len) != len)
			throw std::runtime_error(endError);
	}

	// As many as there are, up to `len`.
	std::size_t tryGet(uint8_t* dest, std::size_t len)
	{
		std::size_t n = std::min(len, data->size() - pos);
		if (n)
			std::memcpy(dest, data->data() + pos, n);
		pos += n;
		return n;
	}

	// Copies share the bytes; each has its own place in them.
	std::shared_ptr<std::vector<uint8_t> const> data;
	std::size_t pos = 0;
	std::string endError = "Read error in get()";
};

#endif // LIERO_BYTES_HPP

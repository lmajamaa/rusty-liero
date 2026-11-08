#ifndef LIERO_FILESYSTEM_HPP
#define LIERO_FILESYSTEM_HPP

#include <string>
#include <vector>
#include <stdexcept>
#include <liero_game.h>

std::string getLeaf(std::string const& path);
std::string getBasename(std::string const& path);
std::string joinPath(std::string const& root, std::string const& leaf);

struct NodeName
{
	NodeName(std::string nameInit, bool isDirInit)
	: name(std::move(nameInit))
	, isDir(isDirInit)
	{
	}

	std::string name;
	bool isDir;
};

// A folder's entries, sorted, with .zip files as folders.
struct DirectoryListing
{
	std::vector<NodeName> subs;

	std::vector<NodeName>::iterator begin() { return subs.begin(); }
	std::vector<NodeName>::iterator end() { return subs.end(); }
};

// A path to list, read or write, where a .zip file reads as a folder. Rust
// owns it (LieroFsNode in rust/liero-game/src/fs.rs).
struct FsNode
{
	FsNode()
	: node(nullptr)
	{
	}

	explicit FsNode(std::string const& path);

	FsNode(FsNode const& other)
	: node(other.node ? liero_fs_node_clone(other.node) : nullptr)
	, path(other.path)
	{
	}

	FsNode(FsNode&& other)
	: node(other.node)
	, path(std::move(other.path))
	{
		other.node = nullptr;
	}

	FsNode& operator=(FsNode other)
	{
		std::swap(node, other.node);
		std::swap(path, other.path);
		return *this;
	}

	~FsNode()
	{
		liero_fs_node_free(node);
	}

	// Whether there's a node, not whether it exists.
	operator void*() const
	{
		return node && !liero_fs_node_is_null(node) ? node : nullptr;
	}

	std::string const& fullPath() const
	{ return path; }

	DirectoryListing iter() const;

	FsNode operator/(std::string const& name) const;

	bool exists() const
	{ return node && liero_fs_node_exists(node); }

	// The whole file, or an exception if it can't be read.
	std::vector<uint8_t> read() const;

	// The Rust node, or null.
	LieroFsNode* rust() const
	{ return node; }

	// Writes the whole file, or throws if it can't.
	void write(std::string const& data) const;

private:
	explicit FsNode(LieroFsNode* node);

	LieroFsNode* node;
	std::string path;
};

#endif // LIERO_FILESYSTEM_HPP

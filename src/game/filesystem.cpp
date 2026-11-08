#include "filesystem.hpp"

// Listing, reading, writing and the .zip folders are in Rust
// (rust/liero-game/src/fs.rs).

std::string getLeaf(std::string const& path)
{
	std::size_t lastSep = path.find_last_of("\\/");
	if (lastSep == std::string::npos)
		return path;
	return path.substr(lastSep + 1);
}

std::string getBasename(std::string const& path)
{
	std::size_t lastSep = path.find_last_of(".");
	if(lastSep == std::string::npos)
		return path;
	return path.substr(0, lastSep);
}

std::string joinPath(std::string const& root, std::string const& leaf)
{
	if(!root.empty()
	&& root[root.size() - 1] != '\\'
	&& root[root.size() - 1] != '/')
	{
		return root + '/' + leaf;
	}
	else
	{
		return root + leaf;
	}
}

namespace
{

LieroStr lstr(std::string const& s)
{
	LieroStr r = { reinterpret_cast<uint8_t const*>(s.data()), s.size() };
	return r;
}

std::string str(LieroStr s)
{
	return std::string(reinterpret_cast<char const*>(s.ptr), s.len);
}

}

FsNode::FsNode(std::string const& path)
: FsNode(liero_fs_node_new(lstr(path)))
{
}

FsNode::FsNode(LieroFsNode* n)
: node(n)
, path(str(liero_fs_node_full_path(n)))
{
}

FsNode FsNode::operator/(std::string const& name) const
{
	if (!node)
		return FsNode();
	return FsNode(liero_fs_node_go(node, lstr(name)));
}

DirectoryListing FsNode::iter() const
{
	DirectoryListing l;
	if (node)
	{
		liero_fs_node_list(node, &l, [](void* ctx, LieroStr name, bool isDir)
		{
			static_cast<DirectoryListing*>(ctx)->subs.push_back(NodeName(str(name), isDir));
		});
	}
	return l;
}

std::vector<uint8_t> FsNode::read() const
{
	std::vector<uint8_t> data;
	bool ok = node && liero_fs_node_read(node, &data, [](void* ctx, LieroStr d)
	{
		static_cast<std::vector<uint8_t>*>(ctx)->assign(d.ptr, d.ptr + d.len);
	});
	if (!ok)
		throw std::runtime_error("Could not read " + path);
	return data;
}

void FsNode::write(std::string const& data) const
{
	if (!node || !liero_fs_node_write(node, lstr(data)))
		throw std::runtime_error("Could not write " + path);
}

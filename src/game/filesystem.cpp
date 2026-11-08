#include "filesystem.hpp"
#include <gvl/io2/fstream.hpp>
#include <cstdio>

// Listing, reading and the .zip folders are in Rust
// (rust/liero-game/src/fs.rs). Writing still opens the file here, at the
// path Rust made ready, until the files written move to Rust.

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

// A file's bytes as a stream: all of them, then its end.
struct BytesPipe : gvl::bucket_pipe
{
	explicit BytesPipe(std::vector<uint8_t> data)
	: data(std::move(data))
	{
	}

	gvl::source_result read_next(std::size_t) override
	{
		if (data.empty())
			return gvl::source_result(gvl::source_result::eos);
		uint8_t const* p = data.data();
		gvl::unique_ptr<gvl::bucket_data_mem> r(gvl::bucket_data_mem::create_from(p, p + data.size(), data.size()));
		data.clear();
		return gvl::source_result(std::move(r));
	}

	std::vector<uint8_t> data;
};

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

gvl::source FsNode::toSource() const
{
	return gvl::to_source(new BytesPipe(read()));
}

gvl::sink FsNode::toSink() const
{
	LieroStr p = node ? liero_fs_node_prepare_write(node) : LieroStr{ nullptr, 0 };
	FILE* f = p.len ? std::fopen(str(p).c_str(), "wb") : nullptr;
	if (!f)
		throw std::runtime_error("Could not write " + path);
	return gvl::sink(new gvl::file_bucket_pipe(f));
}

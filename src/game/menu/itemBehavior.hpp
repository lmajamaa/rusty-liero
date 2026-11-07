#ifndef UUID_8BF7504F0306489E5EA42995B2020DD2
#define UUID_8BF7504F0306489E5EA42995B2020DD2

#include <liero_game.h>

#include <string>
#include <vector>

struct Common;
struct Menu;
struct MenuItem;

struct ItemBehavior
{
	ItemBehavior()
	{
	}

	virtual ~ItemBehavior()
	{
	}

	virtual bool onLeftRight(Menu& menu, MenuItem& item, int dir)
	{
		return true;
	}

	virtual int onEnter(Menu& menu, MenuItem& item)
	{
		return -1;
	}

	virtual void onUpdate(Menu& menu, MenuItem& item)
	{
	}
};

// An item whose keys and value text are in Rust (rust/liero-game/src/item.rs):
// an on/off switch, a number or an enum, editing a setting in place. C++
// does what Rust's result asks: redraw, or open the number box.
struct RustItemBehavior : ItemBehavior
{
	RustItemBehavior(Common& common, LieroItemKind kind);

	bool onLeftRight(Menu& menu, MenuItem& item, int dir) override;
	int onEnter(Menu& menu, MenuItem& item) override;
	void onUpdate(Menu& menu, MenuItem& item) override;

	// The names for the switch (off, on) or the enum's values.
	template<int N>
	void setNames(std::string const (&arr)[N])
	{
		names.clear();
		for (auto const& s : arr)
			names.push_back({ reinterpret_cast<uint8_t const*>(s.data()), s.size() });
	}

	// `data` with the names pointing into this copy.
	LieroItemBehavior view() const;
	void apply(Menu& menu, MenuItem& item, LieroItemAction a);

	Common& common;
	LieroItemBehavior data;
	std::vector<LieroStr> names;
};

#endif // UUID_8BF7504F0306489E5EA42995B2020DD2

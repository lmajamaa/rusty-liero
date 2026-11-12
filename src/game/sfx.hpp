#ifndef LIERO_SFX_HPP
#define LIERO_SFX_HPP

#include <liero_game.h>

// SDL's audio, playing the Rust mixer (rust/liero-game/src/sfx.rs).
struct Sfx
{
	~Sfx();

	void init();
	void deinit();

	// The mixer, if sound started.
	LieroSoundOut out() const
	{
		return LieroSoundOut{ initialized ? mixer : nullptr, nullptr };
	}

	LieroMixer* mixer = nullptr;
	bool initialized = false;
};

extern Sfx sfx;

#endif // LIERO_SFX_HPP

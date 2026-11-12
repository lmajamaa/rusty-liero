#ifndef LIERO_SFX_HPP
#define LIERO_SFX_HPP

#include <vector>
#include "mixer/player.hpp"

struct Common;

struct Sfx
{
	Sfx()
	: mixer(0)
	, initialized(false)
	{
	}
	~Sfx();

	void init();
	void deinit();

	void play(Common& common, int sound, void* id = 0, int loops = 0);
	bool isPlaying(void* id);

	void stop(void* id);
	LieroMixer* mixer;
	bool initialized;
};

extern Sfx sfx;


struct DefaultSoundPlayer : SoundPlayer
{
	DefaultSoundPlayer(Common& common)
	: common(common)
	{
	}

	Common& common;

	// The mixer, if sound started.
	LieroSoundOut out()
	{
#if DISABLE_SOUND
		return LieroSoundOut{ nullptr, nullptr };
#else
		return LieroSoundOut{ sfx.initialized ? sfx.mixer : nullptr, nullptr };
#endif
	}

	void play(int sound, void* id = 0, int loops = 0)
	{
		sfx.play(common, sound, id, loops);
	}

	bool isPlaying(void* id)
	{
		return sfx.isPlaying(id);
	}

	void stop(void* id)
	{
		sfx.stop(id);
	}
};



#endif // LIERO_SFX_HPP

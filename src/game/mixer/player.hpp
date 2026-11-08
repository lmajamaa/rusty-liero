#ifndef LIERO_MIXER_PLAYER_HPP
#define LIERO_MIXER_PLAYER_HPP

#include <memory>
#include <liero_game.h>
#include "../common.hpp"

// Plays the game's sounds. The mixer is in Rust (rust/liero-game/src/sfx.rs),
// and the game tick plays on `out()`.
struct SoundPlayer
{
	virtual ~SoundPlayer() {}
	virtual LieroSoundOut out() = 0;
	virtual void play(int sound, void* id = 0, int loops = 0) = 0;
	virtual bool isPlaying(void* id) = 0;
	virtual void stop(void* id) = 0;
};

// Starts a sound on a mixer at the mixer's current time.
inline void playOnMixer(LieroMixer* mixer, Common& common, int sound, void* id, int loops)
{
	SfxSample const& s = common.sounds[sound];
	liero_mixer_add(mixer, s.samples, s.sample_count, uint32_t(liero_mixer_now(mixer)), id, loops ? LIERO_SOUND_LOOP : LIERO_SOUND_NORMAL);
}

// Plays sounds on a mixer it doesn't own, for the hash run and videotool.
struct RecordSoundPlayer : SoundPlayer
{
	RecordSoundPlayer(Common& common, LieroMixer* mixer)
	: mixer(mixer)
	, common(common)
	{
	}

	LieroMixer* mixer;
	Common& common;

	LieroSoundOut out()
	{
		return LieroSoundOut{ mixer, nullptr };
	}

	void play(int sound, void* id = 0, int loops = 0)
	{
		playOnMixer(mixer, common, sound, id, loops);
	}

	bool isPlaying(void* id)
	{
		return liero_mixer_is_playing(mixer, id);
	}

	void stop(void* id)
	{
		liero_mixer_stop(mixer, id);
	}
};

struct NullSoundPlayer : SoundPlayer
{
	LieroSoundOut out()
	{
		return LieroSoundOut{ nullptr, nullptr };
	}

	void play(int sound, void* id, int loops)
	{
	}

	bool isPlaying(void* id)
	{
		return false;
	}

	void stop(void* id)
	{
	}
};

#endif // LIERO_MIXER_PLAYER_HPP

#include "sfx.hpp"
#include "reader.hpp"
#include "console.hpp"
#include "common.hpp"
#include <vector>
#include <cassert>
#if !DISABLE_SOUND
#	include <SDL.h>
#endif

Sfx sfx;

// SDL's audio callback. Emscripten runs it on the main thread, between the
// game's frames, so the mixer is never used from two threads at once.
extern "C" void SDLCALL Sfx_callback(void *userdata, Uint8 *stream, int len)
{
	liero_mixer_mix(static_cast<LieroMixer*>(userdata), reinterpret_cast<int16_t*>(stream), len / 2);
}

void Sfx::init()
{
#if !DISABLE_SOUND
	if(initialized)
		return;

	SDL_InitSubSystem(SDL_INIT_AUDIO);

	mixer = liero_mixer_new();

	SDL_AudioSpec spec;
	memset(&spec, 0, sizeof(spec));
	spec.channels = 1;
	spec.freq = 44100;
	spec.format = AUDIO_S16SYS;
	spec.size = 4*512;
	spec.callback = Sfx_callback;
	spec.userdata = mixer;

	int ret = SDL_OpenAudio(&spec, NULL);

	if(ret == 0)
	{
		initialized = true;
		SDL_PauseAudio(0);
	}
	else
	{
		Console::writeWarning(std::string("SDL_OpenAudio returned error: ") + SDL_GetError());
		liero_mixer_free(mixer);
		mixer = 0;
	}
#endif
}

void Sfx::deinit()
{
#if !DISABLE_SOUND
	if(!initialized)
		return;
	initialized = false;

	SDL_CloseAudio();
	SDL_QuitSubSystem(SDL_INIT_AUDIO);
	liero_mixer_free(mixer);
	mixer = 0;
#endif
}

void Sfx::play(Common& common, int sound, void* id, int loops)
{
#if !DISABLE_SOUND
	if(!initialized)
		return;

	playOnMixer(mixer, common, sound, id, loops);
#endif
}

void Sfx::stop(void* id)
{
#if !DISABLE_SOUND
	if(!initialized)
		return;

	liero_mixer_stop(mixer, id);
#endif
}

bool Sfx::isPlaying(void* id)
{
#if !DISABLE_SOUND
	if(!initialized)
		return false;

	return liero_mixer_is_playing(mixer, id);
#else
	return false;
#endif
}

Sfx::~Sfx()
{
	deinit();
}

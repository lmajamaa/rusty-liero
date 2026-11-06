#ifndef UUID_96141CB1E20547016970B28195515A14
#define UUID_96141CB1E20547016970B28195515A14

#include <SDL.h>
#include <liero_game.h>

// SDL scancodes to the game's DOS keys, and joystick buttons as extended
// keys. The table and the rules are in Rust (rust/liero-game/src/keys.rs).

inline Uint32 SDLToDOSKey(SDL_Scancode scancode)
{
	return liero_sdl_to_dos_key(scancode);
}

inline Uint32 SDLToDOSKey(SDL_Keysym const& keysym)
{
	return liero_sdl_to_dos_keysym(keysym.scancode);
}

int const DkEscape = LIERO_DK_ESCAPE;

int const MaxJoyButtons = LIERO_MAX_JOY_BUTTONS;

uint32_t const MaxDOSKey 	= LIERO_MAX_DOS_KEY;
uint32_t const JoyKeysStart	= LIERO_JOY_KEYS_START;

inline uint32_t joyButtonToExKey( int joyNum, int joyButton ) {
	return liero_joy_key(joyNum, joyButton);
}

inline bool isExtendedKey( uint32_t k ) {
	return liero_key_is_extended(k);
}

#endif // UUID_96141CB1E20547016970B28195515A14

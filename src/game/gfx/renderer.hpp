#ifndef LIERO_GFX_RENDERER_HPP
#define LIERO_GFX_RENDERER_HPP

#include "../common.hpp"
#include "../rand.hpp"
#include "bitmap.hpp"

// A bitmap to draw on, with its palettes. Rust owns it (LieroRenderer in
// rust/liero-game/src/renderer.rs); the fields here refer to it.
struct Renderer
{
	Renderer();
	~Renderer();
	Renderer(Renderer const&) = delete;
	Renderer& operator=(Renderer const&) = delete;

	void init(int x, int y);
	void clear();
	void loadPalette(Common const& common);
	void setRenderResolution(int x, int y);

	// The renderer in Rust.
	LieroRenderer* rust() const { return r; }

private:
	LieroRenderer* r;
	LieroRendererData& d;

public:
	// the bitmap that is drawn into by this renderer
	Bitmap bmp;
	Palette& pal;
	Palette& origpal;
	int& fadeValue;
	// Resolution to render the game at. This should be modified via
	// setRenderResolution() to ensure that the bitmap is re-allocated
	int& renderResX;
	int& renderResY;
};

#endif // LIERO_GFX_RENDERER_HPP

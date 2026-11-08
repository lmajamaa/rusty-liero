#include "renderer.hpp"
#include "blit.hpp"
#include "../common.hpp"

// Renderers and bitmaps are in Rust (rust/liero-game/src/renderer.rs).

static_assert(sizeof(Palette) == sizeof(LieroColor) * 256, "Palette layout");
static_assert(sizeof(Rect) == sizeof(LieroRect), "Rect layout");

Bitmap::Bitmap()
: Bitmap(liero_bitmap_box_new())
{
	owned = true;
}

Bitmap::Bitmap(LieroBitmapBox* box)
: box(box)
, owned(false)
, v(*liero_bitmap_box_view(box))
, w(v.w)
, h(v.h)
, pitch(v.pitch)
, pixels(v.pixels)
, clip_rect(reinterpret_cast<Rect&>(v.clip))
{
}

Bitmap::~Bitmap()
{
	if (owned)
		liero_bitmap_box_free(box);
}

Renderer::Renderer()
: Renderer(liero_renderer_new())
{
	owned = true;
}

Renderer::Renderer(LieroRenderer* r)
: r(r)
, owned(false)
, d(*liero_renderer_data(r))
, bmp(liero_renderer_bitmap(r))
, pal(reinterpret_cast<Palette&>(d.pal))
, origpal(reinterpret_cast<Palette&>(d.origpal))
, fadeValue(d.fade_value)
, renderResX(d.render_res_x)
, renderResY(d.render_res_y)
{
}

Renderer::~Renderer()
{
	if (owned)
		liero_renderer_free(r);
}

void Renderer::init(int x, int y)
{
	setRenderResolution(x, y);
}

void Renderer::setRenderResolution(int x, int y)
{
	liero_renderer_set_render_resolution(r, x, y);
}

void Renderer::loadPalette(Common const& common)
{
	liero_renderer_load_palette(r, liero_tc_data(common.tc));
}

void Renderer::clear()
{
	liero_renderer_clear(r);
}

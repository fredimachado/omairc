package avatar

import (
	"image"
	"image/color"
	"testing"
)

func TestScaleBoxMatchesAtPath(t *testing.T) {
	parent := image.Rect(0, 0, 80, 70)
	sub := image.Rect(3, 5, 44, 42)
	origin := image.Rect(6, 11, 49, 40)
	odd := image.Rect(1, 2, 38, 33)
	cases := []struct {
		name       string
		src        image.Image
		dstW, dstH int
	}{
		{"nrgba", filledNRGBA(image.Rect(0, 0, 40, 30)), 32, 24},
		{"nrgba-origin", filledNRGBA(origin), 32, 24},
		{"nrgba-sub", filledNRGBA(parent).SubImage(sub), 32, 24},
		{"nrgba-wide", filledNRGBA(image.Rect(0, 0, 200, 10)), 32, 1},
		{"rgba", filledRGBA(image.Rect(0, 0, 40, 30)), 13, 9},
		{"rgba-origin", filledRGBA(origin), 13, 9},
		{"rgba-sub", filledRGBA(parent).SubImage(sub), 13, 9},
		{"ycbcr-444", filledYCbCr(image.Rect(0, 0, 40, 30), image.YCbCrSubsampleRatio444), 32, 24},
		{"ycbcr-420", filledYCbCr(image.Rect(0, 0, 40, 30), image.YCbCrSubsampleRatio420), 32, 24},
		{"ycbcr-420-origin", filledYCbCr(image.Rect(3, 5, 39, 33), image.YCbCrSubsampleRatio420), 32, 16},
		{"ycbcr-420-sub", filledYCbCr(parent, image.YCbCrSubsampleRatio420).SubImage(odd), 15, 11},
		{"ycbcr-422-sub", filledYCbCr(parent, image.YCbCrSubsampleRatio422).SubImage(sub), 16, 10},
		{"ycbcr-440", filledYCbCr(image.Rect(1, 1, 33, 27), image.YCbCrSubsampleRatio440), 8, 6},
		{"ycbcr-411", filledYCbCr(image.Rect(0, 2, 36, 20), image.YCbCrSubsampleRatio411), 8, 6},
		{"ycbcr-410", filledYCbCr(image.Rect(4, 3, 40, 31), image.YCbCrSubsampleRatio410), 8, 6},
		{"paletted", filledPaletted(image.Rect(0, 0, 40, 30)), 32, 24},
		{"paletted-origin", filledPaletted(origin), 9, 8},
		{"paletted-sub", filledPaletted(parent).SubImage(sub), 17, 13},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			assertScaleMatchesAt(t, tc.src, tc.dstW, tc.dstH)
		})
	}
}

func TestScaleBoxEmptyBounds(t *testing.T) {
	assertEmptyScale := func(t *testing.T, src image.Image) {
		t.Helper()
		got := scaleBox(src, 4, 3)
		if got == nil {
			t.Fatal("empty scale returned nil")
		}
		if got.Bounds().Dx() != 4 || got.Bounds().Dy() != 3 {
			t.Fatalf("bounds = %v, want 4x3", got.Bounds())
		}
		for i, pix := range got.Pix {
			if pix != 0 {
				t.Fatalf("pixel %d = %d, want 0", i, pix)
			}
		}
	}
	assertEmptyScale(t, image.NewNRGBA(image.Rect(0, 0, 0, 0)))
	assertEmptyScale(t, image.NewNRGBA(image.Rect(0, 0, 8, 8)).SubImage(image.Rect(2, 2, 2, 2)))
	assertEmptyScale(t, image.NewRGBA(image.Rect(0, 0, 8, 8)).SubImage(image.Rect(1, 1, 1, 1)))
	assertEmptyScale(t, image.NewYCbCr(image.Rect(0, 0, 8, 8), image.YCbCrSubsampleRatio444).SubImage(image.Rect(3, 3, 3, 3)))
	paletted := image.NewPaletted(image.Rect(0, 0, 8, 8), color.Palette{color.Black, color.White})
	assertEmptyScale(t, paletted.SubImage(image.Rect(4, 4, 4, 4)))
	if got := scaleBox(nil, 2, 2); got.Bounds().Dx() != 2 || got.Pix[0] != 0 {
		t.Fatalf("nil source = %v", got.Bounds())
	}
	if got := scaleBox(image.NewNRGBA(image.Rect(0, 0, 0, 0)), 0, 0); !got.Bounds().Empty() {
		t.Fatalf("zero destination = %v", got.Bounds())
	}
}

func TestScaleBoxFastPathDoesNotAllocatePerPixel(t *testing.T) {
	parent := image.Rect(0, 0, 64, 48)
	sub := image.Rect(3, 5, 43, 37)
	cases := []struct {
		name string
		src  image.Image
	}{
		{"nrgba", filledNRGBA(parent)},
		{"nrgba-sub", filledNRGBA(image.Rect(0, 0, 80, 70)).SubImage(sub)},
		{"rgba", filledRGBA(parent)},
		{"rgba-sub", filledRGBA(image.Rect(0, 0, 80, 70)).SubImage(sub)},
		{"ycbcr", filledYCbCr(parent, image.YCbCrSubsampleRatio420)},
		{"ycbcr-sub", filledYCbCr(image.Rect(0, 0, 80, 70), image.YCbCrSubsampleRatio420).SubImage(image.Rect(1, 2, 41, 34))},
		{"paletted", filledPaletted(parent)},
		{"paletted-sub", filledPaletted(image.Rect(0, 0, 80, 70)).SubImage(sub)},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			// A 64x48 At path allocates one color per source pixel. The fast
			// path allocates the destination, and for non-NRGBA a converted buffer.
			allocs := testing.AllocsPerRun(20, func() {
				scaleBox(tc.src, 16, 12)
			})
			if allocs > 12 {
				t.Fatalf("allocs per run = %v, want no per-pixel color allocation", allocs)
			}
		})
	}
}

func TestScaleBoxAveragesBox(t *testing.T) {
	img := image.NewNRGBA(image.Rect(0, 0, 2, 2))
	img.SetNRGBA(0, 0, color.NRGBA{R: 10, A: 255})
	img.SetNRGBA(1, 0, color.NRGBA{R: 30, A: 255})
	img.SetNRGBA(0, 1, color.NRGBA{})
	img.SetNRGBA(1, 1, color.NRGBA{R: 50, A: 255})
	got := scaleBox(img, 1, 1).NRGBAAt(0, 0)
	// (10+30+0+50)/4 = 22, (255+255+0+255)/4 = 191.
	want := color.NRGBA{R: 22, A: 191}
	if got != want {
		t.Fatalf("got %+v, want %+v", got, want)
	}
}

func TestScaleBoxSubImageKeepsOrigin(t *testing.T) {
	parent := image.NewNRGBA(image.Rect(0, 0, 6, 6))
	for y := 0; y < 6; y++ {
		for x := 0; x < 6; x++ {
			parent.SetNRGBA(x, y, color.NRGBA{R: 255, G: 255, B: 255, A: 255})
		}
	}
	parent.SetNRGBA(2, 3, color.NRGBA{R: 10, A: 255})
	parent.SetNRGBA(3, 3, color.NRGBA{R: 30, A: 255})
	parent.SetNRGBA(2, 4, color.NRGBA{})
	parent.SetNRGBA(3, 4, color.NRGBA{R: 50, A: 255})
	sub := parent.SubImage(image.Rect(2, 3, 4, 5))
	got := scaleBox(sub, 1, 1).NRGBAAt(0, 0)
	want := color.NRGBA{R: 22, A: 191}
	if got != want {
		t.Fatalf("got %+v, want %+v", got, want)
	}
}

func TestScaleBoxAveragesAfterUnpremultiply(t *testing.T) {
	img := image.NewRGBA(image.Rect(0, 0, 2, 1))
	img.SetRGBA(0, 0, color.RGBA{R: 255, A: 255})
	img.SetRGBA(1, 0, color.RGBA{})
	got := scaleBox(img, 1, 1).NRGBAAt(0, 0)
	// Opaque red and transparent black, converted first, then averaged.
	want := color.NRGBA{R: 127, A: 127}
	if got != want {
		t.Fatalf("got %+v, want %+v", got, want)
	}
}

func TestScaleBoxRGBAUnpremultiplies(t *testing.T) {
	img := image.NewRGBA(image.Rect(8, 3, 9, 4))
	src := color.RGBA{R: 128, G: 64, B: 32, A: 128}
	img.SetRGBA(8, 3, src)
	got := scaleBox(img, 1, 1).NRGBAAt(0, 0)
	want := color.NRGBAModel.Convert(src).(color.NRGBA)
	if got != want {
		t.Fatalf("got %+v, want %+v", got, want)
	}
}

func TestScaleBoxYCbCrMatchesConvert(t *testing.T) {
	img := image.NewYCbCr(image.Rect(2, 4, 3, 5), image.YCbCrSubsampleRatio444)
	img.Y[img.YOffset(2, 4)] = 0x7f
	img.Cb[img.COffset(2, 4)] = 0x7f
	img.Cr[img.COffset(2, 4)] = 0x7f
	got := scaleBox(img, 1, 1).NRGBAAt(0, 0)
	want := color.NRGBAModel.Convert(color.YCbCr{Y: 0x7f, Cb: 0x7f, Cr: 0x7f}).(color.NRGBA)
	if got != want {
		t.Fatalf("got %+v, want %+v", got, want)
	}
}

func TestScaleBoxPalettedMatchesConvert(t *testing.T) {
	src := color.RGBA{R: 10, A: 10}
	img := image.NewPaletted(image.Rect(4, 5, 5, 6), color.Palette{src})
	img.SetColorIndex(4, 5, 0)
	got := scaleBox(img, 1, 1).NRGBAAt(0, 0)
	want := color.NRGBAModel.Convert(src).(color.NRGBA)
	if got != want {
		t.Fatalf("got %+v, want %+v", got, want)
	}
}

func TestRGBAToNRGBAMatchesModel(t *testing.T) {
	samples := [][3]uint8{{0, 0, 0}, {255, 255, 255}, {128, 64, 32}, {1, 2, 3}, {200, 10, 255}}
	for a := 0; a < 256; a++ {
		for _, rgb := range samples {
			src := color.RGBA{R: rgb[0], G: rgb[1], B: rgb[2], A: uint8(a)}
			want := color.NRGBAModel.Convert(src).(color.NRGBA)
			r, g, b, aa := rgbaToNRGBA(src.R, src.G, src.B, src.A)
			got := color.NRGBA{R: r, G: g, B: b, A: aa}
			if got != want {
				t.Fatalf("RGBA%+v -> %+v, want %+v", src, got, want)
			}
		}
	}
}

func TestYCbCrToRGBMatchesNRGBAModel(t *testing.T) {
	for y := 0; y < 256; y += 17 {
		for cb := 0; cb < 256; cb += 19 {
			for cr := 0; cr < 256; cr += 23 {
				r, g, b := color.YCbCrToRGB(uint8(y), uint8(cb), uint8(cr))
				want := color.NRGBAModel.Convert(color.YCbCr{Y: uint8(y), Cb: uint8(cb), Cr: uint8(cr)}).(color.NRGBA)
				got := color.NRGBA{R: r, G: g, B: b, A: 0xff}
				if got != want {
					t.Fatalf("YCbCr{%d,%d,%d} -> %+v, want %+v", y, cb, cr, got, want)
				}
			}
		}
	}
}

// rampImage is not one of the decoder types, so scaleBox must use At.
type rampImage struct{}

func (rampImage) ColorModel() color.Model { return color.NRGBAModel }
func (rampImage) Bounds() image.Rectangle { return image.Rect(2, 5, 6, 9) }
func (rampImage) At(x, y int) color.Color {
	return color.NRGBA{R: uint8(x), G: uint8(y), B: 7, A: 255}
}

func TestScaleBoxFallbackReadsAt(t *testing.T) {
	src := rampImage{}
	got := scaleBox(src, 4, 4)
	bounds := src.Bounds()
	for y := bounds.Min.Y; y < bounds.Max.Y; y++ {
		for x := bounds.Min.X; x < bounds.Max.X; x++ {
			want := src.At(x, y).(color.NRGBA)
			dx, dy := x-bounds.Min.X, y-bounds.Min.Y
			if got.NRGBAAt(dx, dy) != want {
				t.Fatalf("(%d,%d) = %+v, want %+v", dx, dy, got.NRGBAAt(dx, dy), want)
			}
		}
	}
}

func assertScaleMatchesAt(t *testing.T, src image.Image, dstW, dstH int) {
	t.Helper()
	got := scaleBox(src, dstW, dstH)
	want := scaleBoxAt(src, dstW, dstH)
	if got.Bounds() != want.Bounds() {
		t.Fatalf("bounds %v, want %v", got.Bounds(), want.Bounds())
	}
	w, h := got.Bounds().Dx(), got.Bounds().Dy()
	for y := 0; y < h; y++ {
		for x := 0; x < w; x++ {
			g := got.NRGBAAt(x, y)
			wpx := want.NRGBAAt(x, y)
			if g != wpx {
				t.Fatalf("(%d,%d) = %+v, want %+v", x, y, g, wpx)
			}
		}
	}
}

func filledNRGBA(r image.Rectangle) *image.NRGBA {
	img := image.NewNRGBA(r)
	for y := r.Min.Y; y < r.Max.Y; y++ {
		for x := r.Min.X; x < r.Max.X; x++ {
			img.SetNRGBA(x, y, color.NRGBA{
				R: uint8(x * 13),
				G: uint8(y * 7),
				B: uint8(x + y*3),
				A: uint8(x*5 + y*11),
			})
		}
	}
	return img
}

func filledRGBA(r image.Rectangle) *image.RGBA {
	img := image.NewRGBA(r)
	for y := r.Min.Y; y < r.Max.Y; y++ {
		for x := r.Min.X; x < r.Max.X; x++ {
			img.SetRGBA(x, y, color.RGBA{
				R: uint8(x * 3),
				G: uint8(y * 5),
				B: uint8((x + y) * 9),
				A: uint8((x*13 + y*7) % 256),
			})
		}
	}
	if r.Dx() > 0 && r.Dy() > 0 {
		img.SetRGBA(r.Min.X, r.Min.Y, color.RGBA{R: 10, G: 20, B: 30, A: 0})
		img.SetRGBA(r.Max.X-1, r.Max.Y-1, color.RGBA{R: 9, G: 8, B: 7, A: 255})
		img.SetRGBA(r.Min.X, r.Max.Y-1, color.RGBA{R: 128, G: 64, B: 32, A: 128})
	}
	return img
}

func filledYCbCr(r image.Rectangle, ratio image.YCbCrSubsampleRatio) *image.YCbCr {
	img := image.NewYCbCr(r, ratio)
	for i := range img.Y {
		img.Y[i] = uint8(17*i + 3)
	}
	for i := range img.Cb {
		img.Cb[i] = uint8(128 + (i*13)%128)
		img.Cr[i] = uint8(i * 9)
	}
	return img
}

func filledPaletted(r image.Rectangle) *image.Paletted {
	pal := color.Palette{
		color.NRGBA{R: 10, G: 20, B: 30, A: 40},
		color.NRGBA{R: 255, A: 255},
		color.RGBA{R: 128, A: 128},
		color.RGBA{},
		color.RGBA{R: 200, G: 100, B: 50, A: 255},
		color.Gray{Y: 180},
		color.NRGBA{R: 1, G: 2, B: 3},
	}
	img := image.NewPaletted(r, pal)
	for i := range img.Pix {
		img.Pix[i] = uint8(i % len(pal))
	}
	return img
}

// scaleBoxSink keeps BenchmarkScaleBox from being deleted by the compiler.
var scaleBoxSink *image.NRGBA

// BenchmarkScaleBox times a 1024x1024 shrink to the Kitty transmit edge.
// The image is *image.NRGBA, which is what png.Decode returns for a truecolor avatar.
func BenchmarkScaleBox(b *testing.B) {
	const edge = 1024
	src := image.NewNRGBA(image.Rect(0, 0, edge, edge))
	for i := 0; i < len(src.Pix); i += 4 {
		src.Pix[i] = uint8(i)
		src.Pix[i+1] = uint8(i >> 8)
		src.Pix[i+2] = uint8(i >> 16)
		src.Pix[i+3] = 255
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		scaleBoxSink = scaleBox(src, graphicsMaxEdge, graphicsMaxEdge)
	}
}

package avatar

import (
	"image"
	"image/color"
)

// scaleBox averages source rectangles into a dstW by dstH NRGBA image.
// *image.NRGBA, *image.RGBA, *image.YCbCr, and *image.Paletted are read from
// their pixel buffers. Every other Image uses scaleBoxAt. Both paths use
// boxSpan, then average non-premultiplied NRGBA samples.
func scaleBox(src image.Image, dstW, dstH int) *image.NRGBA {
	if src == nil || src.Bounds().Empty() || dstW <= 0 || dstH <= 0 {
		return emptyScaled(dstW, dstH)
	}
	switch img := src.(type) {
	case *image.NRGBA:
		return scaleBoxNRGBA(img, dstW, dstH)
	case *image.RGBA:
		return scaleBoxNRGBA(rgbaAsNRGBA(img), dstW, dstH)
	case *image.YCbCr:
		return scaleBoxNRGBA(ycbcrAsNRGBA(img), dstW, dstH)
	case *image.Paletted:
		if len(img.Palette) == 0 {
			return scaleBoxAt(img, dstW, dstH)
		}
		return scaleBoxNRGBA(palettedAsNRGBA(img), dstW, dstH)
	default:
		return scaleBoxAt(src, dstW, dstH)
	}
}

// scaleBoxNRGBA is the box filter on a straight-alpha buffer. RGBA, YCbCr,
// and Paletted images are converted into this layout first, so the average
// reads bytes instead of image.Image.At.
func scaleBoxNRGBA(src *image.NRGBA, dstW, dstH int) *image.NRGBA {
	if src == nil || src.Bounds().Empty() || dstW <= 0 || dstH <= 0 {
		return emptyScaled(dstW, dstH)
	}
	dst := image.NewNRGBA(image.Rect(0, 0, dstW, dstH))
	bounds := src.Bounds()
	width, height := bounds.Dx(), bounds.Dy()
	pix := src.Pix
	stride := src.Stride
	minX, minY := bounds.Min.X, bounds.Min.Y
	for y := 0; y < dstH; y++ {
		sy0, sy1 := boxSpan(minY, height, dstH, y)
		for x := 0; x < dstW; x++ {
			sx0, sx1 := boxSpan(minX, width, dstW, x)
			var totalR, totalG, totalB, totalA int64
			n := sx1 - sx0
			for py := sy0; py < sy1; py++ {
				i := (py-minY)*stride + (sx0-minX)*4
				row := pix[i : i+n*4 : i+n*4]
				for p := 0; p < len(row); p += 4 {
					totalR += int64(row[p])
					totalG += int64(row[p+1])
					totalB += int64(row[p+2])
					totalA += int64(row[p+3])
				}
			}
			count := int64(n) * int64(sy1-sy0)
			if count == 0 {
				continue
			}
			dst.SetNRGBA(x, y, color.NRGBA{
				R: uint8(totalR / count),
				G: uint8(totalG / count),
				B: uint8(totalB / count),
				A: uint8(totalA / count),
			})
		}
	}
	return dst
}

// emptyScaled is the safe result for a nil image, an empty bounds, or a
// non-positive destination. The pixels are zero, which is what Image.At
// returns for a coordinate outside an empty image.
func emptyScaled(dstW, dstH int) *image.NRGBA {
	if dstW < 0 {
		dstW = 0
	}
	if dstH < 0 {
		dstH = 0
	}
	return image.NewNRGBA(image.Rect(0, 0, dstW, dstH))
}

// boxSpan returns the half-open source interval averaged into destination
// pixel i. A zero-width interval grows by one pixel, matching the original filter.
func boxSpan(origin, srcLen, dstLen, i int) (int, int) {
	min := origin + i*srcLen/dstLen
	max := origin + (i+1)*srcLen/dstLen
	if max <= min {
		max = min + 1
	}
	return min, max
}

// rgbaAsNRGBA unpremultiplies src into a straight-alpha image with the same bounds.
func rgbaAsNRGBA(src *image.RGBA) *image.NRGBA {
	bounds := src.Bounds()
	dst := image.NewNRGBA(bounds)
	width := bounds.Dx()
	for y := bounds.Min.Y; y < bounds.Max.Y; y++ {
		si := src.PixOffset(bounds.Min.X, y)
		srow := src.Pix[si : si+width*4 : si+width*4]
		di := dst.PixOffset(bounds.Min.X, y)
		drow := dst.Pix[di : di+width*4 : di+width*4]
		for i := 0; i < len(srow); i += 4 {
			r, g, b, a := rgbaToNRGBA(srow[i], srow[i+1], srow[i+2], srow[i+3])
			drow[i] = r
			drow[i+1] = g
			drow[i+2] = b
			drow[i+3] = a
		}
	}
	return dst
}

// rgbaToNRGBA matches color.NRGBAModel.Convert for a color.RGBA pixel.
// Opaque and transparent pixels take nrgbaModel's short circuits. Anything
// in between is unpremultiplied from the 16-bit expansion RGBA.RGBA produces.
func rgbaToNRGBA(r, g, b, a uint8) (uint8, uint8, uint8, uint8) {
	if a == 0xff {
		return r, g, b, 0xff
	}
	if a == 0 {
		return 0, 0, 0, 0
	}
	r16 := uint32(r)<<8 | uint32(r)
	g16 := uint32(g)<<8 | uint32(g)
	b16 := uint32(b)<<8 | uint32(b)
	a16 := uint32(a)<<8 | uint32(a)
	r16 = (r16 * 0xffff) / a16
	g16 = (g16 * 0xffff) / a16
	b16 = (b16 * 0xffff) / a16
	return uint8(r16 >> 8), uint8(g16 >> 8), uint8(b16 >> 8), uint8(a16 >> 8)
}

// ycbcrAsNRGBA expands JPEG planes to straight RGB. YCbCrToRGB's 8-bit result
// is the high byte of color.YCbCr.RGBA, which is what NRGBAModel keeps.
func ycbcrAsNRGBA(src *image.YCbCr) *image.NRGBA {
	bounds := src.Bounds()
	dst := image.NewNRGBA(bounds)
	width := bounds.Dx()
	for y := bounds.Min.Y; y < bounds.Max.Y; y++ {
		di := dst.PixOffset(bounds.Min.X, y)
		drow := dst.Pix[di : di+width*4 : di+width*4]
		for x := bounds.Min.X; x < bounds.Max.X; x++ {
			yi := src.YOffset(x, y)
			ci := src.COffset(x, y)
			r, g, b := color.YCbCrToRGB(src.Y[yi], src.Cb[ci], src.Cr[ci])
			i := (x - bounds.Min.X) * 4
			drow[i] = r
			drow[i+1] = g
			drow[i+2] = b
			drow[i+3] = 0xff
		}
	}
	return dst
}

// palettedAsNRGBA converts the palette once, then expands indexes. The palette
// is what Image.At returns for these pixels, so NRGBAModel runs per entry
// instead of per pixel.
func palettedAsNRGBA(src *image.Paletted) *image.NRGBA {
	table := make([]color.NRGBA, len(src.Palette))
	for i, c := range src.Palette {
		table[i] = color.NRGBAModel.Convert(c).(color.NRGBA)
	}
	bounds := src.Bounds()
	dst := image.NewNRGBA(bounds)
	width := bounds.Dx()
	for y := bounds.Min.Y; y < bounds.Max.Y; y++ {
		si := src.PixOffset(bounds.Min.X, y)
		srow := src.Pix[si : si+width : si+width]
		di := dst.PixOffset(bounds.Min.X, y)
		drow := dst.Pix[di : di+width*4 : di+width*4]
		for x, idx := range srow {
			c := table[idx]
			i := x * 4
			drow[i] = c.R
			drow[i+1] = c.G
			drow[i+2] = c.B
			drow[i+3] = c.A
		}
	}
	return dst
}

// scaleBoxAt is the generic Image.At filter. It allocates a color per source
// pixel; the concrete paths exist so a decoded avatar does not.
func scaleBoxAt(src image.Image, dstW, dstH int) *image.NRGBA {
	if src == nil || src.Bounds().Empty() || dstW <= 0 || dstH <= 0 {
		return emptyScaled(dstW, dstH)
	}
	dst := image.NewNRGBA(image.Rect(0, 0, dstW, dstH))
	bounds := src.Bounds()
	width, height := bounds.Dx(), bounds.Dy()
	for y := 0; y < dstH; y++ {
		minY, maxY := boxSpan(bounds.Min.Y, height, dstH, y)
		for x := 0; x < dstW; x++ {
			minX, maxX := boxSpan(bounds.Min.X, width, dstW, x)
			var totalR, totalG, totalB, totalA int64
			var count int
			for py := minY; py < maxY; py++ {
				for px := minX; px < maxX; px++ {
					c := color.NRGBAModel.Convert(src.At(px, py)).(color.NRGBA)
					totalR += int64(c.R)
					totalG += int64(c.G)
					totalB += int64(c.B)
					totalA += int64(c.A)
					count++
				}
			}
			if count == 0 {
				continue
			}
			dst.SetNRGBA(x, y, color.NRGBA{
				R: uint8(totalR / int64(count)),
				G: uint8(totalG / int64(count)),
				B: uint8(totalB / int64(count)),
				A: uint8(totalA / int64(count)),
			})
		}
	}
	return dst
}

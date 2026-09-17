# macOS app icon

Omairc ships `data/icons/omairc.icns` for the `.app` bundle. The committed
PNG sources live in `omairc.iconset/`. Regenerate them from
`../hicolor/scalable/apps/omairc.svg` when the mark changes, then run
`packaging/macos/generate-icns` on macOS:

```sh
# optional: refresh PNGs from the SVG (requires rsvg-convert)
rsvg-convert -w 1024 -h 1024 ../hicolor/scalable/apps/omairc.svg -o omairc.iconset/icon_512x512@2x.png
# ...other iconset sizes...

packaging/macos/generate-icns
```

CI and local macOS builds call `generate-icns` before `qmake` when
`omairc.icns` is missing or older than the SVG.

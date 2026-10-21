# Screenshots

The desktop app at 1280x900. The images use real data from a copy of the local cache. Nothing is mocked.
The dark images use the palette `ink`. The light images use the palette `paper`.

Each image has two forms:

- `*.png`: the window on a backdrop, with a 12 px corner radius and a shadow.
  Use it where the image is alone, for example in a README or a release post.
- `plain/*.png`: the same window with transparent rounded corners and no backdrop.
  Use it on a background of your own.

| Screen | Dark | Light |
|---|---|---|
| Today | `today-dark.png` | `today-light.png` |
| Activities | `activities-dark.png` | `activities-light.png` |
| Activity (a run, with zones) | `activity-dark.png` | `activity-light.png` |
| Activity (a strength session) | `activity-strength-dark.png` | - |
| Ask (empty state) | `ask-dark.png` | `ask-light.png` |
| Insights | `insights-dark.png` | `insights-light.png` |
| Health | `health-dark.png` | `health-light.png` |
| Reports | `reports-dark.png` | `reports-light.png` |
| Weight | `weight-dark.png` | `weight-light.png` |
| Food (empty state) | `food-dark.png` | - |
| Plan | `plan-dark.png` | - |
| Routes | `routes-dark.png` | - |
| Setup | `setup-dark.png` | `setup-light.png` |

Limits of the cache data:

- The cache has no analysis rows. The Activity page has no route map, no chart and no laps.
- The cache has no meal and no chat session. Food and Ask show their empty states.
- The account has no gear. Gear is not here.

## Retaking them

Use a debug build and a copy of the cache. Never open the live cache.

```sh
sqlite3 ~/.local/share/garmin-coach/cache.sqlite3 ".backup /tmp/shots/cache.sqlite3"
APP=target/debug/garmin-companion SIZE=1280x900 WAIT=10 ~/.claude/helpers/gc-vshoot raw.png \
  --db /tmp/shots/cache.sqlite3 --palette ink --screen health
```

- `gc-vshoot` runs the app in a private virtual KWin. The user's screen is not used.
- `--screen <name>` opens a screen. `--activity <id>` opens one session.
- `GC_SHOW_SETUP=1` shows the Setup screen. Only a debug build reads it.
- `--palette paper` gives the light image.

Add the frame with ImageMagick. `raw.png` is the capture:

```sh
magick -size 1280x900 xc:black -fill white -draw "roundrectangle 0,0 1279,899 12,12" mask.png
magick raw.png -alpha off mask.png -compose CopyOpacity -composite plain/NAME.png
magick -size 1440x1060 gradient:"#3d3629-#1a1613" \
  \( plain/NAME.png -background black -shadow 55x28+0+14 \) -geometry +60+44 -compose over -composite \
  plain/NAME.png -geometry +80+58 -compose over -composite NAME.png
```

For a light image, use the gradient `#e8dfd0-#cfc2ad`.

## desktop/

The folder `desktop/` holds images of the app from the synthetic fixture only. Never add real data here.
Take them with `--persona golfer` in place of `--db`.

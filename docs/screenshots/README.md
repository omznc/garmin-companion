# Screenshots

Desktop, 1280×900, real data out of the local cache — nothing mocked and nothing
posed. Each screen shot in both themes where both are worth showing.

Two forms of every shot:

- `*.png` — the window on a backdrop, with the 12px corner radius and the shadow
  a Linux compositor would draw. Use these anywhere the image stands alone: a
  README hero, a release post, a store listing.
- `plain/*.png` — the same window with transparent rounded corners and no
  backdrop, for dropping onto a background of your own.

| Screen | Dark | Light |
|---|---|---|
| Today | `today-dark.png` | `today-light.png` |
| Activities | `activities-dark.png` | `activities-light.png` |
| Activity | `activity-dark.png` | `activity-light.png` |
| Activity — zones and HR | `activity-zones-dark.png` | — |
| Activity — route and elevation | `activity-route-dark.png` | — |
| Ask | `ask-dark.png` | `ask-light.png` |
| Insights | `insights-dark.png` | `insights-light.png` |
| Health | `health-dark.png` | `health-light.png` |
| Reports | `reports-dark.png` | `reports-light.png` |
| Weight | `weight-dark.png` | `weight-light.png` |
| Food | `food-dark.png` | — |
| Plan | `plan-dark.png` | — |
| Routes | `routes-dark.png` | — |

Gear is not here: the account has no gear registered, so the screen only has its
empty state to show.

## Retaking them

The app is undecorated and GNOME refuses screenshots to unsandboxed clients, so
these were taken in a nested X server, where the window can be sized exactly and
captured by id:

```sh
Xwayland :99 -geometry 1560x1020 -noreset &
env -u WAYLAND_DISPLAY DISPLAY=:99 GDK_BACKEND=x11 ./target/debug/app &
```

With no window manager on that display the window sits at (0,0) and can be
resized straight through `XConfigureWindow`, which is what makes every shot the
same size. Two consequences worth knowing: nothing composites the transparent
corners, so the capture is square and the radius is added afterwards, and GTK
draws its own resize grip into the bottom-right corner, which is painted out.

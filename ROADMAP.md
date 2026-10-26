# Roadmap (draft for review)

**Nothing here is decided.** These are ideas collected from the project's history (mostly the non-goals and deferred items of archived OpenSpec changes) to pick up in future `/openspec-explore` sessions. The process below is a proposal too.

## Proposed process

```
          ┌──────────────────────── Parked (with the reason) ◀──────────────┐
          │                                                                 │
  Later ──▶ Next ──▶ exploring ──▶ proposed ──▶ in progress ──▶ shipped (v0.N)
 (ideas)   (owner    (/openspec-   (change      (review loop,     (archived; its
            picks)    explore)      <topic>-vN)  implement, test)  non-goals feed
                                                                   back into Later)
```

- **Now:** what's in flight, linked to its open change.
- **Next:** the 1–3 ideas the owner picked. Only the owner reorders it.
- **Later:** everything else, by theme, one line each: idea, why, rough size (S/M/L), where it came from.
- **Parked:** declined ideas with the reason and date, so they aren't re-argued.

Proposed rules:
1. A new idea mid-session goes into Later with a one-line why (an agent may add it; never to Next).
2. Starting `/openspec-explore` on an idea marks it *exploring*.
3. Proposing a change moves it to Now with the change name.
4. On sign-off (AGENTS.md step 6) it leaves the roadmap (the archive and the tag record it), and the change's non-goals and known limitations are offered for Later.
5. A declined idea moves to Parked with the reason.

This file lives at the repo root. Open question: should non-goals be added to Later automatically at sign-off, or proposed for the owner to approve?

## Now

- **Microsoft Store release** (`store-release-v10`): in certification.

## Next

*(not picked yet)*

## Later

### Distribution
| Idea | Why | Size | Source |
|---|---|---|---|
| GitHub Actions CI: test, clippy, package on every push and tag | Catches breakage off the owner's machine; SignPath requires an automated build | M | winget explore |
| Community winget repo via SignPath-signed GitHub Releases | `winget install` without the Store; free OSS signing | M | winget explore |
| "Screen Shifter is already running" message instead of exiting silently | A second launch looks like nothing happened | S | screenshot session |
| Title text on the Store poster art | The poster is the main Store image; today it's the icon only | S | Store logos |
| ARM64 build | Windows on Arm laptops | M | — |

### Slots
| Idea | Why | Size | Source |
|---|---|---|---|
| Jump straight to slot N (Ctrl+Alt+1…8) | Faster than scrolling with many slots | S | — |
| Name slots | "Work", "Music": easier to find in the panel | M | slot-panel-v2 non-goal |
| Reorder slots (drag a tile) | Slots only append today | M | slot-panel-v2 non-goal |
| Drag a whole slot onto another monitor's panel | Move a whole context at once | M | panel non-goal (tile-to-tile drag) |
| Keep slots when monitors are plugged or unplugged (laptop docking) | Today a monitor change restores everything and resets slots | L | monitor-slots spec |
| Survive a reboot: match windows by exe and title | Window handles don't survive; layouts are lost on restart | L | v2 non-goal |
| Per-app rules ("Teams always opens on monitor 2, slot 3") | New windows land where they belong | M–L | — |

### Panel and exposé
| Idea | Why | Size | Source |
|---|---|---|---|
| Keyboard navigation of the panel and exposé | Accessibility; mouse-free use | M | v6/v7 non-goals |
| Hi-res icons on panel tiles | Tiles still use 16 px icons | S | slot-close-icons-v9 non-goal |
| Live (continuously updating) previews | Previews of hidden windows are pictures from when they were last visible | L | slot-expose-v7 non-goal |
| Exposé without the GPU path (Remote Desktop, unusual displays) | The exposé is unavailable there today | M | slot-expose-v7 non-goal |

### Input
| Idea | Why | Size | Source |
|---|---|---|---|
| Fix Task View still opening on a three-finger swipe after Windows' setting is restored | Known limitation | M | panel-polish-v8 |
| Settings UI: shortcuts, slot limit, swipe distances, animation speed | Everything is fixed today | L | several non-goals |

### Platform
| Idea | Why | Size | Source |
|---|---|---|---|
| "Own the illusion": live window tracking with WinEvent hooks | Slots follow windows as they open, move and close, not only at switch time | L | v1 deferred (approach B) |
| Taskbar and Win+Tab integration | Hidden windows vanish from both today | L | v1 non-goal |
| HDR-correct capture; multi-GPU capture | Slides and previews on HDR or mixed-GPU setups | M | gpu-motion-v6 non-goals |

## Parked

| Idea | Reason | When |
|---|---|---|
| A plate behind the preview icons | Dark icons on dark windows accepted | 2026-10 (v0.9) |
| Force-closing windows from the exposé | The preview × asks the app to close instead: unsaved work must never be lost | 2026-10 (v0.9) |

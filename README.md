# Xenopalm XP-1

A strange experiment: the [Palmkussion PK-1](../palmkussion-pk1) hand drum
performance synthesizer with its drum models swapped for the eight
experimental engines of [Xenokussion XK-1](https://github.com/mkforsb/xenokussion-xk1-slop),
plus the XK-1's **MUTATE** on each channel, now acting on every note the
player plays.

So a simulated percussionist (swing, human timing and dynamics, ghost notes,
flams, rolls, trills and fills) plays a tumbao, a martillo or a 6/8 on a
Lorenz attractor, a nonlinear gong and a random spectral wavetable. With
MUTATE up, the instruments change underneath the player's hands as it goes.

| Target | Audio | How |
| --- | --- | --- |
| Web (wasm) | WebAudio `AudioWorklet` | `make serve-web` |
| Native Linux | PulseAudio (also PipeWire's pulse server) | `make run-desktop` |

Requirements and other targets are as for the PK-1: Rust with
`wasm32-unknown-unknown`, Dioxus CLI 0.7, and for the desktop
`libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev libpulse-dev pkg-config`.
`make test lint renders bench` work as before. `XENOPALM_LATENCY_MS` sets
the desktop's PulseAudio target latency.

## What came from where

| Part | From |
| --- | --- |
| Transport, player, step sequencer, generator styles, ornaments, inspector, keyboard, drum heads | PK-1, unchanged |
| Shared plate room with low/mid/high EQ | PK-1 |
| The channel voice: MODAL, STRING, COMB, GONG, GRAIN, SPECTRA, FLUX, LORENZ, the exciter, SWEEP, CHAOS, JITTER, FILTER, FOLD/CRUSH, 2× oversampling and decimation | XK-1, unchanged |
| The 28 presets, RND, → copy, MUTATE | XK-1 |
| The XK-1's per-channel plates | Dropped: each channel has a ROOM send into the shared room |

## MUTATE

Each channel has a MUTATE knob from OFF to RND, as on the XK-1. Each time
it acts, every randomizable control moves that fraction of the way towards a
fresh random patch, `clamp(current + (random − current) × MUTATE)`, and the
new values snap in exactly on the hit. At RND every note is a brand new
sound, engine included. OUTPUT, SENSE, PAN and MUTATE itself are never
touched.

**Engine lock**: click a channel's selected engine again to lock it. A
padlock appears, and MUTATE leaves the engine alone while it keeps
mutating everything else, so a MODAL channel stays MODAL but wanders
through materials, tunings and dirt. Click it once more to unlock. Picking
another engine while locked moves the lock to it. Presets and → copy keep
the channel's lock. RND and RND ALL still pick a new engine.

It acts **once per note**: on every note the sequencer plays (written or
improvised), and on every live hit from the keyboard or the drum heads. It
does not act on the strokes *inside* a note:

| Note | Strokes | Mutations |
| --- | --- | --- |
| plain | 1 | 1 |
| FLAM | grace + main | 1 (on the grace, so the flam is one sound) |
| DRAG | 2 graces + main | 1 |
| 2X / 3X / TRILL | 2 / 3 / 4 | 1 |
| RUN | 4 strokes crossing both channels | 1 per channel (its first stroke on each) |

The sequencer runs on the audio thread, so MUTATE does too. The engine sends
each mutated patch back with its status, and the panel adopts it. The knobs
and the HIT scope move with every hit, as they did on the XK-1. The mutated
state is saved when you stop.

## Strokes on alien engines

These voices aren't drums, so a stroke can't be membrane physics any more.
Instead each stroke articulates the voice the way a hand articulates a drum:

| Stroke | Strike | Noise | Decay | Pitch | Macro Y | Level |
| --- | --- | --- | --- | --- | --- | --- |
| Bass | −0.35 | | | −0.4 oct | −0.25 | |
| Heel | −0.3 | | ×0.25 | −0.2 oct | −0.15 | ×0.7 |
| Tip | +0.1 | | ×0.3 | | | ×0.7 |
| Mute | | | ×0.2 | | | ×0.9 |
| Open | | | | | | |
| Slap | +0.4 | +0.25 | ×0.45 | | +0.15 | ×1.05 |

A strike's position offset (centre ↔ rim) adds hardness and moves macro Y.
On MODAL and STRING macro Y is the strike/pluck position, so there it means
exactly what it says. On the other engines it is a timbre nudge. The
decay multiplier lasts until the next hit, so a muted stroke chokes the
resonators the way a hand left on the head would. An open tone plays the
patch exactly as set.

## Layout

```
crates/dsp        no dependencies, 84 unit tests
  src/engines/      the XK-1's eight engines
  src/voice.rs      the XK-1 voice, plus Voice::strike (articulation)
  src/stroke.rs     strokes, ornaments, Strike and Articulation
  src/mutate.rs     RND, MUTATE and RND ALL (moved from the XK-1 panel to the DSP)
  src/performer.rs  the PK-1 player; each hit carries a "mutate" flag
  src/engine.rs     two voices + player + MUTATE + room + EQ; reports mutated patches
  src/params.rs     XK-1 voice params (+ MUTATE, minus per-channel reverb) and PK-1 globals
  src/presets.rs    the XK-1's 28 presets
crates/worklet    C ABI over the engine as a standalone wasm module
crates/app        Dioxus UI (web + desktop)
```

With MUTATE at RND on both channels and a dense pattern, the engine renders
about 50× real time on one desktop core (`make bench`).

# Agent characters

Reference for the agent characters shown in `mockups/hire-agent.html`. `chars.py` is the generator used for the mockup: it is a reference for shapes, states and animations, not code to ship. Airth implements its own generator in the app's JavaScript (or Rust), from the rules below.

## Look

- **Colour** from the department (one hue per department, never a status colour: no amber, red or green).
- **Accessory** from the role: pen for a writer of posts, lens for a watcher, book for a long-form writer. New roles get a new simple accessory.
- **Shape** (`round`, `bean`, `block`) and small traits drawn at random from a seed.
- The seed is stored in the agent's file (`agents/<name>.md`) when the agent is hired. The look never changes after that. "Another look" draws a new seed before hiring only.
- Drawn in SVG by code. No image file, no model-generated picture.

## States

| State | When | Animation |
|---|---|---|
| idle | nothing running, something scheduled | breathes, blinks |
| working | one of its routines or missions runs | bobs, accessory moves |
| needs you | it has an item waiting in Needs you | hops, waves, amber bubble with "!" |
| incident | its last run failed | wobbles, crossed eyes, stars |
| asleep | nothing scheduled | eyes closed, "z" floating |

## Rules

- Animated on Team and in the hiring step. In lists (Needs you, HQ activity) the character is still and moves only when its state changes.
- No animation when the system asks for reduced motion (`prefers-reduced-motion`).
- The character never replaces the agent's name or status text: it adds to them.
- CSS gotcha found in the mockup: a CSS animation's `transform` replaces an SVG `transform` attribute on the same element. Animate an inner group, keep the position on an outer one.

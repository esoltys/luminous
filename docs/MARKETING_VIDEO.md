# Marketing Video

How we plan, review and ship a Luminous marketing video. Use it for launch clips, release "What's New" clips, landing-page loops and Store listing videos. Fill in the brief first; nothing visual starts until it is complete.

## 1. Brief

Write these five answers before any storyboard. If you cannot fill them in, stop and decide.

| Field | Answer |
| --- | --- |
| Message | The one claim the video makes, in a sentence. |
| Audience | One kind of person, not "music lovers". |
| Destination | Landing page, social feed, Store listing, release notes. |
| Length | 15s, 30s or 60s. Each length is its own edit, not a stretched cut. |
| Aspect | 16:9, 1:1 or 9:16. |

Open every review with: **"This video tells [audience] that [message]."**

## 2. Story rules

- **Hook in outcome language.** The first beat says what the viewer gains or stops suffering. Never open with a feature name, a file count or a version number.
- **Value before evidence.** The message lands by the second beat. Everything after it is proof.
- **Two deletion tests** on the finished beat list. Remove the proof beats: the value should still be stated. Remove the value beats: if the video still seems to work, it was a feature tour, not a story.
- **Visualize, don't list.** Show a feature as the change in the user's experience. A text bullet is the last resort.
- **Trace every visual to the product.** Use real captured UI (see [TESTING.md](TESTING.md), remote devtools). If a shot could appear in any other player's video, replace it. Luminous-only material: the parametric EQ curve, the live spectrum, auto and smart playlists that stay complete as the library changes, user-defined tags, state restored on reopen, seven UI languages.
- **Claim only what we do.** Stay inside the Product Scope in AGENTS.md. Never imply specialist-tool depth (bulk retagging, fingerprint lookups).

## 3. Plan

Present the plan as a proposal, one row per frame:

| Frame | Beat (type, seconds) | On screen | Why |
| --- | --- | --- | --- |

**Why** is the frame's job in the story, traced to the message. A frame whose why cannot be traced gets cut.

Review in three passes and no more:

1. **Plan** (the table). Changing a frame costs seconds here.
2. **Sketches.** One static frame per beat at full brand fidelity (real type, colours, copy) with plain blocks for media not yet captured. No motion yet.
3. **Final cut.** One look before export.

## 4. Motion

- **One current.** Pick one dominant direction for the whole film and use it for every ordinary cut. Spend any other direction only on meaning (arrival, elevation) or a visible cause.
- **Continuity across cuts.** How a scene exits sets how the next enters: same axis, same direction, matched speed, cut mid-motion. When an element crosses a cut, write its position, scale, opacity and speed on both sides in the storyboard.
- **No idle motion.** Decorative elements never pulse, breathe or drift. The spectrum and waveform are the signal in a Luminous video; nothing else may look like them.
- **Every animation names its purpose**, as in the Motion budget in AGENTS.md: feedback, spatial continuity, state legibility, bridging a jarring change, first-run explanation, or rare delight.

## 5. Sound and captions

- Assume the video plays muted. Captions are required, not optional.
- Captions stay inside platform safe zones and out of the product UI being shown.
- Music is cleared for commercial use and recorded with its licence in the brief.
- Language: EN and FR follow the landing page. Decide per video whether captions cover all seven UI languages.

## 6. Checks before export

- [ ] One contact sheet, one frame per beat, reviewed together.
- [ ] Text contrast passes on every frame.
- [ ] On-screen text is held long enough to read.
- [ ] Captions sit inside safe zones.
- [ ] Muted watch test: someone can state the message.
- [ ] No hand-typed versions, counts or dates in the pixels; take them from the release data or build.
- [ ] Every claim is true of the shipped build and within Product Scope.

## 7. Production notes

- Start slow, independent work (voice, music, captures) together.
- Batch visual reviews into a contact sheet instead of inspecting single frames.
- After approval, save the brief, style and storyboard skeleton as a template. A release clip has the same shape every release; only the content changes.

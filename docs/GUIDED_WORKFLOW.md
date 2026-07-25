# The guided layer

Every production page in this app is complete and unreadable at the same time: the AI Composer alone
has nine config sections and forty-odd fields, and almost no single run needs more than a handful of
them. The guided layer renders the same page as a short conversation — a few questions with concrete
choices, a recommendation that knows this project, and the underlying section revealed only when its
question is live.

It is a layer, not a replacement. "All controls" returns any page to exactly what it was.

## The pieces

| File | Role |
| --- | --- |
| `src/src/lib/engineCapabilities.js` | What each music/image engine can actually do — tag dialect, flag syntax, duration control, steps, negatives, reference images. |
| `src/src/lib/guidedFlows.js` | The flows, as data: steps with a question, options, and an `apply` that writes into the page's own state. |
| `src/src/components/GuidedFlow.jsx` | Presentation: one question at a time, the progress rail, the escape hatch. Knows nothing about any page. |
| `src/src/components/GuidedPanel.jsx` | What a page mounts. Owns the on/off preference and loads the shared context (brief, channels, engines). |
| `src-tauri/commands/guide.rs` | `guide_proposal` — picks one option per step and says why, from the brief, today's topic, the learnings store, past choices, and the engines' capabilities. |
| `tests/guided-flows.test.mjs` | Holds the capability promise: no flow may offer a control the selected engine ignores. |

## What makes a recommendation

In order of authority:

1. **The AI's pick** for that step (`guide_proposal`), which reads the project brief, the daily topic,
   channel languages/regions, the learnings store, and this user's past choices in this flow.
2. **What this user picked last time**, from `localStorage` per flow.
3. **The step's own `recommended` flag** — a sane static default.

The AI can only answer with option ids the UI already offers. A hallucinated id is dropped, so the
worst case is a missing suggestion, never a broken page. Every choice is written back as a learning
signal (`guided_choice`, keyed `<flow>:<step>`), which is what the next proposal reads — so the
recommendations bend toward this user's habits after a few sessions.

## Capability gating is the point

Engines fail quietly when you send them something they do not parse: Suno reads `[Soft female vocal]`,
ACE-Step ignores it, HeartMuLa *sings* it. So a step or an option can declare `when(ctx)`:

```js
{
  id: "length",
  when: (ctx) => supports(musicEngine(ctx.settings?.music_engine), "durationControl"),
  …
}
```

The length question therefore exists for ACE-Step and HeartMuLa and does not exist for Suno. The same
mechanism keeps `--stylize` out of a FLUX prompt and the shorts aspect ratio out of an engine whose
model has no 9:16. `tests/guided-flows.test.mjs` asserts these per engine, so adding an engine to the
capability table is enough to make the flows adapt.

## Adding a flow to another page

1. Describe the decisions in `guidedFlows.js`. Keep it to the few that matter — a guide that asks
   about everything is just the old form with extra clicks.
2. Give each step a `reveals: "<section id>"` if the page has sections to open.
3. Mount it:

```jsx
<GuidedPanel
  flow={myFlow}
  projectId={activeProjectId}
  extraCtx={{ /* whatever the options need to read */ }}
  actions={{ setThing, run: doTheThing }}
/>
```

4. For a page with collapsible sections, control them from the step: `open={sectionOpen(id)}` where
   `sectionOpen` returns `undefined` outside guided mode, so the full page behaves as before.

Persist anything the flow decides that other parts of the app read (engine choice, render provider)
through `api.saveSettings` — not only in page state. The backend prompts and the scheduler read
settings, not React.

## Current flows

| Flow | Page | Steps |
| --- | --- | --- |
| `composer` | AI Composer | source · reach · mood · sound · visuals · run |
| `music` | Music Studio | engine · scope · length (engine-dependent) · start |
| `images` | Image Generation | coverage · consistency (if characters exist) · quality (engine-dependent) · run |
| `video` | Video Composer | where to render · motion · publish · start |

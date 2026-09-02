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
| `src/src/lib/speech.js` | The two rules behind taking a spoken answer — which language it is in, and when the speaker has finished — free of Web Audio, `localStorage` and Tauri so they can be tested directly. |
| `tests/guided-flows.test.mjs` | Holds the capability promise: no flow may offer a control the selected engine ignores. |

## What makes a recommendation

In order of authority:

1. **The AI's pick** for that step (`guide_proposal`), which reads the project brief, the daily topic,
   channel languages/regions, the learnings store, this user's past choices in this flow, and — since
   2026-09-02 — how their published videos actually performed (`performance_prompt_block`). The last
   of those outranks the others and has to say so when they disagree: a habit and a result are
   different things, and only one of them is evidence. It stays silent below six measured videos.
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

## How a session runs

1. **A template.** `guide_templates` reads the brief, the channels' styles and regions, the declared
   goal, today's topic, recent social posts and the user's own history, and proposes 3–4 aggregate
   preset packs — one recognisable direction each, filling as many controls as that direction implies.
   Each names the questions it could *not* settle; only those get asked.
2. **The open questions**, in the template's own order of importance.
3. **Manifestation.** A section appears when the guide settles it. Once a third appears, everything but
   the previous one collapses — still there, still openable, out of the way. Each carries a traffic
   light: rough (red) · sensible (amber) · chosen (green) · fine-tuned (blue). Status never downgrades,
   so a re-run template cannot overwrite an answer and a hand edit outranks both.
4. **Leaving is normal.** Template, answers, per-section status and the pause position persist per
   project and view. Returning shows the state it was left in, with Resume and "Show all controls" at
   the top and the bottom of the page.

The rules live in `lib/guidedView.js` (pure, tested in `tests/guided-view.test.mjs`); persistence is
`get/save/reset_workflow_state`; the control catalogues a template may write are `lib/controlCatalogs.js`,
validated server-side before anything is applied.

## Voice

`lib/voice.js` reads each question aloud and can take a spoken answer. Speaking cascades from the
backend's Gemini voices (cached on disk per phrase) to the webview's `speechSynthesis` to silence;
listening cascades from `SpeechRecognition` to `MediaRecorder` plus backend transcription to nothing.
`lib/voiceMatch.js` maps the answer locally — "yes" takes the suggestion, "the second one" and naming
an option both work — and only escalates to `guide_interpret` when that is genuinely ambiguous.

**Everything that takes dictation goes through `listen()`**, including the mic buttons outside the
guide. WebKitGTK — the desktop webview on Linux — has no `SpeechRecognition` at all, so a page that
calls the Web Speech API itself has a button that cannot work on the app's own primary platform.
Two did until 2026-09-02.

The rules underneath are in `lib/speech.js` (pure, tested in `tests/speech.test.mjs`):

- `resolveSpeechLanguage` answers in **both** shapes, because the two paths need different ones — a
  BCP-47 tag for `SpeechRecognition.lang`, a language *name* for the sentence `stt_transcribe` puts
  in front of the model. It defaults to the interface language; a page that knows the answer will be
  in the song's language rather than the interface's can pass that instead.
- `createSilenceGate` ends a recording when the speaker stops rather than when a stopwatch does. It
  calibrates a noise floor first (a fan must not read as endless speech), never stops before `minMs`
  (drawing breath is not an empty answer), never stops on silence alone (a pause before starting is
  not the end), and never runs past `maxMs`.

`speak()` puts its line through the interface catalog first (`translateKnown`, a lookup with no
request and no key), so the assistant speaks the language the user is reading. Guide questions and
option labels are all in the shipped inventory, so for the bundled languages this is free.

**Not yet a conversation.** The guide speaks, then waits for a click on "Say it" before it listens.
A hands-free mode is the loop that chains them — speak, listen, interpret, apply, speak the next —
plus a barge-in that stops the speaking when the microphone hears somebody. See `BACKLOG.md`.

## Current flows

| Flow | Page | Steps |
| --- | --- | --- |
| `composer` | AI Composer | source · reach · mood · sound · visuals · run |
| `music` | Music Studio | engine · scope · length (engine-dependent) · start |
| `images` | Image Generation | coverage · consistency (if characters exist) · quality (engine-dependent) · run |
| `video` | Video Composer | where to render · motion · publish · start |
| `bible` | Bible Sources | source · translation · fetch |
| `analysis` | Audio Analysis | how to cut sections · review |
| `characters` | Characters | recurring faces · per-channel variants (when both exist) |
| `upload` | Publishing | channel · who writes the metadata · visibility · queue |
| `social` | Social Presence | purpose · platforms · draft |
| `workflow` | Workflow | how far the run goes · where the heavy steps run · start |

Adding a flow to the registry is enough for the structural tests to cover it: every step needs an id, a
question and at least one option; option ids must be unique; every option must apply without throwing
on a bare context; and step titles must fit the progress rail.

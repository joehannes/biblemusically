# First run, and the welcome tour

## The problem this replaces

First run used to be the setup wizard: terms, then an AI provider key, then permissions, then a
Kaggle account, then Google OAuth, then a folder. Every one of those questions is necessary
eventually. All of them, asked before the person has seen a single screen of the app, put the most
technical minutes in the whole product in front of the person least equipped to sit through them —
and the honest answer to "why do I need a Kaggle account?" at that moment is *you don't know yet*.

So the order changed. First run now asks only what genuinely has to come first, then **shows the
app**, and leaves the connecting until something needs connecting.

## The shape of first run

| Order | What | Why it can't wait |
| --- | --- | --- |
| 1 | **Terms** | Everything after this is about what leaves the machine. Asking afterwards is asking at the point where saying no is expensive. |
| 2 | **How much should I explain?** | Decides the wording of everything that follows, including the tour itself. |
| 3 | **Language** | Same reason, one level up. |
| 4 | **Voice** | The tour narrates. Presets the device's own voice — free, offline, no key. |
| 5 | **The welcome tour** | Eleven stops around the real app. |
| — | *everything else* | Moved to **Set up & configure** in the graduation-cap menu. |

`FIRST_RUN_STEP_IDS` in `src/src/lib/guideSteps.jsx` is the single definition of that split;
`SETUP_STEPS` is its complement. Adding a step to `GUIDE_STEPS` puts it in the setup guide by
default, which is the right default — a new question has to argue its way into first run.

## Audience levels

Asked once, stored as `audience_level`, changeable any time from the same step.

| id | Prompt | What it changes |
| --- | --- | --- |
| `kid` | Show me simply | Short sentences, one idea at a time. |
| `beginner` | I'm new to this | Terms explained on first use; why before how. |
| `adult` | I've used creative apps | Assumes project/track/export; covers only what is specific here. |
| `pro` | I do this professionally | Names the model, the limit and the trade-off; skips reassurance. |

**The levels are about prior knowledge, never about age.** A twelve-year-old who has made videos
before is not a beginner, and a professional composer meeting a diffusion model for the first time
is. Nothing is withheld at any level — the same features are reachable from all four. The level only
decides which wording the app reaches for.

## The tour

`src/src/components/WelcomeTour.jsx`, scripted by `src/src/lib/welcomeStory.js`.

It navigates the **real pages** rather than showing screenshots, because the point is that the
interface stops being unfamiliar, and a picture of a page you have never opened does not do that.
On top of the real page it adds:

- **A blur veil** (`backdrop-blur-[3px]`) — you see the *shape* of each page without being asked to
  read it. Drawn as four panels around the spotlight rather than one panel with a hole, because
  `backdrop-filter` cannot be masked: a "hole" punched with a border-radius still blurs.
- **A spotlight** on the nav entry for the stop, via `[data-tour-nav="<route>"]` in `Shell.jsx`
  (present on the desktop sidebar, the mobile drawer and the mobile bottom bar). Whichever one
  actually has a size on this viewport wins. No match anywhere → plain full-screen veil, no spotlight.
- **Every control blocked** — a transparent full-viewport layer swallows clicks and taps. During the
  tour the app is a diagram, not a control panel, and a first-time user cannot fire a generation by
  accident mid-sentence.
- **Narration** through `lib/voice.js`, in the voice chosen one step earlier.
- **A pause on arrival**, scaled to the length of what is being said (`dwellMs`, clamped to 2–9s),
  before *Next* becomes available. Finishing the narration also ends the pause.

It generates nothing, saves nothing and connects nothing. The only thing it writes is
`welcome_tour_done`.

### The script

Eleven stops — dashboard, brief, sources, composer, music, images, characters, video, upload,
workflow, and a closing stop that points at the setup guide. Each is written **four times over**,
once per level. That is deliberate and not padding: "explain it simply" and "explain it densely" are
not the same text with different adjectives, they answer different questions. The kid version
answers *what happens*; the beginner version answers *why this step exists*; the adult version
assumes creative-app literacy; the professional version names the model, the limit and the
trade-off.

`tests/welcome-story.test.mjs` holds that promise, since prose has no compiler:

- every stop written at every level, with a title and a body;
- no body or title repeated across levels — copy-paste is the cheap way to pass the first check and
  it defeats the entire point of asking;
- simple wording uses **shorter sentences** than professional wording. Not word count: professional
  prose is denser and often *shorter*, so word count fails on writing that is perfectly fine. What
  makes a sentence easy to follow is how much must be held in mind before it ends;
- nothing long enough to overflow the card;
- every stop's route exists — parsed out of `App.jsx`, not duplicated, so a renamed route fails the
  test instead of navigating the tour to a blank page;
- the tour opens and closes on the dashboard;
- `storyFor` never returns an empty card, whatever level it is handed.

### Running it again

It runs automatically once, at the end of first run. Afterwards it lives in the graduation-cap menu
as **Welcome tour**, which is also where anyone who skipped it finds it, and where you go to hear it
again at a different level after changing `audience_level`.

## Why the voice defaults to the system voice

`VoicePicker` takes a `preferSystem` prop, set by the first-run voice step. When the platform ships
voices of its own — Android always does — and the user has not chosen yet, it preselects
`engine: "browser"`.

At first run there is no AI key, so defaulting to Gemini voices would mean every spoken line is a
failed request before falling back anyway. `voicePrefsChosen()` exists precisely so this can tell
"still on defaults" from "deliberately picked Gemini", and never overwrite the latter.

//! Whose voice this is written in.
//!
//! `craft.rs` decides what kind of song a song is — its form, how close it stays to the source, who
//! is speaking. It says nothing about *how the sentences go*, and that is most of what makes one
//! piece of writing feel unlike another. Two lyrics with identical craft settings, one written in
//! the cadence of the King James Bible and one in the flat register of a police report, are not
//! versions of one thing.
//!
//! ## Why the name and the technique both, and not either alone
//!
//! The obvious design is "write like <author>". The obvious correction is to drop the names and
//! give only measurable instructions. The evidence supports neither on its own:
//!
//!   * A bare name produces the model's *stereotype* of that writer. This is the caricature effect
//!     measured for persona prompting generally (CoMPosT, EMNLP 2023; Marked Personas, ACL 2023),
//!     and it is worse for writers with a strong popular image than for obscure ones — exactly
//!     backwards from what a picker wants to offer.
//!   * A bare style guide is *more controllable but less various*: explicit guides have been found
//!     to put a ceiling on the diversity of what comes back, where a name lets the model draw on
//!     what it actually read.
//!   * Instructions and exemplars together are **additive**, and explicit directives exert stronger
//!     foundational control than demonstrations alone (style control in multi-turn generation,
//!     arXiv:2511.13972).
//!
//! So every tradition here carries both: a `direction` written as technique a writer could act on,
//! and `exemplars` naming where it can be heard. The direction is what makes it controllable; the
//! names are what keep it from being a generic literary shimmer.
//!
//! ## Traditions, not impersonations
//!
//! The unit is a **tradition** — a body of technique with a place and a history — rather than a
//! person. That is not squeamishness, it is accuracy: "the cadence of the King James Bible" is a
//! describable set of moves, and "write like C. S. Lewis" is an impression. It also settles the
//! ethics without a special case, since a tradition cannot be impersonated and its techniques were
//! always public. Exemplars lean historical for the same reason: what they did is documented, and
//! nobody's living name is attached to a lyric they did not write.
//!
//! ## The 15 shipped languages, and where they are spoken
//!
//! The app ships translation catalogues for fifteen languages besides English, and until now the
//! only thing it knew about any of them was how to translate its own buttons. A German lyric was an
//! English lyric in German. Each language here carries the traditions actually practised where it is
//! spoken — Andalusian *cante jondo* and Latin American *testimonio* are both Spanish and are not
//! each other — so choosing a language is no longer only choosing a vocabulary.

use serde_json::{json, Value};

/// A surface feature of the prose itself, independent of any tradition.
///
/// These are the stylometric dimensions that are actually measurable in a text — sentence length
/// distribution, clause depth, figuration density, concreteness, formality — which is why they are
/// the ones a prompt can steer reliably. A tradition sets a default for each; a person can override
/// any of them without leaving the tradition.
pub struct Dial {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub instruction: &'static str,
}

pub const RHYTHM: &[Dial] = &[
    Dial { id: "short", label: "Short and level",
        hint: "Sentences of a similar, short length. Nothing subordinate.",
        instruction: "Sentence rhythm: short declarative sentences of roughly even length. Almost no \
                      subordinate clauses. Coordinate with 'and' rather than subordinating with \
                      'although' or 'because'. The effect comes from the sequence, not from any one \
                      sentence." },
    Dial { id: "varied", label: "Varied",
        hint: "Long and short against each other. The commonest good writing.",
        instruction: "Sentence rhythm: vary length deliberately. A long accumulating sentence \
                      followed by a short one, so the short one lands. Never three long sentences in \
                      a row and never four short ones." },
    Dial { id: "periodic", label: "Long and suspended",
        hint: "The sense withheld until the end of the sentence.",
        instruction: "Sentence rhythm: periodic. Build each important sentence so the main clause \
                      arrives last and the meaning is suspended until it does. Subordinate clauses \
                      accumulate before the verb, not after it." },
    Dial { id: "breath", label: "One breath a line",
        hint: "Each line the length of a spoken breath. For anything sung or read aloud.",
        instruction: "Sentence rhythm: each line is one breath — six to twelve syllables — and ends \
                      where a speaker would stop for air. Never break a phrase across two lines \
                      unless the break itself is the point." },
];

pub const FIGURATION: &[Dial] = &[
    Dial { id: "bare", label: "No figures",
        hint: "Say the thing. No metaphors at all.",
        instruction: "Figuration: none. No metaphor, no simile, no personification. Where an image \
                      is wanted, use a literal detail instead. This is a constraint, not a licence \
                      to be vague — the concrete detail has to do the work the metaphor would." },
    Dial { id: "sparing", label: "One image, held",
        hint: "A single figure, returned to. Stronger than many.",
        instruction: "Figuration: one governing image for the whole piece, introduced early and \
                      returned to. No second metaphor competing with it. A reader should be able to \
                      name the image afterwards." },
    Dial { id: "dense", label: "Image on image",
        hint: "Figures crowding each other. Baroque, and deliberate.",
        instruction: "Figuration: dense. Metaphors layered and allowed to collide. The pleasure is in \
                      the accumulation. Keep them from the same field so they compound rather than \
                      merely repeat." },
];

pub const CONCRETENESS: &[Dial] = &[
    Dial { id: "concrete", label: "Things you can point at",
        hint: "Nouns with weight. No abstractions.",
        instruction: "Concreteness: name things. Physical nouns, particular quantities, actual \
                      places. Replace every abstract noun ('hope', 'justice', 'the struggle') with \
                      something a camera could photograph." },
    Dial { id: "mixed", label: "Abstract, then shown",
        hint: "State the idea, then give the thing that proves it.",
        instruction: "Concreteness: pair each abstraction with a particular. Never leave an abstract \
                      claim standing on its own; the next line grounds it in something seen." },
    Dial { id: "abstract", label: "Ideas as the subject",
        hint: "For argument and meditation, where the thought is the event.",
        instruction: "Concreteness: the argument is the subject. Abstractions are allowed to be the \
                      grammatical subjects of sentences. Precision replaces imagery — a distinction \
                      exactly drawn does the work an image would." },
];

pub const FORMALITY: &[Dial] = &[
    Dial { id: "spoken", label: "As people talk",
        hint: "Contractions, fragments, the odd repetition.",
        instruction: "Register: spoken. Contractions, sentence fragments where speech would have \
                      them, the small repetitions of real talk. No word a person would not say out \
                      loud without thinking about it." },
    Dial { id: "neutral", label: "Plain written",
        hint: "Careful but unfussy. Reads as though somebody meant it.",
        instruction: "Register: plain written. Complete sentences, ordinary vocabulary, no slang and \
                      no elevation. The register of somebody explaining something they care about." },
    Dial { id: "heightened", label: "Raised",
        hint: "Formal, rhythmic, a little older than now.",
        instruction: "Register: heightened. Full forms rather than contractions, an older word where \
                      it is the exact one, rhythm that would carry in a large room. Never archaic for \
                      its own sake: no 'thee' unless the tradition asks for it." },
];

/// The four surface dials, in the order they are asked.
pub const DIALS: &[(&str, &[Dial], &str)] = &[
    ("rhythm", RHYTHM, "How the sentences go"),
    ("figuration", FIGURATION, "How much is said in images"),
    ("concreteness", CONCRETENESS, "Things, or ideas"),
    ("formality", FORMALITY, "How raised the voice is"),
];

pub fn dial_choice(dial: &str, id: &str) -> Option<&'static Dial> {
    DIALS.iter().find(|(d, _, _)| *d == dial)
        .and_then(|(_, choices, _)| choices.iter().find(|c| c.id == id))
}

/// What sort of thing a tradition is, which decides where it is offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind { Prose, Verse, Oratory, Story }

impl Kind {
    pub fn id(self) -> &'static str {
        match self { Kind::Prose => "prose", Kind::Verse => "verse",
                     Kind::Oratory => "oratory", Kind::Story => "story" }
    }
}

/// A body of technique with a place and a history.
pub struct Tradition {
    pub id: &'static str,
    pub label: &'static str,
    /// The catalogue code of the language it belongs to, or "" for one that crosses languages.
    pub lang: &'static str,
    /// Where it is practised, in words a person would recognise.
    pub region: &'static str,
    pub kind: Kind,
    /// What the picker says under the label.
    pub hint: &'static str,
    /// Where it can be heard. Both a recognition aid for the reader and real signal for the model —
    /// instructions and exemplars together control style better than either alone.
    pub exemplars: &'static [&'static str],
    /// The technique, written as instructions to a writer. This is the part that has to be specific,
    /// because it is the part that makes the difference between a voice and a shimmer.
    pub direction: &'static str,
    /// Where this tradition is easiest to get wrong, stated so the model is told not to.
    pub guard: &'static str,
}

/// The traditions, grouped by the language they are practised in.
///
/// Ordered by language so the picker can group them, and within a language by how likely somebody is
/// to want it. Every entry is a documented body of technique rather than one writer's habits, and
/// every `direction` says what to *do* — a tradition whose direction could be swapped for another's
/// without changing the output is not a tradition, it is an adjective.
pub const TRADITIONS: &[Tradition] = &[
    // ── crossing every language ─────────────────────────────────────────
    Tradition {
        id: "plain", label: "Plain speech", lang: "", region: "everywhere", kind: Kind::Prose,
        hint: "The shortest way to say a true thing. The default worth beating.",
        exemplars: &["Orwell's six rules", "the best committee minutes you ever read"],
        direction: "Never use a long word where a short one will do. Cut every word that can be cut. \
                    Prefer the active voice, the concrete noun, the everyday verb. Break any of these \
                    rules sooner than write something outright barbarous.",
        guard: "Plain is not flat: it still has to be worth reading. Shortness with nothing in it is \
                not this tradition, it is an absence of one.",
    },
    Tradition {
        id: "ciceronian", label: "The classical arc", lang: "", region: "the Western rhetorical tradition",
        kind: Kind::Oratory,
        hint: "Open, tell, prove, land. The shape most speeches still have.",
        exemplars: &["Cicero", "the funeral oration", "closing arguments"],
        direction: "Open by winning a hearing — one concession, one thing held in common. Then say \
                    plainly what happened, in order and without argument. Then give the reasons, \
                    weakest first, so the last one lands hardest. Close by raising the stakes and \
                    repeating the one phrase you want carried out of the room. The last sentence is \
                    short.",
        guard: "The parts must not be announced. If a listener can hear the outline, it has failed.",
    },
    Tradition {
        id: "call_response", label: "Call and answer", lang: "", region: "African and diasporic oratory and song",
        kind: Kind::Oratory,
        hint: "One voice leads, the room answers. Built to be joined in with.",
        exemplars: &["the ring shout", "gospel preaching", "work songs"],
        direction: "Alternate a leading line with a shorter answering one that stays the same or \
                    changes by one word. Build by repetition with small increments — the same phrase \
                    returning higher each time — and let the last return be the shortest. The answer \
                    must be singable by people who have not heard it before.",
        guard: "The repetition is the argument, not decoration. If the answering line could be cut \
                without loss, the call was doing all the work and this is not the form.",
    },

    // ── English ─────────────────────────────────────────────────────────
    Tradition {
        id: "kjv", label: "King James cadence", lang: "en", region: "English-speaking world",
        kind: Kind::Prose,
        hint: "'And it came to pass.' Parallel clauses joined by 'and', built to be read aloud.",
        exemplars: &["Tyndale", "the Authorised Version", "Lincoln at Gettysburg"],
        direction: "Join clauses with 'and' rather than subordinating them. Use parallelism: the \
                    second clause echoes the shape of the first and advances it by one step. Prefer \
                    the Anglo-Saxon word to the Latinate one. Put the verb early and the weight at the \
                    end of the sentence. Rhythm before elegance — this is written for the ear.",
        guard: "No 'thee', 'thou', 'verily' or '-eth'. The cadence is the inheritance; the pronouns \
                are costume, and they make it read as parody.",
    },
    Tradition {
        id: "hardboiled", label: "American plain", lang: "en", region: "United States",
        kind: Kind::Prose,
        hint: "Declarative, unadorned, and the feeling left underneath.",
        exemplars: &["Hemingway's early stories", "Carver", "the Chandler first person"],
        direction: "Short declarative sentences. Concrete nouns and physical verbs. No adverbs of \
                    manner. State what was done and what was seen; never name the emotion. Dialogue \
                    withholds — people talk around the thing. The omission is the technique: leave out \
                    what the reader can supply and the sentence gets stronger.",
        guard: "Terseness is not the point, restraint is. A flat sentence about nothing is not this \
                tradition. Something has to be being held back.",
    },
    Tradition {
        id: "sermon_anaphora", label: "The preached line", lang: "en",
        region: "African American church tradition, United States", kind: Kind::Oratory,
        hint: "The same opening words, again and again, each time further.",
        exemplars: &["the Black homiletic tradition", "King's cadence", "Baldwin's essays"],
        direction: "Begin successive sentences with the same words and let each go further than the \
                    last. Build in threes. Move from the particular grievance to the general promise \
                    and back down to one image. Take the last line down to a whisper's length — after \
                    a long climb the short sentence is the arrival.",
        guard: "Anaphora without escalation is a list. Each repetition must earn its place by taking \
                the thought somewhere the previous one could not.",
    },
    Tradition {
        id: "ballad", label: "The ballad", lang: "en", region: "Britain, Ireland, Appalachia",
        kind: Kind::Verse,
        hint: "A story in quatrains, told from outside, no explanation.",
        exemplars: &["the Child ballads", "'Barbara Allen'", "Appalachian murder ballads"],
        direction: "Four-line stanzas, lines alternating four and three stresses, second and fourth \
                    lines rhyming. Tell the story from outside: no interiority, no motive given, no \
                    moral drawn. Jump over whole years between stanzas. Use incremental repetition — \
                    a stanza repeated with one detail changed — where the horror or the turn is.",
        guard: "Never explain why anybody did anything. The refusal to explain is what makes a ballad \
                frightening; supplying a motive turns it into a report.",
    },

    // ── German ──────────────────────────────────────────────────────────
    Tradition {
        id: "weimar", label: "Weimar classicism", lang: "de", region: "Germany",
        kind: Kind::Verse,
        hint: "Long, balanced sentences that resolve. Formal and warm at once.",
        exemplars: &["Goethe", "Schiller", "Hölderlin's hymns"],
        direction: "Build long periodic sentences that hold their sense to the end and then resolve \
                    cleanly. Balance abstract nouns against a natural image in the same sentence. Keep \
                    the tone elevated but never cold — the ideal is warmth arrived at through form. \
                    Where the thought turns, mark it with a single short sentence.",
        guard: "Elevation is not fog. Every abstract noun must be one the sentence has earned, and the \
                natural image must be a specific plant, hour or weather rather than 'nature'.",
    },
    Tradition {
        id: "kafkaesque", label: "The flat uncanny", lang: "de", region: "Prague, Central Europe",
        kind: Kind::Prose,
        hint: "Something impossible, reported in the register of an official letter.",
        exemplars: &["Kafka", "Central European bureaucratic prose"],
        direction: "Report the impossible in the register of an official document: precise, \
                    subordinated, unhurried. Nobody in the text remarks that anything is strange. \
                    Attend to procedure and to the exact positions of doors and people. The horror is \
                    entirely in the calm.",
        guard: "Never signal the strangeness — no 'somehow', no 'inexplicably', no dream language. \
                One nudge from the narrator and the effect is gone.",
    },
    Tradition {
        id: "verfremdung", label: "The interrupted scene", lang: "de", region: "Germany",
        kind: Kind::Story,
        hint: "The story stops to address you directly, so you keep thinking.",
        exemplars: &["Brecht", "the Lehrstück", "the songs in Threepenny"],
        direction: "Break the spell on purpose. Announce what is about to happen before it happens, so \
                    the interest is in how rather than what. Let a song or a direct address interrupt \
                    the action and comment on it. Keep the language plain and public — this is written \
                    to be argued with, not fallen into.",
        guard: "Interruption is not sarcasm. The commenting voice takes the material seriously; it is \
                the identification it refuses, not the subject.",
    },
    Tradition {
        id: "dinggedicht", label: "The thing looked at", lang: "de", region: "Germany, Austria",
        kind: Kind::Verse,
        hint: "One object, watched until it gives something up.",
        exemplars: &["Rilke's New Poems", "the Ding-poem"],
        direction: "Take one object and look at it for the whole piece. Describe it with exactness, \
                    from outside, until the description turns and the object seems to look back. The \
                    turn must come from the seeing, never from a statement about meaning. End on the \
                    object, not on the speaker.",
        guard: "The speaker's feelings are not the subject and must not arrive. If the poem could \
                keep going after the turn, it stopped in the wrong place.",
    },

    // ── Spanish ─────────────────────────────────────────────────────────
    Tradition {
        id: "cervantine", label: "The wandering narrator", lang: "es", region: "Spain",
        kind: Kind::Prose,
        hint: "Digressive, ironic, full of proverbs. Warm about its own characters.",
        exemplars: &["Cervantes", "the picaresque", "Lazarillo"],
        direction: "Let the narrator wander, comment, and apologise for wandering. Season the speech \
                    with proverbs, and let a plain character puncture a lofty one with a plainer one. \
                    Be ironic about the characters and fond of them at the same time — the irony must \
                    never curdle into contempt.",
        guard: "Digression has to be entertaining in itself. A detour that is merely a delay is a \
                fault; in this tradition it is the pleasure.",
    },
    Tradition {
        id: "realismo_magico", label: "The marvellous, reported plainly", lang: "es",
        region: "Latin America", kind: Kind::Prose,
        hint: "Wonders stated as fact, in the voice of somebody who was there.",
        exemplars: &["Rulfo", "Borges", "the Caribbean chronicle"],
        direction: "State the marvellous in the same register as the weather. No character is \
                    surprised by it; what surprises them is something ordinary. Anchor every wonder in \
                    exact domestic detail — the price of the thing, the name of the street — so the \
                    impossible arrives already furnished. Time may fold; say so plainly.",
        guard: "This is not whimsy and not decoration. The marvellous element must matter to somebody \
                in the story, or it is a flourish rather than a world.",
    },
    Tradition {
        id: "cante_jondo", label: "Deep song", lang: "es", region: "Andalusia, Spain",
        kind: Kind::Verse,
        hint: "Short, elemental, wounded. Three lines can be enough.",
        exemplars: &["the copla", "Andalusian cante jondo", "Lorca's Poema del cante jondo"],
        direction: "Three or four short lines. Elemental nouns only — knife, moon, water, horse, \
                    blood, road. Repeat one line with a single word changed. State grief as a fact \
                    about the world rather than as a feeling in a speaker. No explanation, no \
                    consolation, and no ending that resolves.",
        guard: "Elemental is not generic: the moon in this tradition does something, it is not \
                scenery. If the nouns could be reordered without loss, they are decoration.",
    },
    Tradition {
        id: "testimonio", label: "The witness speaks", lang: "es", region: "Latin America",
        kind: Kind::Story,
        hint: "One person telling what happened to them, plainly, to be believed.",
        exemplars: &["the testimonio tradition", "oral history transcripts"],
        direction: "First person, past tense, plain vocabulary. Give dates, names of places, numbers \
                    of people. Keep the speaker's own syntax including its repetitions and its \
                    self-corrections. Address an implied listener who is being asked to believe this. \
                    Do not shape the events into a plot — say them in the order they are remembered.",
        guard: "Never make the speaker eloquent on their behalf. A polished sentence in this form \
                reads as somebody else's voice and destroys the only thing it has.",
    },

    // ── French ──────────────────────────────────────────────────────────
    Tradition {
        id: "clarte", label: "French clarity", lang: "fr", region: "France",
        kind: Kind::Prose,
        hint: "Balanced, exact, epigrammatic. The sentence as a small machine.",
        exemplars: &["La Rochefoucauld's maxims", "La Bruyère", "the classical moralists"],
        direction: "Write in balanced antitheses: the second half of the sentence answers the first \
                    and reverses it. Aim for the maxim — a general truth in one sentence, closed and \
                    quotable. Exactness of distinction is the beauty; where two words are near, choose \
                    the one that is right and let the difference be the point.",
        guard: "A maxim that is merely clever is a failure. It has to be true enough to be uncomfortable.",
    },
    Tradition {
        id: "symboliste", label: "Correspondences", lang: "fr", region: "France, Belgium",
        kind: Kind::Verse,
        hint: "Senses crossing into each other. Suggestion rather than statement.",
        exemplars: &["Baudelaire", "Verlaine", "Rimbaud"],
        direction: "Suggest rather than name. Let the senses cross — colours that sound, scents that \
                    have a temperature. Prefer the word chosen for its music over the word chosen for \
                    its accuracy, where the two differ. Keep the subject slightly out of reach: the \
                    poem is the atmosphere around the thing, not the thing.",
        guard: "Vagueness is not suggestion. Every image must be sharply particular even when what it \
                points at is not.",
    },
    Tradition {
        id: "chanson", label: "The chanson", lang: "fr", region: "France, Belgium, Quebec",
        kind: Kind::Story,
        hint: "A story sung, with the whole meaning turned over in the last verse.",
        exemplars: &["Brel", "Brassens", "Barbara"],
        direction: "Tell one small story across the verses, in a conversational register with a \
                    literate vocabulary. Keep the refrain simple and let its meaning change as the \
                    verses go on. Put the reversal in the final verse: the same words as before, now \
                    meaning something else. Wit and grief in the same line is the mode.",
        guard: "The turn must be prepared, not sprung. Everything needed to feel it should already \
                have been said, just not in that order.",
    },

    // ── Italian ─────────────────────────────────────────────────────────
    Tradition {
        id: "dantesque", label: "The vernacular sublime", lang: "it", region: "Italy",
        kind: Kind::Verse,
        hint: "The highest subject in the ordinary language, and a guide beside you.",
        exemplars: &["Dante", "the Commedia's terza rima"],
        direction: "Treat the greatest subject in the plainest living language. Move by concrete \
                    particulars — a named person, a specific torment, a body in a place — rather than \
                    by abstraction. Keep a companion in the text who explains and is questioned. Let \
                    each stanza's last line pull into the next so the whole leans forward.",
        guard: "The sublime comes from the particular. A general damnation is not frightening; a \
                named neighbour in it is.",
    },
    Tradition {
        id: "commedia", label: "Masks and asides", lang: "it", region: "Italy",
        kind: Kind::Story,
        hint: "Fixed types, broad play, and the audience spoken to directly.",
        exemplars: &["commedia dell'arte", "the Goldoni scenario"],
        direction: "Work in fixed types — the miser, the boaster, the clever servant, the lovers — and \
                    let the comedy come from what a type cannot help doing. Speak to the audience \
                    directly between beats. Physical business over psychology. Keep it fast; a scene \
                    ends the moment its joke has landed.",
        guard: "A type is not a flat character: it wants something specific and pursues it \
                relentlessly. Without the wanting there is nothing to laugh at.",
    },
    Tradition {
        id: "neorealismo", label: "Neorealism", lang: "it", region: "Italy",
        kind: Kind::Prose,
        hint: "Ordinary people, plain speech, no consolation offered.",
        exemplars: &["Verga's Sicilian stories", "post-war neorealism", "Levi's plain witness"],
        direction: "Ordinary people in real economic circumstances. Dialogue in the register they \
                    would actually use, including dialect turns. Small events at true scale: the loss \
                    of a day's work is the size it is to the person losing it. Refuse the consoling \
                    ending — stop where the situation stops.",
        guard: "Do not aestheticise poverty. The detail is there because it is true, not because it \
                is picturesque.",
    },

    // ── Portuguese ──────────────────────────────────────────────────────
    Tradition {
        id: "saudade", label: "Saudade", lang: "pt", region: "Portugal, Cape Verde, Brazil",
        kind: Kind::Verse,
        hint: "Longing for what is gone, or never was. Held rather than resolved.",
        exemplars: &["the fado lyric", "the morna", "Pessoa's heteronyms"],
        direction: "Write from a longing that is not going to be satisfied and is not asking to be. \
                    Keep it in the present tense: the absence is a current condition, not a memory. \
                    Anchor it in a place — a street, a harbour, a window. The tone is neither bitter \
                    nor sweet; it holds both without choosing.",
        guard: "This is not sadness looking for a cure. Any line that reaches for comfort or resolution \
                leaves the tradition.",
    },
    Tradition {
        id: "cordel", label: "Cordel", lang: "pt", region: "north-east Brazil",
        kind: Kind::Story,
        hint: "Rhymed sung chapbook verse. News, legend and satire in the same breath.",
        exemplars: &["literatura de cordel", "the repentista's duel"],
        direction: "Six-line stanzas rhyming ABCBDB, in a spoken popular register. Tell a story that \
                    is also news: a flood, a bandit, a marriage, a scandal. Address the listener \
                    directly and ask for their attention at the start. Exaggerate freely — the humour \
                    and the moral sit together without embarrassment.",
        guard: "Keep the metre strict. This form is sung and sold by the sheet; a stanza that does not \
                scan cannot be performed and is simply wrong.",
    },
    Tradition {
        id: "antropofagia", label: "Devouring modernism", lang: "pt", region: "Brazil",
        kind: Kind::Prose,
        hint: "Foreign forms eaten and made local. Playful, syncretic, unembarrassed.",
        exemplars: &["the Modern Art Week", "Oswald de Andrade's manifesto"],
        direction: "Take a foreign form and consume it — keep what is useful, discard the reverence. \
                    Mix registers on purpose: an indigenous word beside a technical one beside \
                    advertising language. Keep it short, aphoristic and funny. Cultural mixture is the \
                    subject as well as the method.",
        guard: "Playfulness is not carelessness. Each borrowing must be visibly changed by being \
                taken, or it is imitation rather than appetite.",
    },

    // ── Dutch ───────────────────────────────────────────────────────────
    Tradition {
        id: "nuchter", label: "Dutch plainness", lang: "nl", region: "Netherlands, Flanders",
        kind: Kind::Prose,
        hint: "Understated to the point of dryness. Grandeur actively refused.",
        exemplars: &["Nescio", "the Dutch domestic novel", "Multatuli's irony"],
        direction: "Understate everything. Report large feelings in small words and let the gap do the \
                    work. Domestic detail carries the weight — a kitchen, a bicycle, the weather. \
                    Puncture any sentence that begins to soar, preferably with something a neighbour \
                    said.",
        guard: "Dryness is not indifference. The restraint has to be visibly costing the speaker \
                something, or there is nothing underneath it.",
    },
    Tradition {
        id: "emblem", label: "Picture and motto", lang: "nl", region: "the Low Countries",
        kind: Kind::Verse,
        hint: "An image, a short motto, and the lesson drawn beneath it.",
        exemplars: &["the Dutch emblem book", "Cats's household verse"],
        direction: "Give one clear picture in a few lines, then a short motto, then a plain \
                    application of it to how a person should live. The picture must be ordinary and \
                    exactly observed. Keep the lesson modest — a household truth rather than a \
                    revelation.",
        guard: "The lesson must arise from the picture rather than being attached to it. If any image \
                would fit the moral, the wrong image was chosen.",
    },
    Tradition {
        id: "flemish_lyric", label: "Flemish nature lyric", lang: "nl", region: "Flanders",
        kind: Kind::Verse,
        hint: "West Flemish words, close-watched weather, and delight taken openly.",
        exemplars: &["Gezelle", "the Flemish parish poets"],
        direction: "Watch one small living thing very closely — a bird, a reed, a beech in a \
                    particular month. Use regional words where they are the exact ones and let their \
                    sound be part of the pleasure. Sound patterning is allowed to be conspicuous: \
                    alliteration, internal rhyme, a line that repeats itself with one vowel changed. \
                    Delight is expressed rather than implied.",
        guard: "The observation must be precise enough to be checked. Rapture over a generic bird is \
                sentimentality, which is what this tradition is always accused of and rarely guilty of.",
    },
    Tradition {
        id: "levenslied", label: "The life-song", lang: "nl", region: "Netherlands, Flanders",
        kind: Kind::Story,
        hint: "Ordinary lives sung plainly — the harbour, the mother, the job, the leaving.",
        exemplars: &["the Dutch levenslied", "the Jordaan song tradition"],
        direction: "One person's life told in verses, in the plainest words, with a chorus anybody in \
                    the room can join by the second time. Name the street, the work and the family. \
                    Sentiment is met head-on rather than avoided, and undercut once — a single wry \
                    line keeps the whole from tipping over.",
        guard: "It is sung by and for the people it is about. A line that looks down on them, however \
                affectionately, is not in this tradition.",
    },

    // ── Polish ──────────────────────────────────────────────────────────
    Tradition {
        id: "gaweda", label: "The rambling tale", lang: "pl", region: "Poland, Lithuania",
        kind: Kind::Story,
        hint: "A talker at a table, circling, digressing, arriving late and well.",
        exemplars: &["the szlachta gawęda", "Mickiewicz's Pan Tadeusz", "Sienkiewicz's narrators"],
        direction: "Write as one person talking to a small company who already know each other. Start \
                    somewhere beside the point. Interrupt yourself to settle a small dispute about a \
                    detail. Address the listeners and take their objections. Arrive at the point late, \
                    and let its force come from how long it took.",
        guard: "The digressions must be answering something, even if only the speaker's own memory. \
                Aimless wandering is not this form, which is intensely sociable.",
    },
    Tradition {
        id: "polish_irony", label: "The examined ordinary", lang: "pl", region: "Poland",
        kind: Kind::Verse,
        hint: "A small object turned over until history falls out of it.",
        exemplars: &["Szymborska", "Herbert's Mr Cogito", "Miłosz's plain late poems"],
        direction: "Begin with something small and unremarkable — a stone, a photograph, an onion. \
                    Examine it with dry precision and slight comedy. Let the historical or moral weight \
                    arrive obliquely, through what the object turns out to imply, and never state it. \
                    End before the reader expects, on a plain observation.",
        guard: "No grandeur and no self-pity, however large the history being touched. The wit is what \
                makes the weight bearable, so it must not be dropped at the end.",
    },
    Tradition {
        id: "messianic", label: "The suffering nation", lang: "pl", region: "Poland",
        kind: Kind::Oratory,
        hint: "Romantic, exalted, addressed to a people rather than a person.",
        exemplars: &["Mickiewicz's Books of the Polish Pilgrimage", "Słowacki"],
        direction: "Address a whole people in the second person. Use biblical parallelism and the \
                    vocative. Treat collective suffering as meaningful rather than merely endured, and \
                    build toward a promise that is not yet visible. The register is exalted throughout \
                    and never ironic.",
        guard: "It speaks to a people, never against another one. A line whose force comes from an \
                enemy has left the tradition for propaganda.",
    },

    // ── Russian ─────────────────────────────────────────────────────────
    Tradition {
        id: "skaz", label: "Skaz", lang: "ru", region: "Russia",
        kind: Kind::Story,
        hint: "A written text pretending to be a person talking, dialect and all.",
        exemplars: &["Leskov", "Gogol's narrators", "Zoshchenko"],
        direction: "Write in the voice of a particular speaker who is not the author: their dialect, \
                    their slang, their filler words, their wrong emphases. The speech is not in \
                    quotation marks — it is the narration. Let the speaker misunderstand the story \
                    they are telling, so the reader sees past them. Keep the warmth; this is not \
                    mockery.",
        guard: "The narrator must be consistent. One sentence in the author's own educated register \
                breaks the whole illusion, which is the only device the form has.",
    },
    Tradition {
        id: "moral_novel", label: "The argument through people", lang: "ru", region: "Russia",
        kind: Kind::Prose,
        hint: "Characters who argue about how to live, and are not settled by the author.",
        exemplars: &["Tolstoy", "Dostoevsky's dialogues", "Chekhov's refusals"],
        direction: "Put a moral question into two people who each hold it honestly, and give the one \
                    you disagree with the better speech. Let physical detail carry interior states — a \
                    hand, a lamp, the price of a coat. Refuse to resolve: end on the question standing, \
                    changed by having been asked.",
        guard: "Neither side may be a straw man. If a reader can tell which one the author prefers \
                from the writing rather than from the events, it has failed.",
    },
    Tradition {
        id: "chastushka", label: "Chastushka", lang: "ru", region: "Russia",
        kind: Kind::Verse,
        hint: "Four rhyming lines, quick, rude and topical.",
        exemplars: &["the village chastushka", "the sung couplet"],
        direction: "Four short lines, strongly rhymed, one joke or grievance. Concrete and local: a \
                    named village, a named trade, somebody everyone knows. The last line turns on the \
                    first three. Rude is allowed and often the point.",
        guard: "It must be sayable in one breath and funny to somebody who was not there. A chastushka \
                that needs explaining is not one.",
    },

    // ── Arabic ──────────────────────────────────────────────────────────
    Tradition {
        id: "saj", label: "Saj' — rhymed prose", lang: "ar", region: "the Arabic-speaking world",
        kind: Kind::Oratory,
        hint: "Prose that rhymes and balances. The oldest Arabic artistic speech.",
        exemplars: &["pre-Islamic oratory", "the maqāmāt", "classical legal pronouncement"],
        direction: "Write prose in short balanced units that end on the same rhyme, several in a row \
                    before the rhyme changes. Pair the units by grammatical shape as well as by sound, \
                    so each answers the one before. Keep the clauses close in length. The dignity of \
                    the form is the point: this is speech for a moment that matters.",
        guard: "The rhyme must not force the sense. A unit padded to reach its rhyme is the failure \
                this form is judged on.",
    },
    Tradition {
        id: "qasida", label: "The classical ode", lang: "ar", region: "Arabia, North Africa, the Levant",
        kind: Kind::Verse,
        hint: "Begin at the abandoned camp, travel, then arrive at the praise.",
        exemplars: &["the Mu'allaqat", "al-Mutanabbi"],
        direction: "Open at a deserted place where somebody used to be, and grieve there briefly. Then \
                    travel — the journey, the mount, the hard country. Then arrive at the subject: \
                    praise, or a claim, or a complaint. Keep one rhyme throughout. Each line is a \
                    complete thought that could stand alone.",
        guard: "The three movements must be proportionate; the opening is short. A whole poem spent at \
                the ruins is a mood, not a qasida.",
    },
    Tradition {
        id: "arabic_free", label: "Modern free verse", lang: "ar", region: "the Arab world",
        kind: Kind::Verse,
        hint: "Classical weight in an open shape. Political and intimate at once.",
        exemplars: &["Nazik al-Mala'ika", "the shi'r hurr movement", "Darwish's cadence"],
        direction: "Keep the classical vocabulary and imagery but break the line where the sense \
                    breaks rather than where the metre would. Let a public grief and a private one be \
                    the same sentence. Repeat a single phrase as a structural spine. Address a place \
                    as though it were a person.",
        guard: "Free does not mean unmeasured: the line still has a foot underneath it. Prose broken \
                into lines is not this tradition.",
    },

    // ── Hebrew ──────────────────────────────────────────────────────────
    Tradition {
        id: "parallelism", label: "Biblical parallelism", lang: "he", region: "the Hebrew Bible and after",
        kind: Kind::Verse,
        hint: "The second line answers the first — repeating it, sharpening it, or reversing it.",
        exemplars: &["the Psalms", "Proverbs", "the prophetic oracle"],
        direction: "Work in pairs of lines. The second line restates the first in different words, or \
                    sharpens it by one degree, or turns against it. Concrete nouns from a shepherd's \
                    and a farmer's world. No enjambment: each line is complete. Where intensity is \
                    wanted, add a third line rather than a longer one.",
        guard: "The second line must do something the first did not. Pure repetition is not \
                parallelism, it is a stammer.",
    },
    Tradition {
        id: "piyyut", label: "Piyyut", lang: "he", region: "Jewish liturgical tradition",
        kind: Kind::Verse,
        hint: "Liturgical poem: acrostic, refrain, made for a congregation to carry.",
        exemplars: &["the classical piyyutim", "Ibn Gabirol", "the Sephardi and Mizrahi repertoires"],
        direction: "Build on a fixed pattern — an alphabetical acrostic, or a repeated closing word in \
                    every stanza. Weave in phrases from scripture so a listener recognises them mid-line. \
                    Address the divine in the second person and the congregation in the first person \
                    plural. Keep the refrain short enough to be joined.",
        guard: "The pattern is a discipline, not a puzzle. If the acrostic is the only reason a line is \
                there, the line is not there.",
    },
    Tradition {
        id: "modern_hebrew", label: "The sacred in the kitchen", lang: "he", region: "Israel",
        kind: Kind::Verse,
        hint: "Liturgical language brought down to a domestic, present-tense life.",
        exemplars: &["Amichai", "Zelda", "Ravikovitch"],
        direction: "Put a phrase from prayer or scripture directly beside something ordinary and \
                    present — a bus, a doctor's waiting room, a shopping bag — and do not explain the \
                    juxtaposition. First person, present tense, plain syntax. Let the ancient language \
                    be tender rather than grand.",
        guard: "The old phrase must be used, not quoted. If it sits in the line like an epigraph, the \
                collision has not happened.",
    },

    // ── Hindi ───────────────────────────────────────────────────────────
    Tradition {
        id: "katha", label: "Katha", lang: "hi", region: "North India",
        kind: Kind::Story,
        hint: "Recite the story, then stop and explain what it means to us here.",
        exemplars: &["the kathavachak's telling", "Ram Katha", "Bhagavata recitation"],
        direction: "Alternate two registers: the narrated episode in a raised traditional voice, then \
                    a plain commentary addressed to the people listening today, applying it to their \
                    lives. Pause on one line and turn it over. Assume the audience knows the story — \
                    the interest is in the telling and the gloss, not the outcome.",
        guard: "The commentary is warm and specific, not a sermon in general terms. It should name the \
                sort of trouble the listeners actually have.",
    },
    Tradition {
        id: "bhakti", label: "Bhakti devotional", lang: "hi", region: "North and Central India",
        kind: Kind::Verse,
        hint: "The divine addressed directly, in the language of the street, often with complaint.",
        exemplars: &["Kabir's dohas", "Mirabai", "Surdas"],
        direction: "Address the divine in the second person, familiarly, as one would a person one has \
                    a claim on. Use the vernacular and household images — the grindstone, the well, the \
                    thread, the beloved's absence. Complaint, teasing and longing are all permitted. \
                    Close on a couplet compact enough to be remembered whole.",
        guard: "Familiarity is not irreverence, and the images are worked rather than decorative. \
                Kabir's loom is a loom.",
    },
    Tradition {
        id: "ghazal", label: "The ghazal", lang: "hi", region: "North India and Pakistan",
        kind: Kind::Verse,
        hint: "Independent couplets, one refrain, the poet named at the end.",
        exemplars: &["Ghalib", "Mir", "the sung Urdu ghazal"],
        direction: "Write independent couplets, each complete in itself, sharing only a rhyme and a \
                    repeated end-word. No narrative connects them; the unity is of mood and of that \
                    returning word. Longing, wine, the beloved and the divine are deliberately not \
                    distinguished. In the last couplet, name the speaker.",
        guard: "Do not join the couplets into an argument. A ghazal that develops a thesis has become \
                a different poem.",
    },

    // ── Indonesian ──────────────────────────────────────────────────────
    Tradition {
        id: "pantun", label: "Pantun", lang: "id", region: "Indonesia, Malaysia, Brunei",
        kind: Kind::Verse,
        hint: "Four lines: two of image, two of meaning, joined by sound not sense.",
        exemplars: &["the Malay pantun", "sung pantun exchanges"],
        direction: "Four lines rhyming ABAB. The first two are a picture from the natural world — a \
                    bird, a boat, a fruit tree — with no stated connection to what follows. The last \
                    two say the human thing. The link between the halves is sound and suggestion, \
                    never explanation.",
        guard: "Never bridge the halves with 'like' or 'so'. The unstated leap is the entire form.",
    },
    Tradition {
        id: "wayang", label: "The puppeteer's voice", lang: "id", region: "Java, Bali",
        kind: Kind::Story,
        hint: "One narrator for every character, with clowns who comment on the story.",
        exemplars: &["the dalang", "the punakawan interludes", "wayang kulit"],
        direction: "One narrating voice performs every character and shifts register between them. \
                    Take the time to describe a scene at length before anything happens in it. Let \
                    comic servant figures interrupt to comment on the noble characters in the language \
                    of today, including on current events. Refinement and coarseness alternate on \
                    purpose.",
        guard: "The comic interruption is not a break from the story, it is where its meaning is \
                argued. Treat it as the most serious part.",
    },
    Tradition {
        id: "hikayat", label: "Hikayat", lang: "id", region: "the Malay world",
        kind: Kind::Prose,
        hint: "Courtly romance-chronicle. Formal, marvellous, told for a king's household.",
        exemplars: &["Hikayat Hang Tuah", "the Sejarah Melayu"],
        direction: "Formal, courteous narration with honorifics and set phrases repeated at each new \
                    episode. Genealogy and lineage matter and are stated. The marvellous is accepted \
                    without comment. Time moves in reigns rather than years. The narrator is a servant \
                    of the court, not a modern observer.",
        guard: "The formulas are the texture and must recur. Stripping them for concision leaves a \
                plot summary rather than a hikayat.",
    },

    // ── Japanese ────────────────────────────────────────────────────────
    Tradition {
        id: "ma", label: "Ma — the charged interval", lang: "ja", region: "Japan",
        kind: Kind::Verse,
        hint: "The pause is the material. What is left out does the work.",
        exemplars: &["the kabuki pause", "Nō timing", "the space in a scroll"],
        direction: "Build around what is not said. Place a silence — a line break, a blank, a held \
                    beat — where the emotion would be, and let the words on either side lean into it. \
                    Fewer elements than feel sufficient. One image, one interval, one return; the \
                    interval is not empty and must be felt as full.",
        guard: "Absence must be shaped, not merely short. If the gap could be closed without loss, it \
                was a cut rather than a ma.",
    },
    Tradition {
        id: "haiku", label: "Season and cut", lang: "ja", region: "Japan",
        kind: Kind::Verse,
        hint: "Two images and the break between them. A word that fixes the season.",
        exemplars: &["Bashō", "Buson", "Issa"],
        direction: "Set two concrete images beside each other and cut sharply between them; the poem \
                    happens in the gap. Include something that fixes the season precisely — a specific \
                    plant, insect, weather or festival. Present tense. No metaphor, no simile, no \
                    conclusion, and no word about how the speaker feels.",
        guard: "Never state the meaning. The moment a haiku explains its own juxtaposition it becomes \
                an aphorism with a line break.",
    },
    Tradition {
        id: "rakugo", label: "Rakugo", lang: "ja", region: "Japan",
        kind: Kind::Story,
        hint: "A seated storyteller playing everyone, building to one punchline.",
        exemplars: &["the classical rakugo repertoire", "the ochi"],
        direction: "One voice performs a whole cast, distinguished by where they look and how they \
                    speak rather than by being named. Ordinary townspeople and small domestic \
                    troubles. Build a chain of misunderstandings, each following logically from the \
                    last, to a single final line that reverses the whole story in a few words.",
        guard: "The ending must be one line and must be prepared from the beginning. A story that \
                merely stops has no ochi and is not rakugo.",
    },

    // ── Korean ──────────────────────────────────────────────────────────
    Tradition {
        id: "pansori", label: "Pansori", lang: "ko", region: "Korea",
        kind: Kind::Story,
        hint: "Sung passages and spoken narration alternating, with the audience calling back.",
        exemplars: &["the five surviving pansori", "the singer and the drummer"],
        direction: "Alternate two modes: spoken narration that moves the story briskly, and sung \
                    passages that stop and dwell on one emotion at length. Shift register hard between \
                    them. Leave room for the listener's shouts of encouragement — the telling assumes \
                    a room that answers. Comedy and grief sit in the same episode.",
        guard: "The sung parts must dwell. Summarising them back into narration collapses the form to \
                a plot.",
    },
    Tradition {
        id: "sijo", label: "Sijo", lang: "ko", region: "Korea",
        kind: Kind::Verse,
        hint: "Three lines: set it up, develop it, then break it open.",
        exemplars: &["the classical sijo", "Hwang Jini"],
        direction: "Three long lines. The first states a situation, the second develops it, and the \
                    third begins with a turn — a surprise, a question, a reversal — and then closes. \
                    Nature imagery carrying a human feeling. The whole is short enough to be sung in \
                    one sitting and to be remembered.",
        guard: "The third line's turn is compulsory. Three lines of steady development is a stanza, \
                not a sijo.",
    },
    Tradition {
        id: "han", label: "Held sorrow", lang: "ko", region: "Korea",
        kind: Kind::Verse,
        hint: "Long-accumulated grief, carried rather than cured — and stubborn with it.",
        exemplars: &["the han of the folk song tradition", "Arirang"],
        direction: "Write from a sorrow that has been carried a long time and has become part of how \
                    the speaker stands. It is not resolved and not sought to be. Endurance and \
                    resentment together, with a thread of stubborn humour. Ordinary work and a named \
                    road or hill anchor it.",
        guard: "Not despair and not nobility. The speaker is still going, still complaining, and would \
                be unrecognisable if either the pain or the persistence were removed.",
    },

    // ── Chinese ─────────────────────────────────────────────────────────
    Tradition {
        id: "pianwen", label: "Parallel prose", lang: "zh", region: "China",
        kind: Kind::Prose,
        hint: "Clauses in matched pairs, balanced by length and by sense.",
        exemplars: &["classical pianwen", "the formal preface"],
        direction: "Write in matched pairs of clauses of equal length, where the parts correspond \
                    position by position — noun against noun, verb against verb, and the senses \
                    balanced or opposed. Allusion to older texts is expected. Keep the sound balanced \
                    as well as the sense.",
        guard: "The parallelism must be exact enough to be felt and varied enough not to tick. Two \
                dozen identical pairs is a metronome.",
    },
    Tradition {
        id: "tang_verse", label: "Regulated verse", lang: "zh", region: "China",
        kind: Kind::Verse,
        hint: "Eight lines, the middle two couplets parallel, the turn near the end.",
        exemplars: &["Du Fu", "Wang Wei", "Li Bai"],
        direction: "Eight lines in four couplets. Open by setting a scene, make the middle two couplets \
                    strictly parallel, and let the last couplet turn from the scene to what it means to \
                    the speaker. Landscape carries feeling: no emotion is named, it is placed in a \
                    mountain, a river, a lamp, a season.",
        guard: "The feeling is never stated. In this tradition 'I was sad' is not a line, it is the \
                admission that no image was found.",
    },
    Tradition {
        id: "pingshu", label: "The storyteller's serial", lang: "zh", region: "China",
        kind: Kind::Story,
        hint: "Told in episodes, each stopping exactly where you cannot bear it to.",
        exemplars: &["pingshu", "the vernacular novel's chapter endings"],
        direction: "Address the listener directly and often. Recap briefly, then advance one episode. \
                    Interrupt to comment on a character's judgement. End on the sword raised, with the \
                    outcome withheld and a formula promising the next instalment. Vernacular \
                    throughout, with set phrases for combat and for beauty.",
        guard: "The break must fall at a decision, not at a lull. Stopping at a resting point wastes \
                the only structural device the form has.",
    },
];

pub fn tradition(id: &str) -> Option<&'static Tradition> {
    TRADITIONS.iter().find(|t| t.id == id)
}

/// The traditions offered for a language: its own, plus the ones that cross languages.
///
/// The cross-language ones come last rather than first. A person who has chosen to write in Korean
/// should be offered pansori and sijo before "plain speech" — the general option is a fallback, and
/// putting it at the top makes the specific ones look like variants of it.
pub fn traditions_for(lang: &str) -> Vec<&'static Tradition> {
    let code = lang.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    let mut own: Vec<&Tradition> = TRADITIONS.iter().filter(|t| t.lang == code).collect();
    own.extend(TRADITIONS.iter().filter(|t| t.lang.is_empty()));
    own
}

/// Every language that has at least one tradition of its own.
pub fn languages() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = TRADITIONS.iter()
        .map(|t| t.lang).filter(|l| !l.is_empty()).collect();
    out.dedup_by(|a, b| a == b);
    let mut seen: Vec<&'static str> = Vec::new();
    for l in out { if !seen.contains(&l) { seen.push(l); } }
    seen
}

#[tauri::command]
pub async fn authorial_catalogue(language: Option<String>) -> Result<Value, String> {
    let lang = language.unwrap_or_default();
    let list: Vec<&Tradition> = if lang.trim().is_empty() {
        TRADITIONS.iter().collect()
    } else {
        traditions_for(&lang)
    };
    Ok(json!({
        "traditions": list.iter().map(|t| json!({
            "id": t.id, "label": t.label, "lang": t.lang, "region": t.region,
            "kind": t.kind.id(), "hint": t.hint, "exemplars": t.exemplars,
        })).collect::<Vec<_>>(),
        "dials": DIALS.iter().map(|(id, choices, label)| json!({
            "id": id, "label": label,
            "options": choices.iter().map(|c| json!({
                "id": c.id, "label": c.label, "hint": c.hint,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "languages": languages(),
    }))
}

/// The voice, rendered as instructions to whoever is writing.
///
/// Pure, and the single place the vocabulary becomes words a model reads — so a lyric, an edition
/// and a retelling all describe a voice the same way. Empty when nothing has been chosen, because a
/// heading with nothing under it reads to a model as a thing to fill in.
///
/// Order matters. The tradition comes first because it is the frame everything else sits inside; the
/// surface dials come after because they modify it; the guard comes last because the end of a block
/// is weighted, and the guard is the instruction most likely to be needed.
pub fn authorial_prompt_block(voice: &Value) -> String {
    let pick = |k: &str| voice.get(k).and_then(|v| v.as_str()).unwrap_or("").trim();
    let mut lines: Vec<String> = Vec::new();

    let trad = tradition(pick("tradition"));
    if let Some(t) = trad {
        lines.push(format!("VOICE — {} ({}, {})", t.label, t.region, t.kind.id()));
        lines.push(format!("  {}", t.direction));
        if !t.exemplars.is_empty() {
            // Named because instructions and exemplars together control style better than either
            // alone — but named as *where this is heard*, never as somebody to impersonate.
            lines.push(format!(
                "  This is the technique of {}. Write in that tradition; do not imitate any \
                 individual's voice and do not mention them.",
                t.exemplars.join(", ")));
        }
    }

    let mut surface: Vec<String> = Vec::new();
    for (dial, _, _) in DIALS {
        if let Some(c) = dial_choice(dial, pick(dial)) {
            surface.push(format!("  {}", c.instruction));
        }
    }
    if !surface.is_empty() {
        if !lines.is_empty() { lines.push(String::new()); }
        lines.push("HOW THE SENTENCES GO".into());
        lines.extend(surface);
    }

    if lines.is_empty() { return String::new(); }

    if let Some(t) = trad {
        lines.push(String::new());
        lines.push(format!("WHERE THIS GOES WRONG: {}", t.guard));
    }
    lines.push(String::new());
    format!("{}\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tradition_says_what_to_do_rather_than_what_it_is_like() {
        // A direction that could be swapped for another tradition's without changing the output is
        // not a tradition, it is an adjective.
        for t in TRADITIONS {
            assert!(t.direction.len() > 180, "{} has no real technique in it", t.id);
            assert!(t.guard.len() > 60, "{} names no failure mode", t.id);
            assert!(!t.hint.is_empty() && !t.region.is_empty(), "{}", t.id);
            // Imperatives, not descriptions: a direction full of "is" and "tends to" is a summary.
            assert!(t.direction.matches(". ").count() >= 2, "{} is one sentence", t.id);
        }
    }

    #[test]
    fn every_shipped_language_has_traditions_of_its_own() {
        // The app ships catalogues for these fifteen besides English, and until this existed the only
        // thing it knew about any of them was how to translate its own buttons.
        for lang in ["en", "de", "es", "fr", "it", "pt", "nl", "pl", "ru",
                     "ar", "he", "hi", "id", "ja", "ko", "zh"] {
            let own: Vec<_> = TRADITIONS.iter().filter(|t| t.lang == lang).collect();
            assert!(own.len() >= 3, "{lang} has only {} tradition(s)", own.len());
        }
    }

    #[test]
    fn a_language_is_offered_its_own_before_the_general_ones() {
        // A person writing in Korean should meet pansori before "plain speech": putting the general
        // option first makes the specific ones look like variants of it.
        let ko = traditions_for("ko");
        let first_general = ko.iter().position(|t| t.lang.is_empty()).unwrap();
        let last_own = ko.iter().rposition(|t| t.lang == "ko").unwrap();
        assert!(last_own < first_general);
        assert!(ko.iter().any(|t| t.id == "pansori"));
        // And the general ones are still there, since they are genuinely usable in any language.
        assert!(ko.iter().any(|t| t.id == "plain"));
    }

    #[test]
    fn a_regional_tag_still_finds_its_language() {
        // "pt-BR" and "es-419" are what a channel's language field actually looks like.
        assert!(traditions_for("pt-BR").iter().any(|t| t.id == "cordel"));
        assert!(traditions_for("es_419").iter().any(|t| t.id == "testimonio"));
        assert!(traditions_for("ZH").iter().any(|t| t.id == "tang_verse"));
        // An unknown language gets the cross-language set rather than nothing.
        let unknown = traditions_for("xx");
        assert!(!unknown.is_empty());
        assert!(unknown.iter().all(|t| t.lang.is_empty()));
    }

    #[test]
    fn one_language_can_hold_traditions_that_are_not_each_other() {
        // Andalusian deep song and Latin American testimonio are both Spanish and share nothing.
        let es = traditions_for("es");
        let jondo = es.iter().find(|t| t.id == "cante_jondo").unwrap();
        let testimonio = es.iter().find(|t| t.id == "testimonio").unwrap();
        assert_ne!(jondo.region, testimonio.region);
        assert_ne!(jondo.kind, testimonio.kind);
    }

    #[test]
    fn the_block_gives_the_technique_and_names_where_it_is_heard() {
        let block = authorial_prompt_block(&json!({ "tradition": "kjv" }));
        assert!(block.contains("King James cadence"));
        assert!(block.contains("parallelism"), "the technique is in it: {block}");
        assert!(block.contains("Tyndale"), "and so are the exemplars");
        // Named as a tradition to write in, never as a person to be.
        assert!(block.contains("do not imitate any individual's voice"));
        assert!(block.contains("WHERE THIS GOES WRONG"));
    }

    #[test]
    fn the_guard_comes_last_because_the_end_of_a_block_is_weighted() {
        let block = authorial_prompt_block(&json!({ "tradition": "hardboiled", "rhythm": "short" }));
        let guard = block.find("WHERE THIS GOES WRONG").unwrap();
        assert!(block.find("HOW THE SENTENCES GO").unwrap() < guard);
        assert!(block.find("VOICE —").unwrap() < guard);
    }

    #[test]
    fn the_surface_dials_work_without_a_tradition_and_the_other_way_round() {
        // They are independent axes: somebody may want short sentences and no tradition at all.
        let only_dials = authorial_prompt_block(&json!({ "rhythm": "short", "figuration": "bare" }));
        assert!(only_dials.contains("HOW THE SENTENCES GO"));
        assert!(!only_dials.contains("VOICE —"));
        assert!(!only_dials.contains("WHERE THIS GOES WRONG"), "no tradition, no guard");

        let only_voice = authorial_prompt_block(&json!({ "tradition": "haiku" }));
        assert!(only_voice.contains("VOICE —"));
        assert!(!only_voice.contains("HOW THE SENTENCES GO"));
    }

    #[test]
    fn nothing_chosen_produces_no_block_at_all() {
        assert_eq!(authorial_prompt_block(&json!({})), "");
        assert_eq!(authorial_prompt_block(&Value::Null), "");
        // An id that no longer exists is dropped rather than half-rendered.
        assert_eq!(authorial_prompt_block(&json!({ "tradition": "gone", "rhythm": "nope" })), "");
    }

    #[test]
    fn every_dial_option_is_an_instruction_that_could_change_an_output() {
        for (dial, choices, _) in DIALS {
            assert!(choices.len() >= 3, "{dial} is not a choice");
            for c in *choices {
                assert!(c.instruction.len() > 120, "{}/{} is an adjective", dial, c.id);
                assert!(c.instruction.starts_with(char::is_uppercase));
            }
        }
    }

    #[test]
    fn traditions_have_distinct_ids_so_a_pick_is_unambiguous() {
        let mut ids: Vec<&str> = TRADITIONS.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "two traditions share an id");
    }
}

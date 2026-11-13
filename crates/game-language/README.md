# Personal language

This owner models a small, bounded slice of language: personal sound sequences
associated with grounded referents. A host supplies actual disjoint encounters
and referents the participants can perceive. Each person independently has an
inclination to initiate or adopt, and may refuse, coin, copy or modify forms;
their phoneme subset and existing associations influence the result. No word or
vocabulary is inherited, and a listener can misunderstand an unfamiliar form.
Updates are prepared from an immutable opening view in parallel and committed
as a sorted, deterministic batch. Events preserve the heard and adopted forms.

`prepare_ungrounded` stages explicitly selected assertions whose referents are
not jointly observable. It lets the speaker use or coin a personal sound form
and records the listener's interpretation from their opening vocabulary, but
never teaches the listener the speaker's intended meaning. The caller supplies
actual disjoint contact and must treat the event's intended meaning as private
to the speaker. The interval host uses the same ungrounded semantics when a
personally offered mechanical claim wins the meeting's topic choice; the
listener's interpretation, rather than the speaker's intention, becomes any
received testimony. A retained jointly witnessed experiment can also ground an
outcome topic at a later meeting. A person's optional `mechanical_interest`
can make such a topic attractive without predefining a word for each apparatus;
they can still decline, coin, interpret differently, or decline adoption.

The ignored `parallel_ungrounded_utterance_benchmark` staged 100,000 proposals
in median times of 52.5, 32.1, 25.5 and 21.6 ms at one, two, four and six
workers. Results matched across worker counts; this is an isolated synthetic
workload with sublinear scaling.

The interval host supplies the shared location and objects each participant
personally carries. An optional personal topic policy lets each participant
propose a preferred topic or remain silent; choosing a carried object models
the speaker voluntarily demonstrating it to the listener. One speaker is
admitted per disjoint encounter, and the listener's hearing and learning
remain independent. Without a topic policy the earlier place-only behavior is
retained. Need referents require a host able to establish an actual signal;
private hunger is not automatically shared. A speaker may also choose a
present participant as a topic, coin a personal sound form for that person,
and share it; the listener can independently refuse or adopt the form. These
are personal associations, not mandatory or globally unique names. An optional
personal naming policy can value novel forms or reuse a known person's form,
weighted by the speaker's directed affinity and their own signed tolerance for
exact-form confusion. A speaker may instead decline to coin a form. Namesake
forms must be in that speaker's vocabulary and within their phoneme inventory.
See `content/scenarios/economy-core-namesake.toml` for an authored example.
Naming absent people, similarity beyond exact forms, grammar, and transmission
of general factual claims remain incomplete.

A participant who previously noticed or personally remembers another
participant's nonverbal signal can choose that physical form as a topic in a
later encounter. The speaker demonstrates the form; speech associates sounds
with the observable signal, not the sender's private intention or the
observer's interpretation. The listener independently chooses whether to
learn. `content/scenarios/economy-core-signal-language.toml` illustrates the
numeric signal-form topic. A form that the speaker did not notice or remember
does not become an available topic merely because it exists in a catalog.

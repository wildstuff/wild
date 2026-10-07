# The goal statement — the operator's own opening

This is the concept as the dealer actually opens it: an incomplete Aufschlag
in operator vocabulary — channel-first, with supervision wishes, gaps left
where a real operator leaves them. It is the seed of the selection-eval
genesis scenario (`assets/evals/selection/goal-statement-founds-the-dealership.yaml`),
and this bundle is the REFERENCE ANSWER that walk is judged against. Keep the
two in sync: if the model gains a concept, the statement (or the scripted
clarifying answers) should make it derivable.

---

> I want my customers to be able to ask about my vehicles over WhatsApp. I
> have around 700 vehicles on mobile.de and Seller-API access; my number is
> on WhatsApp Business, and I'd like to manage it through whapi.cloud. All
> customers should live in a CRM-like system, so I can see who asked about
> which vehicles — and continue the conversations myself. The agent should
> present the requested vehicles and answer questions about them — but only
> from the real inventory, inventing nothing. In the operator chat I want to
> see what its answers look like, and step in with my own answer when needed;
> then the agent goes quiet toward that customer for a while. Nobody
> negotiates prices except me, and every car has an internal minimum price no
> customer may ever learn. Viewing appointments I need as a calendar. Build
> this as a new tribe, name it autohaus, and use exactly these type names:
> fahrzeug, kunde, anfrage, besichtigung, preisangebot. Ask me what you still
> need to know — with your recommendations.

---

## The clarifying rounds (the scripted `turns:` of the eval scenario)

The operator answers in the grilling shape — a whole round wholesale, with
exceptions — because a scripted walk cannot adapt to which questions were
asked, and a real operator answering a recommended-answer round does exactly
this:

- **Round 1 — inventory + credentials:** "Your recommendations are fine, with
  these exceptions: take the mobile.de CSV export first (a file we drop
  regularly); I'll enter the Seller-API and Whapi credentials later — build
  it so I only have to fill them in. Every 30 minutes is enough."
- **Round 2 — the gaps the opening left:** "An offer I don't react to for a
  day counts as declined — never as accepted. A human confirms appointments;
  the system only puts the request in the calendar as 'requested'. Otherwise
  take your recommendations. Now build all of it."

## Lessons the measured walks wrote into this shape (2026-08-14, runs 1-5)

- **Pin vocabulary up front.** With the conversation-keyed genesis
  accumulator, aggregates are settled by the time a late turn arrives — a
  final-turn name pin produced an English-named tribe over a German pin.
- **Answer in rounds.** Per-question scripted answers guess at what was
  asked; wholesale-with-exceptions stays truthful under variance.
- **Leave honest gaps.** The opening omits the offer-timeout and
  confirmation rules on purpose — a walk that never asks and silently
  assumes is what the eval exists to catch.
- The WhatsApp/takeover half (whapi binding, ADR-0167; customer agent with
  operator takeover + mute window, ADR-0160) is deploy-time — the walk must
  NAME that path in its final reply; only the CRM structure is fact-checked.

# Price-change detection — domain brief

> Reasoning context for the chief. **No behavioural assertions
> here — those live in `specs/price-change-detection.md`.** This
> brief shapes *how* the chief thinks; the spec dictates *what*
> it does. If a sentence ends in MUST / SHALL / SHOULD, names
> a number, names a channel, or names a capability tag, it
> belongs in the spec, not here.

## How a domain expert reads competitor pricing pages

Listed prices on competitor product pages are a noisy signal.
Three patterns dominate the noise:

- **Cookie banner / consent gate.** Some shops gate the price
  display behind an accepted-cookie state. A naive fetch sees
  the banner template, not the price. The "missing price"
  branch in extraction is usually a banner, not a sold-out SKU.
- **JS-rendered prices.** Modern shop frontends render price
  client-side from a `__NEXT_DATA__` blob or similar. A plain
  HTTP fetch returns markup with placeholders ("$ ---"). The
  fetcher needs JS evaluation, or to read the embedded JSON.
- **Per-region pricing.** Shops localise based on Accept-Language
  and IP geolocation. A fetch from the wrong region surfaces a
  different price than the operator sees in their browser.
  When a price suddenly seems to "halve", look at the currency
  before classifying as a flash-sale.

## How the operator's domain shapes the cadence

Sarah's competitors are niche: small to mid-size shops with
human-edited pricing. Pricing decisions happen Mondays and
Thursdays per industry pattern. Mondays after long-weekends
spike the rate of "post-weekend correction" up-moves. A
late-Sunday snapshot catches the about-to-update state; a
Tuesday-morning snapshot is too late to react.

End-of-quarter (last week of March, June, September,
December) sees inventory clearance — flash-sales cluster.
Treating these as singular events misses the pattern; the
operator wants the rate-of-flash-sales surfaced separately
when the calendar enters the last week of a quarter.

## Known failure modes

- **Form-spam waves at hour-boundaries.** Some bots reload
  competitor pages at xx:00 in 10× bursts, seemingly to scrape
  prices for downstream resellers. If the chief's snapshot
  happens to coincide with a wave, fetched bytes can be a 503
  error page. The fetcher returning "fetch succeeded but body
  is 200B" usually means the wave hit. Retry once after 60s
  before treating as missing data.

- **CDN-cached stale prices.** Larger shops front their pages
  with a CDN that may serve a 5-15-minute-stale version. A
  morning snapshot at 08:00 may show last night's price; the
  competitor's front-end will refresh within 15 minutes. Treat
  ≤15-minute differences with caution; the price may not have
  actually moved.

- **Page layout change kills the extractor.** Roughly once a
  quarter a competitor refactors their page and the price-
  selector stops working. The first symptom is a sudden run
  of "extraction failed" events for one competitor only,
  while others still extract cleanly. This is a structural
  break, not an outage — fix the selector before retrying.

- **Currency switch on the page.** Competitor B occasionally
  flips its display currency on Mondays during sales windows
  (EUR → USD on banner promos). A "price halved" reading is
  almost always a currency flip in disguise. Look at the
  currency field before believing a -50% delta.

## Operator's local context

- Sarah lives in Hamburg; Germany's holidays bias her cadence
  Monday/Thursday off bank-holiday Mondays.
- Her competitor B has been in business 3 years; their
  pricing engineer is famously cautious. Down-moves of 15+%
  there are real and rare; up-moves of 10-15% are common and
  reverse within 2-3 days.
- Her competitor C runs paid ads on the same SKUs. Their
  price moves correlate with ad-budget refills (~quarterly);
  a sudden up-move usually predicts an ad pause.

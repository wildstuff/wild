# Alert delivery — domain brief

> Reasoning context for the chief. **No behavioural assertions
> here — those live in `specs/alert-delivery.md`.**

## How an operator reads a price-change alert at 8:30 in the morning

Sarah opens Telegram with a coffee in hand. The alert competes
with five other notifications, three of which she is muting
already. The signal/noise ratio determines whether she
*reads* the alert vs. taps "mute this chat for 24h".

What makes her read it:

- The *number* is the headline. `-11% on Acme USB-C dock`
  beats `Pricing Monitor: SKU detected as flash-sale`.
- The classification label is a hint, not the headline.
  If the label is wrong but the number is real, she still
  acts. If the number is wrong, no label saves the alert.
- Reason in plain language. "Listing flagged 'Cyber Monday
  sale' on the page" beats "delta=-11.1%, label=flash-sale,
  confidence=0.78".

What makes her mute the chat:

- Three alerts in a morning for SKUs she hasn't sold in 6
  months. (Operator wants a way to mark SKUs as "dormant"
  — out of scope today, but pinned in `notes/` for later.)
- An alert that arrives 6 hours after the price moved.
  Pricing decisions in her business are reactive within
  the day; a stale alert is worse than no alert.
- An alert about a movement she already saw on a competitor
  newsletter she subscribes to.

## Known failure modes

- **Telegram bot rate limits.** The bot API caps at 30
  messages per second per bot. A 12-SKU alert burst is fine,
  but a 60-SKU spillover (e.g. an extractor regression
  flagging every SKU as "moved") will eat the bot's quota
  and leave Sarah with a partial alert set. The chief should
  prefer one summary message over many individual ones when
  the burst exceeds the cap.

- **Markdown escaping.** Telegram's MarkdownV2 mode rejects
  unescaped underscores, asterisks, parentheses, and
  brackets. Competitor SKU names occasionally contain
  underscores ("Acme_USB_C_Dock"). An unescaped name kills
  the whole message — Telegram returns 400 Bad Request and
  the alert is lost. Plain-text mode is safer; format with
  bold/italic only via the `parse_mode: HTML` route.

- **Operator's phone is on Do-Not-Disturb between 22:00 and
  07:00 local time.** An alert published at 06:55 local
  arrives silent — she sees it at 07:30 when she opens the
  phone. This is fine for `unclear` and `restock` labels.
  For `flash-sale` labels with `delta_pct ≤ -15`, latency
  matters — the price floor is moving and she has hours,
  not minutes, to react.

- **Long URLs break Telegram's preview card.** Competitor
  product pages have query-string-heavy URLs (`?ref=...&
  utm=...`). When the URL exceeds ~250 chars, Telegram
  shows the URL but no preview card. Sarah scans previews;
  no preview = no scan = missed alert. Strip query strings
  before quoting URLs in the message.

## Operator's local context

- Sarah uses Telegram on iOS; the Telegram-iOS app reliably
  delivers notifications even when the device is locked.
- She has one `@PricingBot` chat configured; the bot's
  chat-id lives in `<profile>/secrets/telegram.toml`.
- Her timezone is Europe/Berlin; "morning" means 07:00–09:00
  local. Alerts that fire 14:00 local arrive after she has
  already left her desk and are usually missed until evening.

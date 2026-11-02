# Extract the domain from a link

Give it a web address, get back the site it belongs to.

    https://www.example.com/article/123?q=1   →   example.com

## What you would use it for

You have links — in a spreadsheet, in an inbox, in records a connector
brought in — and you want to group or filter them by site rather than by
the full address. "Which suppliers did these 400 invoices come from?" is
a domain question, not a URL question.

## What it gives back

The bare registered host, with the `www.` prefix removed. A path, a
query string, a port or a fragment are dropped. Other subdomains are
KEPT — `docs.example.com` stays `docs.example.com`, because that is
usually a different thing than `example.com`.

If the input is not a web address it cannot make sense of, it hands the
input back unchanged rather than guessing. That is deliberate: a wrong
domain is worse than an obvious non-answer, because a wrong one gets
counted.

## What it needs from you

Nothing. No key, no account, no network access — it reads the text you
give it and returns text. It never reaches outside this installation.

## What it does not do

- It does not check that the site exists or answers.
- It does not fetch anything from the address.
- It does not tell you whether a domain is a company, a person, or a
  parking page.

## If something looks wrong

The four recorded examples in `golden/` are the promises this component
makes — they say exactly what it does with a `www.` prefix, with a
numbered prefix like `www2.`, with a subdomain, and with input it cannot
parse. If behaviour ever disagrees with one of those, that is a
regression and worth reporting, not a setting you need to find.

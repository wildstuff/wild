# Turn a text into a URL-safe name

Give it any text; get back a lowercase, hyphenated version that is safe
to use in a web address, a file name, or an identifier.

    "Hello World"        →  hello-world
    "Grüße Über Ärger"   →  gruesse-ueber-aerger

## What you would use it for

Anywhere a human-written title has to become a machine-readable name:
page addresses, export file names, folder names, keys in a report. It
is the step between "what someone typed" and "what a system can store".

## What it gives back

Lowercase letters, digits and hyphens — nothing else. German umlauts and
ß are **folded to their spelled-out forms** (ü→ue, ß→ss) rather than
dropped, so a name stays readable and two different words do not collapse
into one. Anything else that is not a letter or digit becomes a hyphen;
runs of hyphens collapse to one; leading and trailing hyphens are
trimmed.

The five recorded examples in `golden/` state exactly this: lowercasing
and spaces, umlaut folding, special characters, collapsing, trimming.

## What it needs from you

The text. No key, no account, no network access — it reads what you give
it and returns text. It never reaches outside this installation.

## What it does not do

- It does not check whether the result is already taken.
- It does not shorten: a long title becomes a long slug.
- It does not transliterate beyond German folding — other scripts become
  hyphens, so a title in one may come back nearly empty. Check the
  result before using it as the only identifier.

## If something looks wrong

The five cases in `golden/` are the promises this component makes. If
behaviour disagrees with one of them, that is a regression worth
reporting — not a setting you need to find.

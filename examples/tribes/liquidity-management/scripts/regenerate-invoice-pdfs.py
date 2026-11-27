#!/usr/bin/env python3
"""Regenerate the inbound-invoice PDFs — WITH line items.

Run:  python3 examples/tribes/liquidity-management/scripts/regenerate-invoice-pdfs.py

It lives OUTSIDE `documents/` on purpose: `wild tribe apply` delivers that whole
subtree into the tribe's file store, so a generator parked there would be handed
to the operator as one of their scans (and counted as one).

Why this exists
---------------
These three PDFs used to carry only a one-line `Leistung:` and the totals. That
made them useless for the thing a document source is FOR: the operator asking
"which invoice had the pallet cages on it?" and getting the page back. The DOCX
siblings in the same folder always had a `Positionen:` table; the PDFs did not,
so a content search over this example could only ever match a supplier name or
an invoice number — both of which the CSV seed already answers.

Each document is the BELEG for an invoice the CSV already books
(`data/invoices-inbound.csv`), and carries that invoice's number, supplier,
dates and totals. It has to: a position is declared `part_of` its invoice with
`enforced: true`, so a line item for an invoice that does not exist is refused
at confirm time — correctly, and that is what these fixtures used to produce
(`dangling-relation: invoice_position.on_invoice references invoice/EK-2026-9001
which does not exist`).

The positions sum to each invoice's NET — the gross the CSV books, minus the
VAT the model's own `vat_amount` function derives at the tribe's `vat_rate`.
That is the figure `invoice-positions-check` compares against, so a correct
document reconciles and a tampered one does not.

Checked in as a generator rather than as three opaque binaries: the next person
who needs a fourth supplier edits a table here instead of reverse-engineering a
PDF. Mirrors `data/regenerate-seed.py`.

No third-party dependency — the PDF is written by hand, uncompressed, so every
line item is greppable in the raw bytes and any parse-text tool can read it.
"""

import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "documents" / "inbound-invoices"

# Each invoice: the CSV row it is the document FOR, plus the positions that make
# up its net. `net` is asserted against the sum below, so a typo in a line item
# fails the run instead of shipping a document that disagrees with itself.
#
# gross/vat/net come from the booked amount: vat = gross x 19 / 119 rounded to
# the cent, net = gross - vat (the model's own reconciliation-safe order).
INVOICES = [
    {
        "file": "EK-2026-0091-techparts.pdf",
        "supplier": "TechParts Inc",
        "address": "Industriestrasse 12, 70565 Stuttgart",
        "iban": "DE02120300000000202051",
        "number": "EK-2026-0091",
        "issued": "2026-05-04",
        "due": "2026-06-03",
        "subject": "Spezialbauteile Sonderlieferung",
        "terms": "Zahlbar innerhalb 30 Tagen. Skonto 2% bei Zahlung innerhalb 10 Tagen.",
        "net": 7142.86,
        "vat": 1357.14,
        "gross": 8500.00,
        "positions": [
            ("Praezisionslager 6205-2RS", 40, 18.90),
            ("Antriebswelle Typ AW-120 gehaertet", 12, 246.50),
            ("Dichtungssatz DS-44 Viton", 25, 31.20),
            ("Aluminiumprofil 40x40 eloxiert, 3 m", 18, 42.75),
            ("Montagesatz Edelstahl M8", 60, 6.35),
            ("Sonderanfertigung Halterung HB-7", 2, 749.18),
        ],
    },
    {
        "file": "EK-2026-0092-logistik.pdf",
        "supplier": "Logistics Express Ltd",
        "address": "Speditionsweg 8, 44137 Dortmund",
        "iban": "DE80200400600004444400",
        "number": "EK-2026-0092",
        "issued": "2026-06-01",
        "due": "2026-07-01",
        "subject": "Speditionsleistungen Sonderfahrten",
        "terms": "Zahlbar innerhalb 30 Tagen netto. Kein Skonto.",
        "net": 2605.04,
        "vat": 494.96,
        "gross": 3100.00,
        "positions": [
            ("Sonderfahrt Stuttgart-Hamburg, 12 t", 2, 685.00),
            ("Beiladung Teilpartie Leipzig", 3, 218.40),
            ("Palettentausch Europalette", 20, 9.80),
            ("Gitterboxen Miete je Woche", 10, 12.60),
            ("Wartezeit Verladung je angefangene Stunde", 4, 64.46),
        ],
    },
    {
        "file": "EK-2026-0093-buero.pdf",
        "supplier": "Miller Office",
        "address": "Buerozeile 5, 10115 Berlin",
        "iban": "DE32700100800012345000",
        "number": "EK-2026-0093",
        "issued": "2026-06-10",
        "due": "2026-07-10",
        "subject": "Bueromaterial Quartalsbedarf",
        "terms": "Zahlbar innerhalb 30 Tagen netto. Kein Skonto.",
        "net": 1008.40,
        "vat": 191.60,
        "gross": 1200.00,
        "positions": [
            ("Kopierpapier A4 80g, Karton zu 2500 Blatt", 12, 24.90),
            ("Toner HP 26X kompatibel", 4, 89.50),
            ("Ordner breit, Ruecken 80 mm", 30, 3.20),
            ("Notizbuch A5 kariert", 25, 4.80),
            ("Schreibtischunterlage Leder-Optik", 5, 27.12),
        ],
    },
]



def money(v: float) -> str:
    """German thousands/decimal, as the shipped documents render it."""
    # 6535.29 -> "6.535,29": swap both separators at once, so no
    # placeholder character is needed (a placeholder is how this line
    # first shipped a NUL into the file).
    return f"{v:,.2f}".translate(str.maketrans({",": ".", ".": ","}))


def lines_for(inv: dict) -> list[str]:
    total = round(sum(q * p for _, q, p in inv["positions"]), 2)
    if abs(total - inv["net"]) > 0.005:
        sys.exit(
            f"{inv['file']}: positions sum to {total:.2f} but the invoice states "
            f"a net of {inv['net']:.2f}. Fix the table — a sample document that "
            f"contradicts its own total teaches the reader that the numbers do "
            f"not matter."
        )
    out = [
        inv["supplier"],
        inv["address"],
        f"IBAN: {inv['iban']}",
        "",
        "RECHNUNG",
        "",
        f"Rechnungsnummer: {inv['number']}",
        f"Rechnungsdatum: {inv['issued']}",
        f"Faelligkeit: {inv['due']}",
        f"Leistung: {inv['subject']}",
        "",
        "Positionen:",
        f"{'Pos':<4}{'Beschreibung':<46}{'Menge':>7}{'Einzelpreis':>14}{'Gesamt':>13}",
    ]
    for i, (desc, qty, unit) in enumerate(inv["positions"], start=1):
        out.append(
            f"{i:<4}{desc:<46}{qty:>7}{money(unit):>14}{money(round(qty * unit, 2)):>13}"
        )
    out += [
        "",
        f"{'Nettobetrag:':<20}{money(inv['net']):>12} EUR",
        f"{'USt. 19%:':<20}{money(inv['vat']):>12} EUR",
        f"{'Gesamtbetrag:':<20}{money(inv['gross']):>12} EUR",
        "",
        inv["terms"],
    ]
    return out


def page_stream(lines: list[str]) -> bytes:
    out = ["BT", "/F1 9 Tf", "12 TL", "40 780 Td"]
    for ln in lines:
        esc = ln.replace("\\", r"\\").replace("(", r"\(").replace(")", r"\)")
        out.append(f"({esc}) Tj")
        out.append("T*")
    out.append("ET")
    return "\n".join(out).encode("latin-1", "replace")


def write_pdf(path: Path, lines: list[str]) -> None:
    stream = page_stream(lines)
    objs = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [4 0 R] /Count 1 >>",
        3: b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>",
        4: (
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] "
            b"/Resources << /Font << /F1 3 0 R >> >> /Contents 5 0 R >>"
        ),
        5: f"<< /Length {len(stream)} >>\nstream\n".encode() + stream + b"\nendstream",
    }
    buf = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = {}
    for num in sorted(objs):
        offsets[num] = len(buf)
        buf += f"{num} 0 obj\n".encode() + objs[num] + b"\nendobj\n"
    xref_at = len(buf)
    buf += f"xref\n0 {len(objs) + 1}\n".encode() + b"0000000000 65535 f \n"
    for num in sorted(objs):
        buf += f"{offsets[num]:010d} 00000 n \n".encode()
    buf += (
        f"trailer\n<< /Size {len(objs) + 1} /Root 1 0 R >>\n"
        f"startxref\n{xref_at}\n%%EOF\n"
    ).encode()
    path.write_bytes(buf)


def main() -> None:
    for inv in INVOICES:
        lines = lines_for(inv)
        path = OUT / inv["file"]
        write_pdf(path, lines)
        print(
            f"  {inv['file']}: {len(inv['positions'])} positions, "
            f"net {inv['net']:.2f} reconciled ({path.stat().st_size} bytes)"
        )


if __name__ == "__main__":
    main()

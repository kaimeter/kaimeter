#!/usr/bin/env python3
"""Extract aluminium default values from the Commission's corrected workbook.

Reads the Annex I and Annex IV tables of the definitive-period default-values
workbook (IR (EU) 2025/2621 as corrected by IR (EU) 2026/1740) and writes
deterministic JSON parameter tables for the aluminium codes, with row-level
provenance attached to every value.

Usage:
    python extract-default-values.py WORKBOOK OUTPUT_DIR \
        --expect-sha256 HEX --retrieved YYYY-MM-DD --source-url URL

The script refuses to run unless WORKBOOK's SHA-256 matches --expect-sha256,
so the committed JSON always names the reviewed file.
"""

import argparse
import hashlib
import json
import pathlib
import re
import sys
import xml.etree.ElementTree as ET
import zipfile

NS = {
    "main": "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
    "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    "pkg": "http://schemas.openxmlformats.org/package/2006/relationships",
}

ALUMINIUM_PREFIX = "76"
NUMBER = re.compile(r"^\d+(\.\d+)?$")
CODE = re.compile(r"^\d{4}(\s?\d{2}){0,2}$")

ANNEX_I_LEGAL = (
    "Implementing Regulation (EU) 2025/2621, Annex I, as corrected by "
    "Implementing Regulation (EU) 2026/1740"
)
ANNEX_IV_LEGAL = (
    "Implementing Regulation (EU) 2025/2621, Annex IV, as corrected by "
    "Implementing Regulation (EU) 2026/1740"
)
OTHER_COUNTRIES = "Other Countries and Territories"
COUNTRY_SHEETS = {"Overview", "Version History", "_Other Countries and Territorie", "Annex IV"}


def shared_strings(archive):
    try:
        root = ET.fromstring(archive.read("xl/sharedStrings.xml"))
    except KeyError:
        return []
    return [
        "".join(node.text or "" for node in item.iter("{%s}t" % NS["main"]))
        for item in root.findall("main:si", NS)
    ]


def sheet_map(archive):
    workbook = ET.fromstring(archive.read("xl/workbook.xml"))
    rels = ET.fromstring(archive.read("xl/_rels/workbook.xml.rels"))
    targets = {
        rel.get("Id", ""): rel.get("Target", "") for rel in rels.findall("pkg:Relationship", NS)
    }
    sheets = []
    sheets_element = workbook.find("main:sheets", NS)
    if sheets_element is None:
        raise ValueError("workbook has no sheets")
    for sheet in sheets_element:
        name = sheet.get("name") or ""
        target = targets.get(sheet.get("{%s}id" % NS["r"]) or "", "")
        if not target:
            raise ValueError(f"no target for sheet {name!r}")
        if not target.startswith("/"):
            target = "xl/" + target.lstrip("./")
        sheets.append((name, target))
    return sheets


def column_index(reference):
    letters = "".join(character for character in reference if character.isalpha())
    index = 0
    for character in letters:
        index = index * 26 + (ord(character) - ord("A") + 1)
    return index - 1


def read_rows(archive, target, strings):
    root = ET.fromstring(archive.read(target))
    for row in root.iter("{%s}row" % NS["main"]):
        values = {}
        for cell in row.findall("main:c", NS):
            kind = cell.get("t")
            value_node = cell.find("main:v", NS)
            inline = cell.find("main:is", NS)
            if kind == "s" and value_node is not None:
                value = strings[int(value_node.text or "0")]
            elif kind == "inlineStr" and inline is not None:
                value = "".join(node.text or "" for node in inline.iter("{%s}t" % NS["main"]))
            elif value_node is not None:
                value = value_node.text or ""
            else:
                value = ""
            values[column_index(cell.get("r") or "")] = value
        width = max(values) + 1 if values else 0
        yield [values.get(index, "").strip() for index in range(width)]


def parse_decimal(text):
    normalized = text.replace(",", ".")
    if normalized in ("-", "\u2013", "N/A", "n/a", "see below", ""):
        return None
    if not NUMBER.match(normalized):
        raise ValueError(f"unexpected value {text!r}")
    return normalized


def parse_code(text):
    if not CODE.match(text) or not text.startswith(ALUMINIUM_PREFIX):
        return None
    return text.replace(" ", "")


def parse_route(text):
    text = text.strip()
    if text.startswith("(") and text.endswith(")"):
        return text[1:-1]
    return text or None


def annex_i_row(cells, country, code):
    value = lambda index: cells[index] if index < len(cells) else ""
    total = parse_decimal(value(4))
    if total is None:
        return None
    return {
        "cnCode": code,
        "country": country,
        "direct": parse_decimal(value(2)),
        "indirect": parse_decimal(value(3)),
        "total": total,
        "route": parse_route(value(5)),
        "description": value(1),
    }


def annex_iv_row(cells, code):
    value = lambda index: cells[index] if index < len(cells) else ""
    highest = parse_decimal(value(2))
    if highest is None:
        return None
    return {
        "cnCode": code,
        "description": value(1),
        "highest": highest,
        "route": parse_route(value(3)),
    }


def collect(workbook):
    with zipfile.ZipFile(workbook) as archive:
        strings = shared_strings(archive)
        countries = {}
        other = []
        annex_iv_rows = []
        for name, target in sheet_map(archive):
            rows = list(read_rows(archive, target, strings))
            if not rows:
                continue
            if name == "Annex IV":
                annex_iv_rows = collect_annex_iv(rows)
                continue
            if name == "_Other Countries and Territorie":
                other = collect_annex_i(rows)
                continue
            if name in COUNTRY_SHEETS:
                continue
            country = rows[0][0] if rows[0] and rows[0][0] else name
            group = collect_annex_i(rows)
            if group:
                countries[country] = group
    return countries, other, annex_iv_rows


def collect_annex_i(rows):
    collected = []
    for cells in rows[2:]:
        if not cells:
            continue
        code = parse_code(cells[0])
        if code is None:
            continue
        row = annex_i_row(cells, "", code)
        if row is not None:
            collected.append(row)
    return collected


def collect_annex_iv(rows):
    collected = []
    for cells in rows[2:]:
        if not cells:
            continue
        code = parse_code(cells[0])
        if code is None:
            continue
        row = annex_iv_row(cells, code)
        if row is not None:
            collected.append(row)
    return collected


def attach_provenance(rows, legal, source, retrieved):
    for row in rows:
        row["legal"] = legal
        row["source"] = source
        row["retrieved"] = retrieved


def codes_map(rows):
    codes = {}
    for row in rows:
        code = row["cnCode"]
        description = row.pop("description")
        if code in codes and codes[code] != description:
            raise ValueError(f"conflicting descriptions for {code}")
        codes[code] = description
    return dict(sorted(codes.items()))


def compact(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def write_json(path, sections):
    lines = ["{"]
    for index, (key, value, row_blocks) in enumerate(sections):
        comma = "," if index + 1 < len(sections) else ""
        if row_blocks:
            lines.append(f"  {json.dumps(key)}: [")
            for row_index, row in enumerate(value):
                row_comma = "," if row_index + 1 < len(value) else ""
                lines.append(f"    {compact(row)}{row_comma}")
            lines.append(f"  ]{comma}")
        else:
            lines.append(f"  {json.dumps(key)}: {compact(value)}{comma}")
    lines.append("}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("workbook")
    parser.add_argument("output_dir")
    parser.add_argument("--expect-sha256", required=True)
    parser.add_argument("--retrieved", required=True)
    parser.add_argument("--source-url", required=True)
    arguments = parser.parse_args()

    digest = hashlib.sha256(pathlib.Path(arguments.workbook).read_bytes()).hexdigest()
    if digest.lower() != arguments.expect_sha256.lower():
        raise SystemExit(f"workbook SHA-256 {digest} does not match --expect-sha256")

    countries, other, annex_iv_rows = collect(arguments.workbook)
    if not countries or not other or not annex_iv_rows:
        raise SystemExit("workbook is missing country, other-countries or Annex IV rows")

    annex_i_rows = []
    for country in sorted(countries):
        for row in countries[country]:
            row["country"] = country
            annex_i_rows.append(row)
    for row in other:
        row["country"] = OTHER_COUNTRIES
    for row in annex_i_rows + other:
        if row["indirect"] is None and row["direct"] != row["total"]:
            raise ValueError(f"direct/total mismatch for {row['country']} {row['cnCode']}")

    attach_provenance(annex_i_rows, ANNEX_I_LEGAL, arguments.source_url, arguments.retrieved)
    attach_provenance(other, ANNEX_I_LEGAL, arguments.source_url, arguments.retrieved)
    attach_provenance(annex_iv_rows, ANNEX_IV_LEGAL, arguments.source_url, arguments.retrieved)

    annex_i_rows.sort(key=lambda row: (row["country"], row["cnCode"]))
    other.sort(key=lambda row: row["cnCode"])
    annex_iv_rows.sort(key=lambda row: row["cnCode"])
    codes = codes_map(annex_i_rows + other)

    output_dir = pathlib.Path(arguments.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    write_json(
        output_dir / "default-values-annex-I.json",
        [
            ("table", "default-values-annex-I", False),
            ("legal", ANNEX_I_LEGAL, False),
            ("source", arguments.source_url, False),
            ("retrieved", arguments.retrieved, False),
            ("workbookSha256", digest.lower(), False),
            ("codes", codes, False),
            ("rows", annex_i_rows, True),
            ("otherCountries", other, True),
        ],
    )

    write_json(
        output_dir / "default-values-annex-IV.json",
        [
            ("table", "default-values-annex-IV", False),
            ("legal", ANNEX_IV_LEGAL, False),
            ("source", arguments.source_url, False),
            ("retrieved", arguments.retrieved, False),
            ("workbookSha256", digest.lower(), False),
            ("rows", annex_iv_rows, True),
        ],
    )

    print(f"annex I rows: {len(annex_i_rows)} across {len(countries)} countries")
    print(f"other-countries rows: {len(other)}")
    print(f"annex IV rows: {len(annex_iv_rows)}")
    print(f"codes: {len(codes)}")


if __name__ == "__main__":
    sys.exit(main())

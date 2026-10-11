"""Validate the original startup-owner subset against immutable Track C G0.

Raw complete IDs include the owned temporary root and are intentionally not
compared across runs. Normalized structure, full work and acknowledgement
receipts must match. Supplemental/component records use separate validators.
"""
import argparse
import json
from pathlib import Path
import re

KEY = ("owner", "fixture", "fixture_signature", "item_count", "viewport",
       "scale_milli", "fonts", "settings", "cold_type")
PARITY = ("structural_output_ids", "revision_receipts", "viewport_receipts",
          "work_units", "work_counters")
COUNTS = {"Small": {16}, "Full": {16, 1000, 10000}}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def uint(value):
    return type(value) is int and 0 <= value <= 2**64 - 1


def validate(data, baseline, source, mode):
    require(re.fullmatch(r"[0-9a-f]{40}", source), "invalid source")
    require(mode in COUNTS, "invalid mode")
    require(baseline.get("status") == "FROZEN_G0"
            and baseline.get("source") == "071243db068835617a054a74fb5734a6dd9f128d",
            "frozen G0 provenance")
    require(type(data.get("schema")) is int and data["schema"] == 1, "schema")
    require(data.get("source") == source and data.get("profile") == "fast-dev"
            and data.get("mode") == mode, "collection provenance")
    expected = {tuple(r[k] for k in KEY): r for r in baseline["records"]
                if r["owner"] == "startup_action_catalog"
                and r["item_count"] in COUNTS[mode]}
    require(len(expected) == len(COUNTS[mode]), "frozen baseline inventory")
    records = data.get("records")
    require(type(records) is list and len(records) == len(expected), "inventory")
    seen, rows = set(), []
    for record in records:
        for field in ("item_count", "scale_milli", "warmups", "samples", "exclusions"):
            require(uint(record.get(field)), f"typed integer: {field}")
        key = tuple(record.get(k) for k in KEY)
        require(key in expected and key not in seen, "unknown or duplicate recipe")
        seen.add(key)
        old = expected[key]
        require(record.get("source") == source and record.get("profile") == "fast-dev"
                and record.get("mode") == mode and record.get("telemetry_enabled") is True,
                "record provenance")
        require((record["warmups"], record["samples"], record["exclusions"]) == (5, 20, 0),
                "sampling protocol")
        for field in PARITY:
            values = record.get(field)
            require(type(values) is list and len(values) == 20, f"array: {field}")
            if field == "work_units":
                require(all(uint(v) for v in values), "typed work units")
            require(values == old[field], f"frozen parity: {field}")
        complete = record.get("complete_output_state_ids")
        require(type(complete) is list and len(complete) == 20
                and all(type(v) is str and re.fullmatch(r"[0-9a-f]{16}", v) for v in complete)
                and len(set(complete)) == 1, "per-owned-root complete identity")
        elapsed = record.get("elapsed_ns")
        require(type(elapsed) is list and len(elapsed) == 20
                and all(uint(v) for v in elapsed), "elapsed samples")
        ordered = sorted(elapsed)
        row = {k: record[k] for k in KEY}
        for field, position in (("p50_ns", 9), ("p95_ns", 18), ("max_ns", 19)):
            require(uint(record.get(field)) and record[field] == ordered[position],
                    f"nearest rank: {field}")
            row.update({"g0_" + field: old[field], "current_" + field: record[field],
                        field + "_ratio": record[field] / old[field]})
        rows.append(row)
    require(seen == set(expected), "incomplete keys")
    return dict(schema=1, source=source, profile="fast-dev", mode=mode,
                status="SUBSET_PARITY_PASS_INDEPENDENT_REVIEW_PENDING", records=records,
                comparison=dict(g0_source=baseline["source"], rows=rows,
                    scope="matched_Full_catalog_CPU_owner" if mode == "Full"
                          else "Small_vs_frozenFull_context_only",
                    complete_identity="per_run_owned_root_not_cross_run_equal",
                    limits="new_coordinator_warm_OS_cache_not_process_or_display_latency"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--mode", choices=COUNTS, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    baseline = json.loads(Path(__file__).with_name("track_c_baseline.json").read_text(encoding="utf-8"))
    data = json.loads(args.input.read_text(encoding="utf-8-sig"))
    result = validate(data, baseline, args.source, args.mode)
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(result, output, indent=2)
        output.write("\n")


if __name__ == "__main__":
    main()

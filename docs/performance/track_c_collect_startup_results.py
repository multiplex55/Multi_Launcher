"""Strict Track C C3-A supplemental collection; no process-latency inference."""
import argparse
import json
import math
from pathlib import Path
import re

STARTUP_MARKER = "TRACK_C3A_STARTUP_RESULT"
COMPONENT_MARKER = "TRACK_C3A_READY_CATALOG_RESULT"
STARTUP_PHASES = ["coordinator_construct", "submit", "wait", "take_result",
                  "validate_transfer", "clone_indexed_actions", "wrap_catalog_arc"]
SCAN_FIELDS = ["calls", "work_units", "elapsed_ns_total", "completed", "errors", "abandoned"]
FROZEN_FIXTURES = {16: "d6f2eaec0d1b6dda", 1000: "4381e84f0b05fcb0",
                   10000: "1abd427c53438770"}
STARTUP_KEYS = {
    "schema", "stage", "owner", "fixture", "source", "profile", "mode",
    "telemetry_enabled", "fixture_signature", "count", "warmups", "samples",
    "exclusions", "phase_names", "elapsed_ns", "phase_ns", "index_scan_scope",
    "index_scan_timing_relation", "latency_claim",
} | {"index_scan_" + field for field in SCAN_FIELDS}
COMPONENT_PHASES = ["omni_construct", "apply_apps_only_settings", "inert_manager_register",
                    "launcher_app_construct_and_transfer", "pending_query_publish",
                    "ctx_run_and_fulloutput_drop", "residual"]
HANDLERS = sorted(["json_folders", "json_bookmarks", "json_todos", "json_dashboard",
                   "note_plugin", "gui_folders", "gui_bookmarks", "gui_todos", "gui_notes",
                   "clipboard_modify"])
COMPONENT_KEYS = {
    "schema", "stage", "owner", "fixture", "source", "profile", "mode",
    "telemetry_enabled", "fixture_signature", "count", "warmups", "samples",
    "exclusions", "phase_names", "query_recipe", "constructor_empty_search_included",
    "exactly_one_frame", "sample_records",
}
SAMPLE_KEYS = {
    "elapsed_ns", "phase_ns", "catalog_receipt", "query_receipt", "action_count",
    "custom_count", "action_payload_utf8_bytes", "action_output_id", "structural_catalog_id",
    "full_state_id", "geometry_id", "viewport_id", "handler_lifecycle",
    "result_generation_before_request", "result_generation_after_frame",
    "provider_revision_after_frame", "catalog_versions_after_frame",
}
CATALOG_KEYS = {
    "root_count", "max_items_present", "max_items", "generation", "custom_count",
    "indexed_count", "catalog_count", "wait_generation_matches", "taken_generation_matches",
    "acknowledged_validation_ok", "result_still_queued",
}
QUERY_KEYS = {
    "sequence", "origin", "terminal", "cause", "started_frame", "terminal_frame",
    "frame_count", "terminal_elapsed_ns", "completion_latency_ns", "acceptance",
    "painted_sequence", "painted_acceptance_frame", "painted_frame", "painted_generation",
    "geometry_rebuilds", "geometry_rows", "geometry_by_cause", "geometry_cause_order",
}
ACCEPTANCE_KEYS = {
    "kind", "frame", "result_generation", "provider_revision", "catalog_versions",
    "catalog_versions_stable", "search_pending", "capacity_deferred",
    "direct_source_before", "direct_source_after", "direct_catalog_before", "direct_catalog_after",
}
GEOMETRY_CAUSES = ["query", "provider_or_catalog", "display_or_resize", "navigation", "other"]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def uint(value):
    require(type(value) is int and 0 <= value <= 2**64 - 1, "unsigned integer")


def exact_keys(value, keys):
    require(type(value) is dict and set(value) == set(keys), "exact object fields")


def object_pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate JSON key")
        result[key] = value
    return result


def bad_constant(_):
    raise ValueError("nonfinite JSON constant")


def read_markers(paths, marker):
    records, payloads = [], set()
    for path in paths:
        require(path.stat().st_size <= 64 * 1024 * 1024, "bounded log size")
        for line in path.read_text(encoding="utf-8-sig").splitlines():
            if marker not in line:
                continue
            require(line.count(marker) == 1, "one marker per line")
            body = line.partition(marker)[2]
            require(body.startswith(" "), "marker separator")
            body = body.strip()
            value = json.loads(body, object_pairs_hook=object_pairs, parse_constant=bad_constant)
            require(type(value) is dict, "record object")
            # Nextest may repeat an identical success-output payload. Different
            # payloads with the same recipe are never deduplicated.
            if body not in payloads:
                payloads.add(body)
                records.append(value)
    require(bool(records), "missing marker records")
    return records


def distribution(values):
    require(type(values) is list and len(values) == 20, "twenty samples")
    for value in values:
        uint(value)
    ordered = sorted(values)
    return dict(p50_ns=ordered[math.ceil(.5 * len(ordered)) - 1],
                p95_ns=ordered[math.ceil(.95 * len(ordered)) - 1], max_ns=ordered[-1])


def header(record, source, mode):
    require(type(source) is str and re.fullmatch("[0-9a-f]{40}", source), "committed source")
    require(mode in ("Small", "Full"), "mode")
    for field, expected in dict(schema=1, source=source, profile="fast-dev", mode=mode,
                                telemetry_enabled=True, warmups=5, samples=20, exclusions=0).items():
        require(type(record[field]) is type(expected) and record[field] == expected, "header " + field)
    uint(record["count"])
    require(record["count"] in ({16} if mode == "Small" else {16, 1000, 10000}), "fixture count")
    require(type(record["fixture_signature"]) is str
            and re.fullmatch("[0-9a-f]{16}", record["fixture_signature"]), "fixture signature")
    require(record["fixture"] == "owned_synthetic_index_tree", "fixture recipe")


def validate_startup(records, original, source, mode):
    require(original.get("source") == source and original.get("profile") == "fast-dev"
            and original.get("mode") == mode, "original collection provenance")
    wanted = {16} if mode == "Small" else {16, 1000, 10000}
    originals = original.get("records")
    require(type(originals) is list and len(originals) == len(wanted), "original inventory")
    require(all(type(r.get("item_count")) is int and r.get("owner") == "startup_action_catalog"
                and r.get("source") == source and r.get("profile") == "fast-dev"
                and r.get("mode") == mode for r in originals), "original owner/source")
    by_count = {r["item_count"]: r for r in originals}
    require(set(by_count) == wanted and len(by_count) == len(originals), "original exact counts")
    require(len(records) == len(wanted), "supplement inventory")
    seen, statistics = set(), []
    for record in records:
        exact_keys(record, STARTUP_KEYS)
        header(record, source, mode)
        count = record["count"]
        require(count not in seen, "duplicate recipe")
        seen.add(count)
        old = by_count[count]
        require(old["fixture_signature"] == FROZEN_FIXTURES[count], "frozen original fixture")
        require(record["fixture_signature"] == old["fixture_signature"], "original fixture join")
        require(record["stage"] == "startup_action_catalog_phase_attribution"
                and record["owner"] == "startup_action_catalog", "supplement owner")
        require(record["phase_names"] == STARTUP_PHASES, "phase names/order")
        require(record["index_scan_scope"] == "sum_of_index_batch_iter_next_step"
                and record["index_scan_timing_relation"] == "overlaps_wait"
                and record["latency_claim"] == "not_process_startup", "timing meaning")
        owner_stats = distribution(record["elapsed_ns"])
        require(record["elapsed_ns"] == old["elapsed_ns"], "exact original elapsed join")
        phases = record["phase_ns"]
        require(type(phases) is list and len(phases) == 20, "phase sample count")
        for index, phase in enumerate(phases):
            require(type(phase) is list and len(phase) == 7, "seven phases")
            for value in phase:
                uint(value)
            require(sum(phase) <= record["elapsed_ns"][index], "disjoint phase bound")
        for field in SCAN_FIELDS:
            distribution(record["index_scan_" + field])
        for index in range(20):
            expected = dict(calls=(count + 511) // 512 + 1, work_units=count,
                            completed=1, errors=0, abandoned=0)
            for field, value in expected.items():
                require(record["index_scan_" + field][index] == value, "actual scan " + field)
            require(record["index_scan_elapsed_ns_total"][index] <= record["elapsed_ns"][index],
                    "nested scan step timing bound")
        statistics.append(dict(count=count, elapsed=owner_stats,
            phases={name: distribution([phase[index] for phase in phases])
                    for index, name in enumerate(STARTUP_PHASES)},
            index_scan_steps=distribution(record["index_scan_elapsed_ns_total"])))
    require(seen == wanted, "complete supplement inventory")
    return dict(schema=1, status="COLLECTED_PENDING_INDEPENDENT_REVIEW", source=source,
                profile="fast-dev", mode=mode, records=records, statistics=statistics,
                native_display="NOT MEASURED", latency_claim="not_process_startup")


def signature(value):
    require(type(value) is str and re.fullmatch("[0-9a-f]{16}", value), "output signature")


def catalog_versions(value):
    require(type(value) is list and len(value) == 3, "catalog versions")
    for version in value:
        uint(version)


def validate_query(sample, count):
    receipt = sample["query_receipt"]
    exact_keys(receipt, QUERY_KEYS)
    for field in ("sequence", "started_frame", "terminal_frame", "frame_count",
                  "terminal_elapsed_ns", "completion_latency_ns", "painted_sequence",
                  "painted_acceptance_frame", "painted_frame", "painted_generation",
                  "geometry_rebuilds", "geometry_rows"):
        uint(receipt[field])
    for field in ("sequence", "started_frame", "terminal_frame", "frame_count",
                  "painted_sequence", "painted_acceptance_frame", "painted_frame"):
        require(receipt[field] == 1, "one fresh request/frame correlation")
    require(receipt["origin"] == "pending_query" and receipt["terminal"] == "completed"
            and receipt["cause"] == "first_usable_root_frame", "query completion")
    require(0 < receipt["completion_latency_ns"] <= receipt["terminal_elapsed_ns"]
            <= sample["phase_ns"][5], "nested request latency bound")
    acceptance = receipt["acceptance"]
    exact_keys(acceptance, ACCEPTANCE_KEYS)
    require(acceptance["kind"] == "search", "normal search acceptance")
    for field in ("frame", "result_generation", "provider_revision"):
        uint(acceptance[field])
    require(acceptance["frame"] == 1, "acceptance frame")
    for field, expected in (("catalog_versions_stable", True), ("search_pending", False),
                            ("capacity_deferred", False)):
        require(acceptance[field] is expected, "accepted completion gate")
    for field in ("direct_source_before", "direct_source_after", "direct_catalog_before",
                  "direct_catalog_after"):
        require(acceptance[field] is None, "search has no direct replacement provenance")
    catalog_versions(acceptance["catalog_versions"])
    require(acceptance["catalog_versions"] == sample["catalog_versions_after_frame"],
            "accepted current catalog")
    require(acceptance["provider_revision"] == sample["provider_revision_after_frame"],
            "accepted current provider")
    require(acceptance["result_generation"] == receipt["painted_generation"]
            == sample["result_generation_after_frame"]
            and sample["result_generation_after_frame"] > sample["result_generation_before_request"],
            "accepted current painted result generation")
    require(receipt["geometry_cause_order"] == GEOMETRY_CAUSES, "geometry cause order")
    require(receipt["geometry_rebuilds"] == 1 and receipt["geometry_rows"] == count + 3,
            "genuine complete cold list geometry")
    pairs = receipt["geometry_by_cause"]
    require(type(pairs) is list and len(pairs) == 5, "five geometry causes")
    totals = [0, 0]
    for pair in pairs:
        require(type(pair) is list and len(pair) == 2, "geometry pair")
        for index, value in enumerate(pair):
            uint(value)
            totals[index] += value
        require(pair[0] != 0 or pair[1] == 0, "rows require rebuild")
    require(totals == [1, count + 3], "geometry cause accounting")
    require(pairs == [[1, count + 3], [0, 0], [0, 0], [0, 0], [0, 0]],
            "pending query owns the cold geometry rebuild")


def validate_component(records, bindings, source, mode):
    require(mode in ("Small", "Full"), "component mode")
    wanted = {16} if mode == "Small" else {16, 1000, 10000}
    require(len(records) == len(wanted), "component inventory")
    require(type(bindings) is dict and set(bindings) == {str(n) for n in wanted},
            "independent component bindings")
    seen, statistics = set(), []
    for record in records:
        exact_keys(record, COMPONENT_KEYS)
        header(record, source, mode)
        count = record["count"]
        require(count not in seen, "duplicate component recipe")
        seen.add(count)
        binding = bindings[str(count)]
        exact_keys(binding, {"fixture_signature", "structural_catalog_id"})
        for value in binding.values():
            signature(value)
        require(record["fixture_signature"] == binding["fixture_signature"], "component fixture binding")
        require(record["stage"] == "ready_catalog_restricted_omni_first_cpu_frame"
                and record["owner"] == "real_omni_constructor+LauncherApp::new+one_egui_Context_run+FullOutput_drop",
                "component owner")
        require(record["query_recipe"] == "omni_list_all_apps"
                and record["constructor_empty_search_included"] is True
                and record["exactly_one_frame"] is True, "component scope")
        require(record["phase_names"] == COMPONENT_PHASES, "component phases")
        samples = record["sample_records"]
        require(type(samples) is list and len(samples) == 20, "component sample count")
        for sample in samples:
            exact_keys(sample, SAMPLE_KEYS)
            for field in ("elapsed_ns", "action_count", "custom_count", "action_payload_utf8_bytes",
                          "result_generation_before_request", "result_generation_after_frame",
                          "provider_revision_after_frame"):
                uint(sample[field])
            require(sample["elapsed_ns"] > 0 and sample["action_payload_utf8_bytes"] > 0,
                    "positive owner/payload")
            catalog_versions(sample["catalog_versions_after_frame"])
            require(sample["action_count"] == count + 3 and sample["custom_count"] == 3,
                    "complete result count")
            for field in ("action_output_id", "structural_catalog_id", "full_state_id", "geometry_id", "viewport_id"):
                signature(sample[field])
            require(sample["structural_catalog_id"] == binding["structural_catalog_id"],
                    "independent normalized catalog identity")
            phases = sample["phase_ns"]
            require(type(phases) is list and len(phases) == 7, "component seven phases")
            for value in phases:
                uint(value)
            require(sum(phases) == sample["elapsed_ns"], "disjoint phases plus residual")
            catalog = sample["catalog_receipt"]
            exact_keys(catalog, CATALOG_KEYS)
            for value in catalog.values():
                uint(value)
            require(catalog == dict(root_count=1, max_items_present=1, max_items=count,
                generation=1, custom_count=3, indexed_count=count, catalog_count=count + 3,
                wait_generation_matches=1, taken_generation_matches=1,
                acknowledged_validation_ok=1, result_still_queued=0), "complete acknowledged catalog")
            validate_query(sample, count)
            lifecycle = sample["handler_lifecycle"]
            exact_keys(lifecycle, {"attempts", "created", "released", "overflowed", "complete"})
            require(lifecycle["overflowed"] is False and lifecycle["complete"] is True,
                    "successful untimed handler teardown")
            for field in ("attempts", "created", "released"):
                require(lifecycle[field] == HANDLERS, "exact ten typed handlers")
        require(len({sample["action_output_id"] for sample in samples}) == 1,
                "fixed owned-root complete Actions across samples")
        require(len({sample["action_payload_utf8_bytes"] for sample in samples}) == 1,
                "fixed owned-root Action payload bytes across samples")
        statistics.append(dict(count=count,
            elapsed=distribution([s["elapsed_ns"] for s in samples]),
            phases={name: distribution([s["phase_ns"][index] for s in samples])
                    for index, name in enumerate(COMPONENT_PHASES)},
            pending_query_to_usable_cpu_frame=distribution([
                s["query_receipt"]["completion_latency_ns"] for s in samples])))
    require(seen == wanted, "complete component inventory")
    return dict(schema=1, status="COLLECTED_PENDING_INDEPENDENT_REVIEW", source=source,
        profile="fast-dev", mode=mode, records=records, statistics=statistics,
        native_display="NOT MEASURED", latency_claim="ready_catalog_component_not_process_startup",
        teardown="untimed_final_callback_release_not_native_thread_join")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--log", type=Path, action="append", required=True)
    parser.add_argument("--stage", choices=("startup", "component"), required=True)
    parser.add_argument("--original", type=Path)
    parser.add_argument("--bindings", type=Path)
    parser.add_argument("--source", required=True)
    parser.add_argument("--mode", choices=("Small", "Full"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.stage == "startup":
        require(args.original is not None and args.bindings is None, "startup original required")
        original = json.loads(args.original.read_text(encoding="utf-8-sig"), object_pairs_hook=object_pairs,
                              parse_constant=bad_constant)
        result = validate_startup(read_markers(args.log, STARTUP_MARKER), original, args.source, args.mode)
    else:
        require(args.bindings is not None and args.original is None, "component bindings required")
        bindings = json.loads(args.bindings.read_text(encoding="utf-8-sig"), object_pairs_hook=object_pairs,
                              parse_constant=bad_constant)
        result = validate_component(read_markers(args.log, COMPONENT_MARKER), bindings, args.source, args.mode)
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(result, output, indent=2)
        output.write("\n")


if __name__ == "__main__":
    main()

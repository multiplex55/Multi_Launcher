"""One static documentation check, not an admission model or Rust parser.

Run only within the M1-A check budget. Writes a new initial receipt and refuses
to overwrite it. A later authorized affected recheck needs its own receipt.
"""
from pathlib import Path
import datetime
import hashlib
import json
import platform
import re
import sys
import time

START = time.perf_counter()
HERE = Path(__file__).resolve()
TASK = HERE.parent.parent
REPO = TASK.parents[2]
RECEIPT = HERE.parent / "static-check-initial.json"
if RECEIPT.exists():
    raise SystemExit("Initial receipt already exists; check budget is not renewed.")


def canonical_bytes(path):
    raw = path.read_bytes()
    raw.decode("utf-8")
    assert not raw.startswith(b"\xef\xbb\xbf"), str(path)
    result = raw.replace(b"\r\n", b"\n")
    assert b"\r" not in result, str(path)
    return result


def digest(path):
    return hashlib.sha256(canonical_bytes(path)).hexdigest()


checks = []


def require(condition, name):
    if not condition:
        raise AssertionError(name)
    checks.append(name)


result = {
    "kind": "static_document_check_not_admission_model",
    "invocation": 1,
    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "command": "C:/Python311/python.exe -X utf8 -B docs/implementation/SEARCH-F002-discovery-admission-consumer-proof/receipts/static_check.py",
    "python": sys.version,
    "platform": sys.platform,
    "platform_detail": platform.platform(),
    "source_revision": "59156d96adcc73939c7171487a0cb582e3286587",
    "checker_sha256_lf": digest(HERE),
    "optional_model_status": "NOT_NEEDED",
    "optional_model_invocations": 0,
    "optional_model_states": 0,
    "optional_model_elapsed_seconds": 0,
}

try:
    manifest = (TASK / "SOURCE_ADMISSION_MANIFEST.md").read_text(encoding="utf-8")
    contract = (TASK / "CONSUMER_COMPATIBILITY_CONTRACT.md").read_text(encoding="utf-8")
    decision = (TASK / "ADMISSION_DECISION.md").read_text(encoding="utf-8")
    source = (REPO / "src/plugin.rs").read_text(encoding="utf-8")
    begin = source.index("pub fn reload_from_dirs")
    end = source.index("\n    pub fn register(", begin)
    registrations = re.findall(r"self\.register_with_settings\(\s*(\w+)", source[begin:end])
    rows = [line for line in manifest.splitlines() if re.match(r"\| `\w+Plugin` /", line)]
    types = [re.match(r"\| `(\w+)`", row).group(1) for row in rows]
    names = [re.match(r"\| `\w+` / `([^`]+)`", row).group(1) for row in rows]
    require(types == registrations, "manifest rows equal ordered current builtin registrations")
    require(len(rows) == len(set(types)) == len(set(names)) == 81, "81 unique types and exact names")
    require(len(rows) <= 256, "roster ceiling")
    require(sum("| HG;" in row for row in rows) == 36, "36 HEAD_GATED rows")
    require(sum("| GG;" in row for row in rows) == 45, "45 REVIEWED_GLOBAL_GRAMMAR rows")
    for row in rows:
        ty, name = re.match(r"\| `(\w+)` / `([^`]+)`", row).groups()
        locator = re.search(r"`(src/[^`]+\.rs):(\d+)/(\d+)`", row)
        path, impl_line, search_line = locator.groups()
        text = (REPO / path).read_text(encoding="utf-8")
        lines = text.splitlines()
        require(lines[int(impl_line) - 1].strip() == f"impl Plugin for {ty} {{", f"{ty} impl locator")
        require("fn search(" in lines[int(search_line) - 1], f"{ty} search locator")
        impl_start = text.index(f"impl Plugin for {ty} {{")
        impl_end = text.index("\n}", impl_start)
        block = text[impl_start:impl_end + 2]

        def body(method):
            match = re.search(r"^    fn " + method + r"\b.*?(?=^    fn |^\})", block, re.M | re.S)
            return match.group() if match else ""

        expected_names = re.findall(r'"([^"\n]+)"', body("name"))
        expected_name = expected_names[0] if expected_names else "mouse_gestures"
        require(name == expected_name, f"{ty} name")
        prefixes = re.findall(r'"([^"\n]+)"', body("query_prefixes"))
        if "PREFIXES" in body("query_prefixes"):
            require('const PREFIXES: &[&str] = &["paste"];' in text, "Paste prefix constant")
            prefixes = ["paste"]
        if not expected_names:
            require('const PLUGIN_NAME: &str = "mouse_gestures";' in text, "Mouse Gestures name constant")
        always = bool(re.search(r"\btrue\b", body("always_search")))
        policy = "Literal" if "Literal" in body("query_policy") else "ActionFilters"
        declaration = f'`{", ".join(prefixes) if prefixes else "[]"}`; {str(always).lower()}; {policy}'
        require(declaration in row, f"{ty} prefix/always/query policy")
        category = "HG" if prefixes and not always else "GG"
        require(f"| {category};" in row, f"{ty} manager category")
        recorded_hash = re.findall(r"`([0-9a-f]{64})`", row)[-1]
        require(digest(REPO / path) == recorded_hash, f"{ty} source hash")
        reg_line = int(re.search(r"Builtin; plugin.rs:(\d+)", row).group(1))
        require("register_with_settings" in source.splitlines()[reg_line - 1], f"{ty} registration locator")
    details = manifest.split("## Global detail:", 1)[1].split("## Unknown,", 1)[0]
    detail_names = re.findall(r"^\| ([a-z_]+) \|", details, re.M)
    global_names = [name for row, name in zip(rows, names) if "| GG;" in row]
    require(sorted(detail_names) == sorted(global_names), "45 unique global details match 45 global owners")
    require(len(detail_names) == 45, "global entry ceiling")
    for path, recorded_hash in re.findall(r"\| `(src/[^`]+\.rs)` \| `([0-9a-f]{64})`", manifest):
        require(digest(REPO / path) == recorded_hash, f"shared source hash {path}")
    docs = [TASK / name for name in [
        "SOURCE_ADMISSION_MANIFEST.md", "CONSUMER_COMPATIBILITY_CONTRACT.md",
        "ADMISSION_DECISION.md", "VERIFICATION.md", "roles/implementer/ROLE_RECORD.md",
        "roles/implementer/ANALYSIS.md",
    ]]
    links = 0
    locators = 0
    for document in docs:
        text = document.read_text(encoding="utf-8")
        for link in re.findall(r"\[[^\]]+\]\(([^)]+)\)", text):
            if "://" in link or link.startswith("#"):
                continue
            target = (document.parent / link.split("#", 1)[0]).resolve()
            require(target.exists() or target == RECEIPT, f"relative reference {document.name}: {link}")
            links += 1
        for path, line in re.findall(r"\b(src/[a-zA-Z0-9_./-]+\.rs)(?::(\d+))?", text):
            if document.name == "ANALYSIS.md" and path in {"src/common.rs", "src/action_filters.rs"}:
                require(not (REPO / path).exists(), f"explicitly recorded locator miss {path}")
                continue
            require((REPO / path).is_file(), f"source path {path}")
            if line:
                require(1 <= int(line) <= len((REPO / path).read_text(encoding="utf-8").splitlines()), f"source locator {path}:{line}")
            locators += 1
    contract_hash = digest(TASK / "CONSUMER_COMPATIBILITY_CONTRACT.md")
    require(contract_hash in decision, "exact contract content hash in acceptance block")
    require(contract.startswith("# SEARCH-F002-consumer-v1\n"), "exact contract version")
    require("REJECTED_CANDIDATE / H0 NO_CHANGE" in contract, "rejected contract status agrees with H0")
    require("**H0 — NO_CHANGE.**" in decision, "H0 decision")
    require("CONTRACT_NOT_ACCEPTED" in decision, "separate consumer acceptance is absent")
    require("PENDING_PARENT_M2_RECEIPT" in decision, "no invented closing SHA")
    require("NOT_NEEDED" in decision and "0 states, 0 invocations, 0 seconds" in decision, "optional model truthful")
    require("C1, ordinary/default-like" in contract and "C2, XML-focused restricted profile" in contract, "two concrete configurations")
    for token in ["Unknown", "DynamicLibrary", "Registered", "disabled", "epoch"]:
        require(token.lower() in manifest.lower(), f"coverage identity counterguard {token}")
    for token in ["instantaneous", "queryexec", "Capacity", "Literal", "SaveIntent", "query_override=None", "args = None", "FreshUserEdit"]:
        require(token in contract, f"compatibility obligation {token}")
    result.update({
        "status": "PASSED", "registration_count": 81, "head_gated_count": 36,
        "global_grammar_count": 45, "global_content_count": 0,
        "unknown_builtin_outer_envelopes": 0, "unknown_external_identity_classes_retained": True,
        "configuration_count": 2, "relative_references_checked": links,
        "source_path_locator_occurrences_checked": locators,
        "artifact_sha256_lf": {str(p.relative_to(TASK)).replace("\\", "/"): digest(p) for p in docs},
    })
except Exception as error:
    result.update({"status": "FAILED", "error": f"{type(error).__name__}: {error}"})

result["elapsed_seconds"] = round(time.perf_counter() - START, 6)
result["assertions_passed"] = len(checks)
result["check_groups"] = [
    "81 ordered registration/type/name/declaration/category/source-hash/locator rows",
    "45 unique global details; 36 head-gated rows; 256/45 roster/body ceilings",
    "shared source file hashes; relative references and source locator ranges",
    "explicitly documented navigation misses are absent, not evidence locators",
    "contract version/content hash, H0, absent acceptance and pending closing receipt",
    "two configurations and retained identity/compatibility counterguards",
    "optional model NOT_NEEDED with zero invocations, states and runtime",
]
RECEIPT.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
print(json.dumps({key: result[key] for key in ["status", "elapsed_seconds", "assertions_passed"]}))
if result["status"] != "PASSED":
    print(result["error"])
    raise SystemExit(1)

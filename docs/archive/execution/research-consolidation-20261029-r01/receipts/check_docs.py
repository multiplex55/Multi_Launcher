"""Bounded integration metadata check, never imports or executes research code.

Run from repository root. The parent runner enforces a 60-second deadline.
Reads docs, JSON/JSONL and local Git metadata only; no network or scoring.
"""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import time
from urllib.parse import unquote, urlsplit

started = time.perf_counter()
repo = Path.cwd().resolve()
root = repo / "docs/plans/execution/research-consolidation-20261029-r01"
manifest = json.loads((root / "receipts/integration-imports.json").read_text(encoding="utf-8-sig"))
base = manifest["stacked_base_sha"]
failures, assertions = [], 0


def check(ok, label):
    global assertions
    assertions += 1
    if not ok:
        failures.append(label)


def git(*args):
    result = subprocess.run(["git", *args], cwd=repo, capture_output=True, timeout=10)
    if result.returncode:
        raise RuntimeError(result.stderr.decode("utf-8", errors="replace"))
    return result.stdout.decode("utf-8")


def text(path):
    return path.read_text(encoding="utf-8-sig")


def slugs(body):
    found, seen = set(), {}
    for heading in re.findall(r"^#{1,6}\s+(.+)$", body, re.M):
        slug = re.sub(r"[^\w\- ]", "", heading.strip().lower()).replace(" ", "-")
        ordinal = seen.get(slug, 0)
        found.add(slug if ordinal == 0 else f"{slug}-{ordinal}")
        seen[slug] = ordinal + 1
    return found


def prose(body):
    lines, fence = [], None
    for line in body.splitlines():
        mark = re.match(r"^\s*(`{3,}|~{3,})", line)
        if mark:
            token = mark[1]
            if fence is None:
                fence = token
            elif token[0] == fence[0] and len(token) >= len(fence):
                fence = None
            continue
        if fence is None:
            lines.append(line)
    return re.sub(r"(`+).*?\1", "", "\n".join(lines))


entries = {}
for line in git("ls-tree", "-r", "HEAD", "--", "docs").splitlines():
    meta, path = line.split("\t", 1)
    mode, kind, blob = meta.split()
    entries[path] = (mode, blob)
imports = manifest["pr2_imports"]
for row in imports:
    check(entries.get(row["path"]) == (row["mode"], row["blob"]), "exact source blob: " + row["path"])
check(len(imports) == 171 and len({r["path"] for r in imports}) == 171, "171 distinct immutable import paths")

changed = set(git("diff", "--name-only", base + "...HEAD").splitlines())
modified_worktree = set(git("diff", "--name-only", "HEAD").splitlines())
changed.update(modified_worktree)
changed.update(git("ls-files", "--others", "--exclude-standard").splitlines())
for path in changed:
    check(path.startswith("docs/"), "docs-only PR2 path: " + path)
check(not git("diff", "--name-only", base, "--", "src", "tests", "crates", ".codex", ".github", "Cargo.toml", "Cargo.lock", "build.rs", "AGENTS.md"), "no production/config/test delta versus actual stacked base")
check(git("rev-parse", "origin/" + manifest["stacked_base_branch"]).strip() == base, "published local base ref remains pinned")
check(git("branch", "--show-current").strip() == "codex/integrate-research-docs-20261029-r01", "actual docs branch")
import_paths = [row["path"] for row in imports]
check(not (modified_worktree & set(import_paths)), "no uncommitted rewrite of immutable imported paths")

md = {repo / r["path"] for r in imports if r["path"].endswith(".md")}
md.update(root.rglob("*.md"))
goal = repo / "docs/plans/research-merge-20261029/GOAL_RESEARCH_INTEGRATION_AND_MERGE.md"
kick = goal.with_name("KICKSTART_RESEARCH_INTEGRATION_AND_MERGE.md")
md.update([goal, kick])
local_links, anchor_links, external_links = 0, 0, 0
for doc in sorted(md):
    body = prose(text(doc))
    targets = re.findall(r"\[[^\]\n]*\]\(([^)\n]+)\)", body)
    targets += re.findall(r"^\[[^\]\n]+\]:\s*(\S+)", body, re.M)
    for raw in targets:
        target = raw.strip().strip("<>")
        url = urlsplit(target)
        if url.scheme:
            external_links += 1
            continue
        path = (doc.parent / unquote(url.path)).resolve() if url.path else doc
        local_links += 1
        exists = path.is_file() or path.is_dir()
        check(exists, "local link: " + str(doc.relative_to(repo)) + " -> " + target)
        if not exists or not url.fragment:
            continue
        anchor_links += 1
        if path.is_dir():
            path = path / "README.md"
        check(path.is_file(), "anchor target readable: " + target)
        if not path.is_file():
            continue
        content = text(path)
        fragment = unquote(url.fragment)
        lines = re.fullmatch(r"L(\d+)(?:-L(\d+))?", fragment)
        if lines:
            first, last = int(lines[1]), int(lines[2] or lines[1])
            check(1 <= first <= last <= len(content.splitlines()), "line bounds: " + target)
        else:
            check(fragment in slugs(content), "heading anchor: " + str(doc.relative_to(repo)) + " -> " + target)

json_files, jsonl_files, jsonl_records = 0, 0, 0
counts = {}
initial_predictions, corrective_predictions = 0, 0
structured = {repo / r["path"] for r in imports if r["path"].endswith((".json", ".jsonl"))}
structured.update(root.rglob("*.json"))
for path in sorted(structured):
    try:
        def reject_constant(value):
            raise ValueError("Nonstandard JSON constant: " + value)
        body = text(path)
        if path.suffix == ".jsonl":
            rows = [json.loads(line, parse_constant=reject_constant) for line in body.splitlines() if line.strip()]
            count = len(rows)
            counts[str(path.relative_to(repo))] = count
            jsonl_files += 1
            jsonl_records += count
            check(count == 300, "300 records in preserved corpus/result: " + str(path.relative_to(repo)))
            if path.name.startswith("cases-"):
                if "P0-P1-P2-corrective" in path.parts:
                    corrective_predictions += count
                else:
                    initial_predictions += count
        else:
            json.loads(body, parse_constant=reject_constant)
            json_files += 1
        check(True, "parse structured document: " + str(path.relative_to(repo)))
    except (ValueError, UnicodeError) as error:
        check(False, "structured document: " + str(path.relative_to(repo)) + ": " + str(error))
check(initial_predictions == 1800, "1800 original predictions preserved")
check(corrective_predictions == 1800, "1800 corrective records preserved without rerun")
check(jsonl_files == 14 and jsonl_records == 4200, "14 JSONL files / 4200 preserved records including both 300-case versions")

research = repo / "docs/research/multi-launcher-research-20261028-r01"
program = text(research / "PROGRAM_STATUS.md")
for row in ["| 01 Static audit | RESEARCH_COMPLETE_WITH_LIMITATIONS |", "| 02 Search discovery | RESEARCH_COMPLETE_WITH_LIMITATIONS |"]:
    check(row in program, "both original studies complete with limitations: " + row)
for relative in ["01-static-audit/follow-on/AUD-F001-preserve-terminal-query-escape", "01-static-audit/follow-on/AUD-F002-diff-mutation-authority", "02-search-discovery/follow-on/SEARCH-F001-built-in-discovery-metadata", "02-search-discovery/follow-on/SEARCH-F002-discovery-admission-consumer-proof"]:
    for name in ["GOAL.md", "KICKSTART.md"]:
        check((research / relative / name).is_file(), "complete canonical pair: " + relative + "/" + name)
queue = text(repo / "docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md")
for label in ["AUD-F001", "SEARCH-F002", "AUD-F002", "SEARCH-F001", "DEFERRED_DEPENDENCY", "not complete or accepted"]:
    check(label in queue, "final source queue disposition: " + label)
admission = repo / "docs/implementation/SEARCH-F002-discovery-admission-consumer-proof"
check("CONTRACT_NOT_ACCEPTED" in text(admission / "CLOSURE_RECEIPT.md"), "consumer remains unaccepted")
check("H0" in text(admission / "ADMISSION_DECISION.md") and "NO_CHANGE" in text(admission / "ADMISSION_DECISION.md"), "admission H0 NO_CHANGE")
contract = (admission / "CONSUMER_COMPATIBILITY_CONTRACT.md").read_bytes().replace(b"\r\n", b"\n")
check(hashlib.sha256(contract).hexdigest() == "8e26d00204f9e1ed703ef17b1cc1d79f1d99089f3e9a39842f2204ddba603013", "exact rejected consumer version hash")
diff = repo / "docs/research/diff-mutation-authority-r01"
check("H0" in text(diff / "DECISION.md"), "Diff H0 retained")
for name in ["GOAL.md", "KICKSTART.md"]:
    check("PROPOSED_NOT_AUTHORIZED" in text(diff / "proposals/diff-failure-information" / name), "new Diff proposal remains unexecuted: " + name)
source_goal = git("show", "124f929dbcc272396a329c0adf380d223ece07b9:" + str(goal.relative_to(repo)).replace("\\", "/"))
check(text(goal).split("## 9. Execution progress — mutable")[0] == source_goal.split("## 9. Execution progress — mutable")[0], "immutable integration Goal prefix exact")
source_kick = git("show", "124f929dbcc272396a329c0adf380d223ece07b9:" + str(kick.relative_to(repo)).replace("\\", "/"))
check(text(kick).replace("\r\n", "\n") == source_kick.replace("\r\n", "\n"), "complete integration Kickstart exact")

print(json.dumps({"kind":"integration_docs_metadata", "status":"PASS" if not failures else "FAIL",
    "assertions":assertions, "failures":failures, "source_imports_exact":len(imports),
    "markdown_files":len(md), "local_links":local_links, "relative_anchor_links":anchor_links,
    "external_links_not_refetched":external_links, "json_files":json_files, "jsonl_files":jsonl_files,
    "jsonl_records":jsonl_records, "initial_predictions":initial_predictions,
    "corrective_prediction_records":corrective_predictions, "jsonl_counts":counts,
    "actual_base_branch":manifest["stacked_base_branch"], "actual_base_sha":base,
    "checked_head":git("rev-parse","HEAD").strip(),
    "script_sha256":hashlib.sha256(Path(__file__).read_bytes().replace(b"\r\n",b"\n")).hexdigest(),
    "elapsed_seconds":round(time.perf_counter()-started,6), "budget_seconds":60,
    "scope":"Preservation/link/structured-data/scope metadata only; no algorithm, source semantics, usability or native execution."}, indent=2))
raise SystemExit(bool(failures))

"""One bounded documentation consistency pass; never executes application code.

Run from the repository root with Python -X utf8 -B. Reads only named research
artifacts, their links, nine source files and three resolved dependency files.
Git subprocesses are read-only; remote URLs are not fetched. This is not the
optional identity model, a native proof, or a semantic source verifier.
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
root = repo / "docs/research/diff-mutation-authority-r01"
queue = "docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md"
base = "59156d96adcc73939c7171487a0cb582e3286587"
failures, checked = [], 0


def check(ok, label):
    global checked
    checked += 1
    if not ok:
        failures.append(label)


def git(*args):
    result = subprocess.run(["git", *args], cwd=repo, capture_output=True, timeout=10)
    if result.returncode:
        raise RuntimeError(result.stderr.decode("utf-8", errors="replace"))
    return result.stdout.decode("utf-8")


def anchors(text):
    found, seen = set(), {}
    for heading in re.findall(r"^#{1,6}\s+(.+)$", text, re.M):
        slug = re.sub(r"[^\w\- ]", "", heading.strip().lower()).replace(" ", "-")
        ordinal = seen.get(slug, 0)
        found.add(slug if ordinal == 0 else f"{slug}-{ordinal}")
        seen[slug] = ordinal + 1
    return found


required = ["README.md", "GOAL.md", "KICKSTART.md", "CONTRACT_AND_EVIDENCE.md",
            "IDENTITY_PROOF.md", "API_REFERENCES.md", "SOURCE_AND_TEST_INVENTORY.md",
            "DECISION.md", "LIMITATIONS.md", "VERIFICATION.md", "REVIEW.md",
            "roles/planner/ROLE_RECORD.md", "roles/planner/ANALYSIS.md",
            "roles/implementer/ROLE_RECORD.md", "roles/implementer/ANALYSIS.md",
            "roles/reviewer/ROLE_RECORD.md", "roles/reviewer/REVIEW.md",
            "proposals/diff-failure-information/GOAL.md",
            "proposals/diff-failure-information/KICKSTART.md"]
for name in required:
    check((root / name).is_file(), f"required artifact: {name}")

cache, local_links, pinned_links, external_links = {}, 0, 0, 0
docs = sorted(root.rglob("*.md")) + [repo / queue]
for doc in docs:
    body = doc.read_text(encoding="utf-8")
    check(bool(body.strip()), f"nonempty: {doc.relative_to(repo)}")
    links = re.findall(r"\[[^\]\n]*\]\(([^)\n]+)\)", body)
    links += re.findall(r"^\[[^\]\n]+\]:\s*(\S+)", body, re.M)
    for raw in links:
        target = raw.strip().strip("<>")
        url = urlsplit(target)
        fragment = unquote(url.fragment)
        text = None
        if url.scheme in ("https", "http"):
            match = re.fullmatch(r"/multiplex55/Multi_Launcher/blob/([0-9a-f]{40})/(.+)", url.path)
            if url.netloc == "github.com" and match:
                pinned_links += 1
                spec = match.group(1) + ":" + unquote(match.group(2))
                try:
                    if spec not in cache:
                        cache[spec] = git("show", spec)
                    text = cache[spec]
                    check(True, f"pinned link exists: {spec}")
                except RuntimeError:
                    check(False, f"pinned link absent: {spec}")
            else:
                external_links += 1
            continue_fragment = text is not None
        elif not url.scheme:
            local_links += 1
            path = (doc.parent / unquote(url.path)).resolve() if url.path else doc
            exists = path.is_file()
            check(exists, f"local link: {doc.relative_to(repo)} -> {target}")
            if exists:
                text = path.read_text(encoding="utf-8")
            continue_fragment = exists
        else:
            check(False, f"unexpected link scheme: {target}")
            continue_fragment = False
        if fragment and continue_fragment:
            lines = re.fullmatch(r"L(\d+)(?:-L(\d+))?", fragment)
            if lines:
                first, last = int(lines[1]), int(lines[2] or lines[1])
                check(1 <= first <= last <= len(text.splitlines()), f"line bounds: {target}")
            else:
                check(fragment in anchors(text), f"heading anchor: {target}")

source_paths = ["src/diff/file_ops.rs", "src/gui/diff_dialog/mod.rs",
                "src/gui/diff_dialog/operation_preview.rs", "src/diff/folder_runtime.rs",
                "src/file_operations.rs", "tests/domain_cases/diff_file_ops.rs",
                "Cargo.toml", "Cargo.lock", "tests/suites/domain.rs"]
dependency_root = Path("C:/Users/Jay/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/trash-5.2.4")
inventory = (root / "SOURCE_AND_TEST_INVENTORY.md").read_text(encoding="utf-8")
for group, paths in (("S", [repo / p for p in source_paths]),
                     ("D", [dependency_root / p for p in ["src/lib.rs", "src/windows.rs", "Cargo.toml.orig"]])):
    for index, path in enumerate(paths, 1):
        ident = f"{group}{index}"
        row = re.search(r"\| " + ident + r" \| `([0-9a-f]{64})` \| (\d+) \|", inventory)
        data = path.read_bytes().replace(b"\r\n", b"\n")
        check(bool(row) and hashlib.sha256(data).hexdigest() == row[1], f"content hash: {ident}")
        check(bool(row) and len(data.splitlines()) == int(row[2]), f"line count: {ident}")

changed = set(git("diff", "--name-only", base).splitlines())
changed.update(git("ls-files", "--others", "--exclude-standard").splitlines())
for path in changed:
    check(path.startswith("docs/research/diff-mutation-authority-r01/") or path == queue,
          f"authorized changed path: {path}")
check(not git("diff", base, "--", "src", "tests", "Cargo.toml", "Cargo.lock", ".codex", "AGENTS.md"),
      "source/tests/manifests/config/instructions unchanged")
check(git("branch", "--show-current").strip() == "codex/research-diff-mutation-authority-r01", "task branch")
for name in ["GOAL.md", "KICKSTART.md"]:
    check("PROPOSED_NOT_AUTHORIZED" in (root / "proposals/diff-failure-information" / name).read_text(encoding="utf-8"),
          f"new proposal remains unexecuted: {name}")

review = (root / "roles/reviewer/REVIEW.md").read_text(encoding="utf-8")
proposal_goal = (root / "proposals/diff-failure-information/GOAL.md").read_text(encoding="utf-8")
proposal_start = (root / "proposals/diff-failure-information/KICKSTART.md").read_text(encoding="utf-8")
amendments = re.findall(r"^> ([34]\. .+)$", review, re.M)
check(len(amendments) == 3, "three exact reviewer paragraph amendments")
for index, amendment in enumerate(amendments):
    check(amendment in (proposal_goal if index < 2 else proposal_start), f"review amendment {index + 1} applied")
for row in re.findall(r"^\| GUI (?:lifetime|identity) \|.+$", review, re.M):
    check(row in proposal_goal, "review lifetime/identity acceptance row applied")

print(json.dumps({"kind": "static_artifact_consistency", "status": "PASS" if not failures else "FAIL",
    "assertions": checked, "failures": failures, "markdown_files": len(docs),
    "local_links": local_links, "pinned_repository_links": pinned_links,
    "external_links_not_refetched": external_links, "source_hashes": 12,
    "head": git("rev-parse", "HEAD").strip(), "base": base,
    "script_sha256": hashlib.sha256(Path(__file__).read_bytes().replace(b"\r\n", b"\n")).hexdigest(),
    "elapsed_seconds": round(time.perf_counter() - started, 6),
    "scope": "Documentation/link/hash/scope consistency only; no behavioral, model or native proof."}, indent=2))
raise SystemExit(bool(failures))
